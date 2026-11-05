//! 키 묶음. Ghostty의 `keybind = 트리거=동작` 문법과 동작 이름을 그대로 받는다(#27).
//!
//! 기본 묶음(Ghostty macOS 기본의 일부) 위에 설정의 keybind를 차례로 얹는다.
//! `트리거=unbind`는 그 묶음을 지우고, `keybind = clear`는 기본 묶음을 모두 지운다

use std::collections::HashMap;

use crate::split::Direction;

/// 수식 키 묶음
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub struct Mods {
    pub shift: bool,
    pub ctrl: bool,
    pub alt: bool,
    pub sup: bool,
}

/// 트리거: 수식 키 + 키 이름(Ghostty 이름으로 맞춘다: `t`, `digit_1`, `bracket_left`, `page_up`…)
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Trigger {
    pub mods: Mods,
    pub key: String,
}

/// 동작
#[derive(Debug, Clone, PartialEq)]
pub enum Action {
    Copy,
    Paste,
    NewTab,
    /// 지금 칸을 닫는다(칸이 하나면 탭이 닫힌다)
    CloseSurface,
    /// 탭을 칸째 닫는다
    CloseTab,
    /// 지금 칸을 나눠 그쪽에 새 칸을 연다. `None`이면 긴 쪽으로 나눈다(Ghostty의 auto)
    NewSplit(Option<Direction>),
    GotoSplit(SplitTarget),
    /// 경계를 그쪽으로 민다(포인트)
    ResizeSplit(Direction, u16),
    EqualizeSplits,
    ToggleSplitZoom,
    /// 1부터
    GotoTab(usize),
    NextTab,
    PrevTab,
    LastTab,
    FontBigger(f32),
    FontSmaller(f32),
    FontReset,
    ScrollPageUp,
    ScrollPageDown,
    ScrollTop,
    ScrollBottom,
    ClearScreen,
    ReloadConfig,
    SelectAll,
    /// 앞(-1)·뒤(+1) 프롬프트로(셸 통합)
    JumpToPrompt(i32),
    /// `text:…`: 글을 그대로 PTY에 보낸다(`\n`, `\x1b` 같은 이스케이프를 푼다)
    Text(String),
}

/// goto_split의 대상
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SplitTarget {
    Previous,
    Next,
    Toward(Direction),
}

/// Ghostty의 방향 이름(top·bottom도 받는다)
fn parse_direction(s: &str) -> Option<Direction> {
    Some(match s.trim() {
        "left" => Direction::Left,
        "right" => Direction::Right,
        "up" | "top" => Direction::Up,
        "down" | "bottom" => Direction::Down,
        _ => return None,
    })
}

/// 키 이름을 Ghostty 이름으로 맞춘다
pub fn normalize_key(k: &str) -> String {
    let k = k.trim().to_ascii_lowercase();
    match k.as_str() {
        "=" | "plus" => "equal".into(),
        "-" => "minus".into(),
        "," => "comma".into(),
        "." => "period".into(),
        "/" => "slash".into(),
        ";" => "semicolon".into(),
        "'" => "quote".into(),
        "`" => "backquote".into(),
        "[" | "left_bracket" => "bracket_left".into(),
        "]" | "right_bracket" => "bracket_right".into(),
        "\\" => "backslash".into(),
        " " => "space".into(),
        "return" => "enter".into(),
        "esc" => "escape".into(),
        "up" => "arrow_up".into(),
        "down" => "arrow_down".into(),
        "left" => "arrow_left".into(),
        "right" => "arrow_right".into(),
        "pageup" => "page_up".into(),
        "pagedown" => "page_down".into(),
        d if d.len() == 1 && d.as_bytes()[0].is_ascii_digit() => format!("digit_{d}"),
        _ => k,
    }
}

/// `super+shift+t`를 읽는다. 키 순서(`a>b`)는 받지 않는다
pub fn parse_trigger(s: &str) -> Result<Trigger, String> {
    if s.contains('>') {
        return Err(format!("키 순서는 아직 받지 않습니다: {s}"));
    }
    let mut mods = Mods::default();
    // `super++`처럼 키 자체가 +인 경우: 마지막 +를 키로 떼어 낸다
    let (head, key) = match s.strip_suffix("++") {
        Some(h) => (h, "plus"),
        None => s.rsplit_once('+').unwrap_or(("", s)),
    };
    for m in head.split('+').filter(|m| !m.trim().is_empty()) {
        match m.trim().to_ascii_lowercase().as_str() {
            "shift" => mods.shift = true,
            "ctrl" | "control" => mods.ctrl = true,
            "alt" | "opt" | "option" => mods.alt = true,
            "super" | "cmd" | "command" => mods.sup = true,
            other => return Err(format!("모르는 수식 키: {other}")),
        }
    }
    if key.trim().is_empty() {
        return Err(format!("키가 없습니다: {s}"));
    }
    Ok(Trigger {
        mods,
        key: normalize_key(key),
    })
}

