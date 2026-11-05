//! btctl: b-terminal 제어 소켓 손님. 스크립트와 에이전트 훅이 탭에 상태·알림을 보내고, 탭을 열고, 화면을 읽는다

mod client;
mod hooks;

use bt_core::proto::{Call, TabInfo, printable};
use bt_core::{AgentState, WaitReason};

use client::{ClientError, call, find_socket, own_tab};

const USAGE: &str = "사용: btctl <명령> [인자]

  ping                              터미널이 떠 있는지
  list [--json]                     탭 목록
  new [--cwd 폴더] [-- 명령 인자...]  새 탭
  focus <탭>                        그 탭으로
  send [--tab N] [--enter] <글...>  탭에 글 넣기(--enter면 끝에 Enter)
  read [--tab N]                    탭 화면을 글로
  notify [--tab N] <제목> [본문]    알림 달기
  status [--tab N] <상태> [--activity 글] [--agent 이름]
        상태: working, waiting-permission, waiting-question, waiting-input, done, error, idle

  hook claude                       Claude Code 훅(표준 입력 JSON)을 탭 상태로
  hook codex <JSON>                 Codex notify를 탭 상태로
  setup claude [--write [파일]]      Claude Code 훅 설정 보이기(--write면 설정 파일에 합침)
  setup codex                       Codex notify 설정 보이기

탭을 주지 않으면 btctl을 부른 탭(BT_TAB_ID), 탭 밖이면 지금 보는 탭입니다.
hook은 터미널이 없거나 실패해도 에이전트를 막지 않도록 항상 0으로 끝납니다.";

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.first().map(String::as_str) == Some("hook") {
        // 훅은 에이전트를 막지 않는다: 무엇이 잘못돼도 0으로 끝내고, BTCTL_DEBUG가 있을 때만 까닭을 찍는다
        if let Err(e) = run_hook(&args[1..]) {
            if std::env::var_os("BTCTL_DEBUG").is_some() {
                eprintln!("btctl hook: {e}");
            }
        }
        return;
    }
    match run(&args) {
        Ok(out) => {
            if !out.is_empty() {
                println!("{out}");
            }
        }
        Err(e) => {
            eprintln!("btctl: {e}");
            std::process::exit(match e {
                Error::Usage(_) => 2,
                _ => 1,
            });
        }
    }
}

#[derive(Debug)]
enum Error {
    Usage(String),
    Client(ClientError),
}

impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Error::Usage(m) => write!(f, "{m}\n\n{USAGE}"),
            Error::Client(e) => write!(f, "{e}"),
        }
    }
}

impl From<ClientError> for Error {
    fn from(e: ClientError) -> Self {
        Error::Client(e)
    }
}

fn usage(m: impl Into<String>) -> Error {
    Error::Usage(m.into())
}

/// `--tab N`을 빼내고 나머지 인자를 돌려준다
fn take_tab(args: &[String]) -> Result<(Option<u64>, Vec<String>), Error> {
    let mut tab = None;
    let mut rest = Vec::new();
    let mut it = args.iter();
    while let Some(a) = it.next() {
        if a == "--tab" {
            let v = it.next().ok_or_else(|| usage("--tab 뒤에 탭 번호가 필요합니다"))?;
            tab = Some(v.parse().map_err(|_| usage(format!("탭 번호가 아닙니다: {v}")))?);
        } else {
            rest.push(a.clone());
        }
    }
    Ok((tab.or_else(own_tab), rest))
}

/// 상태 이름을 읽는다
pub fn parse_state(s: &str) -> Option<AgentState> {
    Some(match s {
        "working" => AgentState::Working,
        "waiting-permission" => AgentState::Waiting(WaitReason::Permission),
        "waiting-question" => AgentState::Waiting(WaitReason::Question),
        "waiting-input" | "waiting" => AgentState::Waiting(WaitReason::Input),
        "done" => AgentState::Done,
        "error" => AgentState::Error,
        "idle" => AgentState::Idle,
        _ => return None,
    })
}

