//! 데스크톱 알림. 창을 보고 있지 않을 때만 띄운다.
//!
//! 알림 글은 터미널에 찍힌 글에서 왔으므로 셸 문자열로 잇지 않고 인자 배열로 넘긴다(AGENTS.md "OSC 문자열은 믿지 않습니다").
//! macOS 앱 묶음으로 돌면 `UNUserNotificationCenter`로 띄우고(vendor winit, #625), 알림을 누르면 그 칸으로 간다(Ghostty와 같다).
//! 묶음 없이 돌면(`cargo run`, 시험) 지금처럼 `osascript`다

use std::process::{Command, Stdio};
use std::sync::OnceLock;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::mpsc::{SyncSender, TrySendError, sync_channel};

/// 기다리는 알림의 상한. 넘으면 버린다(출력이 몰려도 프로세스를 끝없이 띄우지 않는다)
const QUEUE: usize = 4;

/// 칸마다 기억하는 알림 ID 수(#625). 넘으면 오래된 것부터 잊는다(알림 센터에는 남지만 누르면 여전히 그 칸으로 간다)
pub const MAX_PANE_NOTES: usize = 16;

/// 띄울 알림 하나: (ID, 제목, 본문)
type Job = (String, String, String);

fn worker() -> &'static SyncSender<Job> {
    static TX: OnceLock<SyncSender<Job>> = OnceLock::new();
    TX.get_or_init(|| {
        let (tx, rx) = sync_channel::<Job>(QUEUE);
        std::thread::Builder::new()
            .name("bt-desktop-notify".into())
            .spawn(move || {
                // 알림은 한 번에 하나씩 띄운다
                for (id, title, body) in rx {
                    deliver(&id, &title, &body);
                }
            })
            .expect("알림 스레드");
        tx
    })
}

fn deliver(id: &str, title: &str, body: &str) {
    #[cfg(target_os = "macos")]
    if winit::platform::macos::post_user_notification(id, title, body) {
        return;
    }
    #[cfg(not(target_os = "macos"))]
    let _ = id;
    let status = command(title, body)
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status();
    if let Err(e) = status {
        log::debug!("데스크톱 알림을 띄우지 못했습니다: {e}");
    }
}

/// 칸이 낸 알림을 띄운다. 기다리지 않는다. 줄에 넣었으면 알림 ID(칸에 기억해 두었다가 그 칸을 보면 지운다, #625).
/// 줄이 차 있으면 버리고 `None`
pub fn notify_pane(pane: u64, title: &str, body: &str) -> Option<String> {
    queue(note_id(Some(pane)), title, body)
}

fn queue(id: String, title: &str, body: &str) -> Option<String> {
    let title = if title.is_empty() { "b-terminal" } else { title };
    match worker().try_send((id.clone(), defang(title), defang(body))) {
        Ok(()) => Some(id),
        Err(TrySendError::Full(_)) | Err(TrySendError::Disconnected(_)) => None,
    }
}

/// 알림 ID: `bt-<pid>-<칸>-<순번>`, 칸이 없으면 `bt-<pid>-app-<순번>`. pid를 넣어 다른 실행이 남긴 알림을 눌러도 칸 번호를
/// 잘못 읽지 않는다(칸 번호는 실행마다 다시 센다)
fn format_note_id(pid: u32, pane: Option<u64>, seq: u64) -> String {
    match pane {
        Some(p) => format!("bt-{pid}-{p}-{seq}"),
        None => format!("bt-{pid}-app-{seq}"),
    }
}

/// `format_note_id`가 만든 ID이고 pid가 `pid`면 칸 번호. 숫자는 십진 숫자만 받는다
fn parse_note_id(id: &str, pid: u32) -> Option<u64> {
    let digits = |s: &str| !s.is_empty() && s.bytes().all(|b| b.is_ascii_digit());
    let mut parts = id.strip_prefix("bt-")?.split('-');
    let (p, pane, seq) = (parts.next()?, parts.next()?, parts.next()?);
    if parts.next().is_some() || !digits(p) || !digits(pane) || !digits(seq) {
        return None;
    }
    if p.parse::<u32>().ok()? != pid {
        return None;
    }
    pane.parse().ok()
}

fn note_id(pane: Option<u64>) -> String {
    static SEQ: AtomicU64 = AtomicU64::new(1);
    format_note_id(std::process::id(), pane, SEQ.fetch_add(1, Ordering::Relaxed))
}

/// 이 실행이 낸 칸 알림의 ID면 그 칸 번호(알림을 눌렀을 때, #625)
pub fn pane_of_note(id: &str) -> Option<u64> {
    parse_note_id(id, std::process::id())
}

/// 칸의 알림 ID 목록에 더한다. `MAX_PANE_NOTES`를 넘으면 오래된 것을 잊는다
pub fn remember_note(notes: &mut Vec<String>, id: String) {
    notes.push(id);
    if notes.len() > MAX_PANE_NOTES {
        let extra = notes.len() - MAX_PANE_NOTES;
        notes.drain(..extra);
    }
}

/// 알림 센터에 남은 알림을 지운다(그 칸을 봤거나 닫았다). 묶음 없이 돌면 아무것도 하지 않는다
pub fn forget(ids: Vec<String>) {
    #[cfg(target_os = "macos")]
    winit::platform::macos::remove_user_notifications(&ids);
    #[cfg(not(target_os = "macos"))]
    let _ = ids;
}

/// 앱이 끝난다: 이 앱이 낸 알림을 모두 지운다(Ghostty `applicationWillTerminate`와 같다)
pub fn forget_all() {
    #[cfg(target_os = "macos")]
    winit::platform::macos::remove_all_user_notifications();
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

    // #625: 칸 알림의 ID에서 칸을 되찾는다. 다른 실행(다른 pid)이 남긴 ID와 엉터리 ID는 버린다
    #[test]
    fn note_id_round_trips_to_its_pane_in_the_same_process() {
        let id = format_note_id(4242, Some(7), 3);
        assert_eq!(id, "bt-4242-7-3");
        assert_eq!(parse_note_id(&id, 4242), Some(7));
    }

    #[test]
    fn note_id_from_another_run_or_garbage_is_ignored() {
        assert_eq!(parse_note_id("bt-4242-7-3", 4243), None);
        assert_eq!(parse_note_id(&format_note_id(4242, None, 1), 4242), None);
        for bad in [
            "",
            "bt-",
            "bt-4242",
            "bt-4242-7",
            "bt-4242-x-1",
            "bt-4242-7-1-9",
            "xx-4242-7-1",
            "bt-4242--1",
            "bt-4242-+7-1",
        ] {
            assert_eq!(parse_note_id(bad, 4242), None, "{bad:?}");
        }
    }

    #[test]
    fn note_ids_are_unique_and_carry_this_process() {
        let a = note_id(Some(1));
        let b = note_id(Some(1));
        assert_ne!(a, b);
        assert_eq!(pane_of_note(&a), Some(1));
    }

    #[test]
    fn remembered_notes_are_capped_oldest_first() {
        let mut notes = Vec::new();
        for i in 0..(MAX_PANE_NOTES + 3) {
            remember_note(&mut notes, format!("n{i}"));
        }
        assert_eq!(notes.len(), MAX_PANE_NOTES);
        assert_eq!(notes.first().map(String::as_str), Some("n3"));
    }
}
