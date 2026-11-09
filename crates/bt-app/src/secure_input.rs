//! 보안 키보드 입력(#258, ADR-020). macOS의 Carbon `EnableSecureEventInput`은 켜 있는 동안 키 입력이 다른 앱에
//! 새지 않게 한다. 이 호출은 전역이고 켠 횟수와 끈 횟수가 맞아야 해서(Ghostty macos/Sources/Features/SecureInput.swift),
//! 상태를 한 곳에서 든다. 시스템 호출은 주입해서 호출 순서와 횟수를 테스트한다

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use bt_render::CursorStyle;
use bt_term::CursorShape;

/// 보안 입력 상태 기계. 켜고 싶은 상태(`desired`)는 "앱 전체로 켬(`global`)이거나 초점인 암호 칸이 하나라도 있음"이고,
/// 앱이 활성일 때만 시스템에 반영한다. 호출이 실패하면 상태를 바꾸지 않는다(다음 계기에 다시 시도한다)
pub struct SecureInput {
    call: Box<dyn FnMut(bool) -> bool>,
    global: bool,
    /// 칸 단위로 켠 칸(id)과 그 칸이 지금 초점인가
    scoped: HashMap<u64, bool>,
    active: bool,
    enabled: bool,
}

impl SecureInput {
    /// `call(true)`는 켜기, `call(false)`는 끄기. 성공하면 true
    pub fn new(call: Box<dyn FnMut(bool) -> bool>, global: bool) -> Self {
        Self {
            call,
            global,
            scoped: HashMap::new(),
            active: false,
            enabled: false,
        }
    }

    /// 지금 시스템에 켜 둔 상태인가(표시가 이걸 따른다)
    pub fn enabled(&self) -> bool {
        self.enabled
    }

    pub fn global(&self) -> bool {
        self.global
    }

    fn desired(&self) -> bool {
        self.global || self.scoped.values().any(|&focused| focused)
    }

    /// 앱이 활성일 때만, 바라는 상태와 다르면 한 번 부른다. 실패하면 그대로 둔다
    fn apply(&mut self) {
        if !self.active {
            return;
        }
        let want = self.desired();
        if self.enabled != want && (self.call)(want) {
            self.enabled = want;
        }
    }

    pub fn set_global(&mut self, on: bool) {
        self.global = on;
        self.apply();
    }

    /// 칸 단위 상태가 켜 있는가
    pub fn has_scoped(&self, id: u64) -> bool {
        self.scoped.contains_key(&id)
    }

    /// 칸을 칸 단위 상태로 넣거나(초점 여부와 함께) 갱신한다
    pub fn set_scoped(&mut self, id: u64, focused: bool) {
        self.scoped.insert(id, focused);
        self.apply();
    }

    pub fn remove_scoped(&mut self, id: u64) {
        self.scoped.remove(&id);
        self.apply();
    }

    /// 칸 단위 상태를 가진 모든 칸의 초점 여부를 한 번에 맞춘다. `state(id)`는 칸이 없어졌으면 `None`, 있으면 초점 여부
    pub fn sync_scoped(&mut self, state: impl Fn(u64) -> Option<bool>) {
        let ids: Vec<u64> = self.scoped.keys().copied().collect();
        for id in ids {
            match state(id) {
                Some(f) => {
                    self.scoped.insert(id, f);
                }
                None => {
                    self.scoped.remove(&id);
                }
            }
        }
        self.apply();
    }

    /// 앱이 활성이 되었거나 비활성이 되었다. 비활성이 되면 다른 앱에 영향이 없게 끄고, 활성이 되면 다시 켠다
    pub fn set_active(&mut self, active: bool) {
        self.active = active;
        if active {
            self.apply();
        } else if self.enabled && (self.call)(false) {
            self.enabled = false;
        }
    }

    /// 앱이 끝나기 전: 켜 둔 것이 있으면 끈다
    pub fn shutdown(&mut self) {
        self.global = false;
        self.scoped.clear();
        if self.enabled && (self.call)(false) {
            self.enabled = false;
        }
    }
}

/// 시스템 호출(macOS의 Carbon `EnableSecureEventInput`·`DisableSecureEventInput`). 다른 곳에서는 할 수 있는 일이 없어
/// 실패로 돌려준다(그러면 켜졌다고 하지 않는다)
pub fn system_call(enable: bool) -> bool {
    #[cfg(target_os = "macos")]
    {
        winit::platform::macos::set_secure_event_input(enable)
    }
    #[cfg(not(target_os = "macos"))]
    {
        let _ = enable;
        false
    }
}

/// 앱 전체 켜짐을 다음 실행에 남기는 파일(상태 파일과 같은 폴더)
pub fn global_path() -> Option<PathBuf> {
    crate::state::path().map(|p| p.with_file_name("secure-input"))
}

