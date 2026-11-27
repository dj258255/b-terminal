//! 픽셀 스크롤(kitty `pixel_scroll`, #902)의 위치 계산. 스크롤백을 줄보다 작은 단위로 올려 본다.
//!
//! 위치는 (줄, 걸친 픽셀) 한 쌍이다. 줄은 VT 엔진의 `display_offset`이고, 걸친 픽셀 `sub`은 화면을
//! 줄 높이 안에서 위로 얼마나 밀어 그리는지다(0 이상 줄 높이 미만). 맨 아래에서 올라간 거리(픽셀)는
//! `display_offset * 줄 높이 - sub`이다. 그래서 `sub > 0`이면 화면 맨 아래에 다음 줄이 `sub` 픽셀만큼 걸쳐 보인다.
//! kitty `scroll_by_pixels`(screen.c 6515~6538줄)는 `scrolled_by`(내림)와 `pixel_scroll_offset_y`(남은 픽셀)로 같은 위치를
//! 적는다. 이쪽은 올림 쪽에 줄을 두어 걸친 줄이 화면 아래에 오고, 그 줄은 이미 있는 줄(`display_offset - 1` 화면의 맨 아래 줄)이다.
//!
//! 걸친 픽셀은 "그 위치에 머물러 있는 동안만" 맞다. 줄 단위 스크롤·맨 아래로 돌아가기·크기 바뀜은 `reset`하고,
//! 그 밖의 길(프롬프트 이동, 찾기 이동)로 위치가 옮겨지면 기준 줄이 달라져 걸친 픽셀을 쓰지 않는다. 새 출력이 와서
//! 스크롤백이 한 줄 늘면 VT 엔진이 `display_offset`을 같이 올려 보던 자리를 지키므로 기준 줄은 그대로고 걸친 픽셀도 남는다(kitty 2734줄과 같다).

/// 걸친 픽셀 상태. `top`은 걸친 픽셀을 정한 때의 화면 맨 위 줄(스크롤백 맨 위가 0인 절대 줄)이다
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct PixelScroll {
    sub: f64,
    top: i64,
}

/// 한 걸음의 결과: 새 `display_offset`과 걸친 픽셀
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Step {
    pub offset: usize,
    pub sub: f64,
}

impl PixelScroll {
    pub fn reset(&mut self) {
        *self = Self::default();
    }

    /// 지금 위치에서 쓸 수 있는 걸친 픽셀(줄 높이 `ch` 미만). 맨 아래이거나 기준 줄이 달라졌으면 0
    pub fn sub(&self, ch: u32, history: usize, offset: usize) -> f64 {
        if offset == 0 || ch == 0 || self.sub <= 0.0 || self.top != history as i64 - offset as i64 {
            return 0.0;
        }
        self.sub.min(ch as f64)
    }

    /// 그리는 쪽이 쓰는 정수 픽셀. `sub`를 반올림하고 줄 높이 안으로 자른다
    pub fn shift(&self, ch: u32, history: usize, offset: usize) -> u32 {
        let s = self.sub(ch, history, offset).round() as u32;
        s.min(ch.saturating_sub(1))
    }

    /// 위치를 정한다(걸친 픽셀 `sub`, 그때의 `history`·`offset`)
    pub fn set(&mut self, sub: f64, history: usize, offset: usize) {
        self.sub = if offset == 0 { 0.0 } else { sub.max(0.0) };
        self.top = history as i64 - offset as i64;
    }

