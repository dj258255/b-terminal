//! 분할 경계선을 마우스로 끄는 상태(Ghostty `SplitView`의 드래그·두 번 누르기). 창 없이 시험하도록 시간과 누름 기록만 여기서 다룬다.
//! 맞히기·비율 계산은 `split.rs`에 있다.

use crate::split::Axis;
use std::time::{Duration, Instant};

/// 끄는 중인 경계선: 어느 탭의 어느 분할(`split::DividerInfo::path`)인가
#[derive(Debug, Clone, PartialEq)]
pub struct Drag {
    pub group: usize,
    pub path: Vec<bool>,
    pub axis: Axis,
}

/// 경계선을 끄는 중인가와 지난 누름(두 번 누르기를 가리는 데)
#[derive(Debug, Default)]
pub struct DividerState {
    drag: Option<Drag>,
    last: Option<(Instant, usize, Vec<bool>)>,
}

impl DividerState {
    pub fn drag(&self) -> Option<&Drag> {
        self.drag.as_ref()
    }

    pub fn is_dragging(&self) -> bool {
        self.drag.is_some()
    }

    /// 경계선을 눌렀다. 끌기를 시작한다. 같은 경계선을 `interval` 안에 다시 눌렀으면 `true`(칸 크기를 고르게).
    /// 두 번 누른 뒤의 세 번째 누름은 새로 센다
    pub fn press(&mut self, drag: Drag, now: Instant, interval: Duration) -> bool {
        let double = self.last.as_ref().is_some_and(|(t, g, p)| {
            *g == drag.group && *p == drag.path && now.saturating_duration_since(*t) <= interval
        });
        self.last = if double {
            None
        } else {
            Some((now, drag.group, drag.path.clone()))
        };
        self.drag = Some(drag);
        double
    }

    /// 왼쪽 단추를 놓았다 또는 끌기를 멈춘다
    pub fn end(&mut self) {
        self.drag = None;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn drag(path: &[bool]) -> Drag {
        Drag {
            group: 0,
            path: path.to_vec(),
            axis: Axis::Horizontal,
        }
    }

    #[test]
    fn a_second_press_on_the_same_divider_within_the_interval_is_a_double() {
        let mut s = DividerState::default();
        let t = Instant::now();
        let gap = Duration::from_millis(500);
        assert!(!s.press(drag(&[true]), t, gap));
        assert!(s.is_dragging());
        s.end();
        assert!(!s.is_dragging());
        assert!(s.press(drag(&[true]), t + Duration::from_millis(300), gap));
        s.end();
        // 두 번으로 센 뒤에는 새로 센다
        assert!(!s.press(drag(&[true]), t + Duration::from_millis(400), gap));
    }

    #[test]
    fn a_slow_press_or_another_divider_or_tab_is_not_a_double() {
        let mut s = DividerState::default();
        let t = Instant::now();
        let gap = Duration::from_millis(500);
        s.press(drag(&[true]), t, gap);
        assert!(!s.press(drag(&[true]), t + Duration::from_millis(501), gap));
        assert!(!s.press(drag(&[]), t + Duration::from_millis(600), gap));
        let mut other_tab = drag(&[]);
        other_tab.group = 1;
        assert!(!s.press(other_tab, t + Duration::from_millis(700), gap));
    }
}
