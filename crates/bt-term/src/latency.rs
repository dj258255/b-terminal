//! 키 입력 → 화면 지연을 재는 표(#417). 칸마다 하나씩 쥐고, PTY에 사람 입력을 쓴 때("입력 시각")와
//! 그 뒤 처음 PTY 출력이 온 때를 적어 두었다가, 그 출력이 든 프레임이 실제로 올라가면 걸린 시간을 돌려준다.
//!
//! 시계를 부르지 않는다. 때는 모두 인자로 받아서 창 없이 시험한다. 잴 값이 아닌 것(메아리 없는 입력, 1초 넘은
//! 시각)은 버린다.

use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

/// 이보다 오래된 입력 시각은 버린다. 사람이 느끼는 지연이 아니라 프로그램이 한참 뒤에 찍은 출력(오래 걸리는 명령의
/// 결과, 메아리 없는 입력 뒤의 엉뚱한 출력)을 입력 지연으로 잘못 재지 않기 위한 선이다
pub const MAX_AGE: Duration = Duration::from_secs(1);

static ENABLED: AtomicBool = AtomicBool::new(false);

/// 재기를 켠다(앱이 debug 로그가 켜졌을 때만). 꺼져 있으면 칸은 시각을 적지 않는다
pub fn set_enabled(on: bool) {
    ENABLED.store(on, Ordering::Relaxed);
}

pub fn enabled() -> bool {
    ENABLED.load(Ordering::Relaxed)
}

/// 프레임을 시작할 때 떠 둔 값(그 프레임에 든 출력이 있는 칸만)
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Sample {
    input: Instant,
    first_output: Instant,
    frame_start: Instant,
}

/// 올라간 프레임에서 잰 값
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Report {
    /// 입력 시각부터 화면에 올라간 때까지(ms)
    pub total_ms: f64,
    /// 입력 시각부터 PTY 출력이 처음 온 때까지(ms): 프로그램 몫
    pub to_output_ms: f64,
    /// 입력 시각부터 그 출력을 담은 프레임의 화면을 뜬 때까지(ms)
    pub to_frame_ms: f64,
}

impl Report {
    /// 고정된 머리말(`입력 → 화면:`)로 시작하는 로그 줄. 주 세션이 grep으로 모은다
    pub fn line(&self, pane: u64, path: &str) -> String {
        format!(
            "입력 → 화면: {:.1}ms (칸 {pane}, 출력 받음까지 {:.1}ms, 그리기 시작까지 {:.1}ms, {path})",
            self.total_ms, self.to_output_ms, self.to_frame_ms
        )
    }
}

#[derive(Debug, Default)]
pub struct LatencyTracker {
    input: Option<Instant>,
    first_output: Option<Instant>,
}

impl LatencyTracker {
    /// 사람 입력을 PTY에 쓴 때를 적는다. 이미 적혀 있으면 덮지 않는다(가장 이른 것). 다만 `MAX_AGE`를 넘은 것은 낡았으니 새로 적는다
    pub fn note_input(&mut self, now: Instant) {
        let fresh = self.input.is_some_and(|t| now.saturating_duration_since(t) <= MAX_AGE);
        if !fresh {
            self.input = Some(now);
            self.first_output = None;
        }
    }

    /// PTY 출력이 왔다. 입력이 적혀 있고 아직 첫 출력이 없을 때만 적는다
    pub fn note_output(&mut self, now: Instant) {
        if let (Some(input), None) = (self.input, self.first_output) {
            if now >= input && now - input <= MAX_AGE {
                self.first_output = Some(now);
            }
        }
    }

    /// 프레임을 시작한다(칸의 화면을 뜨기 전의 때). 첫 출력이 이 때 이전이면 값을 뜬다. 낡은 입력 시각은 여기서 버린다
    pub fn begin_frame(&mut self, now: Instant) -> Option<Sample> {
        let input = self.input?;
        if now.saturating_duration_since(input) > MAX_AGE {
            self.input = None;
            self.first_output = None;
            return None;
        }
        let first_output = self.first_output.filter(|o| *o <= now)?;
        Some(Sample {
            input,
            first_output,
            frame_start: now,
        })
    }

