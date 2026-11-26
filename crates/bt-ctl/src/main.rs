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
  close [--tab N]                   칸 닫기(확인 없이)
  close-tab [--tab N]               칸이 든 탭(나뉜 칸 모두) 닫기(확인 없이)
  set-title [--tab N] [글...]       칸 제목 덮어쓰기(글이 없으면 지움)
  set-tab-title [--tab N] [글...]   탭 제목 덮어쓰기(글이 없으면 지움)
  send-key [--tab N] [--mods 수식키] <키>
                                    키 이름으로 키 보내기(AppleScript send key와 같은 이름: enter, a, arrowUp. 수식키는 shift·control·option·command를 쉼표로)
  get-text [--tab N] [--extent 범위]
                                    칸의 글 읽기(읽기 전용). 범위: screen(기본)·all(스크롤백 포함)·selection(고른 글)·last-output(마지막 명령 출력, 셸 통합 필요)
  scroll [--tab N] <방향> [줄 수]   스크롤백 움직이기. 방향: up·down(줄 수 기본 1)·page-up·page-down·top·bottom
  resize [--tab N] [--amount 점] <방향>
                                    나뉜 칸의 경계 밀기(left·right·up·down, 점 기본 20)
  quick                             퀵 터미널 보이기·숨기기(전역 단축키와 같음)
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

