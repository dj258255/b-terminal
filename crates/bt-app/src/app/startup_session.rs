//! kitty 세션 파일(#934)로 시작하는 앱 쪽 연결. 읽기·배치 계산은 `session_file`·`session_layout`이 하고, 여기는 만든 계획대로 창·탭·칸을 연다.

use std::path::Path;

use winit::event_loop::ActiveEventLoop;

use super::{App, TabGroup, Win, clean_user_title, window_occluded};
use crate::session_file::{Ctx, TabPlan};
use bt_term::SpawnOptions;

impl App {
    /// 세션 파일을 읽어 그 배치로 연다. 탭을 하나라도 열었으면 참. 읽지 못하면 이유를 로그에 남기고 거짓(세션 없이 시작)
    pub(super) fn open_startup_session(&mut self, event_loop: &ActiveEventLoop, path: &Path) -> bool {
        let text = match crate::session_file::read_file(path) {
            Ok(t) => t,
            Err(e) => {
                log::warn!("세션 파일 {}을 읽지 못해 세션 없이 시작합니다: {e}", path.display());
                return false;
            }
        };
        let base = path.parent().map(Path::to_path_buf).unwrap_or_default();
        let home = std::env::var_os("HOME")
            .map(std::path::PathBuf::from)
            .unwrap_or_default();
        let ctx = Ctx {
            base_dir: &base,
            home: &home,
            env: &|k| std::env::var(k).ok(),
        };
        let plan = crate::session_file::parse(&text, &ctx);
        for w in &plan.warnings {
            log::warn!("세션 파일 {}: {w}", path.display());
        }
        let mut opened = false;
        for (wi, win) in plan.windows.iter().enumerate() {
            if wi == 0 {
                self.apply_start_window(event_loop, true);
            } else {
                // 새 창(`new_os_window`)
                let Some(gfx) = self.create_gfx(event_loop) else { break };
                let mut w = Win::new(self.new_window_font_size(false));
                w.focused = gfx.window.has_focus();
                w.occluded = window_occluded(&gfx.window);
                w.gfx = Some(gfx);
                let old = std::mem::replace(&mut self.w, w);
                self.others.push(old);
                self.load_fonts();
                self.apply_initial_size();
                self.apply_start_window(event_loop, false);
            }
            for tab in &win.tabs {
                opened |= self.open_session_tab(tab);
            }
            if self.w.tabs.is_empty() {
                // 칸을 하나도 띄우지 못한 창: 첫 창이 아니면 닫는다(첫 창은 기본 탭을 연다)
                if wi > 0
                    && let Some(prev) = self.others.pop()
                {
                    self.w = prev;
                }
                continue;
            }
            self.w.active = win.active_tab.min(self.w.tabs.len() - 1);
            self.w.window_title = win.title.as_deref().and_then(clean_user_title);
            self.last_focused = self.w.id();
            self.resize_all();
        }
        if opened {
            log::info!("세션 파일로 시작했습니다: {}", path.display());
        }
        opened
    }

    /// 계획의 탭 하나를 지금 창 끝에 연다. 칸을 모두 띄웠을 때만 참
    fn open_session_tab(&mut self, tab: &TabPlan) -> bool {
        let font_size = self.w.font_size;
        self.ensure_font_size(font_size);
        let size = self.grid_size_for(self.content_rect(), font_size);
        let mut ids = Vec::new();
        for pane in &tab.panes {
            let mut opts: SpawnOptions = self.default_spawn();
            if let Some(program) = &pane.program {
                // 셸 문자열을 잇지 않고 (프로그램, 인자 배열) 그대로 넘긴다
                opts.program = Some(program.clone());
                opts.args = pane.args.clone();
            }
            if let Some(dir) = &pane.cwd {
                if dir.is_dir() {
                    opts.cwd = Some(dir.clone());
                } else {
                    log::warn!("세션 파일: 폴더가 없어 기본 폴더로 띄웁니다: {}", dir.display());
                }
            }
            opts.env.extend(pane.env.iter().cloned());
            if let Some(p) = self.spawn_pane(opts, size, font_size) {
                ids.push(p.id);
                self.events.publish(bt_core::proto::Event::TabOpened { tab: p.id });
                self.panes.push(p);
            }
        }
        let Some(layout) = tab.tree.to_node(&ids).filter(|_| ids.len() == tab.panes.len()) else {
            // 칸을 다 띄우지 못했으면 그 탭은 버린다
            for id in ids {
                if let Some(i) = self.pane_index(id) {
                    self.panes.remove(i);
                    self.events.publish(bt_core::proto::Event::TabClosed { tab: id });
                }
            }
            return false;
        };
        let leaves = layout.leaves();
        let focus = ids.get(tab.focus).copied().unwrap_or(leaves[0]);
        self.w.tabs.push(TabGroup {
            layout,
            focus,
            zoomed: tab.zoomed && leaves.len() > 1,
            title: tab.title.as_deref().and_then(clean_user_title),
            color: Default::default(),
        });
        for (id, pane) in ids.iter().zip(&tab.panes) {
            if let Some(t) = pane.title.as_deref().and_then(clean_user_title) {
                self.set_user_title(*id, &t);
            }
        }
        true
    }
}
