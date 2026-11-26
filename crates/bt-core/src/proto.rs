//! 제어 소켓 프로토콜. 한 줄에 JSON 하나(요청 → 응답). btctl과 훅이 쓴다.
//!
//! 소켓은 같은 사용자만 닿는 폴더(0700)에 있다. 소켓에 붙으면 탭에 글을 넣을 수 있으므로 그 사용자의 셸 권한과 같다(SECURITY.md)

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

/// 요청 한 줄의 상한(바이트). 읽기 전에 본다
pub const MAX_LINE: usize = 1024 * 1024;

/// 앱이 자식 셸에 넘기는 환경 변수
pub const ENV_SOCKET: &str = "BT_SOCKET";
pub const ENV_TAB: &str = "BT_TAB_ID";

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Request {
    #[serde(default)]
    pub id: u64,
    #[serde(flatten)]
    pub call: Call,
}

/// 요청 종류. `tab`이 없으면 부른 쪽의 탭(BT_TAB_ID), 그것도 없으면 지금 보는 탭
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "method", content = "params", rename_all = "snake_case")]
pub enum Call {
    Ping,
    /// 탭 목록
    List,
    /// 퀵 터미널을 보이고 숨긴다(#95)
    QuickTerminal,
    /// 새 탭. `command`가 비면 로그인 셸.
    /// `split`(right·down·left·up·auto)을 주면 새 탭 대신 칸 `tab`(없으면 지금 칸)을 나눠 그쪽에 연다
    New {
        #[serde(default)]
        cwd: Option<PathBuf>,
        #[serde(default)]
        command: Vec<String>,
        #[serde(default)]
        split: Option<String>,
        #[serde(default)]
        tab: Option<u64>,
        /// 새 탭 대신 새 창에 연다(#36)
        #[serde(default)]
        window: bool,
    },
    Focus {
        tab: u64,
    },
    /// 탭에 글을 넣는다(키보드로 친 것처럼)
    Send {
        #[serde(default)]
        tab: Option<u64>,
        text: String,
    },
    /// 탭 화면을 글로 읽는다
    Read {
        #[serde(default)]
        tab: Option<u64>,
        /// 맨 아래에서 위로 이만큼 줄(스크롤백 포함). 없으면 보이는 화면만(#66)
        #[serde(default)]
        lines: Option<usize>,
    },
    /// 탭의 글(화면과 최근 스크롤백)에 `text`가 나올 때까지 기다린다(#66). 이미 있으면 바로 답한다
    Wait {
        #[serde(default)]
        tab: Option<u64>,
        text: String,
        #[serde(default = "default_run_timeout")]
        timeout_ms: u64,
    },
    /// 탭에 명령을 넣고, 셸 통합이 그 명령의 끝(OSC 133;D)을 알릴 때까지 기다려 출력과 종료 코드를 돌려준다.
    /// 에이전트가 다른 탭을 도구로 쓰는 길이다
    Run {
        #[serde(default)]
        tab: Option<u64>,
        command: String,
        #[serde(default = "default_run_timeout")]
        timeout_ms: u64,
    },
    /// 탭에서 마지막으로 끝난 명령(셸 통합)
    LastCommand {
        #[serde(default)]
        tab: Option<u64>,
    },
    /// 설정 파일을 다시 읽는다(키 묶음 reload_config와 같다)
    ReloadConfig,
    /// 칸을 닫는다(AppleScript `close`와 같게 확인 막대 없이, 되살리기 목록에도 넣지 않는다, #858)
    Close {
        #[serde(default)]
        tab: Option<u64>,
    },
    /// 칸이 든 탭(나뉜 칸 모두)을 닫는다(AppleScript `close tab`과 같다, #858)
    CloseTab {
        #[serde(default)]
        tab: Option<u64>,
    },
    /// 칸의 제목을 덮어쓴다. 빈 글이면 지운다. PTY에 아무것도 쓰지 않는다(#858)
    SetTitle {
        #[serde(default)]
        tab: Option<u64>,
        title: String,
    },
    /// 칸이 든 탭의 제목을 덮어쓴다. 빈 글이면 지운다(#858)
    SetTabTitle {
        #[serde(default)]
        tab: Option<u64>,
        title: String,
    },
    /// 키 이름으로 키를 보낸다(AppleScript `send key`와 같은 이름·같은 길, 읽기 전용 칸은 거절, #858)
    SendKey {
        #[serde(default)]
        tab: Option<u64>,
        key: String,
        /// 쉼표로 가른 `shift`·`control`·`option`·`command`
        #[serde(default)]
        modifiers: Option<String>,
    },
    /// 칸의 글을 읽는다(읽기 전용). `extent`는 screen·all·selection·last-output(#858)
    GetText {
        #[serde(default)]
        tab: Option<u64>,
        extent: String,
    },
    /// 스크롤백을 움직인다. `to`는 up·down·page-up·page-down·top·bottom, `lines`는 up·down의 줄 수(없으면 1)(#858)
    Scroll {
        #[serde(default)]
        tab: Option<u64>,
        to: String,
        #[serde(default)]
        lines: Option<u32>,
    },
    /// 나뉜 칸의 경계를 `direction`(left·right·up·down)으로 `points`만큼 민다(키 묶음 `resize_split`과 같다, #858)
    Resize {
        #[serde(default)]
        tab: Option<u64>,
        direction: String,
        points: u16,
    },
    /// 탭에 알림을 단다(OSC 알림과 같게 다룬다)
    Notify {
        #[serde(default)]
        tab: Option<u64>,
        #[serde(default)]
        title: String,
        #[serde(default)]
        body: String,
    },
}

