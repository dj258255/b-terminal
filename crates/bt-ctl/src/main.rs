//! btctl: b-terminal 제어 소켓 손님. 스크립트와 에이전트 훅이 탭에 상태·알림을 보내고, 탭을 열고, 화면을 읽는다

mod client;

use bt_core::proto::{Call, TabInfo, printable};

use client::{ClientError, call, find_socket, own_tab};

const USAGE: &str = "사용: btctl <명령> [인자]

  ping                              터미널이 떠 있는지
  list [--json]                     탭 목록
  new [--cwd 폴더] [-- 명령 인자...]  새 탭
  focus <탭>                        그 탭으로
  send [--tab N] [--enter] <글...>  탭에 글 넣기(--enter면 끝에 Enter)
  read [--tab N]                    탭 화면을 글로
  notify [--tab N] <제목> [본문]    알림 달기(OSC 알림과 같게 다룸)
  reload-config                     설정 파일을 다시 읽기

탭을 주지 않으면 btctl을 부른 탭(BT_TAB_ID), 탭 밖이면 지금 보는 탭입니다.";

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
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
                    // 제목은 터미널 출력에서 온 글이다. 이 터미널을 조작하지 못하게 제어 문자를 지운다
                    let title = printable(&t.title);
                    let process = printable(t.process.as_deref().unwrap_or(""));
                    format!("{mark}{:>3}  {:<24} {}{}", t.id, title, process, unread)
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
        "reload-config" => {
            let v = call(&socket, Call::ReloadConfig)?;
            let warnings: Vec<String> = v["warnings"]
                .as_array()
                .map(|a| {
                    a.iter()
                        .filter_map(|w| w.as_str().map(|s| format!("경고: {}", printable(s))))
                        .collect()
                })
                .unwrap_or_default();
            Ok(if warnings.is_empty() {
                "설정을 다시 읽었습니다".into()
            } else {
                warnings.join("\n")
            })
        }
        other => Err(usage(format!("모르는 명령: {other}"))),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tab_flag_is_extracted() {
        let args: Vec<String> = ["--tab", "3", "hello", "world"].iter().map(|s| s.to_string()).collect();
        let (tab, rest) = take_tab(&args).unwrap();
        assert_eq!(tab, Some(3));
        assert_eq!(rest, vec!["hello", "world"]);
        assert!(take_tab(&["--tab".to_string()]).is_err());
    }
}
