//! 탭 묶음(#933)을 `App`에 잇는 곳. 묶음 규칙은 `tabbundle`(순수 모델)에 있고, 여기는 창의 탭 목록·막대 자리·메뉴에 그 결과를 옮기는 이음뿐이다.

use super::*;
use crate::tabbundle::{self, BarItem, Bundle, BundleOp, Id};

impl App {
    /// 탭마다의 묶음 번호
    pub(super) fn bundle_ids(&self) -> Vec<Option<Id>> {
        self.w.tabs.iter().map(|t| t.bundle).collect()
    }

    /// 탭 줄에 놓이는 항목들(머리표와 보이는 탭)
    pub(super) fn bar_items(&self) -> Vec<BarItem> {
        tabbundle::bar_items(&self.bundle_ids(), &self.w.bundles, self.w.active)
    }

    fn bundle_of(&self, id: Id) -> Option<&Bundle> {
        self.w.bundles.iter().find(|b| b.id == id)
    }

    /// 탭 `gi`가 묶음에 들었으면 그 표시
    pub(super) fn bundle_mark(&self, gi: usize) -> Option<tabbar::BundleMark> {
        let b = self.bundle_of(self.w.tabs.get(gi)?.bundle?)?;
        Some(tabbar::BundleMark {
            color: b.color,
            chip: false,
        })
    }

    /// 묶음 머리표 한 칸. 글은 이름과(접혔으면) 가려진 탭 수다
    pub(super) fn chip_label(&self, id: Id) -> tabbar::TabLabel {
        let b = self.bundle_of(id).cloned().unwrap_or_else(|| Bundle::new(id));
        let members = self.w.tabs.iter().filter(|t| t.bundle == Some(id)).count();
        let shown = usize::from(self.w.tabs.get(self.w.active).is_some_and(|t| t.bundle == Some(id)));
        tabbar::TabLabel {
            title: tabbundle::chip_title(&b, members, shown),
            spans: None,
            active: false,
            unread: false,
            progress: None,
            color: Default::default(),
            bundle: Some(tabbar::BundleMark {
                color: b.color,
                chip: true,
            }),
        }
    }

    /// 탭 줄 글꼴의 칸 폭(px)
    fn bar_cell_w(&self) -> u32 {
        self.w.fonts.chrome().map_or(1, |term| {
            tabbar::bar_fonts(term, self.w.title_fonts.as_ref()).metrics().width
        })
    }

    /// 항목마다 정해진 폭(머리표만 `Some`). 그리는 쪽(`draw_placed`)이 쓰는 값과 같아 그린 자리와 누르는 자리가 맞는다
    pub(super) fn bar_fixed(&self, items: &[BarItem]) -> Vec<Option<u32>> {
        let cw = self.bar_cell_w();
        items
            .iter()
            .map(|it| match it {
                BarItem::Chip(id) => Some(tabbar::chip_px(&self.chip_label(*id).title, cw)),
                BarItem::Tab(_) => None,
            })
            .collect()
    }

    /// 마우스 아래의 탭 줄 항목(탭 줄 사각형 안일 때만)
    pub(super) fn item_at_cursor(&self) -> Option<BarItem> {
        let size = self.w.gfx.as_ref()?.window.inner_size();
        let g = self.bar_geom();
        if !g.in_bar_at(self.w.cursor_pos.x, self.w.cursor_pos.y, size.width, size.height) {
            return None;
        }
        let items = self.bar_items();
        let idx = if g.vertical() {
            self.vertical_tab_at_cursor(&g, size.height)?
        } else {
            tabbar::hit_items(
                g.tabs_x(size.width).1,
                g.tab_x(self.w.cursor_pos.x, size.width),
                &self.bar_fixed(&items),
                self.tab_layout(),
            )?
        };
        items.get(idx).copied()
    }

    /// 마우스 아래의 묶음 머리표
    fn chip_at_cursor(&self) -> Option<Id> {
        match self.item_at_cursor()? {
            BarItem::Chip(id) => Some(id),
            BarItem::Tab(_) => None,
        }
    }

    /// 묶음 머리표를 눌렀다: 왼쪽은 접기·펴기, 오른쪽은 묶음 메뉴. 머리표 위가 아니면 거짓
    pub(super) fn chip_press(&mut self, button: MouseButton) -> bool {
        let Some(id) = self.chip_at_cursor() else { return false };
        match button {
            MouseButton::Left => self.tab_group_op(Some(id), BundleOp::Toggle),
            MouseButton::Right => self.bundle_menu_popup(id),
            _ => {}
        }
        true
    }

    /// 머리표의 오른쪽 단추 메뉴(macOS, 모달)
    fn bundle_menu_popup(&mut self, id: Id) {
        #[cfg(target_os = "macos")]
        if let Some(b) = self.bundle_of(id) {
            let dark = crate::tabcolor::is_dark(self.config.theme.background);
            crate::context_menu::popup_bundle(id, b.collapsed, b.color, dark);
        }
        #[cfg(not(target_os = "macos"))]
        let _ = id;
    }

