//! kitty 마커 동작(`toggle_marker`·`remove_marker`·`scroll_to_mark`, #929)의 앱 쪽 연결. 판단은 `bt_term::marks`가 한다.

use super::App;
use crate::keymap::Action;

impl App {
    /// 마커 동작 셋을 지금 고른 칸에 한다. 마커 동작이 아니면 `false`
    pub(super) fn run_marker_action(&mut self, action: &Action) -> bool {
        let note = match action {
            Action::ToggleMarker(spec) => match self.active_pane().map(|t| t.session.toggle_marker(spec)) {
                Some(Err(e)) => Some(format!("마커를 켜지 못했습니다: {e}")),
                _ => None,
            },
            Action::RemoveMarker => {
                if let Some(t) = self.active_pane() {
                    t.session.remove_marker();
                }
                None
            }
            Action::ScrollToMark(prev, mark) => {
                if let Some(t) = self.active_pane() {
                    t.session.scroll_to_mark(*prev, *mark);
                }
                None
            }
            _ => return false,
        };
        if let Some(n) = note {
            self.toast(n);
        }
        self.request_redraw();
        true
    }

    /// 설정의 `link-url-regex`를 컴파일해 둔다(시작과 설정 다시 읽기)
    pub(super) fn rebuild_link_regex(&mut self) {
        let (u, warnings) = bt_term::link_regex::UserLinkRegexes::new(&self.config.link_url_regex);
        for w in warnings {
            log::warn!("설정: {w}");
        }
        self.link_regex = u;
    }
}
