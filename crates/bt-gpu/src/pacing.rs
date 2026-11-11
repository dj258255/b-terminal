//! 창 길의 프레임 간격(ADR-038): 그림판을 얻느라 메인 스레드가 막히지 않게 올릴 때를 정하고,
//! 창이 보이는지 아직 모를 때 층을 올리기 전에 먼저 알아본다. 둘 다 시계·창 없이 시험하는 순수한 계산이다

use std::collections::VecDeque;
use std::time::{Duration, Instant};

/// 화면 주사율을 모를 때 쓰는 값(60Hz)
pub const DEFAULT_REFRESH: Duration = Duration::from_nanos(16_666_667);

/// 줄 깊이(`FramePacer`가 줄 서 있다고 세는 프레임 수)의 기본값. surface에는 이보다 하나 큰 값(`surface_latency`, 최대 2)을 청한다.
/// 줄 깊이 1이면 보통 그림판은 2장만 쓰인다. 실험 E14(Apple M2 Pro 내장 120Hz, 2000×1360)에서 그림판 한 장이 약 10.5MB여서
/// 3장보다 창 하나의 물리 메모리가 약 10.5MB 줄었고, 그림판을 얻는 시간은 늘지 않았다. 외부 화면·60Hz 화면은 재지 않았다.
/// 다만 surface에 청한 지연을 줄 깊이와 같은 1로 하면(그림판 최대 2장) 화면이 그림판을 오래 쥘 때 마지막 프레임이 화면에 붙지 않고
/// 멈추는 일이 있었다(#447, 실험 E22에서 main 25회 중 11회). 지금은 surface 지연을 2로 청해 그림판을 최대 3장까지 허락하고, 셋째 장은
/// 화면이 그림판을 오래 쥘 때만 생긴다(셰이더 없는 창 10회 중 4회, 셰이더를 건 창은 늘). 고친 뒤 10회 중 0회가 멈췄다. 줄 깊이는 `BT_FRAME_LATENCY`(1~3)로 고른다
pub const DEFAULT_FRAME_LATENCY: u32 = 1;

/// 측정용 환경 변수 `BT_FRAME_LATENCY`의 값에서 줄 깊이를 고른다. 1~3이면 그 값, 없거나 그 밖이면 기본값이다.
/// surface에 청하는 지연은 `surface_latency`가 따로 정한다
pub fn frame_latency_from_env(value: Option<&str>) -> u32 {
    match value.and_then(|v| v.trim().parse::<u32>().ok()) {
        Some(n) if (1..=3).contains(&n) => n,
        _ => DEFAULT_FRAME_LATENCY,
    }
}

/// surface에 청하는 `desired_maximum_frame_latency`: 줄 깊이 + 1, 단 2를 넘지 않게(#447).
/// wgpu-hal metal은 이 값 + 1장을 `maximumDrawableCount`로 정하고 Metal은 2~3장만 받는다. 줄 깊이와 같게 청하면(깊이 1이면 지연 1,
/// 그림판 2장) 화면이 그림판을 오래 쥘 때 마지막 프레임이 화면에 붙지 않고 멈추는 일이 있었다. 미루기 계산은 줄 깊이를 그대로 쓴다
pub fn surface_latency(depth: u32) -> u32 {
    (depth + 1).min(2)
}

/// `FramePacer::check`의 답
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PaceDecision {
    /// 지금 그려 올려도 그림판을 기다리지 않는다
    Go,
    /// 이만큼 뒤에 다시 물어라. 그때는 `Go`다
    Wait(Duration),
}

/// 화면 주사율(밀리헤르츠, winit `refresh_rate_millihertz`)에서 한 장 간격을 구한다.
/// 모르거나 말이 안 되는 값(30Hz 아래·360Hz 위)이면 60Hz로 본다
pub fn interval_from_millihertz(millihertz: Option<u32>) -> Duration {
    match millihertz {
        Some(m) if (30_000..=360_000).contains(&m) => Duration::from_nanos(1_000_000_000_000 / m as u64),
        _ => DEFAULT_REFRESH,
    }
}

