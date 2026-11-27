//! 탭 줄의 자리 계산(#909, kitty `tab_bar_edge`·`tab_bar_margin_width`·`tab_bar_margin_height`).
//! 창 높이 → (탭 줄 사각형, 칸 격자가 들어갈 세로 범위)를 한 곳에서 정한다. 그리는 쪽·누르기·끌기·가운데 클릭·오른쪽 단추 메뉴·
//! 진행 막대·안 읽음 점·창 크기(칸 수 → 픽셀)가 모두 이 계산을 쓰므로 그린 자리와 누르는 자리가 어긋나지 않는다.
//! kitty `os_window_regions`(`kitty/state.c` 825~886줄)를 옮겼다: 위쪽이면 `central.top = 높이 + 안 + 밖`·`tab_bar.top = 밖`,
//! 아래쪽이면 `central.bottom = 창 - (높이 + 안 + 밖)`·`tab_bar.top = central.bottom + 안`.
//! 여백 값이 모두 0이고 위쪽이면 탭 줄은 창 위(제목 막대 비움 바로 밑)에서 높이만큼이라 지금과 같다

use bt_core::tabstyle::{TabBarEdge, TabBarLook};

/// pt를 픽셀로(반올림, 음수 없음)
pub fn pt_px(pt: f32, scale: f64) -> u32 {
    (pt as f64 * scale).round().max(0.0) as u32
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BarGeom {
    /// 제목 막대 밑으로 콘텐츠가 들어가는 창에서 위쪽 비움(px, #192)
    pub inset: u32,
    /// 탭 줄 높이(px). 탭 줄이 없으면 0이고 그러면 여백도 없다
    pub bar_h: u32,
    pub bottom: bool,
    /// 탭 줄 좌우 여백(px)
    pub margin_x: u32,
    /// 창 가장자리와 탭 줄 사이(px)
    pub outer: u32,
    /// 탭 줄과 칸 사이(px)
    pub inner: u32,
}

impl BarGeom {
    pub fn new(look: &TabBarLook, scale: f64, inset: u32, bar_h: u32) -> Self {
        Self {
            inset,
            bar_h,
            bottom: look.edge == TabBarEdge::Bottom,
            margin_x: pt_px(look.margin_width, scale),
            outer: pt_px(look.margin_height.0, scale),
            inner: pt_px(look.margin_height.1, scale),
        }
    }

    /// 탭 줄이 차지하는 세로 띠의 높이(여백 포함). 탭 줄이 없으면 0
    pub fn band(&self) -> u32 {
        if self.bar_h == 0 {
            0
        } else {
            self.outer + self.bar_h + self.inner
        }
    }

    /// 칸이 들어가는 세로 범위 (맨 위 y, 높이). 위쪽 탭 줄이면 띠 밑에서, 아래쪽이면 띠 위까지
    pub fn content_y(&self, win_h: u32) -> (u32, u32) {
        let top = self.inset + if self.bottom { 0 } else { self.band() };
        let bottom = win_h.saturating_sub(if self.bottom { self.band() } else { 0 });
        (top.min(win_h), bottom.saturating_sub(top))
    }

    /// 칸 위로 오는 것(배지 등)이 비켜야 할 위쪽 높이
    pub fn content_top(&self) -> u32 {
        self.inset + if self.bottom { 0 } else { self.band() }
    }

    /// 탭 줄 맨 위 y(창 맨 위에서)
    pub fn bar_top(&self, win_h: u32) -> i32 {
        if self.bottom {
            win_h.saturating_sub(self.outer + self.bar_h) as i32
        } else {
            (self.inset + self.outer) as i32
        }
    }

    /// 탭 줄 사각형(여백을 뺀, 탭 줄 바탕이 칠해지는 곳)의 세로 범위 [위, 아래)
    pub fn bar_y(&self, win_h: u32) -> (i32, i32) {
        let t = self.bar_top(win_h);
        (t, t + self.bar_h as i32)
    }

    /// y가 탭 줄 사각형 안인가. 탭 줄이 없으면 거짓
    pub fn in_bar(&self, y: f64, win_h: u32) -> bool {
        let (t, b) = self.bar_y(win_h);
        self.bar_h > 0 && y >= t as f64 && y < b as f64
    }

    /// y가 탭 줄과 그 여백을 합친 띠 안인가(칸이 아닌 곳). 탭 줄이 없으면 거짓
    pub fn in_band(&self, y: f64, win_h: u32) -> bool {
        if self.band() == 0 {
            return false;
        }
        let (top, h) = if self.bottom {
            (win_h.saturating_sub(self.band()), self.band())
        } else {
            (self.inset, self.band())
        };
        y >= top as f64 && y < (top + h) as f64
    }

    /// 탭이 놓이는 가로 범위 (시작 x, 폭). 좌우 여백을 뺀다(여백이 창보다 크면 폭 1)
    pub fn tabs_x(&self, win_w: u32) -> (i32, u32) {
        let m = self.margin_x.min(win_w / 2);
        (m as i32, win_w.saturating_sub(2 * m).max(1))
    }

    /// 창 가로 좌표를 탭 줄 안쪽 좌표로(여백만큼 왼쪽으로)
    pub fn tab_x(&self, x: f64, win_w: u32) -> f64 {
        x - self.tabs_x(win_w).0 as f64
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn look(edge: TabBarEdge, mw: f32, mh: (f32, f32)) -> TabBarLook {
        TabBarLook {
            edge,
            margin_width: mw,
            margin_height: mh,
            ..TabBarLook::default()
        }
    }

    #[test]
    fn defaults_are_the_old_top_bar() {
        let g = BarGeom::new(&TabBarLook::default(), 2.0, 56, 51);
        assert_eq!(g.band(), 51);
        assert_eq!(g.content_y(800), (56 + 51, 800 - 56 - 51));
        assert_eq!(g.bar_top(800), 56);
        assert_eq!(g.tabs_x(1000), (0, 1000));
        // 지금 코드: 탭 줄 안은 y < inset + bar_h
        for y in [0.0, 55.9, 56.0, 106.9, 107.0, 799.0] {
            assert_eq!(g.in_band(y, 800), (56.0..107.0).contains(&y), "{y}");
            assert_eq!(g.in_bar(y, 800), g.in_band(y, 800), "{y}");
        }
    }

    #[test]
    fn no_bar_means_no_band_or_margin() {
        let g = BarGeom::new(&look(TabBarEdge::Bottom, 5.0, (3.0, 4.0)), 2.0, 0, 0);
        assert_eq!(g.band(), 0);
        assert_eq!(g.content_y(600), (0, 600));
        assert!(!g.in_band(599.0, 600) && !g.in_bar(599.0, 600));
    }

    #[test]
    fn bottom_edge_puts_the_bar_under_the_cells_like_kitty() {
        // kitty: central.bottom = 창 - (높이 + 안 + 밖), tab_bar.top = central.bottom + 안
        let g = BarGeom::new(&look(TabBarEdge::Bottom, 0.0, (3.0, 4.0)), 2.0, 56, 50);
        assert_eq!((g.outer, g.inner, g.band()), (6, 8, 64));
        assert_eq!(g.content_y(800), (56, 800 - 64 - 56));
        assert_eq!(g.bar_top(800), 800 - 6 - 50);
        assert_eq!(g.bar_y(800), (744, 794));
        assert_eq!(g.content_y(800).0 + g.content_y(800).1, 800 - 64);
        assert_eq!(g.bar_top(800), 736 + 8, "탭 줄 위 = 칸 아래 + 안 여백");
        assert!(g.in_bar(744.0, 800) && g.in_bar(793.9, 800) && !g.in_bar(794.0, 800) && !g.in_bar(743.9, 800));
        assert!(g.in_band(736.0, 800) && g.in_band(799.9, 800) && !g.in_band(735.9, 800));
        assert_eq!(g.content_top(), 56, "아래쪽이면 배지는 제목 막대 바로 밑");
    }

    #[test]
    fn top_edge_margins_follow_kitty() {
        // kitty: tab_bar.top = 밖, central.top = 높이 + 안 + 밖
        let g = BarGeom::new(&look(TabBarEdge::Top, 0.0, (3.0, 4.0)), 1.0, 0, 20);
        assert_eq!(g.bar_top(500), 3);
        assert_eq!(g.content_y(500), (27, 473));
        assert!(!g.in_bar(2.9, 500) && g.in_bar(3.0, 500) && g.in_bar(22.9, 500) && !g.in_bar(23.0, 500));
        assert!(g.in_band(0.0, 500) && g.in_band(26.9, 500) && !g.in_band(27.0, 500));
    }

    #[test]
    fn side_margin_shrinks_the_tab_range_and_shifts_hits() {
        let g = BarGeom::new(&look(TabBarEdge::Top, 5.0, (0.0, 0.0)), 2.0, 0, 20);
        assert_eq!(g.tabs_x(400), (10, 380));
        assert_eq!(g.tab_x(10.0, 400), 0.0);
        // 여백이 창 폭의 절반보다 크면 줄여 폭이 0이 되지 않는다
        assert_eq!(
            BarGeom::new(&look(TabBarEdge::Top, 500.0, (0.0, 0.0)), 1.0, 0, 20).tabs_x(100),
            (50, 1)
        );
    }

    #[test]
    fn cells_and_bar_tile_the_window_without_overlap() {
        for bottom in [false, true] {
            let edge = if bottom { TabBarEdge::Bottom } else { TabBarEdge::Top };
            let g = BarGeom::new(&look(edge, 0.0, (2.0, 3.0)), 1.0, 28, 30);
            let (top, h) = g.content_y(700);
            let (bt, bb) = g.bar_y(700);
            let cells_end = (top + h) as i32;
            assert!(bb <= top as i32 || bt >= cells_end, "{bottom}: 탭 줄과 칸이 겹침");
            assert_eq!(h + g.band() + g.inset, 700, "{bottom}: 제목 막대 비움 + 칸 + 띠 = 창");
        }
    }
}
