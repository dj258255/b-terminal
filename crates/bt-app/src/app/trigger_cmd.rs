//! 트리거(#928, ADR-102)의 앱 쪽 연결. 줄 검사와 판단은 `bt_term::triggers`가 하고, 여기서는 사건을 탭 표시로 바꾸는 일만 한다.
//! 알림은 기존 알림 길(`SessionEvent::Notification`), 벨은 기존 벨 길(`SessionEvent::Bell`)을 그대로 탄다. 어느 쪽도 PTY에 쓰지 않는다.

use super::App;

impl App {
    /// 트리거 `attention`: 지금 보고 있는 칸이 아니면 탭에 주의 표시(안 읽음)를 단다. 소리도 데스크톱 알림도 없다
    pub(super) fn on_trigger_attention(&mut self, idx: usize) {
        let watching = self.w.focused && self.is_visible(idx);
        if !watching {
            self.panes[idx].unread = true;
        }
    }

    /// 설정의 `trigger`를 모든 칸에 다시 건다(설정 다시 읽기). 못 읽는 규칙은 경고만 남긴다
    pub(super) fn sync_triggers(&self) {
        for t in &self.panes {
            for w in t.session.set_triggers(&self.config.triggers) {
                log::warn!("설정: trigger를 무시합니다({w})");
            }
        }
    }
}