/// 올린 프레임이 아직 화면에 오르지 않고 줄 서 있다고 보고 세어, 줄이 가득 차면 다음 프레임을 미룬다(`depth`가 줄 깊이).
/// 줄 깊이는 surface 지연(`surface_latency`)과 다를 수 있다: surface는 그림판을 하나 더 허락하고 미룸 계산은 `depth`장만 센다.
///
/// Metal은 `nextDrawable`을 `desired_maximum_frame_latency + 1`장으로 돌려 쓰고(`wgpu-hal` metal `surface.rs` 330줄)
/// 모두 쓰는 중이면 그 호출이 빈 장이 날 때까지 막는다(334줄 `setAllowsNextDrawableTimeout(false)`, 375줄 `nextDrawable`).
/// 막히는지 미리 물을 길은 wgpu에 없으므로(`acquire_texture`의 `timeout`은 쓰이지 않는다, 349줄 TODO) 올린 때를 세어 짐작한다:
/// 화면은 한 주기에 한 장을 가져가니 `depth`주기 안에 올린 프레임이 `depth`장이면 줄이 찼다.
/// 짐작이라 어긋날 수 있고, 어긋나면 옛날처럼 한 번 기다릴 뿐이다
#[derive(Debug, Clone)]
pub struct FramePacer {
    interval: Duration,
    depth: usize,
    presents: VecDeque<Instant>,
    /// 얻기에서 실제로 기다린 프레임에서 배운, 화면이 한 장을 가져가는 간격(들은 주기보다 길 때만)
    learned: Option<Duration>,
}

/// 이보다 오래 기다린 얻기만 "막혔다"로 본다(시계 흔들림을 배우지 않게)
const BLOCKED: Duration = Duration::from_millis(1);
/// 막히지 않은 프레임마다 배운 간격을 줄이는 비율. 화면이 다시 빨라졌는지는 막히지 않는 것으로만 알 수 있어 천천히 돌아간다
const UNLEARN: f64 = 0.99;

impl FramePacer {
    /// `depth`는 줄 깊이(surface 지연은 `surface_latency(depth)`)
    pub fn new(depth: usize, interval: Duration) -> Self {
        Self {
            interval,
            depth: depth.max(1),
            presents: VecDeque::new(),
            learned: None,
        }
    }

    /// 지금 쓰는 화면 한 장 간격(들은 주기)
    pub fn interval(&self) -> Duration {
        self.interval
    }

    /// 화면 주사율이 바뀌었다(다른 화면으로 옮겨 갔을 때)
    pub fn set_interval(&mut self, interval: Duration) {
        if interval != self.interval {
            self.learned = None;
        }
        self.interval = interval;
    }

    /// 창에 한 장 올렸다. 그림판을 얻느라 `waited`만큼 기다렸다
    ///
    /// 들은 주기(`interval`)는 화면 주사율이지만, 계속 올리는 동안 화면이 실제로 그만큼 자주 가져가는지는 모른다
    /// (실험 E11: 120Hz 화면에서 16.6ms마다 한 장씩 빠져 얻기가 프레임마다 약 16ms 막혔다). 그래서 얻기에서 막힌 프레임이면
    /// 앞 장과의 간격이 실제로 한 장이 빠지는 간격이라 보고 배운다(들은 주기의 네 배까지). 막히지 않은 프레임이 이어지면
    /// 배운 간격을 조금씩 줄여 들은 주기로 돌아간다
    pub fn record_present_waited(&mut self, at: Instant, waited: Duration) {
        let prev = self.presents.back().copied();
        if waited >= BLOCKED {
            let gap = prev.and_then(|p| at.checked_duration_since(p));
            if let Some(gap) = gap.filter(|g| *g > self.interval) {
                self.learned = Some(gap.min(self.interval * 4));
            }
        } else if let Some(l) = self.learned {
            let next = l.mul_f64(UNLEARN);
            self.learned = (next > self.interval).then_some(next);
        }
        self.record_present(at);
    }