/// 훅 입력을 읽어 상태를 보낸다
fn run_hook(args: &[String]) -> Result<(), Error> {
    use std::io::Read;
    let agent = args
        .first()
        .map(String::as_str)
        .ok_or_else(|| usage("hook 뒤에 claude 또는 codex가 필요합니다"))?;
    // 탭 안이 아니면 할 일이 없다(다른 터미널에서 돈 claude의 훅)
    if std::env::var_os(bt_core::proto::ENV_SOCKET).is_none() {
        return Ok(());
    }
    let input: serde_json::Value = match agent {
        "claude" => {
            let mut buf = Vec::new();
            std::io::stdin()
                .take(bt_core::proto::MAX_LINE as u64)
                .read_to_end(&mut buf)
                .map_err(|e| Error::Client(ClientError::Io(e)))?;
            serde_json::from_slice(&buf).map_err(|e| usage(format!("훅 입력이 JSON이 아닙니다: {e}")))?
        }
        "codex" => {
            let raw = args
                .last()
                .filter(|_| args.len() > 1)
                .ok_or_else(|| usage("codex 훅 JSON이 필요합니다"))?;
            serde_json::from_str(raw).map_err(|e| usage(format!("codex 훅 입력이 JSON이 아닙니다: {e}")))?
        }
        other => return Err(usage(format!("모르는 에이전트: {other}"))),
    };
    let update = match agent {
        "claude" => hooks::claude(&input),
        _ => hooks::codex(&input),
    };
    let Some(update) = update else { return Ok(()) };
    let socket = find_socket()?;
    call(
        &socket,
        Call::Status {
            tab: own_tab(),
            state: update.state,
            activity: update.activity,
            agent: Some(agent.to_string()),
        },
    )?;
    Ok(())
}

/// 설정 파일 자리: CLAUDE_CONFIG_DIR가 있으면 그 안, 없으면 ~/.claude/settings.json
fn claude_settings_path() -> std::path::PathBuf {
    std::env::var_os("CLAUDE_CONFIG_DIR")
        .map(std::path::PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|h| std::path::Path::new(&h).join(".claude")))
        .unwrap_or_default()
        .join("settings.json")
}

fn setup(args: &[String]) -> Result<String, Error> {
    let exe = std::env::current_exe()
        .map(|p| p.to_string_lossy().into_owned())
        .unwrap_or_else(|_| "btctl".into());
    match args.first().map(String::as_str) {
        Some("claude") => {
            if args.get(1).map(String::as_str) != Some("--write") {
                let snippet = serde_json::to_string_pretty(&hooks::claude_settings(&exe)).unwrap_or_default();
                return Ok(format!(
                    "# {} 에 아래 \"hooks\"를 합치세요(btctl setup claude --write가 대신 합칩니다)\n{snippet}",
                    claude_settings_path().display()
                ));
            }
            let path = args
                .get(2)
                .map(std::path::PathBuf::from)
                .unwrap_or_else(claude_settings_path);
            let mut settings: serde_json::Value = match std::fs::read(&path) {
                Ok(raw) => serde_json::from_slice(&raw)
                    .map_err(|e| usage(format!("{}를 JSON으로 읽지 못했습니다: {e}", path.display())))?,
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => serde_json::json!({}),
                Err(e) => return Err(Error::Client(ClientError::Io(e))),
            };
            let added = hooks::merge_claude_settings(&mut settings, &exe);
            if added == 0 {
                return Ok(format!("{}에 이미 훅이 있습니다", path.display()));
            }
            // 덮어쓰기 전에 원본을 남긴다
            if path.exists() {
                let backup = path.with_extension("json.bt-backup");
                std::fs::copy(&path, &backup).map_err(|e| Error::Client(ClientError::Io(e)))?;
            }
            if let Some(dir) = path.parent() {
                std::fs::create_dir_all(dir).map_err(|e| Error::Client(ClientError::Io(e)))?;
            }
            let body = serde_json::to_string_pretty(&settings).unwrap_or_default() + "\n";
            std::fs::write(&path, body).map_err(|e| Error::Client(ClientError::Io(e)))?;
            Ok(format!(
                "{}에 훅 {added}개 이벤트를 더했습니다(원본은 .bt-backup)",
                path.display()
            ))
        }
        Some("codex") => Ok(format!(
            "# ~/.codex/config.toml 맨 위(어떤 [표]보다 앞)에 한 줄을 넣으세요. 이미 notify가 있으면 하나만 둘 수 있습니다\nnotify = [\"{exe}\", \"hook\", \"codex\"]"
        )),
        _ => Err(usage("setup 뒤에 claude 또는 codex가 필요합니다")),
    }
}