fn unescape(s: &str) -> String {
    let mut out = String::new();
    let mut it = s.chars().peekable();
    while let Some(c) = it.next() {
        if c != '\\' {
            out.push(c);
            continue;
        }
        match it.next() {
            Some('n') => out.push('\n'),
            Some('r') => out.push('\r'),
            Some('t') => out.push('\t'),
            Some('e') => out.push('\x1b'),
            Some('\\') => out.push('\\'),
            Some('x') => {
                let hex: String = it.by_ref().take(2).collect();
                if let Ok(b) = u8::from_str_radix(&hex, 16) {
                    out.push(b as char);
                }
            }
            Some(other) => {
                out.push('\\');
                out.push(other);
            }
            None => out.push('\\'),
        }
    }
    out
}

/// 동작 이름을 읽는다. `None`이면 unbind
pub fn parse_action(s: &str) -> Result<Option<Action>, String> {
    let (name, arg) = s
        .split_once(':')
        .map(|(a, b)| (a.trim(), Some(b)))
        .unwrap_or((s.trim(), None));
    let num = |d: f32| arg.and_then(|a| a.trim().parse::<f32>().ok()).unwrap_or(d);
    Ok(Some(match name {
        "unbind" => return Ok(None),
        "copy_to_clipboard" => Action::Copy,
        "paste_from_clipboard" | "paste_from_selection" => Action::Paste,
        "new_tab" | "new_window" => Action::NewTab,
        "close_surface" => Action::CloseSurface,
        "close_tab" => Action::CloseTab,
        "new_split" => match arg.map(str::trim).unwrap_or("auto") {
            "auto" => Action::NewSplit(None),
            d => Action::NewSplit(Some(parse_direction(d).ok_or("new_split:right·down·left·up·auto")?)),
        },
        "goto_split" => Action::GotoSplit(match arg.map(str::trim).unwrap_or("") {
            "previous" => SplitTarget::Previous,
            "next" => SplitTarget::Next,
            d => SplitTarget::Toward(parse_direction(d).ok_or("goto_split:previous·next·left·right·up·down")?),
        }),
        "resize_split" => {
            let (d, n) = arg.unwrap_or("").split_once(',').ok_or("resize_split:방향,크기")?;
            Action::ResizeSplit(
                parse_direction(d).ok_or("resize_split의 방향은 left·right·up·down")?,
                n.trim().parse().map_err(|_| "resize_split의 크기는 정수")?,
            )
        }
        "equalize_splits" => Action::EqualizeSplits,
        "toggle_split_zoom" => Action::ToggleSplitZoom,
        "goto_tab" => Action::GotoTab(
            arg.and_then(|a| a.trim().parse().ok())
                .filter(|n| *n >= 1)
                .ok_or("goto_tab:N이 필요합니다")?,
        ),
        "next_tab" => Action::NextTab,
        "previous_tab" => Action::PrevTab,
        "last_tab" => Action::LastTab,
        "increase_font_size" => Action::FontBigger(num(1.0)),
        "decrease_font_size" => Action::FontSmaller(num(1.0)),
        "reset_font_size" => Action::FontReset,
        "scroll_page_up" => Action::ScrollPageUp,
        "scroll_page_down" => Action::ScrollPageDown,
        "scroll_to_top" => Action::ScrollTop,
        "scroll_to_bottom" => Action::ScrollBottom,
        "clear_screen" => Action::ClearScreen,
        "reload_config" => Action::ReloadConfig,
        "select_all" => Action::SelectAll,
        "jump_to_prompt" => Action::JumpToPrompt(arg.and_then(|a| a.trim().parse().ok()).unwrap_or(-1)),
        "text" => Action::Text(unescape(arg.unwrap_or(""))),
        "csi" => Action::Text(format!("\x1b[{}", arg.unwrap_or(""))),
        "esc" => Action::Text(format!("\x1b{}", arg.unwrap_or(""))),
        other => return Err(format!("b-terminal에 없는 동작입니다: {other}")),
    }))
}