    /// 창에 한 장 올렸다
    pub fn record_present(&mut self, at: Instant) {
        self.presents.push_back(at);
        while self.presents.len() > self.depth {
            self.presents.pop_front();
        }
    }

    /// `now`에 올려도 되는가
    pub fn check(&self, now: Instant) -> PaceDecision {
        if self.presents.len() < self.depth {
            return PaceDecision::Go;
        }
        let Some(oldest) = self.presents.front() else {
            return PaceDecision::Go;
        };
        let leaves = *oldest + self.learned.unwrap_or(self.interval) * self.depth as u32;
        match leaves.checked_duration_since(now) {
            Some(wait) if !wait.is_zero() => PaceDecision::Wait(wait),
            _ => PaceDecision::Go,
        }
    }
}

/// 창 surface가 보이는지에 대해 아는 것
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum SurfaceVisibility {
    /// 아직 모른다(창을 띄운 직후)
    #[default]
    Unknown,
    /// 마지막으로 올린 프레임이 창에 올랐다
    Visible,
    /// 마지막 시도에서 창이 가려졌다고 했다
    Hidden,
}

impl SurfaceVisibility {
    /// 층을 올리기 전에 그림판부터 얻어 봐야 하는가. 가려졌을 때 그림판 얻기는 기다리지 않고 바로 돌아오므로(`wgpu-hal` metal
    /// `surface.rs` 359~368줄 occlusionState) 21MB 층을 올리고 나서 건너뛰는 일을 막는다
    pub fn probe_before_upload(self) -> bool {
        self != Self::Visible
    }