fn run(args: &[String]) -> Result<String, Error> {
    let Some((cmd, rest)) = args.split_first() else {
        return Err(usage("명령이 필요합니다"));
    };
    if matches!(cmd.as_str(), "-h" | "--help" | "help") {
        return Ok(USAGE.into());
    }
    if cmd == "--version" {
        return Ok(format!("btctl {}", env!("CARGO_PKG_VERSION")));
    }
    // setup은 터미널 없이 된다
    if cmd == "setup" {
        return setup(rest);
    }
    let socket = find_socket()?;
    match cmd.as_str() {
        "ping" => {
            let v = call(&socket, Call::Ping)?;
            Ok(format!(
                "{} {} ({})",
                v["app"].as_str().unwrap_or("?"),
                v["version"].as_str().unwrap_or("?"),
                socket.display()
            ))
        }
        "list" => {
            let v = call(&socket, Call::List)?;
            if rest.iter().any(|a| a == "--json") {
                return Ok(v.to_string());
            }
            let tabs: Vec<TabInfo> =
                serde_json::from_value(v).map_err(|e| Error::Client(ClientError::Remote(e.to_string())))?;
            Ok(tabs
                .iter()
                .map(|t| {
                    let mark = if t.active { "*" } else { " " };
                    let unread = if t.unread { " ●" } else { "" };
                    // 제목·하는 일은 터미널 출력에서 온 글이다. 이 터미널을 조작하지 못하게 제어 문자를 지운다
                    let what = printable(&t.activity.clone().unwrap_or_default());
                    let title = printable(&t.title);
                    format!(
                        "{mark}{:>3}  {:<12} {:<10} {}{}",
                        t.id,
                        t.state.label(),
                        title,
                        what,
                        unread
                    )
                })
                .collect::<Vec<_>>()
                .join("\n"))
        }
        "new" => {
            let mut cwd = None;
            let mut command = Vec::new();
            let mut it = rest.iter();
            while let Some(a) = it.next() {
                match a.as_str() {
                    "--cwd" => cwd = Some(it.next().ok_or_else(|| usage("--cwd 뒤에 폴더가 필요합니다"))?.into()),
                    "--" => {
                        command = it.by_ref().cloned().collect();
                    }
                    other => return Err(usage(format!("모르는 인자: {other}"))),
                }
            }
            let v = call(&socket, Call::New { cwd, command })?;
            Ok(v["tab"].to_string())
        }
        "focus" => {
            let tab = rest
                .first()
                .and_then(|t| t.parse().ok())
                .ok_or_else(|| usage("탭 번호가 필요합니다"))?;
            call(&socket, Call::Focus { tab })?;
            Ok(String::new())
        }
        "send" => {
            let (tab, rest) = take_tab(rest)?;
            let enter = rest.iter().any(|a| a == "--enter");
            let words: Vec<&str> = rest.iter().filter(|a| *a != "--enter").map(String::as_str).collect();
            let mut text = words.join(" ");
            if enter {
                text.push('\r');
            }
            if text.is_empty() {
                return Err(usage("넣을 글이 필요합니다"));
            }
            call(&socket, Call::Send { tab, text })?;
            Ok(String::new())
        }
        "read" => {
            let (tab, _) = take_tab(rest)?;
            let v = call(&socket, Call::Read { tab })?;
            // 화면 글은 그대로 돌려준다(스크립트가 읽는 값). 다만 엔진이 지운 이스케이프는 이미 없다
            Ok(v["text"].as_str().unwrap_or_default().to_string())
        }
        "notify" => {
            let (tab, rest) = take_tab(rest)?;
            let title = rest.first().cloned().ok_or_else(|| usage("제목이 필요합니다"))?;
            let body = rest.get(1..).map(|r| r.join(" ")).unwrap_or_default();
            call(&socket, Call::Notify { tab, title, body })?;
            Ok(String::new())
        }
        "status" => {
            let (tab, rest) = take_tab(rest)?;
            let mut state = None;
            let mut activity = None;
            let mut agent = None;
            let mut it = rest.iter();
            while let Some(a) = it.next() {
                match a.as_str() {
                    "--activity" => activity = it.next().cloned(),
                    "--agent" => agent = it.next().cloned(),
                    s => state = Some(parse_state(s).ok_or_else(|| usage(format!("모르는 상태: {s}")))?),
                }
            }
            let state = state.ok_or_else(|| usage("상태가 필요합니다"))?;
            call(
                &socket,
                Call::Status {
                    tab,
                    state,
                    activity,
                    agent,
                },
            )?;
            Ok(String::new())
        }
        other => Err(usage(format!("모르는 명령: {other}"))),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn state_names() {
        assert_eq!(
            parse_state("waiting-permission"),
            Some(AgentState::Waiting(WaitReason::Permission))
        );
        assert_eq!(parse_state("done"), Some(AgentState::Done));
        assert_eq!(parse_state("nope"), None);
    }

    #[test]
    fn tab_flag_is_extracted() {
        let args: Vec<String> = ["--tab", "3", "hello", "world"].iter().map(|s| s.to_string()).collect();
        let (tab, rest) = take_tab(&args).unwrap();
        assert_eq!(tab, Some(3));
        assert_eq!(rest, vec!["hello", "world"]);
        assert!(take_tab(&["--tab".to_string()]).is_err());
    }
}
