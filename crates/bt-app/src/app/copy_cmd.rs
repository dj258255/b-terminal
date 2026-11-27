//! vi 복사 모드(#930)의 앱 쪽 연결. 커서 움직임·고르기는 `bt_term::copy_mode`, 키 테이블과 동작 이름은 `crate::copy_mode`가 한다.
//! 여기는 들어가기·나가기·복사, 입력기 끄기, 키 테이블과 칸의 상태 맞추기만 한다

use super::App;
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
        self.sync_confirm_ime();
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