    /// 지금 탭의 오른쪽 단추 메뉴에 붙는 묶음 항목들
    #[cfg(target_os = "macos")]
    pub(super) fn tab_group_menu(&self) -> Vec<crate::context_menu::GroupEntry> {
        let ids = self.bundle_ids();
        let i = self.w.active;
        let bundle = self
            .w
            .tabs
            .get(i)
            .and_then(|t| t.bundle)
            .and_then(|b| self.bundle_of(b));
        let mut probe = ids.clone();
        let can_join = i < probe.len() && tabbundle::join_neighbor(&mut probe, i).is_some();
        crate::context_menu::group_entries(
            bundle.is_some(),
            tabbundle::is_sole_member(&ids, i),
            can_join,
            bundle.is_some_and(|b| b.collapsed),
            bundle.map_or(Default::default(), |b| b.color),
        )
    }

    /// 탭 목록을 `ids`대로 바꾸고(`mv`가 있으면 탭 자리도 옮기고) 쓰이지 않는 묶음을 지운다
    pub(super) fn apply_bundle_edit(&mut self, ids: Vec<Option<Id>>, mv: Option<(usize, usize)>) {
        if let Some((from, to)) = mv {
            let len = self.w.tabs.len();
            if from < len && to < len && from != to {
                self.w.active = tabbar::active_after_move(self.w.active, from, to);
                let tab = self.w.tabs.remove(from);
                self.w.tabs.insert(to, tab);
            }
        }
        for (t, id) in self.w.tabs.iter_mut().zip(&ids) {
            t.bundle = *id;
        }
        tabbundle::prune(&mut self.w.bundles, &ids);
        self.request_redraw_all();
    }

    /// 탭 `from`을 묶음 `b`의 머리표 위에 놓았다
    pub(super) fn reorder_tab_to_chip(&mut self, from: usize, b: Id) {
        if from >= self.w.tabs.len() {
            return;
        }
        let mut ids = self.bundle_ids();
        let mv = tabbundle::reorder_to_chip(&mut ids, from, b);
        self.apply_bundle_edit(ids, Some(mv));
    }

    /// `new_tab_group`: 지금 탭을 새 묶음에 혼자 넣는다. 이미 혼자뿐인 묶음이면 아무것도 하지 않는다
    pub(super) fn tab_group_new(&mut self) {
        let i = self.w.active;
        let mut ids = self.bundle_ids();
        if i >= ids.len() || tabbundle::is_sole_member(&ids, i) {
            return;
        }
        let id = tabbundle::next_id(&self.w.bundles);
        self.w.bundles.push(Bundle {
            color: tabbundle::default_color(id),
            ..Bundle::new(id)
        });
        let mv = tabbundle::new_bundle(&mut ids, i, id);
        self.apply_bundle_edit(ids, mv);
    }

    /// `leave_tab_group`: 지금 탭을 묶음에서 뺀다
    pub(super) fn tab_group_leave(&mut self) {
        let mut ids = self.bundle_ids();
        if self.w.active >= ids.len() {
            return;
        }
        let mv = tabbundle::leave(&mut ids, self.w.active);
        self.apply_bundle_edit(ids, mv);
    }

    /// `join_tab_group`: 지금 탭을 옆 탭의 묶음에 넣는다(왼쪽 먼저)
    pub(super) fn tab_group_join(&mut self) {
        let mut ids = self.bundle_ids();
        if self.w.active < ids.len() && tabbundle::join_neighbor(&mut ids, self.w.active).is_some() {
            self.apply_bundle_edit(ids, None);
        }
    }

    /// 묶음 하나에 하는 일. `target`이 없으면 지금 탭의 묶음이다(묶음에 들지 않았으면 아무것도 하지 않는다)
    pub(super) fn tab_group_op(&mut self, target: Option<Id>, op: BundleOp) {
        let Some(id) = target.or_else(|| self.w.tabs.get(self.w.active).and_then(|t| t.bundle)) else {
            return;
        };
        let Some(pos) = self.w.bundles.iter().position(|b| b.id == id) else {
            return;
        };
        match op {
            BundleOp::Toggle => self.w.bundles[pos].collapsed ^= true,
            BundleOp::Rename => {
                let now = self.w.bundles[pos].name.clone();
                self.w.palette = None;
                self.w.title_prompt = Some((TitleTarget::Bundle(id), now));
            }
            BundleOp::SetTitle(text) => self.set_bundle_name(id, &text),
            BundleOp::Color(c) => self.w.bundles[pos].color = c,
            BundleOp::Dissolve => {
                let ids: Vec<Option<Id>> = self.bundle_ids().into_iter().map(|b| b.filter(|b| *b != id)).collect();
                self.apply_bundle_edit(ids, None);
            }
        }
        self.request_redraw_all();
    }

    /// 묶음 이름을 정한다. 빈 글이면 기본 이름으로 돌아간다
    pub(super) fn set_bundle_name(&mut self, id: Id, text: &str) {
        if let Some(b) = self.w.bundles.iter_mut().find(|b| b.id == id) {
            b.name = tabbundle::clean_name(text).unwrap_or_default();
            self.request_redraw_all();
        }
    }

    /// 창 상태 파일에서 읽은 묶음들을 지금 창에 놓는다(탭은 이미 파일의 묶음 번호를 달고 있다)
    pub(super) fn restore_bundles(&mut self, saved: &[crate::state::SavedBundle]) {
        let mut reg = tabbundle::load_bundles(saved);
        let mut ids = self.bundle_ids();
        tabbundle::settle_loaded(&mut ids, &mut reg);
        for (t, id) in self.w.tabs.iter_mut().zip(&ids) {
            t.bundle = *id;
        }
        self.w.bundles = reg;
    }
}
