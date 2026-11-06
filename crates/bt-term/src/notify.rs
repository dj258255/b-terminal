//! PTY 바이트에서 VT 엔진이 다루지 않는 신호를 뽑는다: 알림(OSC 9·99·777), 작업 폴더(OSC 7), 진행 표시(OSC 9;4).
//!
//! 바이트는 읽기 조각 경계에서 잘려 온다. 스캐너는 OSC 하나가 끝날 때까지(BEL 또는 ESC \) 모아 두었다가 해석한다.
//! 터미널에 찍히는 글은 누구나 만들 수 있으므로(원격 서버, 남이 만든 파일의 cat) 길이를 자르고 제어 문자를 지운다

use std::path::PathBuf;

/// 프로그램이 터미널에 보낸 알림
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Notification {
    pub title: String,
    pub body: String,
}

/// 스캐너가 뽑은 신호
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Signal {
    Notify(Notification),
    /// 셸이 알린 작업 폴더(OSC 7). 호스트가 이 컴퓨터인지는 받는 쪽이 본다(ssh 안의 셸도 보낸다)
    Cwd {
        host: String,
        path: PathBuf,
    },
    /// 셸 통합 표지(OSC 133, 또는 VS Code의 OSC 633 A~D)
    Prompt(PromptMark),
    /// 셸이 알린, 지금 실행하는 명령 글(OSC 633;E)
    CommandLine(String),
    /// 진행 표시(OSC 9;4;상태;값). 상태 0은 지움, 1 보통, 2 오류, 3 끝 모름, 4 멈춤
    Progress {
        state: u8,
        percent: u8,
    },
    /// kitty 그래픽 명령(APC `ESC _ G … ESC \`)의 `G` 뒤 바이트(#76)
    Graphics(Vec<u8>),
    /// 화면 지우기(ED 2: 보이는 화면, ED 3: 스크롤백)나 초기화(RIS). 놓인 그림을 함께 지운다
    Erase {
        visible: bool,
        scrollback: bool,
    },
}

/// 셸 통합 표지. <https://gitlab.freedesktop.org/Per_Bothner/specifications/blob/master/proposals/semantic-prompts.md>
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PromptMark {
    /// A: 프롬프트가 시작된다
    PromptStart,
    /// B: 사람이 명령을 치기 시작한다(프롬프트 끝)
    CommandStart,
    /// C: 명령이 실행된다(출력 시작)
    CommandExecuted,
    /// D: 명령이 끝났다(종료 코드)
    CommandFinished(Option<i32>),
}

/// OSC 하나의 상한. 넘으면 버린다(상한은 모으기 전에 본다)
const MAX_OSC: usize = 8 * 1024;
const MAX_TITLE: usize = 120;
const MAX_BODY: usize = 500;

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
enum St {
    #[default]
    Ground,
    Esc,
    Osc,
    /// OSC 안에서 ESC를 봤다(다음이 \면 끝)
    OscEsc,
    /// 상한을 넘은 OSC를 끝까지 버리는 중
    Skip,
    SkipEsc,
    /// APC(`ESC _`) 안
    Apc,
    ApcEsc,
    /// CSI(`ESC [`) 안: 화면 지우기(ED)만 알아본다
    Csi,
}

/// 조각난 바이트를 이어 가며 신호를 찾는다
#[derive(Debug, Default)]
pub struct Scanner {
    st: St,
    buf: Vec<u8>,
    /// APC 바이트(kitty 그래픽 조각). OSC와 따로 둔다
    apc: Vec<u8>,
    /// APC가 상한을 넘어 버리는 중
    apc_skip: bool,
    /// CSI 매개변수(짧게만)
    csi: Vec<u8>,
}

/// APC 하나의 상한. kitty는 조각을 4096바이트로 보내지만 한 번에 보내는 도구도 있다
const MAX_APC: usize = 8 * 1024 * 1024;

