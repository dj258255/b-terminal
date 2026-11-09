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
    /// 새 창(#36)
    NewWindow,
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
    /// 찾기 막대를 연다(#39)
    StartSearch,
    EndSearch,
    /// 퀵 터미널을 보이고 숨긴다(#95)
    ToggleQuickTerminal,
    /// 명령 팔레트를 열고 닫는다(#77)
    TogglePalette,
    /// 다음 맞은 곳: 참이면 위(오래된 쪽)로
    NavigateSearch(bool),
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
    /// 전체 화면 켜고 끄기(Ghostty `toggle_fullscreen`, #126)
    ToggleFullscreen,
    /// 줄 단위 스크롤: 양수면 아래로(Ghostty `scroll_page_lines`)
    ScrollLines(i32),
    /// 화면의 몇 분의 일 스크롤: 양수면 아래로(Ghostty `scroll_page_fractional`)
    ScrollFraction(f32),
    /// 앱을 끝낸다(Ghostty `quit`, ⌘Q). 프로그램이 도는 칸이 있으면 묻는다(#108)
    Quit,
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
        "`" | "grave_accent" => "backquote".into(),
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

/// `super+shift+t`를 읽는다. 키 순서(`a>b`)는 `Keymap::build`가 `>`로 나눠 키마다 부른다
pub fn parse_trigger(s: &str) -> Result<Trigger, String> {
    if s.contains('>') {
        return Err(format!("키 하나에 `>`가 있습니다: {s}"));
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
        "new_tab" => Action::NewTab,
        "new_window" => Action::NewWindow,
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
        "start_search" => Action::StartSearch,
        "toggle_quick_terminal" => Action::ToggleQuickTerminal,
        "toggle_command_palette" => Action::TogglePalette,
        "end_search" => Action::EndSearch,
        // next는 위(오래된 출력)로: 터미널은 맨 아래에서 찾기 시작한다(iTerm2와 같다)
        "navigate_search" => match arg.map(str::trim).unwrap_or("next") {
            "next" => Action::NavigateSearch(true),
            "previous" => Action::NavigateSearch(false),
            _ => return Err("navigate_search:next·previous".into()),
        },
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
        "quit" => Action::Quit,
        "toggle_fullscreen" => Action::ToggleFullscreen,
        "scroll_page_lines" => Action::ScrollLines(
            arg.and_then(|a| a.trim().parse().ok())
                .ok_or("scroll_page_lines:줄 수(양수면 아래로)")?,
        ),
        "scroll_page_fractional" => Action::ScrollFraction(
            arg.and_then(|a| a.trim().parse::<f32>().ok())
                .filter(|f| f.is_finite())
                .ok_or("scroll_page_fractional:화면의 몇 분의 일(양수면 아래로)")?,
        ),
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
    /// 키 순서(`ctrl+a>n`, #106): 두 키 이상. 한 키짜리는 `map`에 둔다
    seqs: HashMap<Vec<Trigger>, Action>,
    /// 전역 단축키(`keybind = global:…`, #95): 앱이 앞에 없어도 받는다
    pub global: Vec<(Trigger, Action)>,
}

/// 기본 묶음(Ghostty macOS 기본에서 b-terminal에 있는 것). 다른 OS는 super 대신 ctrl+shift
fn defaults() -> Vec<(&'static str, &'static str)> {
    let mut v = vec![
        ("super+c", "copy_to_clipboard"),
        ("super+v", "paste_from_clipboard"),
        ("super+t", "new_tab"),
        ("super+n", "new_window"),
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
        // 찾기(Ghostty와 같은 동작 이름)
        ("super+f", "start_search"),
        ("super+shift+p", "toggle_command_palette"),
        ("super+g", "navigate_search:next"),
        ("super+shift+g", "navigate_search:previous"),
        ("super+equal", "increase_font_size:1"),
        ("super+shift+equal", "increase_font_size:1"),
        ("super+minus", "decrease_font_size:1"),
        ("super+0", "reset_font_size"),
        ("super+k", "clear_screen"),
        ("super+a", "select_all"),
        ("super+shift+comma", "reload_config"),
        ("super+q", "quit"),
        ("super+ctrl+f", "toggle_fullscreen"),
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
                km.seqs.clear();
                continue;
            }
            // 트리거에도 =가 올 수 있다(super+=). 마지막 =를 나눔 자리로 본다
            let Some(eq) = line.rfind('=') else {
                warnings.push(format!("keybind는 `트리거=동작` 꼴입니다: {line}"));
                continue;
            };
            let (t, a) = (&line[..eq], &line[eq + 1..]);
            // global: 접두사(Ghostty와 같다): 전역 단축키로 따로 둔다
            if let Some(t) = t.strip_prefix("global:") {
                match (parse_trigger(t), parse_action(a)) {
                    (Ok(trig), Ok(Some(act))) => {
                        km.global.retain(|(g, _)| *g != trig);
                        km.global.push((trig, act));
                    }
                    (Ok(trig), Ok(None)) => km.global.retain(|(g, _)| *g != trig),
                    (Err(e), _) | (_, Err(e)) => warnings.push(format!("keybind {line}: {e}")),
                }
                continue;
            }
            // text:에 =가 들어간 경우를 위해 동작 이름이 알려진 첫 =로 다시 나눠 본다
            let (t, a) = match line.find('=') {
                Some(first) if first < eq && parse_action(&line[first + 1..]).is_ok() => {
                    (&line[..first], &line[first + 1..])
                }
                _ => (t, a),
            };
            let keys = match t.split('>').map(parse_trigger).collect::<Result<Vec<_>, _>>() {
                Ok(k) => k,
                Err(e) => {
                    warnings.push(format!("keybind {line}: {e}"));
                    continue;
                }
            };
            match parse_action(a) {
                Ok(Some(act)) => km.bind(keys, act),
                Ok(None) => km.unbind(&keys),
                Err(e) => warnings.push(format!("keybind {line}: {e}")),
            }
        }
        (km, warnings)
    }

    /// 전역 단축키 글(global-hotkey 크레이트의 꼴, 예: `ctrl+Backquote`). 받지 못하는 키면 없다
    pub fn hotkey_string(t: &Trigger) -> Option<String> {
        let key = match t.key.as_str() {
            k if k.len() == 1 && k.as_bytes()[0].is_ascii_lowercase() => k.to_ascii_uppercase(),
            k if k.starts_with("digit_") => format!("Digit{}", &k["digit_".len()..]),
            k if k.len() >= 2 && k.starts_with('f') && k[1..].parse::<u8>().is_ok_and(|n| (1..=24).contains(&n)) => {
                k.to_ascii_uppercase()
            }
            "backquote" => "Backquote".into(),
            "space" => "Space".into(),
            "enter" => "Enter".into(),
            "tab" => "Tab".into(),
            "escape" => "Escape".into(),
            "minus" => "Minus".into(),
            "equal" => "Equal".into(),
            "comma" => "Comma".into(),
            "period" => "Period".into(),
            "slash" => "Slash".into(),
            "semicolon" => "Semicolon".into(),
            "quote" => "Quote".into(),
            "backslash" => "Backslash".into(),
            "bracket_left" => "BracketLeft".into(),
            "bracket_right" => "BracketRight".into(),
            _ => return None,
        };
        let mut parts = Vec::new();
        if t.mods.sup {
            parts.push("super".to_string());
        }
        if t.mods.ctrl {
            parts.push("ctrl".to_string());
        }
        if t.mods.alt {
            parts.push("alt".to_string());
        }
        if t.mods.shift {
            parts.push("shift".to_string());
        }
        parts.push(key);
        Some(parts.join("+"))
    }

    #[cfg(test)]
    pub fn lookup(&self, t: &Trigger) -> Option<&Action> {
        self.map.get(t)
    }

    /// 키(들)에 동작을 건다. 겹치는 묶음은 나중 것이 이긴다(Ghostty):
    /// 순서의 앞 키에 걸린 한 키 묶음·짧은 순서, 이 키들로 시작하는 긴 순서를 지운다
    fn bind(&mut self, keys: Vec<Trigger>, action: Action) {
        let n = keys.len();
        self.seqs
            .retain(|k, _| !(k.len() < n && keys.starts_with(k) || k.len() > n && k.starts_with(&keys)));
        if n == 1 {
            self.map.insert(keys[0].clone(), action);
        } else {
            self.map.remove(&keys[0]);
            self.seqs.insert(keys, action);
        }
    }

    fn unbind(&mut self, keys: &[Trigger]) {
        match keys {
            [t] => {
                self.map.remove(t);
            }
            _ => {
                self.seqs.remove(keys);
            }
        }
    }

    /// 키들(한 키 이상)에 걸린 동작
    fn sequence(&self, keys: &[Trigger]) -> Option<&Action> {
        match keys {
            [t] => self.map.get(t),
            _ => self.seqs.get(keys),
        }
    }

    /// 이 키들로 시작하는 더 긴 순서가 있는가
    fn is_prefix(&self, keys: &[Trigger]) -> bool {
        self.seqs.keys().any(|k| k.len() > keys.len() && k.starts_with(keys))
    }

    /// 이 동작에 걸린 트리거들(명령 팔레트가 단축키를 보일 때)
    pub fn triggers_for(&self, action: &Action) -> Vec<&Trigger> {
        self.map.iter().filter(|(_, a)| *a == action).map(|(t, _)| t).collect()
    }
}