/// 트리거 → 동작
#[derive(Debug, Clone, Default)]
pub struct Keymap {
    map: HashMap<Trigger, Action>,
}

/// 기본 묶음(Ghostty macOS 기본에서 b-terminal에 있는 것). 다른 OS는 super 대신 ctrl+shift
fn defaults() -> Vec<(&'static str, &'static str)> {
    let mut v = vec![
        ("super+c", "copy_to_clipboard"),
        ("super+v", "paste_from_clipboard"),
        ("super+t", "new_tab"),
        ("super+w", "close_surface"),
        ("super+alt+w", "close_tab"),
        ("super+shift+bracket_left", "previous_tab"),
        ("super+shift+bracket_right", "next_tab"),
        // 화면 분할(Ghostty macOS 기본과 같다)
        ("super+d", "new_split:right"),
        ("super+shift+d", "new_split:down"),
        ("super+bracket_left", "goto_split:previous"),
        ("super+bracket_right", "goto_split:next"),
        ("super+alt+arrow_up", "goto_split:up"),
        ("super+alt+arrow_down", "goto_split:down"),
        ("super+alt+arrow_left", "goto_split:left"),
        ("super+alt+arrow_right", "goto_split:right"),
        ("super+ctrl+arrow_up", "resize_split:up,10"),
        ("super+ctrl+arrow_down", "resize_split:down,10"),
        ("super+ctrl+arrow_left", "resize_split:left,10"),
        ("super+ctrl+arrow_right", "resize_split:right,10"),
        ("super+ctrl+equal", "equalize_splits"),
        ("super+shift+enter", "toggle_split_zoom"),
        ("super+equal", "increase_font_size:1"),
        ("super+shift+equal", "increase_font_size:1"),
        ("super+minus", "decrease_font_size:1"),
        ("super+0", "reset_font_size"),
        ("super+k", "clear_screen"),
        ("super+a", "select_all"),
        ("super+shift+comma", "reload_config"),
        ("super+home", "scroll_to_top"),
        ("super+end", "scroll_to_bottom"),
        ("super+page_up", "scroll_page_up"),
        ("super+page_down", "scroll_page_down"),
        ("super+arrow_up", "jump_to_prompt:-1"),
        ("super+arrow_down", "jump_to_prompt:1"),
        ("shift+page_up", "scroll_page_up"),
        ("shift+page_down", "scroll_page_down"),
        ("shift+home", "scroll_to_top"),
        ("shift+end", "scroll_to_bottom"),
    ];
    for (t, a) in [
        ("super+1", "goto_tab:1"),
        ("super+2", "goto_tab:2"),
        ("super+3", "goto_tab:3"),
        ("super+4", "goto_tab:4"),
        ("super+5", "goto_tab:5"),
        ("super+6", "goto_tab:6"),
        ("super+7", "goto_tab:7"),
        ("super+8", "goto_tab:8"),
        ("super+9", "last_tab"),
    ] {
        v.push((t, a));
    }
    v
}

impl Keymap {
    /// 기본 묶음 위에 설정의 keybind를 얹는다. 읽지 못한 줄은 경고로 돌려준다
    pub fn build(user: &[String]) -> (Self, Vec<String>) {
        let mut km = Keymap::default();
        let mut warnings = Vec::new();
        let swap_super = !cfg!(target_os = "macos");
        for (t, a) in defaults() {
            if let (Ok(mut trig), Ok(Some(act))) = (parse_trigger(t), parse_action(a)) {
                if swap_super && trig.mods.sup {
                    trig.mods.sup = false;
                    trig.mods.ctrl = true;
                    trig.mods.shift = true;
                }
                km.map.insert(trig, act);
            }
        }
        for line in user {
            let line = line.trim();
            if line == "clear" {
                km.map.clear();
                continue;
            }
            // 트리거에도 =가 올 수 있다(super+=). 마지막 =를 나눔 자리로 본다
            let Some(eq) = line.rfind('=') else {
                warnings.push(format!("keybind는 `트리거=동작` 꼴입니다: {line}"));
                continue;
            };
            let (t, a) = (&line[..eq], &line[eq + 1..]);
            // text:에 =가 들어간 경우를 위해 동작 이름이 알려진 첫 =로 다시 나눠 본다
            let (t, a) = match line.find('=') {
                Some(first) if first < eq && parse_action(&line[first + 1..]).is_ok() => {
                    (&line[..first], &line[first + 1..])
                }
                _ => (t, a),
            };
            let trig = match parse_trigger(t) {
                Ok(tr) => tr,
                Err(e) => {
                    warnings.push(format!("keybind {line}: {e}"));
                    continue;
                }
            };
            match parse_action(a) {
                Ok(Some(act)) => {
                    km.map.insert(trig, act);
                }
                Ok(None) => {
                    km.map.remove(&trig);
                }
                Err(e) => warnings.push(format!("keybind {line}: {e}")),
            }
        }
        (km, warnings)
    }