impl Scanner {
    /// 바이트를 먹이고 끝난 OSC에서 뽑은 신호를 돌려준다
    pub fn feed(&mut self, bytes: &[u8]) -> Vec<Signal> {
        self.feed_at(bytes).into_iter().map(|(_, s)| s).collect()
    }

    /// `feed`와 같고, 신호마다 그 OSC가 끝난 바로 뒤의 자리(이번 조각 안)를 함께 돌려준다.
    /// 읽기 스레드가 그 자리까지 VT에 먹인 뒤 커서 줄을 적거나, 명령 출력을 C와 D 사이에서 잘라 낸다
    pub fn feed_at(&mut self, bytes: &[u8]) -> Vec<(usize, Signal)> {
        let mut out = Vec::new();
        for (i, &b) in bytes.iter().enumerate() {
            self.st = match (self.st, b) {
                (St::Ground, 0x1b) => St::Esc,
                (St::Ground, 0x9d) => {
                    // 8비트 OSC는 UTF-8 글의 이어지는 바이트와 겹치므로 보지 않는다
                    St::Ground
                }
                (St::Ground, _) => St::Ground,
                (St::Esc, b']') => {
                    self.buf.clear();
                    St::Osc
                }
                (St::Esc, b'_') => {
                    self.apc.clear();
                    self.apc_skip = false;
                    St::Apc
                }
                (St::Esc, b'[') => {
                    self.csi.clear();
                    St::Csi
                }
                (St::Esc, b'c') => {
                    out.push((
                        i + 1,
                        Signal::Erase {
                            visible: true,
                            scrollback: true,
                        },
                    ));
                    St::Ground
                }
                (St::Esc, 0x1b) => St::Esc,
                (St::Esc, _) => St::Ground,
                (St::Apc, 0x1b) => St::ApcEsc,
                (St::Apc, b) => {
                    if self.apc.len() >= MAX_APC {
                        self.apc = Vec::new();
                        self.apc_skip = true;
                    } else if !self.apc_skip {
                        self.apc.push(b);
                    }
                    St::Apc
                }
                (St::ApcEsc, b'\\') => {
                    let raw = std::mem::take(&mut self.apc);
                    if !self.apc_skip && raw.first() == Some(&b'G') {
                        out.push((i + 1, Signal::Graphics(raw[1..].to_vec())));
                    }
                    St::Ground
                }
                // APC 안의 ESC 뒤가 \가 아니면 끊긴 것이다
                (St::ApcEsc, _) => St::Ground,
                (St::Csi, b'0'..=b'9' | b';' | b'?') => {
                    if self.csi.len() < 16 {
                        self.csi.push(b);
                    }
                    St::Csi
                }
                (St::Csi, 0x1b) => St::Esc,
                (St::Csi, 0x40..=0x7e) => {
                    if b == b'J' {
                        match self.csi.as_slice() {
                            b"2" => out.push((
                                i + 1,
                                Signal::Erase {
                                    visible: true,
                                    scrollback: false,
                                },
                            )),
                            b"3" => out.push((
                                i + 1,
                                Signal::Erase {
                                    visible: false,
                                    scrollback: true,
                                },
                            )),
                            _ => {}
                        }
                    }
                    St::Ground
                }
                (St::Csi, _) => St::Csi,
                (St::Osc, 0x07) => {
                    self.finish(&mut out, i + 1);
                    St::Ground
                }
                (St::Osc, 0x1b) => St::OscEsc,
                (St::Osc, b) => {
                    if self.buf.len() >= MAX_OSC {
                        self.buf.clear();
                        St::Skip
                    } else {
                        self.buf.push(b);
                        St::Osc
                    }
                }
                (St::OscEsc, b'\\') => {
                    self.finish(&mut out, i + 1);
                    St::Ground
                }
                // ESC 뒤가 \가 아니면 OSC가 끊긴 것이다. 새 이스케이프의 시작으로 본다
                (St::OscEsc, b']') => {
                    self.buf.clear();
                    St::Osc
                }
                (St::OscEsc, _) => St::Ground,
                (St::Skip, 0x07) => St::Ground,
                (St::Skip, 0x1b) => St::SkipEsc,
                (St::Skip, _) => St::Skip,
                (St::SkipEsc, b'\\') => St::Ground,
                (St::SkipEsc, _) => St::Skip,
            };
        }
        out
    }

