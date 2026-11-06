//! btctl: b-terminal 제어 소켓 손님. 스크립트와 에이전트 훅이 탭에 상태·알림을 보내고, 탭을 열고, 화면을 읽는다

mod client;

use bt_core::proto::{Call, TabInfo, printable};

use client::{ClientError, call, find_socket, own_tab};

const USAGE: &str = "사용: btctl <명령> [인자]

  ping                              터미널이 떠 있는지
  list [--json]                     탭 목록
  new [--cwd 폴더] [--split 방향 | --window] [-- 명령 인자...]
                                    새 탭. --split right·down·left·up·auto면 탭(칸)을 나눠, --window면 새 창에 연다
  focus <탭>                        그 탭으로
  send [--tab N] [--enter] <글...>  탭에 글 넣기(--enter면 끝에 Enter)
  read [--tab N] [--lines N]        탭 화면을 글로(--lines면 맨 아래에서 위로 N줄, 스크롤백 포함)
  wait [--tab N] [--timeout 초] -- <글...>
                                    탭 글(화면과 최근 스크롤백 500줄)에 그 글이 나올 때까지 기다림. 기한이 지나면 1로 끝남
  notify [--tab N] <제목> [본문]    알림 달기(OSC 알림과 같게 다룸)
  reload-config                     설정 파일을 다시 읽기
  run [--tab N] [--timeout 초] [--json] -- <명령...>
                                    탭에서 명령을 돌리고 끝날 때까지 기다려 출력을 찍고 그 종료 코드로 끝남(셸 통합 필요)
  last [--tab N] [--json]           탭에서 마지막으로 끝난 명령의 출력과 종료 코드

탭 번호는 칸마다 하나입니다(화면을 나누면 칸마다 따로).
탭을 주지 않으면 btctl을 부른 칸(BT_TAB_ID), 탭 밖이면 지금 고른 칸입니다.";

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

/// 명령 결과를 찍는다. 글로 찍을 때는 그 명령의 종료 코드로 btctl도 끝난다(스크립트·에이전트가 그대로 쓰게)
fn finish_command(v: serde_json::Value, json: bool) -> Result<String, Error> {
    if json {
        return Ok(v.to_string());
    }
    let r: bt_core::proto::CommandResult =
        serde_json::from_value(v).map_err(|e| Error::Client(ClientError::Remote(e.to_string())))?;
    print!("{}", r.output);
    use std::io::Write;
    let _ = std::io::stdout().flush();
    std::process::exit(r.exit_code.unwrap_or(0).clamp(0, 255));
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
            let mut split = None;
            let mut window = false;
            let (tab, rest) = take_tab(rest)?;
            let mut it = rest.iter();
            while let Some(a) = it.next() {
                match a.as_str() {
                    "--cwd" => cwd = Some(it.next().ok_or_else(|| usage("--cwd 뒤에 폴더가 필요합니다"))?.into()),
                    "--split" => {
                        let d = it.next().ok_or_else(|| usage("--split 뒤에 방향이 필요합니다"))?;
                        if !["right", "down", "left", "up", "auto"].contains(&d.as_str()) {
                            return Err(usage(format!("--split 방향은 right·down·left·up·auto입니다: {d}")));
                        }
                        split = Some(d.clone());
                    }
                    "--window" => window = true,
                    "--" => {
                        command = it.by_ref().cloned().collect();
                    }
                    other => return Err(usage(format!("모르는 인자: {other}"))),
                }
            }
            if window && split.is_some() {
                return Err(usage("--window와 --split은 함께 쓸 수 없습니다"));
            }
            // 나눌 때만 칸을 쓴다(새 탭은 어느 칸에서 불러도 같다)
            let tab = if split.is_some() { tab } else { None };
            let v = call(
                &socket,
                Call::New {
                    cwd,
                    command,
                    split,
                    tab,
                    window,
                },
            )?;
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
            let (tab, rest) = take_tab(rest)?;
            let mut lines = None;
            let mut it = rest.iter();
            while let Some(a) = it.next() {
                match a.as_str() {
                    "--lines" => {
                        let v = it.next().ok_or_else(|| usage("--lines 뒤에 줄 수가 필요합니다"))?;
                        lines = Some(
                            v.parse::<usize>()
                                .map_err(|_| usage(format!("줄 수가 아닙니다: {v}")))?,
                        );
                    }
                    other => return Err(usage(format!("모르는 인자: {other}"))),
                }
            }
            let v = call(&socket, Call::Read { tab, lines })?;
            if v["truncated"].as_bool() == Some(true) {
                eprintln!("btctl: 글이 길어 앞부분을 버렸습니다");
            }
            // 화면 글은 그대로 돌려준다(스크립트가 읽는 값). 다만 엔진이 지운 이스케이프는 이미 없다
            Ok(v["text"].as_str().unwrap_or_default().to_string())
        }
        "wait" => {
            let (tab, rest) = take_tab(rest)?;
            let mut timeout_ms = 60_000u64;
            let mut words = Vec::new();
            let mut it = rest.iter();
            while let Some(a) = it.next() {
                match a.as_str() {
                    "--timeout" => {
                        let v = it.next().ok_or_else(|| usage("--timeout 뒤에 초가 필요합니다"))?;
                        timeout_ms = v
                            .parse::<u64>()
                            .map_err(|_| usage(format!("초가 아닙니다: {v}")))?
                            .saturating_mul(1000);
                    }
                    "--" => words.extend(it.by_ref().cloned()),
                    other => words.push(other.to_string()),
                }
            }
            if words.is_empty() {
                return Err(usage("기다릴 글이 필요합니다"));
            }
            call(
                &socket,
                Call::Wait {
                    tab,
                    text: words.join(" "),
                    timeout_ms,
                },
            )?;
            Ok(String::new())
        }
        "notify" => {
            let (tab, rest) = take_tab(rest)?;
            let title = rest.first().cloned().ok_or_else(|| usage("제목이 필요합니다"))?;
            let body = rest.get(1..).map(|r| r.join(" ")).unwrap_or_default();
            call(&socket, Call::Notify { tab, title, body })?;
            Ok(String::new())
        }
        "run" => {
            let (tab, rest) = take_tab(rest)?;
            let mut timeout_ms = 60_000u64;
            let mut json = false;
            let mut words = Vec::new();
            let mut it = rest.iter();
            while let Some(a) = it.next() {
                match a.as_str() {
                    "--timeout" => {
                        let v = it.next().ok_or_else(|| usage("--timeout 뒤에 초가 필요합니다"))?;
                        timeout_ms = v
                            .parse::<u64>()
                            .map_err(|_| usage(format!("초가 아닙니다: {v}")))?
                            .saturating_mul(1000);
                    }
                    "--json" => json = true,
                    "--" => words.extend(it.by_ref().cloned()),
                    other => words.push(other.to_string()),
                }
            }
            if words.is_empty() {
                return Err(usage("돌릴 명령이 필요합니다"));
            }
            let v = call(
                &socket,
                Call::Run {
                    tab,
                    command: words.join(" "),
                    timeout_ms,
                },
            )?;
            finish_command(v, json)
        }
        "last" => {
            let (tab, rest) = take_tab(rest)?;
            let v = call(&socket, Call::LastCommand { tab })?;
            finish_command(v, rest.iter().any(|a| a == "--json"))
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
