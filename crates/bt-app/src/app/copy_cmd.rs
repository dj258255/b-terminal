//! vi 복사 모드(#930)의 앱 쪽 연결. 커서 움직임·고르기는 `bt_term::copy_mode`, 키 테이블과 동작 이름은 `crate::copy_mode`가 한다.
//! 여기는 들어가기·나가기·복사, 입력기 끄기, 키 테이블과 칸의 상태 맞추기만 한다

use super::{App, SearchBar};
use crate::copy_mode::{CopyAction, TABLE};
use crate::keymap::Action;

impl App {
    /// 고른 칸이 복사 모드 중인가
    pub(super) fn copy_mode_on(&self) -> bool {
        self.active_pane().is_some_and(|t| t.session.copy_mode_active())
    }

    /// 복사 모드 동작이면 하고 참. 아니면 거짓
    pub(super) fn run_copy_mode_action(&mut self, action: &Action) -> bool {
        match action {
            Action::ActivateCopyMode => self.enter_copy_mode(),
            Action::CopyMode(a) => self.copy_mode_action(*a),
            _ => return false,
        }
        self.request_redraw();
        true
    }

    fn enter_copy_mode(&mut self) {
        // 찾기 막대·팔레트 같은 입력 막대가 열려 있으면 그쪽이 키를 받는다. 막대 위에서는 들어가지 않는다
        if self.w.search.is_some()
            || self.w.palette.is_some()
            || self.w.title_prompt.is_some()
            || self.w.hints.is_some()
        {
            return;
        }
        if !self.keymap.has_table(TABLE) {
            self.toast("복사 모드 키 테이블(copy_mode)이 없어 들어가지 않았습니다".into());
            return;
        }
        if self.copy_mode_on() {
            return;
        }
        // 조합 중인 글자는 들어가기 전에 칸에 확정한다(모드 중에는 PTY로 보내지 않는다)
        self.settle_preedit();
        self.w.preedit.clear();
        let entered = self.active_pane().is_some_and(|t| t.session.copy_mode_enter());
        if entered {
            self.w
                .keyseq
                .apply_table_action(&self.keymap, &Action::ActivateKeyTable(TABLE.into()));
            self.sync_confirm_ime();
        }
    }

    fn copy_mode_action(&mut self, a: CopyAction) {
        if !self.copy_mode_on() {
            // 모드 밖에서 온 동작(명령 팔레트·다른 칸): 남은 테이블만 정리한다
            self.drop_copy_table();
            return;
        }
        match a {
            CopyAction::Op(op) => {
                if let Some(t) = self.active_pane() {
                    t.session.copy_mode_op(op);
                }
            }
            CopyAction::Copy => {
                let selected = self.active_pane().is_some_and(|t| t.session.copy_mode_has_selection());
                if selected
                    && self.copy_selection(bt_term::CopyFormat::default())
                    && self.config.app_notifications.clipboard_copy
                {
                    self.toast("복사했습니다".into());
                }
                self.leave_copy_mode_active();
            }
            CopyAction::Exit => self.leave_copy_mode_active(),
            CopyAction::SearchOpen { forward } => self.copy_search_open(forward),
            CopyAction::SearchNext { reverse } => {
                let Some((query, forward)) = self.w.copy_search_last.clone() else {
                    self.toast("먼저 / 나 ? 로 찾을 글을 정해 주세요".into());
                    return;
                };
                self.copy_search_run(&query, forward != reverse);
            }
            CopyAction::Jump(kind) => self.w.copy_jump_wait = Some(kind),
            CopyAction::JumpAgain { reverse } => {
                let Some((kind, c)) = self.w.copy_jump_last else { return };
                let kind = if reverse { kind.reversed() } else { kind };
                if let Some(t) = self.active_pane() {
                    t.session.copy_mode_jump(kind, c, true);
                }
            }
        }
    }

    /// 고른 칸의 복사 모드를 나간다(스크롤 자리를 되돌리고 키 테이블을 내리고 입력기를 다시 켠다)
    fn leave_copy_mode_active(&mut self) {
        if let Some(t) = self.active_pane() {
            t.session.copy_mode_exit();
        }
        self.drop_copy_table();
    }

    fn drop_copy_table(&mut self) {
        self.w.keyseq.remove_table(TABLE);
        self.w.copy_jump_wait = None;
        // 복사 모드의 `/ ?`가 연 찾기 막대는 모드와 함께 닫는다(막대가 키를 쥔 채 남지 않게)
        if self.w.copy_search_open.is_some() {
            self.end_search();
        }
        self.sync_confirm_ime();
    }