    fn finish(&mut self, out: &mut Vec<(usize, Signal)>, at: usize) {
        let raw = std::mem::take(&mut self.buf);
        let text = String::from_utf8_lossy(&raw);
        if let Some(sig) = parse_osc(&text) {
            out.push((at, sig));
        }
    }
}

fn parse_osc(s: &str) -> Option<Signal> {
    let (code, rest) = s.split_once(';')?;
    match code {
        "9" => {
            // ConEmu·Windows Terminal의 진행 표시: 9;4;상태;값
            if let Some(p) = rest.strip_prefix("4;") {
                let mut it = p.split(';');
                let state = it.next()?.trim().parse::<u8>().ok()?;
                let percent = it
                    .next()
                    .and_then(|v| v.trim().parse::<u8>().ok())
                    .unwrap_or(0)
                    .min(100);
                return Some(Signal::Progress {
                    state: state.min(4),
                    percent,
                });
            }
            // 9;1~9;12 같은 다른 ConEmu 명령은 알림이 아니다
            if rest
                .split_once(';')
                .is_some_and(|(n, _)| n.chars().all(|c| c.is_ascii_digit()) && !n.is_empty())
            {
                return None;
            }
            let body = clean(rest, MAX_BODY);
            (!body.is_empty()).then(|| {
                Signal::Notify(Notification {
                    title: String::new(),
                    body,
                })
            })
        }
        "777" => {
            // 777;notify;제목;본문
            let rest = rest.strip_prefix("notify;")?;
            let (title, body) = rest.split_once(';').unwrap_or((rest, ""));
            let (title, body) = (clean(title, MAX_TITLE), clean(body, MAX_BODY));
            (!title.is_empty() || !body.is_empty()).then_some(Signal::Notify(Notification { title, body }))
        }
        "99" => {
            // kitty: 99;메타데이터;내용. p=body면 본문, 아니면 제목. 여러 조각(d=0)은 합치지 않고 각각 알린다
            let (meta, payload) = rest.split_once(';').unwrap_or(("", rest));
            if meta.split(':').any(|kv| kv == "e=1") {
                // base64 내용은 풀지 않는다
                return None;
            }
            let is_body = meta.split(':').any(|kv| kv == "p=body");
            let text = clean(payload, MAX_BODY);
            if text.is_empty() {
                return None;
            }
            Some(Signal::Notify(if is_body {
                Notification {
                    title: String::new(),
                    body: text,
                }
            } else {
                Notification {
                    title: truncate(&text, MAX_TITLE),
                    body: String::new(),
                }
            }))
        }
        "133" | "633" => {
            let mut parts = rest.split(';');
            let kind = parts.next()?;
            Some(match kind {
                "A" => Signal::Prompt(PromptMark::PromptStart),
                "B" => Signal::Prompt(PromptMark::CommandStart),
                "C" => Signal::Prompt(PromptMark::CommandExecuted),
                "D" => Signal::Prompt(PromptMark::CommandFinished(
                    parts.next().and_then(|c| c.trim().parse().ok()),
                )),
                // VS Code: 633;E;명령(;는 \x3b, \는 \\로 적는다)
                "E" if code == "633" => {
                    let cmd = rest.split_once(';').map(|x| x.1).unwrap_or("");
                    let cmd = cmd
                        .split(';')
                        .next()
                        .unwrap_or("")
                        .replace("\\x3b", ";")
                        .replace("\\\\", "\\");
                    Signal::CommandLine(clean(&cmd, MAX_BODY))
                }
                _ => return None,
            })
        }
        "7" => {
            // 7;file://호스트/경로 (경로는 퍼센트 인코딩)
            let rest = rest.strip_prefix("file://")?;
            let slash = rest.find('/')?;
            let host = clean(&rest[..slash], 255);
            let decoded = percent_decode(&rest[slash..])?;
            (decoded.starts_with('/') && !decoded.contains('\0')).then(|| Signal::Cwd {
                host,
                path: PathBuf::from(decoded),
            })
        }
        _ => None,
    }
}