/// 파일에 `1`이 있으면 켜짐. 없거나 읽지 못하면 꺼짐
pub fn load_global(path: &Path) -> bool {
    std::fs::read_to_string(path).is_ok_and(|t| t.trim() == "1")
}

/// 적는다. 끄면 파일을 지운다
pub fn save_global(path: &Path, on: bool) -> std::io::Result<()> {
    if !on {
        return match std::fs::remove_file(path) {
            Err(e) if e.kind() != std::io::ErrorKind::NotFound => Err(e),
            _ => Ok(()),
        };
    }
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir)?;
    }
    std::fs::write(path, "1\n")
}

/// 한 번 살핀 뒤 칸의 암호 입력 상태. 프로세스가 끝난 칸은 더 칠 암호가 없으므로 꺼짐(끝난 칸에 보안 입력이 남지
/// 않게). termios를 읽지 못하면 그대로 둔다
pub fn password_after_poll(prev: bool, exited: bool, polled: Option<bool>) -> bool {
    if exited {
        return false;
    }
    polled.unwrap_or(prev)
}

/// 칸의 커서 모양(Ghostty renderer/cursor.zig `style`의 순서): 조합 중이면 네모, 암호 입력이면 자물쇠(커서를 숨기는
/// 상태보다 앞), 숨겼으면 없음, 초점이 없으면 속 빈 네모, 깜빡임에서 꺼진 때는 없음, 아니면 프로그램이 정한 모양
pub fn cursor_style(
    password: bool,
    preedit: bool,
    visible: bool,
    focused: bool,
    blink_hidden: bool,
    shape: CursorShape,
) -> Option<CursorStyle> {
    if preedit {
        return Some(CursorStyle::Block);
    }
    if password {
        return Some(CursorStyle::Lock);
    }
    if !visible {
        return None;
    }
    if !focused {
        return Some(CursorStyle::Hollow);
    }
    if blink_hidden {
        return None;
    }
    Some(match shape {
        CursorShape::Block => CursorStyle::Block,
        CursorShape::Beam => CursorStyle::Beam,
        CursorShape::Underline => CursorStyle::Underline,
        CursorShape::HollowBlock => CursorStyle::Hollow,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn exited_pane_leaves_password_input() {
        assert!(!password_after_poll(true, true, None), "끝난 칸은 암호 입력이 아니다");
        assert!(!password_after_poll(true, true, Some(true)));
        assert!(password_after_poll(false, false, Some(true)));
        assert!(!password_after_poll(true, false, Some(false)));
        assert!(password_after_poll(true, false, None), "읽지 못하면 그대로");
    }
    use std::cell::RefCell;
    use std::rc::Rc;

    /// 호출을 적어 두는 가짜 시스템. `fail`이 켜 있으면 호출이 실패한다
    struct Fake {
        log: Rc<RefCell<Vec<bool>>>,
        fail: Rc<RefCell<bool>>,
    }

    fn fake(global: bool) -> (SecureInput, Fake) {
        let log = Rc::new(RefCell::new(Vec::new()));
        let fail = Rc::new(RefCell::new(false));
        let (l, f) = (log.clone(), fail.clone());
        let si = SecureInput::new(
            Box::new(move |on| {
                l.borrow_mut().push(on);
                !*f.borrow()
            }),
            global,
        );
        (si, Fake { log, fail })
    }

    impl Fake {
        fn calls(&self) -> Vec<bool> {
            self.log.borrow().clone()
        }
    }

    #[test]
    fn nothing_is_called_while_the_app_is_inactive() {
        let (mut si, f) = fake(false);
        si.set_scoped(1, true);
        si.set_global(true);
        assert!(f.calls().is_empty(), "활성이 아니면 시스템을 부르지 않는다");
        assert!(!si.enabled());
        si.set_active(true);
        assert_eq!(f.calls(), vec![true], "활성이 되면 한 번 켠다");
        assert!(si.enabled());
    }

    #[test]
    fn focused_password_pane_turns_it_on_and_off_once() {
        let (mut si, f) = fake(false);
        si.set_active(true);
        si.set_scoped(7, true);
        si.set_scoped(7, true);
        assert_eq!(f.calls(), vec![true], "같은 상태를 다시 알려도 다시 켜지 않는다");
        si.set_scoped(7, false);
        assert_eq!(f.calls(), vec![true, false], "초점을 잃으면 끈다");
        assert!(!si.enabled());
        si.set_scoped(7, true);
        si.remove_scoped(7);
        assert_eq!(f.calls(), vec![true, false, true, false]);
    }

    #[test]
    fn two_password_panes_keep_it_on_until_the_last_loses_focus() {
        let (mut si, f) = fake(false);
        si.set_active(true);
        si.set_scoped(1, true);
        si.set_scoped(2, true);
        assert_eq!(f.calls(), vec![true], "하나만 켜 두면 된다");
        si.set_scoped(1, false);
        assert_eq!(f.calls(), vec![true], "다른 칸이 아직 초점이다");
        si.set_scoped(2, false);
        assert_eq!(f.calls(), vec![true, false]);
    }

    #[test]
    fn global_stays_on_without_any_pane_and_off_pane_does_not_turn_it_off() {
        let (mut si, f) = fake(false);
        si.set_active(true);
        si.set_global(true);
        si.set_scoped(1, true);
        si.set_scoped(1, false);
        assert_eq!(f.calls(), vec![true], "앱 전체 켜짐이면 칸 상태와 상관없이 켜 둔다");
        si.set_global(false);
        assert_eq!(f.calls(), vec![true, false]);
    }

    #[test]
    fn resign_turns_it_off_and_become_active_turns_it_back_on() {
        let (mut si, f) = fake(true);
        si.set_active(true);
        si.set_active(false);
        assert!(!si.enabled());
        si.set_active(false);
        si.set_active(true);
        si.set_active(true);
        assert_eq!(f.calls(), vec![true, false, true], "켠 횟수와 끈 횟수가 맞는다");
    }

    #[test]
    fn a_failed_call_leaves_the_state_and_is_retried_later() {
        let (mut si, f) = fake(false);
        si.set_active(true);
        *f.fail.borrow_mut() = true;
        si.set_global(true);
        assert!(!si.enabled(), "실패하면 켜졌다고 하지 않는다");
        *f.fail.borrow_mut() = false;
        si.set_global(true);
        assert_eq!(f.calls(), vec![true, true]);
        assert!(si.enabled());
        // 끄기가 실패해도 켜 둔 채로 남는다
        *f.fail.borrow_mut() = true;
        si.set_global(false);
        assert!(si.enabled());
    }

    #[test]
    fn sync_scoped_follows_focus_and_drops_closed_panes() {
        let (mut si, f) = fake(false);
        si.set_active(true);
        si.set_scoped(1, false);
        si.set_scoped(2, false);
        assert!(f.calls().is_empty());
        si.sync_scoped(|id| (id == 1).then_some(true));
        assert!(si.has_scoped(1));
        assert!(!si.has_scoped(2), "닫힌 칸은 뺀다");
        assert_eq!(f.calls(), vec![true]);
        si.sync_scoped(|_| None);
        assert_eq!(f.calls(), vec![true, false], "칸이 닫히면 끈다");
    }

    #[test]
    fn shutdown_releases_what_it_holds() {
        let (mut si, f) = fake(true);
        si.set_active(true);
        si.shutdown();
        assert_eq!(f.calls(), vec![true, false]);
        si.shutdown();
        assert_eq!(f.calls(), vec![true, false], "이미 껐으면 다시 부르지 않는다");
    }

    #[test]
    fn global_flag_survives_a_restart_through_its_file() {
        let dir = std::env::temp_dir().join(format!("bt-secure-input-test-{}", std::process::id()));
        let path = dir.join("secure-input");
        let _ = std::fs::remove_dir_all(&dir);
        assert!(!load_global(&path), "파일이 없으면 꺼짐");
        std::fs::create_dir_all(&dir).unwrap();
        save_global(&path, true).unwrap();
        assert!(load_global(&path));
        save_global(&path, false).unwrap();
        assert!(!load_global(&path));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn cursor_style_follows_ghosttys_priority() {
        use CursorShape::*;
        use bt_render::CursorStyle as S;
        let st = |pw, pre, vis, foc, hid, shape| cursor_style(pw, pre, vis, foc, hid, shape);
        // 암호 입력이면 초점이 없어도, 커서를 숨기는 상태여도, 깜빡임에서 꺼져도 자물쇠
        assert_eq!(st(true, false, true, true, false, Block), Some(S::Lock));
        assert_eq!(st(true, false, false, true, false, Block), Some(S::Lock));
        assert_eq!(st(true, false, true, false, false, Beam), Some(S::Lock));
        assert_eq!(st(true, false, true, true, true, Beam), Some(S::Lock));
        // 조합 중이면 네모가 먼저
        assert_eq!(st(true, true, true, true, false, Beam), Some(S::Block));
        // 암호가 아닐 때는 기존 규칙
        assert_eq!(st(false, false, false, true, false, Block), None);
        assert_eq!(st(false, false, true, false, false, Beam), Some(S::Hollow));
        assert_eq!(st(false, false, true, true, true, Block), None);
        assert_eq!(st(false, false, true, true, false, Beam), Some(S::Beam));
        assert_eq!(st(false, false, true, true, false, Underline), Some(S::Underline));
        assert_eq!(st(false, false, true, true, false, Block), Some(S::Block));
        assert_eq!(st(false, false, true, true, false, HollowBlock), Some(S::Hollow));
    }
}
