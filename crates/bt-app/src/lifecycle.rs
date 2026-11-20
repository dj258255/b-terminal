//! 명령 실행·창 수명 판단(#178). 창과 상관없는 순수 함수라 여기서 테스트한다

use std::time::Duration;

/// 칸의 명령이 끝났을 때 할 일
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExitAction {
    /// 칸을 닫는다(Ghostty 기본)
    Close,
    /// 칸을 남기고 키를 기다린다
    Hold(HoldReason),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HoldReason {
    /// `abnormal-command-exit-runtime`보다 짧게 끝났다
    Abnormal,
    /// `wait-after-command`
    Waiting,
}

/// 명령이 끝났을 때 칸을 닫을지 남길지. 비정상(너무 빨리 끝남)이 `wait-after-command`보다 앞선다.
/// Ghostty처럼 Linux에서는 종료 코드가 0이 아닐 때만 비정상으로 보고, macOS는 어떤 코드든 본다
pub fn exit_action(
    wait_after: bool,
    runtime: Duration,
    threshold_ms: u32,
    code: Option<u32>,
    any_code_is_abnormal: bool,
) -> ExitAction {
    let quick = runtime < Duration::from_millis(threshold_ms as u64);
    if quick && (any_code_is_abnormal || code != Some(0)) {
        ExitAction::Hold(HoldReason::Abnormal)
    } else if wait_after {
        ExitAction::Hold(HoldReason::Waiting)
    } else {
        ExitAction::Close
    }
}

/// 남긴 칸에 그려 줄 안내. 줄 바꿈은 `\r\n`
pub fn hold_message(reason: HoldReason, code: Option<u32>, runtime: Duration) -> String {
    let code = code.map_or("알 수 없음".to_string(), |c| c.to_string());
    match reason {
        HoldReason::Abnormal => format!(
            "\r\n명령이 너무 빨리 끝났습니다(종료 코드 {code}, 실행 {}ms).\r\n\
             명령을 찾지 못했거나 인자가 틀렸을 수 있습니다.\r\n\
             아무 키나 누르면 이 칸을 닫습니다.\r\n",
            runtime.as_millis()
        ),
        HoldReason::Waiting => {
            format!("\r\n명령이 끝났습니다(종료 코드 {code}).\r\n아무 키나 누르면 이 칸을 닫습니다.\r\n")
        }
    }
}

/// 마지막 창이 닫혔을 때 앱이 할 일
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LastWindow {
    Stay,
    QuitNow,
    QuitAfter(Duration),
}

/// `quit-after-last-window-closed`와 `-delay`(최소 1초로 올린다)로 정한다
pub fn last_window_closed(quit: bool, delay: Option<Duration>) -> LastWindow {
    match (quit, delay) {
        (false, _) => LastWindow::Stay,
        (true, None) => LastWindow::QuitNow,
        (true, Some(d)) => LastWindow::QuitAfter(d.max(Duration::from_secs(1))),
    }
}

/// 첫 칸에 쓸 설정 명령: `initial-command`가 `command`보다 앞선다
pub fn first_command<'a>(initial: Option<&'a str>, command: Option<&'a str>) -> Option<&'a str> {
    initial.or(command)
}

/// 설정의 명령 글을 셸을 거쳐 띄우는 (프로그램, 인자)로 바꾼다(Ghostty처럼 `/bin/sh -c`)
pub fn shell_command(cmd: &str) -> (String, Vec<String>) {
    ("/bin/sh".into(), vec!["-c".into(), format!("exec {cmd}")])
}

/// 첫 창 없이 띄울까: `initial-window = false`이고 명령줄 `-e`가 없을 때(`-e`는 띄울 창이 있어야 한다)
pub fn start_without_window(initial_window: bool, cli_command: bool) -> bool {
    !initial_window && !cli_command
}

/// 입력기가 확정한 글(`Ime::Commit`)이 남은 칸을 닫는가: 보낼 바이트가 있으면 닫는다(#715).
/// 한국어 입력기는 스페이스를 `KeyboardInput`이 아니라 확정으로 보낸다. 조합 중인 글자는 확정이 아니라 닫지 않는다(Ghostty와 같음)
pub fn is_dismiss_commit(text: &str) -> bool {
    !text.is_empty()
}

/// 남은 칸을 닫는 "아무 키"인가: 수정 키만 누른 것은 세지 않는다
pub fn is_dismiss_key(key: &winit::keyboard::Key) -> bool {
    use winit::keyboard::{Key, NamedKey};
    !matches!(
        key,
        Key::Named(
            NamedKey::Shift
                | NamedKey::Control
                | NamedKey::Alt
                | NamedKey::AltGraph
                | NamedKey::Super
                | NamedKey::Meta
                | NamedKey::Hyper
                | NamedKey::CapsLock
                | NamedKey::NumLock
                | NamedKey::ScrollLock
                | NamedKey::Fn
                | NamedKey::FnLock
        ) | Key::Dead(_)
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use winit::keyboard::{Key, NamedKey};

    fn ms(n: u64) -> Duration {
        Duration::from_millis(n)
    }

    #[test]
    fn normal_exit_closes() {
        assert_eq!(exit_action(false, ms(5000), 250, Some(0), true), ExitAction::Close);
        assert_eq!(
            exit_action(false, ms(250), 250, Some(0), true),
            ExitAction::Close,
            "기준과 같으면 정상"
        );
    }

    #[test]
    fn quick_exit_is_abnormal() {
        assert_eq!(
            exit_action(false, ms(249), 250, Some(0), true),
            ExitAction::Hold(HoldReason::Abnormal),
            "macOS는 코드 0이어도 비정상"
        );
        assert_eq!(
            exit_action(false, ms(10), 250, Some(0), false),
            ExitAction::Close,
            "Linux는 코드가 0이면 비정상이 아니다"
        );
        assert_eq!(
            exit_action(false, ms(10), 250, Some(1), false),
            ExitAction::Hold(HoldReason::Abnormal)
        );
        assert_eq!(
            exit_action(false, ms(10), 250, None, false),
            ExitAction::Hold(HoldReason::Abnormal),
            "코드를 모르면 0이 아닌 것으로 본다"
        );
        assert_eq!(
            exit_action(false, ms(10), 0, Some(1), true),
            ExitAction::Close,
            "0이면 판단을 끈다"
        );
    }

    #[test]
    fn wait_after_command_holds_and_abnormal_wins() {
        assert_eq!(
            exit_action(true, ms(5000), 250, Some(0), true),
            ExitAction::Hold(HoldReason::Waiting)
        );
        assert_eq!(
            exit_action(true, ms(10), 250, Some(0), true),
            ExitAction::Hold(HoldReason::Abnormal)
        );
    }

    #[test]
    fn messages_name_code_and_runtime() {
        let m = hold_message(HoldReason::Abnormal, Some(127), ms(12));
        assert!(m.contains("127") && m.contains("12ms") && m.contains("아무 키"), "{m}");
        let m = hold_message(HoldReason::Waiting, Some(0), ms(5000));
        assert!(m.contains("아무 키") && m.ends_with("\r\n"), "{m:?}");
        assert_eq!(
            m.matches('\n').count(),
            m.matches("\r\n").count(),
            "줄 바꿈은 모두 CRLF"
        );
    }

    #[test]
    fn last_window_decisions() {
        assert_eq!(last_window_closed(false, None), LastWindow::Stay);
        assert_eq!(last_window_closed(false, Some(ms(5000))), LastWindow::Stay);
        assert_eq!(last_window_closed(true, None), LastWindow::QuitNow);
        assert_eq!(
            last_window_closed(true, Some(ms(5000))),
            LastWindow::QuitAfter(ms(5000))
        );
        assert_eq!(
            last_window_closed(true, Some(ms(100))),
            LastWindow::QuitAfter(Duration::from_secs(1)),
            "1초보다 짧으면 1초로 올린다"
        );
    }

    #[test]
    fn first_command_prefers_initial() {
        assert_eq!(first_command(Some("a"), Some("b")), Some("a"));
        assert_eq!(first_command(None, Some("b")), Some("b"));
        assert_eq!(first_command(None, None), None);
    }

    #[test]
    fn shell_command_wraps_with_sh() {
        assert_eq!(
            shell_command("htop -d 5"),
            (
                "/bin/sh".to_string(),
                vec!["-c".to_string(), "exec htop -d 5".to_string()]
            )
        );
    }

    #[test]
    fn window_less_start() {
        assert!(start_without_window(false, false));
        assert!(!start_without_window(false, true), "-e로 띄우면 창이 있어야 한다");
        assert!(!start_without_window(true, false));
    }

    #[test]
    fn modifier_keys_do_not_dismiss() {
        assert!(!is_dismiss_key(&Key::Named(NamedKey::Shift)));
        assert!(!is_dismiss_key(&Key::Named(NamedKey::Super)));
        assert!(!is_dismiss_key(&Key::Named(NamedKey::CapsLock)));
        assert!(is_dismiss_key(&Key::Named(NamedKey::Enter)));
        assert!(is_dismiss_key(&Key::Character("a".into())));
    }

    /// 입력기가 확정한 글(한국어 입력 소스의 스페이스 등, `Ime::Commit`)도 보낼 바이트라 남은 칸을 닫는다(#715).
    /// Ghostty는 `insertText`로 온 글도 `keyCallback`의 `encodeKey`를 지나 바이트가 생기면 닫는다
    #[test]
    fn committed_text_dismisses_held_pane() {
        assert!(is_dismiss_commit(" "), "한국어 입력기의 스페이스");
        assert!(is_dismiss_commit("가"), "조합을 마친 글자");
        assert!(!is_dismiss_commit(""), "빈 확정은 바이트가 없다");
    }
}
