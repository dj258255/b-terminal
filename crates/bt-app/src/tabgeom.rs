//! 탭 줄의 자리 계산(#909, kitty `tab_bar_edge`·`tab_bar_margin_width`·`tab_bar_margin_height`).
//! 창 높이 → (탭 줄 사각형, 칸 격자가 들어갈 세로 범위)를 한 곳에서 정한다. 그리는 쪽·누르기·끌기·가운데 클릭·오른쪽 단추 메뉴·
//! 진행 막대·안 읽음 점·창 크기(칸 수 → 픽셀)가 모두 이 계산을 쓰므로 그린 자리와 누르는 자리가 어긋나지 않는다.
//! kitty `os_window_regions`(`kitty/state.c` 825~886줄)를 옮겼다: 위쪽이면 `central.top = 높이 + 안 + 밖`·`tab_bar.top = 밖`,
//! 아래쪽이면 `central.bottom = 창 - (높이 + 안 + 밖)`·`tab_bar.top = central.bottom + 안`.
//! 세로 탭 줄(`left`·`right`, #935)은 위의 계산을 가로로 돌린 것이다: `central.left = 폭 + 안 + 밖`(왼쪽) 또는 `central.right = 창 - (폭 + 안 + 밖)`(오른쪽).
//! 폭은 `vtabs`가 정하고(`with_vertical_width`·`sized`·`wanted`), 세로 줄이면 `band()`(세로 높이)는 0이고 `band_x()`가 가로 폭을 가져간다.
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
    /// 탭 줄을 둔 가장자리. 세로(`left`·`right`)면 `bar_h`는 탭 한 줄(행)의 높이다(#935)
    pub edge: TabBarEdge,
    /// 세로 탭 줄의 폭(px). 가로 줄이거나 아직 정하지 않았으면 0이고, 0이면 세로 줄도 자리를 차지하지 않는다
    pub vw: u32,
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
            edge: look.edge,
            vw: 0,
        }
    }

    /// 세로 탭 줄의 폭(px)을 정한다. 가로 줄에서는 무시한다
    pub fn with_vertical_width(mut self, vw: u32) -> Self {
        if self.edge.vertical() {
            self.vw = vw;
        }
        self
    }

    /// 창 폭 `win_w`와 탭 줄 글꼴의 칸 폭 `cell_w`로 세로 줄의 폭을 정한다(kitty `vertical_tab_bar_cols`).
    /// `max_length`는 `tab-title-max-length`(0이면 28칸)
    pub fn sized(self, look: &TabBarLook, win_w: u32, cell_w: u32, max_length: u32) -> Self {
        if !self.edge.vertical() {
            return self;
        }
        let avail = win_w as i64 - self.outer as i64 - self.inner as i64;
        let cols = crate::vtabs::bar_cols(avail, cell_w, crate::vtabs::desired_cols(look, max_length));
        self.with_vertical_width(cols * cell_w.max(1))
    }

    /// 창이 아직 없어 폭을 모를 때(창 크기를 정할 때): 바라는 폭 그대로
    pub fn wanted(self, look: &TabBarLook, cell_w: u32, max_length: u32) -> Self {
        let cols = crate::vtabs::desired_cols(look, max_length);
        self.with_vertical_width(cols * cell_w.max(1))
    }

    /// 세로 탭 줄이 지금 자리를 차지하는가(세로 가장자리이고 탭 줄이 있고 폭이 있다)
    pub fn vertical(&self) -> bool {
        self.edge.vertical() && self.bar_h > 0 && self.vw > 0
    }

    /// 세로 줄이 칸의 가로 폭에서 가져가는 띠의 폭(여백 포함). 가로 줄이면 0
    pub fn band_x(&self) -> u32 {
        if self.vertical() {
            self.outer + self.vw + self.inner
        } else {
            0
        }
    }

    /// 칸이 들어가는 가로 범위 (맨 왼쪽 x, 폭). 왼쪽 세로 줄이면 띠 오른쪽에서, 오른쪽이면 띠 왼쪽까지
    pub fn content_x(&self, win_w: u32) -> (u32, u32) {
        let band = self.band_x().min(win_w);
        let left = if self.edge == TabBarEdge::Left { band } else { 0 };
        (left, win_w - band)
    }

    /// 세로 탭 줄 사각형의 가로 범위 (시작 x, 폭)
    pub fn bar_x(&self, win_w: u32) -> (i32, u32) {
        match self.edge {
            TabBarEdge::Right => (win_w.saturating_sub(self.outer + self.vw) as i32, self.vw),
            _ => (self.outer as i32, self.vw),
        }
    }

    /// 세로 줄의 탭이 놓이는 세로 범위 (시작 y, 높이). 제목 막대 비움 밑에서 창 끝까지, 위아래로 `tab-bar-margin-width`를 뺀다
    pub fn tabs_y(&self, win_h: u32) -> (i32, u32) {
        let top = self.inset.min(win_h);
        let room = win_h - top;
        let m = self.margin_x.min(room / 2);
        ((top + m) as i32, (room - 2 * m).max(1))
    }

    /// (x, y)가 탭 줄 사각형 안인가. 세로 줄은 가로 범위도 본다. 탭 줄이 없으면 거짓
    pub fn in_bar_at(&self, x: f64, y: f64, win_w: u32, win_h: u32) -> bool {
        if !self.edge.vertical() {
            return self.in_bar(y, win_h);
        }
        let (bx, bw) = self.bar_x(win_w);
        self.vertical() && x >= bx as f64 && x < (bx + bw as i32) as f64 && y >= self.inset as f64 && y < win_h as f64
    }

    /// (x, y)가 탭 줄과 그 여백을 합친 띠 안인가(칸이 아닌 곳). 탭 줄이 없으면 거짓
    pub fn in_band_at(&self, x: f64, y: f64, win_w: u32, win_h: u32) -> bool {
        if !self.edge.vertical() {
            return self.in_band(y, win_h);
        }
        if !self.vertical() || y < self.inset as f64 || y >= win_h as f64 {
            return false;
        }
        let band = self.band_x().min(win_w) as f64;
        match self.edge {
            TabBarEdge::Right => x >= win_w as f64 - band && x < win_w as f64,
            _ => x >= 0.0 && x < band,
        }
    }

    /// 탭 줄이 차지하는 세로 띠의 높이(여백 포함). 탭 줄이 없으면 0
    pub fn band(&self) -> u32 {
        if self.bar_h == 0 || self.edge.vertical() {
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
        self.bar_h > 0 && !self.edge.vertical() && y >= t as f64 && y < b as f64
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

    // ---- 세로 탭 줄(#935) ----

    fn vgeom(edge: TabBarEdge, mw: f32, mh: (f32, f32), vw: u32) -> BarGeom {
        BarGeom::new(&look(edge, mw, mh), 1.0, 30, 40).with_vertical_width(vw)
    }

    #[test]
    fn left_bar_takes_width_from_the_cells_and_not_height() {
        // kitty: central.left = 폭 + 안 + 밖, tab_bar.left = 밖
        let g = vgeom(TabBarEdge::Left, 0.0, (3.0, 4.0), 200);
        assert!(g.vertical());
        assert_eq!((g.outer, g.inner, g.band_x()), (3, 4, 207));
        assert_eq!(g.band(), 0, "세로 줄은 칸의 높이를 줄이지 않는다");
        assert_eq!(g.content_y(800), (30, 770), "제목 막대 비움 밑에서 창 끝까지");
        assert_eq!(g.content_x(1000), (207, 793));
        assert_eq!(g.bar_x(1000), (3, 200));
        assert_eq!(g.content_top(), 30);
        // 칸 격자와 탭 줄과 여백이 창 폭을 빈틈없이 나눈다
        let (cx, cw) = g.content_x(1000);
        let (bx, bw) = g.bar_x(1000);
        assert_eq!(bx as u32 + bw + g.inner, cx);
        assert_eq!(g.outer + bw + g.inner + cw, 1000);
    }

    #[test]
    fn right_bar_puts_the_cells_first_like_kitty() {
        // kitty: central.right = 창 - (폭 + 안 + 밖), tab_bar.left = central.right + 안
        let g = vgeom(TabBarEdge::Right, 0.0, (3.0, 4.0), 200);
        assert_eq!(g.content_x(1000), (0, 793));
        assert_eq!(g.bar_x(1000), (793 + 4, 200));
        assert_eq!(g.bar_x(1000).0 as u32 + 200 + 3, 1000);
    }

    #[test]
    fn vertical_margin_width_is_above_and_below_the_tabs() {
        // kitty: tab_bar_margin_width는 세로 줄에서 위·아래 여백
        let g = vgeom(TabBarEdge::Left, 5.0, (0.0, 0.0), 200);
        assert_eq!(g.tabs_y(800), (35, 800 - 35 - 5));
        let g = vgeom(TabBarEdge::Right, 0.0, (0.0, 0.0), 200);
        assert_eq!(g.tabs_y(800), (30, 770));
        // 여백이 높이보다 커도 높이는 1 이상
        let g = vgeom(TabBarEdge::Left, 9999.0, (0.0, 0.0), 200);
        assert!(g.tabs_y(800).1 >= 1);
    }

    #[test]
    fn vertical_hits_use_x_and_y() {
        let g = vgeom(TabBarEdge::Left, 0.0, (2.0, 3.0), 100);
        // 탭 줄 사각형: x 2..102, y 30..800
        assert!(g.in_bar_at(2.0, 30.0, 1000, 800) && g.in_bar_at(101.9, 799.0, 1000, 800));
        assert!(!g.in_bar_at(1.9, 100.0, 1000, 800) && !g.in_bar_at(102.0, 100.0, 1000, 800));
        assert!(!g.in_bar_at(50.0, 29.9, 1000, 800), "제목 막대 비움은 탭 줄이 아니다");
        // 띠는 여백 포함: x 0..105
        assert!(g.in_band_at(0.0, 100.0, 1000, 800) && g.in_band_at(104.9, 100.0, 1000, 800));
        assert!(!g.in_band_at(105.0, 100.0, 1000, 800));
        let r = vgeom(TabBarEdge::Right, 0.0, (2.0, 3.0), 100);
        assert!(r.in_band_at(895.0, 100.0, 1000, 800) && r.in_band_at(999.9, 100.0, 1000, 800));
        assert!(!r.in_band_at(894.9, 100.0, 1000, 800));
        assert!(r.in_bar_at(898.0, 100.0, 1000, 800) && !r.in_bar_at(897.9, 100.0, 1000, 800));
        // 가로 줄이면 x는 보지 않고 지금 판정과 같다
        let h = BarGeom::new(&look(TabBarEdge::Top, 0.0, (0.0, 0.0)), 1.0, 30, 40);
        assert!(h.in_bar_at(5000.0, 30.0, 1000, 800) == h.in_bar(30.0, 800));
        assert!(!h.vertical() && h.band_x() == 0);
    }

    #[test]
    fn a_vertical_edge_without_width_or_bar_is_no_band() {
        // 탭 줄이 없다(높이 0)거나 폭이 0이면 아무 자리도 차지하지 않는다
        let none = BarGeom::new(&look(TabBarEdge::Left, 4.0, (3.0, 4.0)), 1.0, 30, 0).with_vertical_width(200);
        assert!(!none.vertical());
        assert_eq!((none.band_x(), none.band()), (0, 0));
        assert_eq!(none.content_x(900), (0, 900));
        assert!(!none.in_band_at(1.0, 100.0, 900, 800) && !none.in_bar_at(1.0, 100.0, 900, 800));
        let nowidth = vgeom(TabBarEdge::Right, 0.0, (3.0, 4.0), 0);
        assert!(!nowidth.vertical());
        assert_eq!(nowidth.content_x(900), (0, 900));
    }

    #[test]
    fn sized_width_comes_from_window_width_and_title_length() {
        let l = look(TabBarEdge::Left, 0.0, (3.0, 4.0));
        // 넓은 창: 28칸 * 칸 폭 10
        let g = BarGeom::new(&l, 1.0, 0, 40).sized(&l, 2000, 10, 0);
        assert_eq!(g.vw, 280);
        // 좁은 창(600 - 7 = 593px → 59칸의 3분의 1 = 19칸)
        let g = BarGeom::new(&l, 1.0, 0, 40).sized(&l, 600, 10, 0);
        assert_eq!(g.vw, 190);
        // 제목 길이 12: 12 + 4칸
        let g = BarGeom::new(&l, 1.0, 0, 40).sized(&l, 2000, 10, 12);
        assert_eq!(g.vw, 160);
        // 가로 줄이면 건드리지 않는다
        let t = look(TabBarEdge::Top, 0.0, (0.0, 0.0));
        assert_eq!(BarGeom::new(&t, 1.0, 0, 40).sized(&t, 2000, 10, 0).vw, 0);
        // 창을 만들기 전(폭을 모를 때)은 바라는 폭을 쓴다
        let g = BarGeom::new(&l, 1.0, 0, 40).wanted(&l, 10, 0);
        assert_eq!(g.vw, 280);
        assert_eq!(g.band_x(), 280 + 7);
    }
}