/// 칸 하나를 다루는 새 명령(#858)의 요청을 만든다. 이 명령이 아니면 `None`
fn build_call(cmd: &str, rest: &[String]) -> Result<Option<Call>, Error> {
    if !matches!(
        cmd,
        "close" | "close-tab" | "set-title" | "set-tab-title" | "send-key" | "get-text" | "scroll" | "resize"
    ) {
        return Ok(None);
    }
    let (tab, rest) = take_tab(rest)?;
    let no_extra = |rest: &[String]| match rest.first() {
        Some(a) => Err(usage(format!("모르는 인자: {a}"))),
        None => Ok(()),
    };
    let call = match cmd {
        "close" => {
            no_extra(&rest)?;
            Call::Close { tab }
        }
        "close-tab" => {
            no_extra(&rest)?;
            Call::CloseTab { tab }
        }
        // 글이 없으면 제목을 지운다
        "set-title" => Call::SetTitle {
            tab,
            title: rest.join(" "),
        },
        "set-tab-title" => Call::SetTabTitle {
            tab,
            title: rest.join(" "),
        },
        "send-key" => {
            let mut modifiers = None;
            let mut key = None;
            let mut it = rest.iter();
            while let Some(a) = it.next() {
                if a == "--mods" {
                    modifiers = Some(
                        it.next()
                            .ok_or_else(|| usage("--mods 뒤에 shift·control·option·command(쉼표로 가름)가 필요합니다"))?
                            .clone(),
                    );
                } else if key.is_none() {
                    key = Some(a.clone());
                } else {
                    return Err(usage(format!("키 이름은 하나만 줍니다: {a}")));
                }
            }
            Call::SendKey {
                tab,
                key: key.ok_or_else(|| usage("키 이름이 필요합니다(예: enter, a, arrowUp)"))?,
                modifiers,
            }
        }
        "get-text" => {
            let mut extent = "screen".to_string();
            let mut it = rest.iter();
            while let Some(a) = it.next() {
                match a.as_str() {
                    "--extent" => {
                        let v = it.next().ok_or_else(|| usage("--extent 뒤에 범위가 필요합니다"))?;
                        if !["screen", "all", "selection", "last-output"].contains(&v.as_str()) {
                            return Err(usage(format!("--extent는 screen·all·selection·last-output입니다: {v}")));
                        }
                        extent = v.clone();
                    }
                    other => return Err(usage(format!("모르는 인자: {other}"))),
                }
            }
            Call::GetText { tab, extent }
        }
        "scroll" => {
            let to = rest.first().ok_or_else(|| usage("방향이 필요합니다"))?;
            if !["up", "down", "page-up", "page-down", "top", "bottom"].contains(&to.as_str()) {
                return Err(usage(format!(
                    "방향은 up·down·page-up·page-down·top·bottom입니다: {to}"
                )));
            }
            let lines = match rest.get(1) {
                Some(n) => Some(n.parse::<u32>().map_err(|_| usage(format!("줄 수가 아닙니다: {n}")))?),
                None => None,
            };
            if rest.len() > 2 {
                return Err(usage(format!("모르는 인자: {}", rest[2])));
            }
            Call::Scroll {
                tab,
                to: to.clone(),
                lines,
            }
        }
        _ => {
            let mut points = 20u16;
            let mut direction = None;
            let mut it = rest.iter();
            while let Some(a) = it.next() {
                if a == "--amount" {
                    let v = it.next().ok_or_else(|| usage("--amount 뒤에 점 수가 필요합니다"))?;
                    points = v
                        .parse::<u16>()
                        .ok()
                        .filter(|p| *p > 0)
                        .ok_or_else(|| usage(format!("점 수는 1 이상의 정수여야 합니다: {v}")))?;
                } else if ["left", "right", "up", "down"].contains(&a.as_str()) && direction.is_none() {
                    direction = Some(a.clone());
                } else {
                    return Err(usage(format!("방향은 left·right·up·down 하나입니다: {a}")));
                }
            }
            Call::Resize {
                tab,
                direction: direction.ok_or_else(|| usage("방향이 필요합니다"))?,
                points,
            }
        }
    };
    Ok(Some(call))
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
    if let Some(c) = build_call(cmd, rest)? {
        let socket = find_socket()?;
        let v = call(&socket, c)?;
        return Ok(match cmd.as_str() {
            "get-text" => {
                if v["truncated"].as_bool() == Some(true) {
                    eprintln!("btctl: 글이 길어 앞부분을 버렸습니다");
                }
                v["text"].as_str().unwrap_or_default().to_string()
            }
            _ => String::new(),
        });
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
        "quick" => {
            call(&socket, Call::QuickTerminal)?;
            Ok(String::new())
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

    fn v(a: &[&str]) -> Vec<String> {
        a.iter().map(|s| s.to_string()).collect()
    }

    fn build(cmd: &str, a: &[&str]) -> Result<Option<Call>, Error> {
        build_call(cmd, &v(a))
    }

    fn is_usage<T: std::fmt::Debug>(r: Result<T, Error>) -> bool {
        matches!(r, Err(Error::Usage(_)))
    }

    #[test]
    fn close_and_close_tab_take_a_pane() {
        assert_eq!(
            build("close", &["--tab", "3"]).unwrap(),
            Some(Call::Close { tab: Some(3) })
        );
        assert_eq!(
            build("close-tab", &["--tab", "3"]).unwrap(),
            Some(Call::CloseTab { tab: Some(3) })
        );
        assert!(is_usage(build("close", &["남는", "인자"])));
    }

    #[test]
    fn titles_join_words_and_may_be_empty() {
        assert_eq!(
            build("set-title", &["--tab", "2", "빌드", "중"]).unwrap(),
            Some(Call::SetTitle {
                tab: Some(2),
                title: "빌드 중".into()
            })
        );
        assert_eq!(
            build("set-tab-title", &["--tab", "2"]).unwrap(),
            Some(Call::SetTabTitle {
                tab: Some(2),
                title: String::new()
            })
        );
    }

    #[test]
    fn send_key_needs_a_name_and_reads_mods() {
        assert_eq!(
            build("send-key", &["--tab", "1", "--mods", "control,shift", "c"]).unwrap(),
            Some(Call::SendKey {
                tab: Some(1),
                key: "c".into(),
                modifiers: Some("control,shift".into())
            })
        );
        assert!(is_usage(build("send-key", &["--tab", "1"])));
        assert!(is_usage(build("send-key", &["a", "b"])));
        assert!(is_usage(build("send-key", &["--mods"])));
    }

    #[test]
    fn get_text_extent_is_checked() {
        assert_eq!(
            build("get-text", &["--tab", "1", "--extent", "selection"]).unwrap(),
            Some(Call::GetText {
                tab: Some(1),
                extent: "selection".into()
            })
        );
        assert_eq!(
            build("get-text", &["--tab", "1"]).unwrap(),
            Some(Call::GetText {
                tab: Some(1),
                extent: "screen".into()
            })
        );
        assert!(is_usage(build("get-text", &["--extent", "nope"])));
    }

    #[test]
    fn scroll_and_resize_arguments() {
        assert_eq!(
            build("scroll", &["--tab", "1", "up", "5"]).unwrap(),
            Some(Call::Scroll {
                tab: Some(1),
                to: "up".into(),
                lines: Some(5)
            })
        );
        assert_eq!(
            build("scroll", &["--tab", "1", "top"]).unwrap(),
            Some(Call::Scroll {
                tab: Some(1),
                to: "top".into(),
                lines: None
            })
        );
        assert!(is_usage(build("scroll", &["sideways"])));
        assert!(is_usage(build("scroll", &["up", "많이"])));
        assert_eq!(
            build("resize", &["--tab", "1", "--amount", "40", "left"]).unwrap(),
            Some(Call::Resize {
                tab: Some(1),
                direction: "left".into(),
                points: 40
            })
        );
        assert_eq!(
            build("resize", &["--tab", "1", "down"]).unwrap(),
            Some(Call::Resize {
                tab: Some(1),
                direction: "down".into(),
                points: 20
            })
        );
        assert!(is_usage(build("resize", &["diagonal"])));
        assert!(is_usage(build("resize", &["--amount", "0", "left"])));
    }

    #[test]
    fn other_commands_are_not_built_here() {
        assert_eq!(build("ping", &[]).unwrap(), None);
    }

    #[test]
    fn usage_lists_the_new_commands() {
        for c in [
            "close",
            "close-tab",
            "set-title",
            "set-tab-title",
            "send-key",
            "get-text",
            "scroll",
            "resize",
        ] {
            assert!(USAGE.contains(c), "{c}");
        }
    }
}