/// 제어 문자를 지우고 앞뒤 공백을 자른 뒤 길이(글자 수)를 자른다
fn clean(s: &str, max: usize) -> String {
    let filtered: String = s.chars().map(|c| if c.is_control() { ' ' } else { c }).collect();
    truncate(filtered.trim(), max)
}

fn truncate(s: &str, max: usize) -> String {
    if s.chars().count() <= max {
        return s.to_string();
    }
    let mut out: String = s.chars().take(max.saturating_sub(1)).collect();
    out.push('…');
    out
}

fn percent_decode(s: &str) -> Option<String> {
    let bytes = s.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%' {
            let hex = s.get(i + 1..i + 3)?;
            out.push(u8::from_str_radix(hex, 16).ok()?);
            i += 3;
        } else {
            out.push(bytes[i]);
            i += 1;
        }
    }
    String::from_utf8(out).ok()
}

#[cfg(test)]
mod apc_tests {
    use super::*;

    /// #76: APC _G를 조각난 바이트에서도 뽑고, 다른 APC와 화면 지우기를 가린다
    #[test]
    fn graphics_apc_and_erase_signals() {
        let mut s = Scanner::default();
        let bytes = b"ab\x1b_Ga=q,i=1;AAAA\x1b\\cd\x1b_Xother\x1b\\\x1b[2J\x1b[3J\x1b[0J\x1bc";
        let mut got = Vec::new();
        for ch in bytes.chunks(3) {
            got.extend(s.feed(ch));
        }
        assert_eq!(
            got,
            vec![
                Signal::Graphics(b"a=q,i=1;AAAA".to_vec()),
                Signal::Erase {
                    visible: true,
                    scrollback: false
                },
                Signal::Erase {
                    visible: false,
                    scrollback: true
                },
                Signal::Erase {
                    visible: true,
                    scrollback: true
                },
            ]
        );
        // 끝난 자리는 ESC \ 바로 뒤
        let at = Scanner::default().feed_at(b"xx\x1b_Gq\x1b\\yy");
        assert_eq!(at[0].0, 8, "x x ESC _ G q ESC \\ 다음 자리");
    }

