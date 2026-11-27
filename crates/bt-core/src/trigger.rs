//! 트리거 설정(`trigger = 정규식 => 동작;동작`)을 읽는 한 곳(#928, ADR-102).
//!
//! iTerm2 Triggers의 안전한 부분집합이다. 동작은 넷뿐이고 모두 **화면에 찍힌 글을 입력으로 바꾸지 않는다**:
//! `highlight`(맞은 글의 글자·바탕색), `notify`(데스크톱 알림), `bell`(벨), `attention`(탭 주의 표시).
//! 글 보내기·명령 실행·프로필 바꾸기·파일 열기는 영구히 없다. 이름만 보고도 이유를 알려 주려고 오류 글에 적는다.

use crate::{Rgb, user_regex};

/// 설정에 둘 수 있는 트리거 수. 칸의 칠하기 번호가 4비트(1~15)라서 15다
pub const MAX_TRIGGERS: usize = 15;

/// `highlight`의 색. 정하지 않은 쪽은 칸의 원래 색을 그대로 둔다
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Highlight {
    pub fg: Option<Rgb>,
    pub bg: Option<Rgb>,
}

/// 트리거 한 개: 정규식과 동작들
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TriggerSpec {
    pub pattern: String,
    pub highlight: Option<Highlight>,
    pub notify: bool,
    pub bell: bool,
    pub attention: bool,
}

/// 입력이 되는 동작 이름. 일부러 지원하지 않는다고 알려 주는 데만 쓴다
const FORBIDDEN: &[&str] = &[
    "send",
    "sendtext",
    "inject",
    "run",
    "runcommand",
    "coprocess",
    "script",
    "open",
    "profile",
    "exec",
    "shell",
    "password",
];

fn hex(c: Rgb) -> String {
    format!("#{:02x}{:02x}{:02x}", c.r, c.g, c.b)
}

impl TriggerSpec {
    /// `정규식 => 동작;동작` 한 줄을 읽는다. `=>`는 맨 뒤의 것을 가른다(정규식 안에 `=>`가 있어도 된다).
    /// 정규식의 앞뒤 공백은 지운다(공백이 필요하면 `[ ]`나 `\x20`). 못 읽으면 사람이 읽을 이유를 돌려준다
    pub fn parse(value: &str) -> Result<Self, String> {
        let Some((pattern, actions)) = value.rsplit_once("=>") else {
            return Err("`정규식 => 동작` 꼴이 아닙니다".into());
        };
        let pattern = pattern.trim();
        user_regex::compile(pattern, false)?;
        let mut spec = TriggerSpec {
            pattern: pattern.to_string(),
            highlight: None,
            notify: false,
            bell: false,
            attention: false,
        };
        for item in actions.split(';') {
            let item = item.trim();
            if item.is_empty() {
                continue;
            }
            let (name, args) = match item.split_once(':') {
                Some((n, a)) => (n.trim(), Some(a.trim())),
                None => (item, None),
            };
            let dup = |name: &str| format!("동작 {name}이 두 번 나옵니다");
            match (name, args) {
                ("highlight", Some(args)) => {
                    if spec.highlight.is_some() {
                        return Err(dup(name));
                    }
                    spec.highlight = Some(parse_highlight(args)?);
                }
                ("highlight", None) => {
                    return Err("highlight에는 색이 필요합니다(highlight:fg=#rrggbb,bg=#rrggbb)".into());
                }
                ("notify" | "bell" | "attention", Some(_)) => {
                    return Err(format!("{name}은 값을 받지 않습니다"));
                }
                ("notify", None) => {
                    if std::mem::replace(&mut spec.notify, true) {
                        return Err(dup(name));
                    }
                }
                ("bell", None) => {
                    if std::mem::replace(&mut spec.bell, true) {
                        return Err(dup(name));
                    }
                }
                ("attention", None) => {
                    if std::mem::replace(&mut spec.attention, true) {
                        return Err(dup(name));
                    }
                }
                (other, _) => {
                    let lower = other.to_ascii_lowercase().replace(['_', '-', ' '], "");
                    return Err(if FORBIDDEN.contains(&lower.as_str()) {
                        format!(
                            "{other}은 지원하지 않습니다: 화면에 찍힌 글이 입력이나 명령이 되는 동작은 넣지 않습니다(동작은 highlight·notify·bell·attention뿐)"
                        )
                    } else {
                        format!("모르는 동작입니다: {other}(동작은 highlight·notify·bell·attention)")
                    });
                }
            }
        }
        if spec.highlight.is_none() && !spec.notify && !spec.bell && !spec.attention {
            return Err("동작이 없습니다(highlight·notify·bell·attention 중 하나 이상)".into());
        }
        Ok(spec)
    }

    /// 설정 파일에 되쓸 꼴(`parse`가 다시 같은 값으로 읽는다)
    pub fn to_config_value(&self) -> String {
        let mut acts = Vec::new();
        if let Some(h) = &self.highlight {
            let mut parts = Vec::new();
            if let Some(c) = h.fg {
                parts.push(format!("fg={}", hex(c)));
            }
            if let Some(c) = h.bg {
                parts.push(format!("bg={}", hex(c)));
            }
            acts.push(format!("highlight:{}", parts.join(",")));
        }
        if self.notify {
            acts.push("notify".into());
        }
        if self.bell {
            acts.push("bell".into());
        }
        if self.attention {
            acts.push("attention".into());
        }
        format!("{} => {}", self.pattern, acts.join(";"))
    }

    /// 알림·벨·탭 표시 중 하나라도 있다(읽는 쪽이 새 줄을 검사해야 한다)
    pub fn fires(&self) -> bool {
        self.notify || self.bell || self.attention
    }
}

fn parse_highlight(args: &str) -> Result<Highlight, String> {
    let mut h = Highlight::default();
    for kv in args.split(',') {
        let kv = kv.trim();
        let Some((k, v)) = kv.split_once('=') else {
            return Err(format!("highlight 값을 읽지 못했습니다: {kv}(fg=색 또는 bg=색)"));
        };
        let c = Rgb::parse_color(v).ok_or_else(|| format!("색을 읽지 못했습니다: {}", v.trim()))?;
        let slot = match k.trim() {
            "fg" => &mut h.fg,
            "bg" => &mut h.bg,
            other => return Err(format!("highlight에 모르는 값입니다: {other}(fg·bg만)")),
        };
        if slot.replace(c).is_some() {
            return Err(format!("highlight의 {}가 두 번 나옵니다", k.trim()));
        }
    }
    Ok(h)
}
