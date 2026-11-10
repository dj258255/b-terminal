//! 창 길의 프레임 간격(ADR-038): 그림판을 얻느라 메인 스레드가 막히지 않게 올릴 때를 정하고,
//! 창이 보이는지 아직 모를 때 층을 올리기 전에 먼저 알아본다. 둘 다 시계·창 없이 시험하는 순수한 계산이다

use std::collections::VecDeque;
use std::time::{Duration, Instant};

/// 화면 주사율을 모를 때 쓰는 값(60Hz)
pub const DEFAULT_REFRESH: Duration = Duration::from_nanos(16_666_667);

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

/// 올린 프레임이 아직 화면에 오르지 않고 줄 서 있다고 보고 세어, 줄이 가득 차면 다음 프레임을 미룬다.
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
}

impl FramePacer {
    /// `depth`는 `desired_maximum_frame_latency`와 같은 값
    pub fn new(depth: usize, interval: Duration) -> Self {
        Self {
            interval,
            depth: depth.max(1),
            presents: VecDeque::new(),
        }
    }

    /// 화면 주사율이 바뀌었다(다른 화면으로 옮겨 갔을 때)
    pub fn set_interval(&mut self, interval: Duration) {
        self.interval = interval;
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
        let leaves = *oldest + self.interval * self.depth as u32;
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
}