    /// `/`·`?`: 찾기 막대를 연다. 이미 열린 막대(다른 길로 연 것)는 닫고 새로 연다. 막대가 열려 있는 동안
    /// 키는 막대가 받고(`search_key`) IME도 켜진다(확정한 글만 들어온다)
    fn copy_search_open(&mut self, forward: bool) {
        self.end_search();
        let Some(id) = self.focused_id() else { return };
        self.w.preedit.clear();
        self.w.search = Some(SearchBar::new(id, String::new()));
        self.w.copy_search_open = Some(forward);
        self.sync_confirm_ime();
    }

    /// 막대에서 Enter: 친 글(비었으면 마지막으로 찾은 글)로 커서를 옮기고 막대를 닫는다. 맞은 곳 강조는 남긴다
    pub(super) fn copy_search_confirm(&mut self, forward: bool) {
        let typed = self.w.search.as_ref().map(|s| s.query.clone()).unwrap_or_default();
        self.w.search = None;
        self.w.copy_search_open = None;
        self.w.preedit.clear();
        self.sync_confirm_ime();
        let query = if typed.is_empty() {
            self.w.copy_search_last.as_ref().map(|(q, _)| q.clone())
        } else {
            Some(typed)
        };
        match query {
            Some(q) => {
                self.w.copy_search_last = Some((q.clone(), forward));
                self.copy_search_run(&q, forward);
            }
            None => self.request_redraw(),
        }
    }

    fn copy_search_run(&mut self, query: &str, forward: bool) {
        let found = self
            .active_pane()
            .is_some_and(|t| t.session.copy_mode_search(query, forward));
        if !found {
            self.toast(format!("찾지 못했습니다: {query}"));
        }
    }

    /// 글자를 기다리는 `f F t T`가 키를 받는다. 받았으면 참(키는 어디에도 가지 않는다)
    pub(super) fn copy_jump_key(&mut self, event: &winit::event::KeyEvent) -> bool {
        use winit::keyboard::{Key, NamedKey};
        let Some(kind) = self.w.copy_jump_wait else {
            return false;
        };
        if !self.copy_mode_on() {
            self.w.copy_jump_wait = None;
            return false;
        }
        // 수식 키만 눌린 것은 글자가 아니다. 계속 기다린다
        if let Key::Named(
            NamedKey::Shift
            | NamedKey::Control
            | NamedKey::Alt
            | NamedKey::AltGraph
            | NamedKey::Super
            | NamedKey::Meta
            | NamedKey::Hyper
            | NamedKey::CapsLock
            | NamedKey::Fn
            | NamedKey::FnLock,
        ) = &event.logical_key
        {
            return true;
        }
        self.w.copy_jump_wait = None;
        if !self.mods.super_key() && !self.mods.control_key() {
            if let Some(c) = event
                .text
                .as_ref()
                .and_then(|t| t.chars().next())
                .filter(|c| !c.is_control())
            {
                self.copy_jump_char(kind, c);
            }
        }
        true
    }

    pub(super) fn copy_jump_char(&mut self, kind: bt_term::JumpKind, c: char) {
        self.w.copy_jump_last = Some((kind, c));
        if let Some(t) = self.active_pane() {
            t.session.copy_mode_jump(kind, c, false);
        }
        self.request_redraw();
    }

    /// 칸 하나의 복사 모드를 나간다(그 칸이 고른 칸이 아니게 됐거나 닫히기 전)
    pub(super) fn leave_copy_mode_of(&mut self, id: u64) {
        let left = self
            .pane_index(id)
            .is_some_and(|i| self.panes[i].session.copy_mode_exit());
        if left {
            self.drop_copy_table();
            self.request_redraw();
        }
    }

    /// 키를 받기 전에 키 테이블과 칸의 모드 상태가 어긋났으면 맞춘다: 사용자가 `deactivate_key_table`로 테이블만 내렸거나,
    /// 모드 중이던 칸이 사라졌을 때. 모드는 칸이 쥐고 테이블은 창이 쥐어 어긋날 수 있다
    pub(super) fn sync_copy_mode_table(&mut self) {
        let table = self.w.keyseq.has_table(TABLE);
        let on = self.copy_mode_on();
        if table && !on {
            self.drop_copy_table();
        } else if !table && on {
            if let Some(t) = self.active_pane() {
                t.session.copy_mode_exit();
            }
            self.sync_confirm_ime();
            self.request_redraw();
        }
    }
}