    /// 위로(스크롤백 쪽, 양수) `delta` 픽셀 움직인 새 위치. 맨 아래(0)와 스크롤백 끝 사이로 자른다.
    /// 움직일 수 없으면(스크롤백이 없거나 `ch`가 0, 값이 유한하지 않음) `None`
    pub fn plan(&self, delta: f64, ch: u32, history: usize, offset: usize) -> Option<Step> {
        if ch == 0 || history == 0 || !delta.is_finite() {
            return None;
        }
        let chf = ch as f64;
        let total = offset as f64 * chf - self.sub(ch, history, offset);
        let max = history as f64 * chf;
        let new = (total + delta).clamp(0.0, max);
        // 올림: 걸친 줄이 화면 아래에 온다. 줄 경계 위에서는 걸친 픽셀이 0이다
        let off = ((new / chf).ceil() as usize).min(history);
        let sub = (off as f64 * chf - new).clamp(0.0, chf);
        Some(Step {
            offset: off,
            sub: if off == 0 { 0.0 } else { sub },
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run(p: &mut PixelScroll, delta: f64, ch: u32, history: usize, offset: usize) -> (usize, u32) {
        let s = p.plan(delta, ch, history, offset).expect("움직일 수 있다");
        p.set(s.sub, history, s.offset);
        (s.offset, p.shift(ch, history, s.offset))
    }

    /// 줄보다 작은 움직임은 줄과 걸친 픽셀로 나뉜다. 맨 아래에서 5픽셀 올리면 한 줄 위(`display_offset` 1)에 줄 높이-5픽셀이 걸친다
    #[test]
    fn a_small_move_up_is_a_line_and_a_shift() {
        let mut p = PixelScroll::default();
        assert_eq!(run(&mut p, 5.0, 16, 100, 0), (1, 11));
        // 11 + 5 = 16: 한 줄을 채우면 줄 경계에 선다
        assert_eq!(run(&mut p, 11.0, 16, 100, 1), (1, 0));
        assert_eq!(run(&mut p, 1.0, 16, 100, 1), (2, 15));
    }

    /// 거꾸로(아래로) 같은 거리만큼 되돌리면 같은 자리로 온다
    #[test]
    fn moving_down_undoes_moving_up() {
        let mut p = PixelScroll::default();
        let mut off = 0;
        for d in [7.0, 9.0, 21.0] {
            off = run(&mut p, d, 16, 100, off).0;
            assert!(off > 0);
        }
        // 7 + 9 + 21 = 37픽셀
        assert_eq!(run(&mut p, -37.0, 16, 100, off), (0, 0));
    }

    /// 맨 아래 밑으로, 스크롤백 끝 위로는 가지 않는다
    #[test]
    fn it_stops_at_both_ends() {
        let mut p = PixelScroll::default();
        assert_eq!(run(&mut p, -50.0, 16, 100, 0), (0, 0));
        assert_eq!(run(&mut p, 1_000_000.0, 16, 100, 0), (100, 0));
        assert_eq!(run(&mut p, 1.0, 16, 100, 100), (100, 0));
    }

    /// 스크롤백이 없거나 줄 높이를 모르면 움직이지 않는다
    #[test]
    fn nothing_to_scroll_is_none() {
        let p = PixelScroll::default();
        assert!(p.plan(5.0, 16, 0, 0).is_none());
        assert!(p.plan(5.0, 0, 10, 0).is_none());
        assert!(p.plan(f64::NAN, 16, 10, 0).is_none());
    }

    /// 줄 단위로 위치가 옮겨졌거나(기준 줄이 달라짐) 맨 아래이면 걸친 픽셀은 쓰지 않는다
    #[test]
    fn a_moved_anchor_or_the_bottom_drops_the_shift() {
        let mut p = PixelScroll::default();
        run(&mut p, 5.0, 16, 100, 0);
        assert_eq!(p.shift(16, 100, 1), 11);
        // 다른 길(프롬프트 이동 등)로 옮겨졌다
        assert_eq!(p.shift(16, 100, 7), 0);
        // 맨 아래
        assert_eq!(p.shift(16, 100, 0), 0);
        // 새 출력이 한 줄 보태져도 보던 자리를 지키면(history+1, offset+1) 남는다
        assert_eq!(p.shift(16, 101, 2), 11);
    }

    /// 글자 크기가 바뀌어 줄 높이가 줄면 줄 높이 안으로 자른다
    #[test]
    fn a_smaller_cell_clamps_the_shift() {
        let mut p = PixelScroll::default();
        run(&mut p, 5.0, 16, 100, 0);
        assert_eq!(p.shift(8, 100, 1), 7);
    }

    /// 작은 값을 여러 번 보태도 반올림 오차가 쌓이지 않는다(트랙패드는 0.5픽셀 같은 값도 보낸다)
    #[test]
    fn fractional_deltas_do_not_drift() {
        let mut p = PixelScroll::default();
        let mut offset = 0;
        for _ in 0..64 {
            let s = p.plan(0.5, 16, 100, offset).unwrap();
            p.set(s.sub, 100, s.offset);
            offset = s.offset;
        }
        // 32픽셀 = 두 줄: 줄 경계
        assert_eq!((offset, p.shift(16, 100, offset)), (2, 0));
    }
}