fn default_run_timeout() -> u64 {
    60_000
}

impl Call {
    /// 창 스레드의 답을 기다리는 상한. run은 명령이 끝날 때까지
    pub fn reply_timeout(&self) -> std::time::Duration {
        match self {
            Call::Run { timeout_ms, .. } | Call::Wait { timeout_ms, .. } => {
                std::time::Duration::from_millis(timeout_ms.saturating_add(3000))
            }
            _ => std::time::Duration::from_secs(5),
        }
    }
}

/// `run`·`last_command`의 결과
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CommandResult {
    #[serde(default)]
    pub command: Option<String>,
    pub output: String,
    #[serde(default)]
    pub exit_code: Option<i32>,
    pub duration_ms: u64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Response {
    #[serde(default)]
    pub id: u64,
    pub ok: bool,
    #[serde(default, skip_serializing_if = "serde_json::Value::is_null")]
    pub result: serde_json::Value,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

impl Response {
    pub fn ok(id: u64, result: serde_json::Value) -> Self {
        Self {
            id,
            ok: true,
            result,
            error: None,
        }
    }

    pub fn err(id: u64, msg: impl Into<String>) -> Self {
        Self {
            id,
            ok: false,
            result: serde_json::Value::Null,
            error: Some(msg.into()),
        }
    }
}

/// `list` 결과의 한 줄
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TabInfo {
    pub id: u64,
    pub title: String,
    pub active: bool,
    pub unread: bool,
    #[serde(default)]
    pub cwd: Option<PathBuf>,
    #[serde(default)]
    pub process: Option<String>,
}

/// 소켓 폴더. macOS는 사용자마다 다른 `$TMPDIR`, 그 밖에는 `$XDG_RUNTIME_DIR`, 둘 다 없으면 /tmp 아래 사용자 폴더
pub fn socket_dir(uid: u32) -> PathBuf {
    let base = std::env::var_os("XDG_RUNTIME_DIR")
        .or_else(|| std::env::var_os("TMPDIR"))
        .map(PathBuf::from)
        .filter(|p| p.is_absolute())
        .unwrap_or_else(|| PathBuf::from("/tmp"));
    base.join(format!("b-terminal-{uid}"))
}

/// 소켓 폴더가 이 사용자 전용인지 본다: 링크가 아닌 폴더, 주인이 `uid`, 그룹·남 권한 없음.
/// 서버(만들 때)와 btctl(붙기 전) 모두 이 검사를 지나야 쓴다. 남이 먼저 만든 폴더의 가짜 소켓에 붙지 않게
#[cfg(unix)]
pub fn check_private_dir(dir: &Path, uid: u32) -> std::io::Result<()> {
    use std::os::unix::fs::MetadataExt;
    let meta = std::fs::symlink_metadata(dir)?;
    let bad = |why: &str| {
        std::io::Error::new(
            std::io::ErrorKind::PermissionDenied,
            format!("{}: {why}", dir.display()),
        )
    };
    if !meta.file_type().is_dir() {
        return Err(bad("폴더가 아니다(링크일 수 있다)"));
    }
    if meta.uid() != uid {
        return Err(bad("다른 사용자의 폴더다"));
    }
    if meta.mode() & 0o077 != 0 {
        return Err(bad("다른 사용자에게 권한이 열려 있다"));
    }
    Ok(())
}

/// 소켓 파일 하나가 이 사용자의 소켓인지 본다(링크를 따라가지 않는다)
#[cfg(unix)]
pub fn check_own_socket(path: &Path, uid: u32) -> std::io::Result<()> {
    use std::os::unix::fs::{FileTypeExt, MetadataExt};
    let meta = std::fs::symlink_metadata(path)?;
    if !meta.file_type().is_socket() || meta.uid() != uid {
        return Err(std::io::Error::new(
            std::io::ErrorKind::PermissionDenied,
            format!("{}: 이 사용자의 소켓이 아니다", path.display()),
        ));
    }
    Ok(())
}

/// 터미널 출력·스크립트에서 온 글을 화면(탭·창 제목, 확인 막대, 다른 터미널)에 보일 때 다듬는다(#680).
/// 제어 문자(C0·C1·DEL)와 줄·문단 구분자는 공백으로, 글 순서를 뒤집거나 글을 숨기는 서식 문자는 지운다.
/// 서식 문자 목록은 Ghostty `UntrustedURL.isUnsafeCharacter`와 같되, 이모지·글자를 잇는 ZWJ(U+200D)·ZWNJ(U+200C)는 남긴다
pub fn printable(s: &str) -> String {
    s.chars()
        .filter(|&c| !hides_or_reorders(c))
        .map(|c| {
            if c.is_control() || matches!(c, '\u{2028}' | '\u{2029}') {
                ' '
            } else {
                c
            }
        })
        .collect()
}

/// 보이는 글의 순서를 바꾸거나(양방향 표지·덮어쓰기·격리) 폭 없이 숨는 서식 문자
fn hides_or_reorders(c: char) -> bool {
    matches!(c,
        '\u{061C}' | '\u{200B}' | '\u{200E}' | '\u{200F}'
        | '\u{202A}'..='\u{202E}'
        | '\u{2060}'
        | '\u{2066}'..='\u{2069}'
        | '\u{FEFF}')
}

/// 앱 프로세스 하나의 소켓 경로
pub fn socket_path(dir: &Path, pid: u32) -> PathBuf {
    dir.join(format!("{pid}.sock"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn requests_round_trip_as_one_line_json() {
        let req = Request {
            id: 7,
            call: Call::Notify {
                tab: Some(2),
                title: "빌드".into(),
                body: "끝났습니다".into(),
            },
        };
        let line = serde_json::to_string(&req).unwrap();
        assert!(!line.contains('\n'));
        assert_eq!(serde_json::from_str::<Request>(&line).unwrap(), req);
        let parsed: Request = serde_json::from_str(r#"{"id":1,"method":"ping"}"#).unwrap();
        assert_eq!(parsed.call, Call::Ping);
        let send: Request = serde_json::from_str(r#"{"method":"send","params":{"text":"ls\r"}}"#).unwrap();
        assert_eq!(
            send.call,
            Call::Send {
                tab: None,
                text: "ls\r".into()
            }
        );
    }

    #[test]
    fn printable_strips_c0_c1_and_del() {
        assert_eq!(printable("a\x1b[2Jb\u{9b}31mc\x7fd"), "a [2Jb 31mc d");
        assert_eq!(printable("한글 ok"), "한글 ok");
    }

    /// 방향 바꾸기·폭 없는 서식 문자는 지우고 줄·문단 구분자는 공백으로(#680). 목록은 Ghostty `UntrustedURL.isUnsafeCharacter`와
    /// 같고, 이모지·글자 잇기에 쓰는 ZWJ(U+200D)·ZWNJ(U+200C)는 남긴다
    #[test]
    fn printable_drops_bidi_and_invisible_format() {
        assert_eq!(printable("ok\u{202E}gnp.sh"), "okgnp.sh");
        assert_eq!(printable("a\u{061C}b\u{200B}c\u{200E}d\u{200F}e"), "abcde");
        assert_eq!(printable("a\u{202A}b\u{202D}c\u{2066}d\u{2069}e"), "abcde");
        assert_eq!(printable("a\u{2060}b\u{FEFF}c"), "abc");
        assert_eq!(printable("a\u{2028}b\u{2029}c"), "a b c");
        // 가족 이모지(ZWJ로 이음)와 ZWNJ는 그대로
        assert_eq!(printable("👨\u{200D}👩\u{200D}👧"), "👨\u{200D}👩\u{200D}👧");
        assert_eq!(printable("می\u{200C}خواهم"), "می\u{200C}خواهم");
    }

    /// #858: 새 명령의 요청 형식. 한 줄 JSON으로 왕복하고, 칸을 주지 않아도 읽힌다
    #[test]
    fn extended_requests_round_trip() {
        let calls = [
            Call::Close { tab: Some(3) },
            Call::CloseTab { tab: None },
            Call::SetTitle {
                tab: Some(1),
                title: "빌드".into(),
            },
            Call::SetTabTitle {
                tab: None,
                title: String::new(),
            },
            Call::SendKey {
                tab: Some(2),
                key: "enter".into(),
                modifiers: Some("control".into()),
            },
            Call::GetText {
                tab: None,
                extent: "selection".into(),
            },
            Call::Scroll {
                tab: None,
                to: "up".into(),
                lines: Some(5),
            },
            Call::Resize {
                tab: Some(4),
                direction: "left".into(),
                points: 20,
            },
        ];
        for call in calls {
            let req = Request { id: 9, call };
            let line = serde_json::to_string(&req).unwrap();
            assert!(!line.contains('\n'));
            assert_eq!(serde_json::from_str::<Request>(&line).unwrap(), req);
        }
        let k: Request = serde_json::from_str(r#"{"method":"send_key","params":{"key":"a"}}"#).unwrap();
        assert_eq!(
            k.call,
            Call::SendKey {
                tab: None,
                key: "a".into(),
                modifiers: None
            }
        );
    }

    #[test]
    fn unknown_method_is_an_error_not_a_panic() {
        assert!(serde_json::from_str::<Request>(r#"{"method":"rm_rf"}"#).is_err());
    }
}
