//! 세로 탭 줄(#935)과 탭 끌어 새 창으로 떼기를 `App`에 잇는 곳. 계산은 `tabgeom`·`vtabs`·`tabdetach`에 있고
//! 여기는 창·설정에서 값을 꺼내 그 계산에 넘기는 얇은 이음뿐이다.

use super::*;

impl App {
    /// 탭 줄 자리 계산. `visible`은 탭 줄을 그리는가, `inset`은 제목 막대 비움(px).
    /// 세로 줄의 폭은 지금 창 폭과 탭 줄 글꼴의 칸 폭으로 정한다(창이 없으면 바라는 폭)
    pub(super) fn bar_geom_with(&self, visible: bool, inset: u32) -> crate::tabgeom::BarGeom {
        let bar_h = if visible { self.tabbar_px() } else { 0 };
        let look = &self.config.tab_bar_look;
        let g = crate::tabgeom::BarGeom::new(look, self.scale, inset, bar_h);
        if !g.edge.vertical() {
            return g;
        }
        let cell_w = self.chrome_metrics().map_or(1, |m| m.width);
        let max_length = self.config.tab_title.max_length;
        match self.w.gfx.as_ref() {
            Some(gfx) => g.sized(look, gfx.window.inner_size().width, cell_w, max_length),
            None => g.wanted(look, cell_w, max_length),
        }
    }

    /// 창 크기를 정할 때(창 폭을 모를 때) 세로 탭 줄이 칸의 가로 폭에서 가져갈 띠의 폭(px). 가로 줄이거나 탭 줄이 없으면 0
    pub(super) fn tabbar_width_wanted(&self) -> u32 {
        let look = &self.config.tab_bar_look;
        let bar_h = if self.tab_bar_visible(self.w.tabs.len()) {
            self.tabbar_px()
        } else {
            0
        };
        let cell_w = self.chrome_metrics().map_or(1, |m| m.width);
        crate::tabgeom::BarGeom::new(look, self.scale, 0, bar_h)
            .wanted(look, cell_w, self.config.tab_title.max_length)
            .band_x()
    }

    /// 세로 줄에 쌓인 탭들의 자리(탭 줄 맨 위에서 잰 y)
    pub(super) fn vertical_rows(&self, g: &crate::tabgeom::BarGeom, win_h: u32) -> crate::vtabs::Rows {
        let (_, th) = g.tabs_y(win_h);
        crate::vtabs::rows(
            self.bar_items().len(),
            th,
            g.bar_h,
            crate::vtabs::gap_px(g.bar_h),
            self.config.tab_bar_look.align,
        )
    }

    /// 탭 줄을 따라가는 방향의 좌표: 세로 줄이면 y, 가로 줄이면 x(끌기 시작·옮길 자리가 이 값을 본다)
    pub(super) fn tab_axis(&self, p: PhysicalPosition<f64>) -> f64 {
        if self.bar_geom().vertical() { p.y } else { p.x }
    }

    /// 세로 줄의 마우스 아래 항목 번호(탭 줄 사각형 안일 때만). 머리표도 한 항목이다(#933)
    pub(super) fn vertical_tab_at_cursor(&self, g: &crate::tabgeom::BarGeom, win_h: u32) -> Option<usize> {
        let (ty, _) = g.tabs_y(win_h);
        crate::vtabs::tab_at(&self.vertical_rows(g, win_h), self.w.cursor_pos.y - ty as f64)
    }

    /// 탭을 끌다 놓은 마우스 자리가 창 틀 밖인가(화면 좌표). 창 위치를 모르면 안으로 본다
    fn cursor_outside_frame(&self) -> bool {
        let Some(g) = self.w.gfx.as_ref() else { return false };
        let (Ok(inner), Ok(outer)) = (g.window.inner_position(), g.window.outer_position()) else {
            return false;
        };
        let cursor = (
            inner.x as f64 + self.w.cursor_pos.x,
            inner.y as f64 + self.w.cursor_pos.y,
        );
        crate::tabdetach::is_outside(
            cursor,
            (outer.x, outer.y),
            (g.window.outer_size().width, g.window.outer_size().height),
            crate::tabdetach::DETACH_SLACK_PX,
        )
    }

    /// 탭을 누르던 왼쪽 단추를 놓았다: 끌기가 시작됐으면 창 안에서는 놓은 자리로 옮기고 창 밖에서는 새 창으로 뗀다
    pub(super) fn tab_drag_release(&mut self, event_loop: &ActiveEventLoop) {
        let Some((win_w, win_h)) = self.w.gfx.as_ref().map(|g| {
            let s = g.window.inner_size();
            (s.width, s.height)
        }) else {
            self.w.tab_drag.cancel();
            return;
        };
        let geom = self.bar_geom();
        let count = self.w.tabs.len();
        let items = self.bar_items();
        let at = if geom.vertical() {
            let (ty, _) = geom.tabs_y(win_h);
            crate::vtabs::drop_index(
                &self.vertical_rows(&geom, win_h),
                items.len(),
                self.w.cursor_pos.y - ty as f64,
            )
        } else {
            tabbar::drop_item(
                geom.tabs_x(win_w).1,
                geom.tab_x(self.w.cursor_pos.x, win_w),
                &self.bar_fixed(&items),
                self.tab_layout(),
            )
        };
        // 놓은 항목이 탭이면 그 탭 자리로, 묶음 머리표면 그 묶음의 첫 탭 자리로(묶음 맨 앞에 들어간다, #933)
        let dropped = at.and_then(|i| items.get(i).copied());
        let to = dropped.and_then(|it| match it {
            crate::tabbundle::BarItem::Tab(t) => Some(t),
            crate::tabbundle::BarItem::Chip(b) => self.w.tabs.iter().position(|t| t.bundle == Some(b)),
        });
        let outside = !self.w.quick && self.cursor_outside_frame();
        match self.w.tab_drag.release_at(count, to, outside) {
            tabbar::TabDrop::Move { from, to } => match dropped {
                Some(crate::tabbundle::BarItem::Chip(b)) => self.reorder_tab_to_chip(from, b),
                _ => self.reorder_tab(from, to),
            },
            tabbar::TabDrop::Detach { from } => self.detach_tab_to_cursor(from, event_loop),
            tabbar::TabDrop::None => {}
        }
    }

    /// `from` 탭을 새 창으로 떼고, 새 창을 놓은 자리(마우스) 근처에 둔다
    fn detach_tab_to_cursor(&mut self, from: usize, event_loop: &ActiveEventLoop) {
        if from >= self.w.tabs.len() {
            return;
        }
        let origin = self.w.gfx.as_ref().and_then(|g| {
            let inner = g.window.inner_position().ok()?;
            let cursor = (
                inner.x as f64 + self.w.cursor_pos.x,
                inner.y as f64 + self.w.cursor_pos.y,
            );
            Some(crate::tabdetach::window_origin(cursor, self.scale))
        });
        if self.w.active != from {
            self.activate(from);
        }
        self.move_tab_to_new_window_at(event_loop, origin);
    }
}