    #[test]
    fn oversized_apc_is_dropped_without_growing() {
        let mut s = Scanner::default();
        s.feed(b"\x1b_G");
        let chunk = vec![b'A'; 1024 * 1024];
        for _ in 0..9 {
            assert!(s.feed(&chunk).is_empty());
        }
        assert!(s.feed(b"\x1b\\").is_empty(), "상한을 넘은 APC는 버린다");
        assert!(s.apc.capacity() <= MAX_APC * 2);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn notes(input: &[u8]) -> Vec<Signal> {
        Scanner::default().feed(input)
    }

    fn n(title: &str, body: &str) -> Signal {
        Signal::Notify(Notification {
            title: title.into(),
            body: body.into(),
        })
    }

    #[test]
    fn osc9_777_and_99_become_notifications() {
        assert_eq!(notes(b"\x1b]9;Build finished\x07"), vec![n("", "Build finished")]);
        assert_eq!(
            notes("\x1b]777;notify;make;빌드가 끝났습니다\x1b\\".as_bytes()),
            vec![n("make", "빌드가 끝났습니다")]
        );
        assert_eq!(notes(b"\x1b]99;i=1:d=0;Build done\x1b\\"), vec![n("Build done", "")]);
        assert_eq!(notes(b"\x1b]99;i=1:p=body;all green\x07"), vec![n("", "all green")]);
    }

    #[test]
    fn split_across_reads_is_reassembled() {
        let mut s = Scanner::default();
        assert!(s.feed(b"out\x1b]77").is_empty());
        assert!(s.feed(b"7;notify;ti").is_empty());
        assert_eq!(s.feed(b"tle;body\x1b").len(), 0);
        assert_eq!(s.feed(b"\\more"), vec![n("title", "body")]);
    }

    #[test]
    fn progress_is_not_a_notification() {
        assert_eq!(
            notes(b"\x1b]9;4;1;40\x07"),
            vec![Signal::Progress { state: 1, percent: 40 }]
        );
        assert_eq!(
            notes(b"\x1b]9;4;0;\x07"),
            vec![Signal::Progress { state: 0, percent: 0 }]
        );
        assert!(
            notes(b"\x1b]9;9;\"C:\\\\\"\x07").is_empty(),
            "다른 ConEmu 명령은 버린다"
        );
    }

    #[test]
    fn shell_integration_marks_and_their_positions() {
        let input = b"out\x1b]133;C\x07hello\n\x1b]133;D;2\x1b\\\x1b]133;A\x07$ \x1b]633;E;git commit -m a\\x3bb\x07";
        let got = Scanner::default().feed_at(input);
        assert_eq!(got[0], (11, Signal::Prompt(PromptMark::CommandExecuted)));
        assert_eq!(&input[11..17], b"hello\n", "C가 끝난 자리부터 출력이다");
        assert_eq!(got[1].1, Signal::Prompt(PromptMark::CommandFinished(Some(2))));
        assert_eq!(got[2].1, Signal::Prompt(PromptMark::PromptStart));
        assert_eq!(got[3].1, Signal::CommandLine("git commit -m a;b".into()));
        assert_eq!(
            Scanner::default().feed(b"\x1b]133;D\x07"),
            vec![Signal::Prompt(PromptMark::CommandFinished(None))]
        );
    }

    #[test]
    fn osc7_reports_decoded_cwd() {
        assert_eq!(
            notes(b"\x1b]7;file://mac.local/Users/me/My%20Dir\x07"),
            vec![Signal::Cwd {
                host: "mac.local".into(),
                path: PathBuf::from("/Users/me/My Dir")
            }]
        );
        assert!(notes(b"\x1b]7;http://x/y\x07").is_empty());
    }

    #[test]
    fn control_chars_are_stripped_and_length_capped() {
        let evil = format!("\x1b]777;notify;t\x1b[31m;{}\x07", "가".repeat(2000));
        // 제목 안의 ESC는 OSC를 끊는다: 끊긴 OSC는 알림을 내지 않는다
        assert!(notes(evil.as_bytes()).is_empty());
        let long = format!("\x1b]9;{}\x07", "a".repeat(5000));
        let out = notes(long.as_bytes());
        let Signal::Notify(note) = &out[0] else { panic!() };
        assert_eq!(note.body.chars().count(), MAX_BODY);
        assert!(note.body.ends_with('…'));
        let tabbed = notes(b"\x1b]9;a\tb\x07");
        assert_eq!(tabbed, vec![n("", "a b")]);
    }

    #[test]
    fn oversized_osc_is_dropped_without_growing_memory() {
        let mut s = Scanner::default();
        let mut huge = b"\x1b]9;".to_vec();
        huge.extend(std::iter::repeat_n(b'x', MAX_OSC + 10));
        huge.push(0x07);
        assert!(s.feed(&huge).is_empty());
        assert!(s.buf.capacity() <= MAX_OSC * 2);
        // 뒤따르는 정상 OSC는 받는다
        assert_eq!(s.feed(b"\x1b]9;ok\x07"), vec![n("", "ok")]);
    }
}
