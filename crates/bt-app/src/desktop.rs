//! 데스크톱 알림. 창을 보고 있지 않을 때만 띄운다.
//!
//! 알림 글은 터미널에 찍힌 글에서 왔으므로 셸 문자열로 잇지 않고 인자 배열로 넘긴다(AGENTS.md "OSC 문자열은 믿지 않습니다")

use std::process::{Command, Stdio};
use std::sync::OnceLock;
use std::sync::mpsc::{SyncSender, TrySendError, sync_channel};

/// 기다리는 알림의 상한. 넘으면 버린다(출력이 몰려도 프로세스를 끝없이 띄우지 않는다)
const QUEUE: usize = 4;

fn worker() -> &'static SyncSender<(String, String)> {
    static TX: OnceLock<SyncSender<(String, String)>> = OnceLock::new();
    TX.get_or_init(|| {
        let (tx, rx) = sync_channel::<(String, String)>(QUEUE);
        std::thread::Builder::new()
            .name("bt-desktop-notify".into())
            .spawn(move || {
                // 알림 프로세스는 한 번에 하나만 띄운다
                for (title, body) in rx {
                    let status = command(&title, &body)
                        .stdout(Stdio::null())
                        .stderr(Stdio::null())
                        .status();
                    if let Err(e) = status {
                        log::debug!("데스크톱 알림을 띄우지 못했습니다: {e}");
                    }
                }
            })
            .expect("알림 스레드");
        tx
    })
}

/// 알림을 띄운다. 기다리지 않는다. 줄이 차 있으면 버리고 거짓
pub fn notify(title: &str, body: &str) -> bool {
    let title = if title.is_empty() { "b-terminal" } else { title };
    match worker().try_send((defang(title), defang(body))) {
        Ok(()) => true,
        Err(TrySendError::Full(_)) | Err(TrySendError::Disconnected(_)) => false,
    }
}

/// 인자 앞의 '-'를 무력화한다. `--`를 모르는 도구가 옵션으로 읽지 않게 하는 두 번째 방어
fn defang(s: &str) -> String {
    if s.starts_with('-') {
        format!("\u{2060}{s}")
    } else {
        s.to_string()
    }
}

#[cfg(target_os = "macos")]
fn command(title: &str, body: &str) -> Command {
    // AppleScript 안에서 글을 잇지 않고 run 핸들러의 인자로 받는다
    let mut c = Command::new("osascript");
    c.args([
        "-e",
        "on run argv",
        "-e",
        "display notification (item 2 of argv) with title (item 1 of argv)",
        "-e",
        "end run",
        // 여기부터는 스크립트 인자다. 제목이 "-e"여도 옵션으로 읽지 않는다
        "--",
        title,
        body,
    ]);
    c
}

#[cfg(not(target_os = "macos"))]
fn command(title: &str, body: &str) -> Command {
    let mut c = Command::new("notify-send");
    c.args(["--app-name=b-terminal", "--", title, body]);
    c
}

/// 링크를 연다(#38). 사용자가 Cmd-클릭했을 때만 부른다. 주소는 `classify`를 통과한 것이고, 셸을 거치지 않고 인자로 넘긴다
pub fn open_link(target: &bt_term::links::OpenTarget) {
    use bt_term::links::OpenTarget;
    let mut cmd = if cfg!(target_os = "macos") {
        Command::new("open")
    } else {
        Command::new("xdg-open")
    };
    match target {
        OpenTarget::Web(url) => cmd.arg(url),
        OpenTarget::Open(path) => cmd.arg(path),
        // 열면 실행되는 파일은 Finder에서 보이기만 한다(Linux는 그 폴더를 연다)
        OpenTarget::Reveal(path) if cfg!(target_os = "macos") => cmd.arg("-R").arg(path),
        OpenTarget::Reveal(path) => cmd.arg(path.parent().unwrap_or(path)),
    };
    // 기다리지 않는다. 끝난 프로세스는 따로 거두는 스레드가 기다린다
    match cmd
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
    {
        Ok(mut child) => {
            std::thread::spawn(move || child.wait());
        }
        Err(e) => log::warn!("링크를 열지 못했습니다: {e}"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn leading_dash_cannot_become_an_option() {
        let c = command(&defang("-e"), &defang("do shell script \"x\""));
        let args: Vec<_> = c.get_args().map(|a| a.to_string_lossy().into_owned()).collect();
        let sep = args.iter().position(|a| a == "--").expect("-- 구분자");
        assert!(args[sep + 1..].iter().all(|a| !a.starts_with('-')));
    }

    #[test]
    fn hostile_text_stays_a_single_argument() {
        let evil = "\" & do shell script \"touch /tmp/pwned\" & \"";
        let c = command("t", evil);
        let args: Vec<_> = c.get_args().map(|a| a.to_string_lossy().into_owned()).collect();
        assert_eq!(args.last().map(String::as_str), Some(evil));
        assert!(!args.iter().any(|a| a.contains("touch") && a != evil));
    }
}