/// 키를 하나씩 받아 키 순서를 맞춘다(#106). Ghostty처럼 시간 제한은 없고,
/// 순서 중간에 묶이지 않은 키가 오면 그때까지 쌓은 키를 프로그램에 그대로 보낸다
#[derive(Debug, Default)]
pub struct Sequencer {
    /// 기다리는 앞 키들과, 누른 순간에 만든 PTY 바이트
    pending: Vec<(Trigger, Vec<u8>)>,
}

/// 키 하나를 받은 결과: PTY에 보낼 바이트(먼저)와 부를 동작(나중)
#[derive(Debug, Default, PartialEq)]
pub struct KeyOutcome {
    pub send: Vec<u8>,
    pub action: Option<Action>,
}

impl Sequencer {
    /// 앞 키를 받고 다음 키를 기다리는 중인가
    #[cfg(test)]
    pub fn waiting(&self) -> bool {
        !self.pending.is_empty()
    }

    /// 키 하나(`trigger`: 키 묶음 이름, 수식 키만이면 없다; `bytes`: 그 키가 PTY에 보낼 바이트)
    pub fn feed(&mut self, km: &Keymap, trigger: Option<Trigger>, bytes: Vec<u8>) -> KeyOutcome {
        match &trigger {
            Some(t) => {
                let mut keys: Vec<Trigger> = self.pending.iter().map(|(k, _)| k.clone()).collect();
                keys.push(t.clone());
                if let Some(action) = km.sequence(&keys).cloned() {
                    self.pending.clear();
                    return KeyOutcome {
                        send: Vec::new(),
                        action: Some(action),
                    };
                }
                if km.is_prefix(&keys) {
                    self.pending.push((t.clone(), bytes));
                    return KeyOutcome::default();
                }
            }
            // 수식 키만 누른 것: 순서 중이어도 깨지 않는다
            None if bytes.is_empty() => return KeyOutcome::default(),
            None => {}
        }
        if self.pending.is_empty() {
            return KeyOutcome {
                send: bytes,
                action: None,
            };
        }
        // 순서가 깨졌다: 쌓은 키를 보내고, 이 키는 처음부터 다시 본다(새 순서의 앞 키일 수도 있다)
        let mut send: Vec<u8> = self.pending.drain(..).flat_map(|(_, b)| b).collect();
        let rest = self.feed(km, trigger, bytes);
        send.extend(rest.send);
        KeyOutcome {
            send,
            action: rest.action,
        }
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
        assert_eq!(parse_action("quit").unwrap(), Some(Action::Quit));
        assert_eq!(parse_action("start_search").unwrap(), Some(Action::StartSearch));
        assert_eq!(
            parse_action("new_window").unwrap(),
            Some(Action::NewWindow),
            "새 탭이 아니라 새 창"
        );
        assert_eq!(
            parse_action("navigate_search:previous").unwrap(),
            Some(Action::NavigateSearch(false))
        );
        assert!(parse_action("navigate_search:sideways").is_err());
        assert!(parse_action("resize_split:left").is_err());
        assert!(parse_action("new_split:diagonal").is_err());
        assert_eq!(
            parse_action("toggle_fullscreen").unwrap(),
            Some(Action::ToggleFullscreen)
        );
        assert_eq!(
            parse_action("scroll_page_lines:-3").unwrap(),
            Some(Action::ScrollLines(-3))
        );
        assert_eq!(
            parse_action("scroll_page_fractional:0.5").unwrap(),
            Some(Action::ScrollFraction(0.5))
        );
        assert!(parse_action("scroll_page_lines").is_err(), "줄 수가 있어야 한다");
        assert!(parse_action("scroll_page_fractional:x").is_err());
        assert!(parse_action("toggle_window_decorations").is_err());
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
            assert_eq!(
                km.lookup(&sup("q")),
                Some(&Action::Quit),
                "⌘Q도 키 묶음으로(닫기 확인을 거치게)"
            );
        }
        assert_eq!(
            km.lookup(&trig("ctrl+alt+e")),
            Some(&Action::Text("echo a=b\r".into())),
            "text 안의 =는 그대로"
        );
        assert_eq!(w.len(), 1, "{w:?}");
    }

    #[test]
    fn global_keybinds_are_kept_apart_and_converted() {
        let (km, w) = Keymap::build(&[
            "global:ctrl+grave_accent=toggle_quick_terminal".into(),
            "global:super+shift+f9=new_window".into(),
            "global:super+shift+f9=unbind".into(),
        ]);
        assert!(w.is_empty(), "{w:?}");
        assert_eq!(km.global.len(), 1, "unbind로 지운다");
        let (t, a) = &km.global[0];
        assert_eq!(a, &Action::ToggleQuickTerminal);
        assert_eq!(Keymap::hotkey_string(t).as_deref(), Some("ctrl+Backquote"));
        assert!(km.lookup(t).is_none(), "앱 안 키 묶음에는 넣지 않는다");
        assert_eq!(
            Keymap::hotkey_string(&trig("super+alt+t")).as_deref(),
            Some("super+alt+T")
        );
        assert_eq!(
            Keymap::hotkey_string(&trig("ctrl+digit_1")).as_deref(),
            Some("ctrl+Digit1")
        );
        assert_eq!(Keymap::hotkey_string(&trig("ctrl+page_up")), None);
    }

    #[test]
    fn sequences_parse_and_replace_overlapping_binds() {
        let (km, w) = Keymap::build(&[
            "ctrl+a=new_tab".into(),
            "ctrl+a>n=new_window".into(),
            "ctrl+a>shift+x>c=close_surface".into(),
            "ctrl+b>d=new_split:down".into(),
            "ctrl+b>d=unbind".into(),
            "ctrl+a>>n=new_tab".into(),
        ]);
        assert_eq!(w.len(), 1, "빈 키가 낀 순서는 경고: {w:?}");
        assert_eq!(
            km.lookup(&trig("ctrl+a")),
            None,
            "앞 키가 되면 한 키 묶음은 지운다(나중 것이 이김)"
        );
        assert_eq!(km.sequence(&[trig("ctrl+a"), trig("n")]), Some(&Action::NewWindow));
        assert!(km.is_prefix(&[trig("ctrl+a")]));
        assert!(km.is_prefix(&[trig("ctrl+a"), trig("shift+x")]));
        assert!(!km.is_prefix(&[trig("ctrl+b")]), "unbind로 지운 순서는 앞 키도 아니다");
        // 다시 한 키로 묶으면 그 키로 시작하는 순서를 지운다
        let (km, _) = Keymap::build(&["ctrl+a>n=new_window".into(), "ctrl+a=new_tab".into()]);
        assert_eq!(km.lookup(&trig("ctrl+a")), Some(&Action::NewTab));
        assert!(!km.is_prefix(&[trig("ctrl+a")]));
        // clear는 순서도 지운다
        let (km, _) = Keymap::build(&["ctrl+a>n=new_window".into(), "clear".into()]);
        assert!(!km.is_prefix(&[trig("ctrl+a")]));
    }

    #[test]
    fn sequencer_waits_then_runs_or_flushes_like_ghostty() {
        let (km, _) = Keymap::build(&["ctrl+a>n=new_window".into(), "ctrl+a>ctrl+a=text:A".into()]);
        let mut sq = Sequencer::default();
        let key = |s: &str| Some(trig(s));
        // 앞 키: 보내지 않고 기다린다
        assert_eq!(sq.feed(&km, key("ctrl+a"), vec![1]), KeyOutcome::default());
        assert!(sq.waiting());
        // 수식 키만 누른 것(바이트 없음)은 기다림을 깨지 않는다
        assert_eq!(sq.feed(&km, None, vec![]), KeyOutcome::default());
        assert!(sq.waiting());
        assert_eq!(
            sq.feed(&km, key("n"), b"n".to_vec()),
            KeyOutcome {
                send: vec![],
                action: Some(Action::NewWindow)
            }
        );
        assert!(!sq.waiting());
        // 묶이지 않은 키: 쌓은 키와 이 키를 그대로 보낸다
        sq.feed(&km, key("ctrl+a"), vec![1]);
        assert_eq!(
            sq.feed(&km, key("x"), b"x".to_vec()),
            KeyOutcome {
                send: vec![1, b'x'],
                action: None
            }
        );
        assert!(!sq.waiting());
        // 앞 키를 두 번: 그 순서에 묶인 동작
        sq.feed(&km, key("ctrl+a"), vec![1]);
        assert_eq!(
            sq.feed(&km, key("ctrl+a"), vec![1]).action,
            Some(Action::Text("A".into()))
        );
        // 순서가 깨진 뒤 이 키는 처음부터 다시 본다: 기본 묶음이면 동작이 된다
        if cfg!(target_os = "macos") {
            sq.feed(&km, key("ctrl+a"), vec![1]);
            assert_eq!(
                sq.feed(&km, key("super+t"), vec![]),
                KeyOutcome {
                    send: vec![1],
                    action: Some(Action::NewTab)
                }
            );
        }
        // 순서 밖에서는 늘 하던 대로
        assert_eq!(
            sq.feed(&km, key("q"), b"q".to_vec()),
            KeyOutcome {
                send: b"q".to_vec(),
                action: None
            }
        );
    }

    #[test]
    fn clear_removes_all_defaults() {
        let (km, _) = Keymap::build(&["clear".into(), "super+q=new_tab".into()]);
        assert_eq!(km.map.len(), 1);
    }
}
