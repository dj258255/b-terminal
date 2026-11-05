//! 데스크톱 알림. 창을 보고 있지 않을 때만 띄운다.
//!
//! 알림 글은 터미널에 찍힌 글에서 왔으므로 셸 문자열로 잇지 않고 인자 배열로 넘긴다(AGENTS.md "OSC 문자열은 믿지 않습니다")

use std::process::{Command, Stdio};

/// 알림을 띄운다. 기다리지 않는다
pub fn notify(title: &str, body: &str) {
    let title = if title.is_empty() {
        "b-terminal".to_string()
    } else {
        title.to_string()
    };
    let body = body.to_string();
    std::thread::spawn(move || {
        let status = command(&title, &body)
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status();
        if let Err(e) = status {
            log::debug!("데스크톱 알림을 띄우지 못했습니다: {e}");
        }
    });
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hostile_text_stays_a_single_argument() {
        let evil = "\" & do shell script \"touch /tmp/pwned\" & \"";
        let c = command("t", evil);
        let args: Vec<_> = c.get_args().map(|a| a.to_string_lossy().into_owned()).collect();
        assert_eq!(args.last().map(String::as_str), Some(evil));
        assert!(!args.iter().any(|a| a.contains("touch") && a != evil));
    }
}