    /// 한 번 올려 본 결과를 반영한다. `presented`는 올렸는가, `occluded`는 가려졌다는 답을 받았는가
    pub fn after(self, presented: bool, occluded: bool) -> Self {
        if presented {
            Self::Visible
        } else if occluded {
            Self::Hidden
        } else {
            self
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ms(n: u64) -> Duration {
        Duration::from_millis(n)
    }

    #[test]
    fn surface_latency_is_one_more_than_the_queue_depth_but_at_most_two() {
        assert_eq!(surface_latency(1), 2);
        assert_eq!(surface_latency(2), 2);
        assert_eq!(surface_latency(3), 2);
    }

    #[test]
    fn interval_follows_the_reported_refresh_rate() {
        assert_eq!(interval_from_millihertz(Some(120_000)), Duration::from_nanos(8_333_333));
        assert_eq!(interval_from_millihertz(Some(60_000)), Duration::from_nanos(16_666_666));
    }

    #[test]
    fn interval_falls_back_to_sixty_hertz_for_missing_or_absurd_rates() {
        for m in [None, Some(0), Some(1), Some(29_999), Some(360_001), Some(u32::MAX)] {
            assert_eq!(interval_from_millihertz(m), DEFAULT_REFRESH, "{m:?}");
        }
    }

    #[test]
    fn frame_latency_accepts_one_to_three_and_defaults_otherwise() {
        for (v, want) in [("1", 1), ("2", 2), ("3", 3), (" 1\n", 1)] {
            assert_eq!(frame_latency_from_env(Some(v)), want, "{v:?}");
        }
        for v in [
            None,
            Some(""),
            Some("0"),
            Some("4"),
            Some("-1"),
            Some("abc"),
            Some("1.5"),
            Some("99999999999"),
        ] {
            assert_eq!(frame_latency_from_env(v), DEFAULT_FRAME_LATENCY, "{v:?}");
        }
    }

    #[test]
    fn frame_latency_defaults_to_one() {
        assert_eq!(DEFAULT_FRAME_LATENCY, 1);
        assert_eq!(frame_latency_from_env(None), 1);
        assert_eq!(frame_latency_from_env(Some("2")), 2, "예전 값은 환경 변수로 고른다");
    }

    #[test]
    fn depth_one_defers_a_second_present_within_one_interval() {
        let t0 = Instant::now();
        let mut p = FramePacer::new(1, ms(16));
        assert_eq!(p.check(t0), PaceDecision::Go);
        p.record_present(t0);
        assert_eq!(p.check(t0 + ms(1)), PaceDecision::Wait(ms(15)));
        assert_eq!(p.check(t0 + ms(16)), PaceDecision::Go);
        // 한 주기에 한 장씩 올리면 기다리지 않는다
        let mut q = FramePacer::new(1, ms(16));
        for i in 0..50 {
            let now = t0 + ms(16 * i);
            assert_eq!(q.check(now), PaceDecision::Go, "{i}번째");
            q.record_present(now);
        }
    }

    #[test]
    fn depth_one_learns_a_slow_drain_and_keeps_one_entry() {
        let t0 = Instant::now();
        let mut p = FramePacer::new(1, ms(8));
        p.record_present_waited(t0, Duration::ZERO);
        p.record_present_waited(t0 + ms(16), ms(8));
        assert_eq!(
            p.check(t0 + ms(17)),
            PaceDecision::Wait(ms(15)),
            "마지막 장 + 배운 16ms"
        );
        assert_eq!(p.presents.len(), 1);
    }

    #[test]
    fn depth_zero_is_treated_as_one() {
        let t0 = Instant::now();
        let mut p = FramePacer::new(0, ms(16));
        p.record_present(t0);
        assert_eq!(p.check(t0), PaceDecision::Wait(ms(16)));
    }

    #[test]
    fn first_frames_go_without_waiting() {
        let t0 = Instant::now();
        let mut p = FramePacer::new(2, ms(16));
        assert_eq!(p.check(t0), PaceDecision::Go);
        p.record_present(t0);
        assert_eq!(p.check(t0), PaceDecision::Go, "줄이 한 장이면 한 장 더 올려도 된다");
    }

    #[test]
    fn a_full_queue_defers_until_the_oldest_frame_has_left() {
        let t0 = Instant::now();
        let mut p = FramePacer::new(2, ms(16));
        p.record_present(t0);
        p.record_present(t0 + ms(1));
        // 두 장이 줄 서 있다: 가장 오래된 것이 나가는 때(t0 + 2주기)까지
        assert_eq!(p.check(t0 + ms(2)), PaceDecision::Wait(ms(30)));
        assert_eq!(p.check(t0 + ms(31)), PaceDecision::Wait(ms(1)));
        assert_eq!(p.check(t0 + ms(32)), PaceDecision::Go);
    }

    #[test]
    fn waiting_the_returned_time_always_ends_in_go() {
        let t0 = Instant::now();
        for depth in 1..=3 {
            let mut p = FramePacer::new(depth, ms(8));
            for i in 0..depth {
                p.record_present(t0 + ms(i as u64));
            }
            let now = t0 + ms(depth as u64);
            let PaceDecision::Wait(d) = p.check(now) else {
                panic!("깊이 {depth}: 줄이 찼는데 기다리라고 하지 않았다");
            };
            assert_eq!(p.check(now + d), PaceDecision::Go, "깊이 {depth}");
        }
    }

    #[test]
    fn old_presents_stop_counting() {
        let t0 = Instant::now();
        let mut p = FramePacer::new(2, ms(16));
        p.record_present(t0);
        p.record_present(t0 + ms(1));
        assert_eq!(p.check(t0 + ms(100)), PaceDecision::Go);
    }

    #[test]
    fn steady_one_frame_per_refresh_never_waits() {
        let t0 = Instant::now();
        let mut p = FramePacer::new(2, ms(16));
        for i in 0..50 {
            let now = t0 + ms(16 * i);
            assert_eq!(p.check(now), PaceDecision::Go, "{i}번째");
            p.record_present(now);
        }
    }

    #[test]
    fn a_faster_display_shortens_the_wait() {
        let t0 = Instant::now();
        let mut slow = FramePacer::new(2, ms(16));
        let mut fast = FramePacer::new(2, ms(8));
        for p in [&mut slow, &mut fast] {
            p.record_present(t0);
            p.record_present(t0);
        }
        assert_eq!(slow.check(t0), PaceDecision::Wait(ms(32)));
        assert_eq!(fast.check(t0), PaceDecision::Wait(ms(16)));
        fast.set_interval(ms(4));
        assert_eq!(fast.check(t0), PaceDecision::Wait(ms(8)));
    }

    #[test]
    fn unknown_and_hidden_surfaces_probe_before_uploading_layers() {
        assert!(SurfaceVisibility::Unknown.probe_before_upload());
        assert!(SurfaceVisibility::Hidden.probe_before_upload());
        assert!(!SurfaceVisibility::Visible.probe_before_upload());
    }

    #[test]
    fn visibility_follows_the_last_attempt() {
        use SurfaceVisibility::*;
        assert_eq!(Unknown.after(true, false), Visible);
        assert_eq!(Unknown.after(false, true), Hidden);
        assert_eq!(Visible.after(false, true), Hidden);
        assert_eq!(Hidden.after(true, false), Visible, "가려졌다가 보이면 올라간다");
        // 가려진 것도 올린 것도 아닌 실패(Outdated 등)는 아는 것을 바꾸지 못한다
        assert_eq!(Visible.after(false, false), Visible);
        assert_eq!(Hidden.after(false, false), Hidden);
        assert_eq!(Unknown.after(false, false), Unknown);
    }

    #[test]
    fn blocked_acquires_teach_the_real_drain_interval() {
        // 120Hz라고 들었지만 화면이 16ms마다 한 장씩 가져간다: 얻기에서 기다린 프레임의 간격을 배운다
        let t0 = Instant::now();
        let mut p = FramePacer::new(2, ms(8));
        p.record_present_waited(t0, Duration::ZERO);
        p.record_present_waited(t0 + ms(16), ms(8));
        assert_eq!(
            p.check(t0 + ms(17)),
            PaceDecision::Wait(ms(15)),
            "가장 오래된 장 + 16ms × 2"
        );
        // 기다림이 짧으면(1ms 아래) 배우지 않는다
        let mut q = FramePacer::new(2, ms(8));
        q.record_present_waited(t0, Duration::ZERO);
        q.record_present_waited(t0 + ms(16), Duration::from_micros(500));
        assert_eq!(q.check(t0 + ms(17)), PaceDecision::Go);
    }

    #[test]
    fn frames_that_do_not_wait_slowly_return_to_the_reported_interval() {
        let t0 = Instant::now();
        let mut p = FramePacer::new(2, ms(8));
        p.record_present_waited(t0, Duration::ZERO);
        p.record_present_waited(t0 + ms(16), ms(8));
        // 기다리지 않은 프레임이 이어지면 배운 간격이 조금씩 줄어 들은 값으로 돌아온다
        let mut now = t0 + ms(16);
        for _ in 0..200 {
            now += ms(16);
            p.record_present_waited(now, Duration::ZERO);
        }
        assert_eq!(p.check(now + ms(1)), PaceDecision::Go, "8ms × 2 = 16ms가 지났다");
        // 같은 주기를 다시 알려도(앱은 프레임마다 알린다) 배운 값은 남는다
        p.record_present_waited(now + ms(32), ms(8));
        p.set_interval(ms(8));
        assert_eq!(p.check(now + ms(33)), PaceDecision::Wait(ms(31)), "32ms를 배웠다");
        // 다른 화면으로 옮겨 주기가 바뀌면 배운 값을 버린다
        p.set_interval(ms(16));
        assert_eq!(p.check(now + ms(33)), PaceDecision::Go, "16ms × 2 = 32ms가 지났다");
    }

    #[test]
    fn interval_reports_the_period_the_screen_announced_not_the_learned_one() {
        // 로그 "화면 한 장 간격"은 들은 값을 찍는다(#429): 배운 간격이 있어도 바뀌지 않는다
        let now = Instant::now();
        let mut p = FramePacer::new(1, ms(8));
        p.record_present(now);
        p.record_present_waited(now + ms(16), ms(8));
        assert_eq!(p.interval(), ms(8));
        p.set_interval(ms(16));
        assert_eq!(p.interval(), ms(16));
    }
}