    /// 그 프레임이 올라갔다. 잰 값을 돌려주고 시각을 지운다. 그 사이 시각이 바뀌었거나 너무 오래 걸렸으면 None
    pub fn presented(&mut self, sample: &Sample, now: Instant) -> Option<Report> {
        if self.input != Some(sample.input) {
            return None;
        }
        self.input = None;
        self.first_output = None;
        let total = now.saturating_duration_since(sample.input);
        if total > MAX_AGE {
            return None;
        }
        let ms = |a: Instant, b: Instant| b.saturating_duration_since(a).as_secs_f64() * 1000.0;
        Some(Report {
            total_ms: total.as_secs_f64() * 1000.0,
            to_output_ms: ms(sample.input, sample.first_output),
            to_frame_ms: ms(sample.input, sample.frame_start),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn at(base: Instant, ms: u64) -> Instant {
        base + Duration::from_millis(ms)
    }

    #[test]
    fn measures_input_to_presented() {
        let t0 = Instant::now();
        let mut t = LatencyTracker::default();
        t.note_input(at(t0, 0));
        t.note_output(at(t0, 4));
        let s = t.begin_frame(at(t0, 6)).expect("출력이 든 프레임");
        let r = t.presented(&s, at(t0, 12)).expect("올라감");
        assert_eq!(r.total_ms, 12.0);
        assert_eq!(r.to_output_ms, 4.0);
        assert_eq!(r.to_frame_ms, 6.0);
        // 시각을 지웠다: 같은 입력을 두 번 재지 않는다
        assert!(t.begin_frame(at(t0, 20)).is_none());
    }

    #[test]
    fn keeps_earliest_input() {
        let t0 = Instant::now();
        let mut t = LatencyTracker::default();
        t.note_input(at(t0, 0));
        t.note_input(at(t0, 30));
        t.note_output(at(t0, 40));
        let s = t.begin_frame(at(t0, 41)).unwrap();
        assert_eq!(t.presented(&s, at(t0, 50)).unwrap().total_ms, 50.0);
    }

    #[test]
    fn no_output_means_no_report_and_next_input_restarts() {
        let t0 = Instant::now();
        let mut t = LatencyTracker::default();
        t.note_input(at(t0, 0));
        // 메아리가 없다: 프레임이 떠도 값이 없다
        assert!(t.begin_frame(at(t0, 10)).is_none());
        // 1초 넘게 지난 뒤 다음 입력은 새로 적는다
        t.note_input(at(t0, 1500));
        t.note_output(at(t0, 1503));
        let s = t.begin_frame(at(t0, 1504)).unwrap();
        assert_eq!(t.presented(&s, at(t0, 1510)).unwrap().total_ms, 10.0);
    }

    #[test]
    fn output_without_input_is_ignored() {
        let t0 = Instant::now();
        let mut t = LatencyTracker::default();
        t.note_output(at(t0, 5));
        assert!(t.begin_frame(at(t0, 6)).is_none());
        // 그 출력은 다음 입력의 첫 출력이 되지 않는다
        t.note_input(at(t0, 10));
        assert!(t.begin_frame(at(t0, 11)).is_none());
    }

    #[test]
    fn only_first_output_counts() {
        let t0 = Instant::now();
        let mut t = LatencyTracker::default();
        t.note_input(at(t0, 0));
        t.note_output(at(t0, 3));
        t.note_output(at(t0, 8));
        let s = t.begin_frame(at(t0, 9)).unwrap();
        assert_eq!(t.presented(&s, at(t0, 11)).unwrap().to_output_ms, 3.0);
    }

    #[test]
    fn output_after_frame_start_is_not_in_that_frame() {
        let t0 = Instant::now();
        let mut t = LatencyTracker::default();
        t.note_input(at(t0, 0));
        // 화면을 뜬 뒤에 온 출력은 이 프레임에 없다
        assert!(t.begin_frame(at(t0, 2)).is_none());
        t.note_output(at(t0, 3));
        let s = t.begin_frame(at(t0, 4)).unwrap();
        assert_eq!(t.presented(&s, at(t0, 9)).unwrap().total_ms, 9.0);
    }

    #[test]
    fn stale_input_is_dropped() {
        let t0 = Instant::now();
        let mut t = LatencyTracker::default();
        t.note_input(at(t0, 0));
        t.note_output(at(t0, 5));
        // 1초 넘게 지난 뒤의 프레임: 찍지 않고 시각도 버린다
        assert!(t.begin_frame(at(t0, 1200)).is_none());
        t.note_output(at(t0, 1201));
        assert!(t.begin_frame(at(t0, 1202)).is_none());
    }

    #[test]
    fn too_slow_present_is_dropped() {
        let t0 = Instant::now();
        let mut t = LatencyTracker::default();
        t.note_input(at(t0, 0));
        t.note_output(at(t0, 5));
        let s = t.begin_frame(at(t0, 6)).unwrap();
        assert!(t.presented(&s, at(t0, 1500)).is_none());
    }

    #[test]
    fn skipped_frame_keeps_the_stamp_for_the_next_one() {
        let t0 = Instant::now();
        let mut t = LatencyTracker::default();
        t.note_input(at(t0, 0));
        t.note_output(at(t0, 2));
        // 첫 프레임은 올리지 못했다(presented를 부르지 않는다). 다음 프레임에서 잰다
        let _skipped = t.begin_frame(at(t0, 3)).unwrap();
        let s = t.begin_frame(at(t0, 20)).unwrap();
        let r = t.presented(&s, at(t0, 25)).unwrap();
        assert_eq!(r.total_ms, 25.0);
    }

    #[test]
    fn presented_twice_reports_once() {
        let t0 = Instant::now();
        let mut t = LatencyTracker::default();
        t.note_input(at(t0, 0));
        t.note_output(at(t0, 2));
        let s = t.begin_frame(at(t0, 3)).unwrap();
        assert!(t.presented(&s, at(t0, 5)).is_some());
        // 이미 지웠으니 같은 값으로 또 부르면 없다
        assert!(t.presented(&s, at(t0, 6)).is_none());
    }

    #[test]
    fn log_line_has_fixed_prefix() {
        let r = Report {
            total_ms: 12.34,
            to_output_ms: 3.0,
            to_frame_ms: 4.5,
        };
        assert_eq!(
            r.line(7, "GPU"),
            "입력 → 화면: 12.3ms (칸 7, 출력 받음까지 3.0ms, 그리기 시작까지 4.5ms, GPU)"
        );
    }
}