    pub fn lookup(&self, t: &Trigger) -> Option<&Action> {
        self.map.get(t)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn trig(s: &str) -> Trigger {
        parse_trigger(s).unwrap()
    }

    #[test]
    fn triggers_and_key_names() {
        let t = trig("super+shift+T");
        assert!(t.mods.sup && t.mods.shift && !t.mods.ctrl);
        assert_eq!(t.key, "t");
        assert_eq!(trig("ctrl+1").key, "digit_1");
        assert_eq!(trig("ctrl+digit_1").key, "digit_1");
        assert_eq!(trig("super+]").key, "bracket_right");
        assert_eq!(trig("super++").key, "equal");
        assert!(parse_trigger("ctrl+a>n").is_err());
        assert!(parse_trigger("hyper+a").is_err());
    }

    #[test]
    fn actions() {
        assert_eq!(parse_action("goto_tab:3").unwrap(), Some(Action::GotoTab(3)));
        assert_eq!(
            parse_action("increase_font_size:2").unwrap(),
            Some(Action::FontBigger(2.0))
        );
        assert_eq!(parse_action("text:hi\\n").unwrap(), Some(Action::Text("hi\n".into())));
        assert_eq!(parse_action("text:\\x1bb").unwrap(), Some(Action::Text("\x1bb".into())));
        assert_eq!(parse_action("unbind").unwrap(), None);
        assert_eq!(
            parse_action("toggle_split_zoom").unwrap(),
            Some(Action::ToggleSplitZoom)
        );
        assert_eq!(
            parse_action("new_split:down").unwrap(),
            Some(Action::NewSplit(Some(Direction::Down)))
        );
        assert_eq!(parse_action("new_split").unwrap(), Some(Action::NewSplit(None)));
        assert_eq!(
            parse_action("goto_split:top").unwrap(),
            Some(Action::GotoSplit(SplitTarget::Toward(Direction::Up)))
        );
        assert_eq!(
            parse_action("resize_split:left,20").unwrap(),
            Some(Action::ResizeSplit(Direction::Left, 20))
        );
        assert_eq!(parse_action("close_surface").unwrap(), Some(Action::CloseSurface));
        assert!(parse_action("resize_split:left").is_err());
        assert!(parse_action("new_split:diagonal").is_err());
        assert!(parse_action("toggle_fullscreen").is_err());
        assert!(parse_action("goto_tab:0").is_err());
    }

    #[test]
    fn user_binds_override_and_unbind_defaults() {
        let (km, w) = Keymap::build(&[
            "super+t=unbind".into(),
            "super+shift+n=new_tab".into(),
            "ctrl+alt+e=text:echo a=b\\r".into(),
            "super+x=no_such_action".into(),
        ]);
        let sup = |k: &str| Trigger {
            mods: Mods {
                sup: !cfg!(not(target_os = "macos")),
                ..Mods::default()
            },
            key: k.into(),
        };
        if cfg!(target_os = "macos") {
            assert_eq!(km.lookup(&sup("t")), None, "unbind로 기본 묶음을 지운다");
            assert_eq!(km.lookup(&sup("c")), Some(&Action::Copy), "건드리지 않은 기본은 남는다");
            assert_eq!(km.lookup(&trig("super+shift+n")), Some(&Action::NewTab));
        }
        assert_eq!(
            km.lookup(&trig("ctrl+alt+e")),
            Some(&Action::Text("echo a=b\r".into())),
            "text 안의 =는 그대로"
        );
        assert_eq!(w.len(), 1, "{w:?}");
    }

    #[test]
    fn clear_removes_all_defaults() {
        let (km, _) = Keymap::build(&["clear".into(), "super+q=new_tab".into()]);
        assert_eq!(km.map.len(), 1);
    }
}
