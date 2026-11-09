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
    /// 닫은 칸·탭을 되살린다(`undo-timeout` 안에서, #164)
    Undo,
    /// 되살린 것을 다시 닫는다
    Redo,
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
    /// 고른 것의 끝점을 옮긴다(Ghostty `adjust_selection`, #138)
    AdjustSelection(bt_term::SelectionAdjust),
    /// 마우스 아래 링크 주소를 클립보드에(Ghostty `copy_url_to_clipboard`)
    CopyUrl,
    /// 칸의 글을 파일로 쓰고 그 경로를 어떻게 할지(Ghostty `write_scrollback_file` 등, #125)
    WriteFile(FileSource, FileThen),
    /// 입력 막대로 지금 칸의 제목을 묻는다(Ghostty `prompt_surface_title`, #124)
    PromptSurfaceTitle,
    /// 지금 칸의 제목을 정한다. 빈 글이면 사람이 붙인 제목을 지운다(Ghostty `set_surface_title`)
    SetSurfaceTitle(String),
    /// 전체 화면 켜고 끄기(Ghostty `toggle_fullscreen`, #126)
    ToggleFullscreen,
    /// 창 최대화 켜고 끄기(Ghostty `toggle_maximize`, #148)
    ToggleMaximize,
    /// 창을 다른 창 위에 항상 띄우기 켜고 끄기(Ghostty `toggle_window_float_on_top`, #148)
    ToggleFloatOnTop,
    /// 줄 단위 스크롤: 양수면 아래로(Ghostty `scroll_page_lines`)
    ScrollLines(i32),
    /// 화면의 몇 분의 일 스크롤: 양수면 아래로(Ghostty `scroll_page_fractional`)
    ScrollFraction(f32),
    /// 앱을 끝낸다(Ghostty `quit`, ⌘Q). 프로그램이 도는 칸이 있으면 묻는다(#108)
    Quit,
    /// 키를 먹고 아무것도 하지 않는다(Ghostty `ignore`). 프로그램에도 보내지 않는다
    Ignore,
    /// `text:…`: 글을 그대로 PTY에 보낸다(`\n`, `\x1b` 같은 이스케이프를 푼다)
    Text(String),
    /// 입력 막대로 지금 탭의 제목을 묻는다(Ghostty `prompt_tab_title`, #207)
    PromptTabTitle,
    /// 입력 막대로 지금 창의 제목을 묻는다(Ghostty `prompt_window_title`, #207)
    PromptWindowTitle,
    /// 지금 탭의 제목을 덮어쓴다. 빈 글이면 덮어쓴 제목을 지운다(Ghostty `set_tab_title`, #207)
    SetTabTitle(String),
    /// 지금 창의 제목을 덮어쓴다. 빈 글이면 덮어쓴 제목을 지운다(Ghostty `set_window_title`, #207)
    SetWindowTitle(String),
    /// 지금 칸의 터미널 제목을 클립보드로 복사한다(Ghostty `copy_title_to_clipboard`, #207)
    CopyTitle,
}

/// 파일로 쓸 글
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FileSource {
    Scrollback,
    Screen,
    Selection,
}

/// 쓴 파일의 경로를: 클립보드에 넣기, 칸에 붙여넣기, 기본 앱으로 열기
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FileThen {
    Copy,
    Paste,
    Open,
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
        "undo" => Action::Undo,
        "redo" => Action::Redo,
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
        "toggle_maximize" => Action::ToggleMaximize,
        "toggle_window_float_on_top" => Action::ToggleFloatOnTop,
        "prompt_surface_title" => Action::PromptSurfaceTitle,
        "copy_url_to_clipboard" => Action::CopyUrl,
        "adjust_selection" => {
            use bt_term::SelectionAdjust::*;
            Action::AdjustSelection(match arg.map(str::trim) {
                Some("left") => Left,
                Some("right") => Right,
                Some("up") => Up,
                Some("down") => Down,
                Some("page_up") => PageUp,
                Some("page_down") => PageDown,
                Some("home") => Home,
                Some("end") => End,
                Some("beginning_of_line") => BeginningOfLine,
                Some("end_of_line") => EndOfLine,
                _ => {
                    return Err(
                        "adjust_selection:left·right·up·down·page_up·page_down·home·end·beginning_of_line·end_of_line"
                            .into(),
                    );
                }
            })
        }
        "write_scrollback_file" | "write_screen_file" | "write_selection_file" => {
            let source = match name {
                "write_scrollback_file" => FileSource::Scrollback,
                "write_screen_file" => FileSource::Screen,
                _ => FileSource::Selection,
            };
            let then = match arg.map(str::trim) {
                Some("copy") => FileThen::Copy,
                Some("paste") => FileThen::Paste,
                Some("open") => FileThen::Open,
                _ => return Err(format!("{name}:copy·paste·open")),
            };
            Action::WriteFile(source, then)
        }
        "set_surface_title" => Action::SetSurfaceTitle(arg.unwrap_or("").to_string()),
        "prompt_tab_title" => Action::PromptTabTitle,
        "prompt_window_title" => Action::PromptWindowTitle,
        "set_tab_title" => Action::SetTabTitle(arg.unwrap_or("").to_string()),
        "set_window_title" => Action::SetWindowTitle(arg.unwrap_or("").to_string()),
        "copy_title_to_clipboard" => Action::CopyTitle,
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
        "ignore" => Action::Ignore,
        "text" => Action::Text(unescape(arg.unwrap_or(""))),
        "csi" => Action::Text(format!("\x1b[{}", arg.unwrap_or(""))),
        "esc" => Action::Text(format!("\x1b{}", arg.unwrap_or(""))),
        other => return Err(format!("b-terminal에 없는 동작입니다: {other}")),
    }))
}

/// 묶음 접두사가 만드는 성질(Ghostty `all:` `unconsumed:` `performable:`)
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct BindFlags {
    /// 동작을 하고도 키를 프로그램에 보낸다
    pub unconsumed: bool,
    /// 동작을 할 수 있을 때만 키를 먹는다. 할 수 없으면 묶이지 않은 키처럼 프로그램에 보낸다
    pub performable: bool,
    /// 모든 칸에 건다
    pub all: bool,
}

/// 묶음 하나: 동작과 접두사 성질
#[derive(Debug, Clone, PartialEq)]
struct Bound {
    action: Action,
    flags: BindFlags,
}

/// 트리거 이름 `catch_all`: 다른 묶음에 걸리지 않은 모든 키에 맞는다(Ghostty)
const CATCH_ALL: &str = "catch_all";

/// 묶인 트리거 `pat`이 실제로 눌린 키 `t`에 맞는가. `catch_all`은 수식 키가 같은 아무 키에 맞는다
fn trigger_matches(pat: &Trigger, t: &Trigger) -> bool {
    pat == t || (pat.key == CATCH_ALL && pat.mods == t.mods)
}

fn seq_matches(pat: &[Trigger], keys: &[Trigger]) -> bool {
    pat.len() == keys.len() && pat.iter().zip(keys).all(|(p, k)| trigger_matches(p, k))
}

/// 키들을 찾은 결과
enum Resolved<'a> {
    /// 동작이 걸려 있다
    Action(&'a Bound),
    /// 더 긴 순서의 앞 키들이다
    Prefix,
    None,
}

/// 트리거 → 동작
#[derive(Debug, Clone, Default)]
pub struct Keymap {
    map: HashMap<Trigger, Bound>,
    /// 키 순서(`ctrl+a>n`, #106): 두 키 이상. 한 키짜리는 `map`에 둔다
    seqs: HashMap<Vec<Trigger>, Bound>,
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
    // 되살리기(Ghostty macOS 기본, performable): 다른 OS의 기본에는 없다. 되살릴 것이 없으면 키는 프로그램으로 간다
    if cfg!(target_os = "macos") {
        v.extend([
            ("super+z", "undo"),
            ("super+shift+t", "undo"),
            ("super+shift+z", "redo"),
        ]);
    }
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
                let performable = matches!(act, Action::Undo | Action::Redo);
                km.map.insert(
                    trig,
                    Bound {
                        action: act,
                        flags: BindFlags {
                            performable,
                            ..BindFlags::default()
                        },
                    },
                );
            }
        }
        for line in user {
            let line = line.trim();
            if line == "clear" {
                km.map.clear();
                km.seqs.clear();
                continue;
            }
            // 접두사(Ghostty와 같다): `all:` `unconsumed:` `performable:` `global:`을 겹쳐 쓸 수 있다
            let mut flags = BindFlags::default();
            let mut is_global = false;
            let mut line = line;
            loop {
                if let Some(r) = line.strip_prefix("all:") {
                    flags.all = true;
                    line = r;
                } else if let Some(r) = line.strip_prefix("unconsumed:") {
                    flags.unconsumed = true;
                    line = r;
                } else if let Some(r) = line.strip_prefix("performable:") {
                    flags.performable = true;
                    line = r;
                } else if let Some(r) = line.strip_prefix("global:") {
                    is_global = true;
                    line = r;
                } else {
                    break;
                }
            }
            // 트리거에도 =가 올 수 있다(super+=). 마지막 =를 나눔 자리로 본다
            let Some(eq) = line.rfind('=') else {
                warnings.push(format!("keybind는 `트리거=동작` 꼴입니다: {line}"));
                continue;
            };
            let (t, a) = (&line[..eq], &line[eq + 1..]);
            // global: 접두사: 전역 단축키로 따로 둔다(다른 접두사는 전역에서는 뜻이 없어 받기만 한다)
            if is_global {
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
            // all: 묶음에는 키 순서를 쓸 수 없다(Ghostty)
            if flags.all && keys.len() > 1 {
                warnings.push(format!("keybind {line}: all: 묶음에는 키 순서(>)를 쓸 수 없습니다"));
                continue;
            }
            match parse_action(a) {
                Ok(Some(act)) => km.bind(keys, act, flags),
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
    fn flags(&self, keys: &[Trigger]) -> Option<BindFlags> {
        self.exact(keys).map(|b| b.flags)
    }

    #[cfg(test)]
    pub fn lookup(&self, t: &Trigger) -> Option<&Action> {
        self.map.get(t).map(|b| &b.action)
    }

    /// 키(들)에 동작을 건다. 겹치는 묶음은 나중 것이 이긴다(Ghostty):
    /// 순서의 앞 키에 걸린 한 키 묶음·짧은 순서, 이 키들로 시작하는 긴 순서를 지운다
    fn bind(&mut self, keys: Vec<Trigger>, action: Action, flags: BindFlags) {
        let action = Bound { action, flags };
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

    /// 키들(한 키 이상)에 정확히 걸린 묶음(`catch_all`은 보지 않는다)
    fn exact(&self, keys: &[Trigger]) -> Option<&Bound> {
        match keys {
            [t] => self.map.get(t),
            _ => self.seqs.get(keys),
        }
    }

    /// `catch_all`로 묶인 것 중 이 키들에 맞는 묶음
    fn wildcard(&self, keys: &[Trigger]) -> Option<&Bound> {
        match keys {
            // 수식이 같은 catch_all을 먼저, 없고 수식이 있으면 수식 없는 catch_all로(Ghostty)
            [t] => self
                .map
                .get(&Trigger {
                    mods: t.mods,
                    key: CATCH_ALL.into(),
                })
                .or_else(|| {
                    (t.mods != Mods::default())
                        .then(|| {
                            self.map.get(&Trigger {
                                mods: Mods::default(),
                                key: CATCH_ALL.into(),
                            })
                        })
                        .flatten()
                }),
            _ => self.seqs.iter().find(|(pat, _)| seq_matches(pat, keys)).map(|(_, b)| b),
        }
    }

    /// 키들(한 키 이상)에 걸린 동작
    #[cfg(test)]
    fn sequence(&self, keys: &[Trigger]) -> Option<&Action> {
        self.exact(keys).map(|b| &b.action)
    }

    /// 이 키들로 시작하는 더 긴 순서가 있는가(정확히 같은 키로)
    #[cfg(test)]
    fn is_prefix(&self, keys: &[Trigger]) -> bool {
        self.seqs.keys().any(|k| k.len() > keys.len() && k.starts_with(keys))
    }

    /// 눌린 키들을 찾는다. 순서: 정확한 묶음, 정확한 순서의 앞 키, `catch_all` 묶음, `catch_all`이 낀 순서의 앞 키.
    /// 그래서 `ctrl+a>n`이 있으면 `catch_all`이 있어도 `ctrl+a`는 앞 키가 된다
    fn resolve(&self, keys: &[Trigger]) -> Resolved<'_> {
        if let Some(b) = self.exact(keys) {
            return Resolved::Action(b);
        }
        if self.seqs.keys().any(|k| k.len() > keys.len() && k.starts_with(keys)) {
            return Resolved::Prefix;
        }
        if let Some(b) = self.wildcard(keys) {
            return Resolved::Action(b);
        }
        if self
            .seqs
            .keys()
            .any(|k| k.len() > keys.len() && seq_matches(&k[..keys.len()], keys))
        {
            return Resolved::Prefix;
        }
        Resolved::None
    }

    /// 이 동작에 걸린 트리거들(명령 팔레트가 단축키를 보일 때)
    pub fn triggers_for(&self, action: &Action) -> Vec<&Trigger> {
        self.map
            .iter()
            .filter(|(_, b)| b.action == *action)
            .map(|(t, _)| t)
            .collect()
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
    /// `all:` 접두사가 붙은 묶음에서 나온 동작인가
    pub all: bool,
}

impl Sequencer {
    /// 앞 키를 받고 다음 키를 기다리는 중인가
    #[cfg(test)]
    pub fn waiting(&self) -> bool {
        !self.pending.is_empty()
    }

    /// 키 하나(`trigger`: 키 묶음 이름, 수식 키만이면 없다; `bytes`: 그 키가 PTY에 보낼 바이트)
    #[cfg(test)]
    pub fn feed(&mut self, km: &Keymap, trigger: Option<Trigger>, bytes: Vec<u8>) -> KeyOutcome {
        self.feed_with(km, trigger, bytes, &mut |_| true)
    }

    /// `feed`와 같고, `performable:` 묶음의 동작을 지금 할 수 있는지 `can`으로 묻는다
    pub fn feed_with(
        &mut self,
        km: &Keymap,
        trigger: Option<Trigger>,
        bytes: Vec<u8>,
        can: &mut dyn FnMut(&Action) -> bool,
    ) -> KeyOutcome {
        match &trigger {
            Some(t) => {
                let mut keys: Vec<Trigger> = self.pending.iter().map(|(k, _)| k.clone()).collect();
                keys.push(t.clone());
                match km.resolve(&keys) {
                    // all: 묶음에는 performable이 효과가 없다(Ghostty)
                    Resolved::Action(b) if !b.flags.performable || b.flags.all || can(&b.action) => {
                        self.pending.clear();
                        // unconsumed: 동작을 하고 이 키도 프로그램에 보낸다(앞서 쌓은 키는 이미 순서에 먹혔다).
                        // all: 묶음은 특정 칸에 묶이지 않아 늘 키를 먹는다(Ghostty)
                        let send = if b.flags.unconsumed && !b.flags.all {
                            bytes
                        } else {
                            Vec::new()
                        };
                        return KeyOutcome {
                            send,
                            action: Some(b.action.clone()),
                            all: b.flags.all,
                        };
                    }
                    Resolved::Prefix => {
                        self.pending.push((t.clone(), bytes));
                        return KeyOutcome::default();
                    }
                    // 묶이지 않았거나, performable인데 할 수 없다: 묶이지 않은 키로 본다
                    _ => {}
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
                all: false,
            };
        }
        // 순서 중 묶이지 않은 키가 catch_all=ignore에 맞으면 순서 전체를 버린다(Ghostty)
        if let Some(t) = &trigger {
            if km
                .wildcard(std::slice::from_ref(t))
                .is_some_and(|b| b.action == Action::Ignore)
            {
                self.pending.clear();
                return KeyOutcome::default();
            }
        }
        // 순서가 깨졌다: 쌓은 키를 보내고, 이 키는 처음부터 다시 본다(새 순서의 앞 키일 수도 있다)
        let mut send: Vec<u8> = self.pending.drain(..).flat_map(|(_, b)| b).collect();
        let rest = self.feed_with(km, trigger, bytes, can);
        send.extend(rest.send);
        KeyOutcome {
            send,
            action: rest.action,
            all: rest.all,
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
        assert_eq!(parse_action("toggle_maximize").unwrap(), Some(Action::ToggleMaximize));
        assert_eq!(
            parse_action("toggle_window_float_on_top").unwrap(),
            Some(Action::ToggleFloatOnTop)
        );
        assert_eq!(
            parse_action("toggle_fullscreen").unwrap(),
            Some(Action::ToggleFullscreen)
        );
        assert_eq!(
            parse_action("prompt_surface_title").unwrap(),
            Some(Action::PromptSurfaceTitle)
        );
        assert_eq!(
            parse_action("write_scrollback_file:copy").unwrap(),
            Some(Action::WriteFile(FileSource::Scrollback, FileThen::Copy))
        );
        assert_eq!(
            parse_action("write_screen_file:open").unwrap(),
            Some(Action::WriteFile(FileSource::Screen, FileThen::Open))
        );
        assert_eq!(
            parse_action("write_selection_file:paste").unwrap(),
            Some(Action::WriteFile(FileSource::Selection, FileThen::Paste))
        );
        assert!(parse_action("write_screen_file").is_err(), "할 일이 있어야 한다");
        assert!(parse_action("write_screen_file:mail").is_err());
        assert_eq!(
            parse_action("adjust_selection:end_of_line").unwrap(),
            Some(Action::AdjustSelection(bt_term::SelectionAdjust::EndOfLine))
        );
        assert_eq!(
            parse_action("adjust_selection:page_up").unwrap(),
            Some(Action::AdjustSelection(bt_term::SelectionAdjust::PageUp))
        );
        assert!(parse_action("adjust_selection").is_err());
        assert!(parse_action("adjust_selection:sideways").is_err());
        assert_eq!(parse_action("copy_url_to_clipboard").unwrap(), Some(Action::CopyUrl));
        assert_eq!(
            parse_action("set_surface_title:빌드 서버").unwrap(),
            Some(Action::SetSurfaceTitle("빌드 서버".into()))
        );
        assert_eq!(
            parse_action("set_surface_title").unwrap(),
            Some(Action::SetSurfaceTitle(String::new())),
            "글이 없으면 지우기"
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
                action: Some(Action::NewWindow),
                ..Default::default()
            }
        );
        assert!(!sq.waiting());
        // 묶이지 않은 키: 쌓은 키와 이 키를 그대로 보낸다
        sq.feed(&km, key("ctrl+a"), vec![1]);
        assert_eq!(
            sq.feed(&km, key("x"), b"x".to_vec()),
            KeyOutcome {
                send: vec![1, b'x'],
                action: None,
                ..Default::default()
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
                    action: Some(Action::NewTab),
                    ..Default::default()
                }
            );
        }
        // 순서 밖에서는 늘 하던 대로
        assert_eq!(
            sq.feed(&km, key("q"), b"q".to_vec()),
            KeyOutcome {
                send: b"q".to_vec(),
                action: None,
                ..Default::default()
            }
        );
    }

    fn feed_all(sq: &mut Sequencer, km: &Keymap, k: &str, bytes: &[u8], can: bool) -> KeyOutcome {
        sq.feed_with(km, Some(trig(k)), bytes.to_vec(), &mut |_| can)
    }

    #[test]
    fn undo_redo_actions_parse_and_default_keys_are_performable_on_macos() {
        assert_eq!(parse_action("undo").unwrap(), Some(Action::Undo));
        assert_eq!(parse_action("redo").unwrap(), Some(Action::Redo));
        let (km, _) = Keymap::build(&[]);
        if cfg!(target_os = "macos") {
            for (k, a) in [
                ("super+z", Action::Undo),
                ("super+shift+t", Action::Undo),
                ("super+shift+z", Action::Redo),
            ] {
                assert_eq!(km.lookup(&trig(k)), Some(&a), "{k}");
                assert_eq!(
                    km.flags(&[trig(k)]),
                    Some(BindFlags {
                        performable: true,
                        ..Default::default()
                    }),
                    "{k}: 되살릴 것이 없으면 키는 프로그램으로"
                );
            }
            // 되살릴 것이 없으면 ⌘Z는 묶이지 않은 키처럼 프로그램에 간다
            let mut sq = Sequencer::default();
            let out = feed_all(&mut sq, &km, "super+z", b"z", false);
            assert_eq!((out.action, out.send), (None, b"z".to_vec()));
            let out = feed_all(&mut sq, &km, "super+z", b"z", true);
            assert_eq!(out.action, Some(Action::Undo));
        } else {
            assert_eq!(km.lookup(&trig("ctrl+shift+z")), None, "Ghostty는 macOS에만 묶는다");
        }
    }

    #[test]
    fn prefixes_are_parsed_and_stack_in_any_order() {
        let (km, w) = Keymap::build(&[
            "unconsumed:ctrl+alt+u=new_tab".into(),
            "performable:ctrl+alt+p=copy_to_clipboard".into(),
            "all:ctrl+alt+a=text:x".into(),
            "unconsumed:performable:ctrl+alt+b=new_window".into(),
            "performable:all:unconsumed:ctrl+alt+c=new_window".into(),
            "ctrl+alt+d=new_tab".into(),
            "unconsumed:ctrl+alt+e>n=new_tab".into(),
            "global:unconsumed:ctrl+alt+g=toggle_quick_terminal".into(),
            "nope:ctrl+alt+z=new_tab".into(),
        ]);
        let f = |k: &str| km.flags(&[trig(k)]);
        assert_eq!(
            f("ctrl+alt+u"),
            Some(BindFlags {
                unconsumed: true,
                ..Default::default()
            })
        );
        assert_eq!(
            f("ctrl+alt+p"),
            Some(BindFlags {
                performable: true,
                ..Default::default()
            })
        );
        assert_eq!(
            f("ctrl+alt+a"),
            Some(BindFlags {
                all: true,
                ..Default::default()
            })
        );
        assert_eq!(
            f("ctrl+alt+b"),
            Some(BindFlags {
                unconsumed: true,
                performable: true,
                all: false
            })
        );
        assert_eq!(
            f("ctrl+alt+c"),
            Some(BindFlags {
                unconsumed: true,
                performable: true,
                all: true
            })
        );
        assert_eq!(f("ctrl+alt+d"), Some(BindFlags::default()));
        assert_eq!(
            km.flags(&[trig("ctrl+alt+e"), trig("n")]),
            Some(BindFlags {
                unconsumed: true,
                ..Default::default()
            })
        );
        assert_eq!(km.global.len(), 1, "global:에 다른 접두사가 겹쳐도 전역 단축키");
        assert_eq!(w.len(), 1, "모르는 접두사는 경고: {w:?}");
        // 접두사를 붙인 unbind는 그 묶음을 지운다
        let (km, _) = Keymap::build(&["ctrl+alt+d=new_tab".into(), "performable:ctrl+alt+d=unbind".into()]);
        assert_eq!(km.lookup(&trig("ctrl+alt+d")), None);
    }

    #[test]
    fn ignore_action_swallows_the_key() {
        assert_eq!(parse_action("ignore").unwrap(), Some(Action::Ignore));
        let (km, _) = Keymap::build(&["ctrl+alt+i=ignore".into()]);
        let mut sq = Sequencer::default();
        assert_eq!(
            feed_all(&mut sq, &km, "ctrl+alt+i", b"i", true),
            KeyOutcome {
                action: Some(Action::Ignore),
                ..Default::default()
            }
        );
    }

    #[test]
    fn unconsumed_runs_the_action_and_also_sends_the_bytes() {
        let (km, _) = Keymap::build(&["unconsumed:ctrl+alt+u=new_tab".into(), "ctrl+alt+k=new_tab".into()]);
        let mut sq = Sequencer::default();
        assert_eq!(
            feed_all(&mut sq, &km, "ctrl+alt+u", b"U", true),
            KeyOutcome {
                send: b"U".to_vec(),
                action: Some(Action::NewTab),
                all: false
            }
        );
        // 접두사 없는 묶음은 먹는다
        assert_eq!(
            feed_all(&mut sq, &km, "ctrl+alt+k", b"K", true),
            KeyOutcome {
                action: Some(Action::NewTab),
                ..Default::default()
            }
        );
    }

    #[test]
    fn performable_consumes_only_when_the_action_can_run() {
        let (km, _) = Keymap::build(&["performable:ctrl+alt+c=copy_to_clipboard".into()]);
        let mut sq = Sequencer::default();
        assert_eq!(
            feed_all(&mut sq, &km, "ctrl+alt+c", b"\x03", true),
            KeyOutcome {
                action: Some(Action::Copy),
                ..Default::default()
            },
            "할 수 있으면 먹고 동작"
        );
        assert_eq!(
            feed_all(&mut sq, &km, "ctrl+alt+c", b"\x03", false),
            KeyOutcome {
                send: b"\x03".to_vec(),
                ..Default::default()
            },
            "할 수 없으면 묶이지 않은 키처럼 프로그램에 보낸다"
        );
        // unconsumed와 겹치면: 할 수 있을 때 동작도 하고 키도 보낸다
        let (km, _) = Keymap::build(&["unconsumed:performable:ctrl+alt+c=copy_to_clipboard".into()]);
        let out = feed_all(&mut sq, &km, "ctrl+alt+c", b"\x03", true);
        assert_eq!((out.send, out.action), (b"\x03".to_vec(), Some(Action::Copy)));
        let out = feed_all(&mut sq, &km, "ctrl+alt+c", b"\x03", false);
        assert_eq!((out.send, out.action), (b"\x03".to_vec(), None));
    }

    #[test]
    fn all_prefix_is_reported_in_the_outcome() {
        let (km, _) = Keymap::build(&["all:ctrl+alt+a=text:x".into()]);
        let mut sq = Sequencer::default();
        let out = feed_all(&mut sq, &km, "ctrl+alt+a", b"a", true);
        assert!(out.all);
        assert_eq!(out.action, Some(Action::Text("x".into())));
    }

    #[test]
    fn catch_all_matches_only_keys_with_no_other_bind() {
        let (km, w) = Keymap::build(&["clear".into(), "a=new_tab".into(), "catch_all=ignore".into()]);
        assert!(w.is_empty(), "{w:?}");
        let mut sq = Sequencer::default();
        assert_eq!(feed_all(&mut sq, &km, "a", b"a", true).action, Some(Action::NewTab));
        let out = feed_all(&mut sq, &km, "b", b"b", true);
        assert_eq!(
            (out.send, out.action),
            (vec![], Some(Action::Ignore)),
            "묶이지 않은 키는 catch_all이 먹는다"
        );
        // 수식 키만 누른 것은 catch_all에 맞지 않는다
        assert_eq!(sq.feed(&km, None, vec![]), KeyOutcome::default());
        // 수식이 붙은 catch_all은 그 수식이 같은 키에만 맞는다
        let (km, _) = Keymap::build(&["clear".into(), "ctrl+catch_all=ignore".into()]);
        assert_eq!(
            feed_all(&mut sq, &km, "ctrl+x", b"\x18", true).action,
            Some(Action::Ignore)
        );
        let out = feed_all(&mut sq, &km, "x", b"x", true);
        assert_eq!((out.send, out.action), (b"x".to_vec(), None));
    }

    #[test]
    fn catch_all_inside_a_sequence() {
        let (km, w) = Keymap::build(&[
            "clear".into(),
            "ctrl+a>n=new_window".into(),
            "ctrl+a>catch_all=ignore".into(),
        ]);
        assert!(w.is_empty(), "{w:?}");
        let mut sq = Sequencer::default();
        assert_eq!(feed_all(&mut sq, &km, "ctrl+a", &[1], true), KeyOutcome::default());
        assert!(sq.waiting());
        // 정확히 묶인 키가 catch_all보다 먼저
        assert_eq!(feed_all(&mut sq, &km, "n", b"n", true).action, Some(Action::NewWindow));
        // 그 밖의 키는 catch_all: 앞 키도 이 키도 프로그램에 보내지 않는다
        feed_all(&mut sq, &km, "ctrl+a", &[1], true);
        assert_eq!(
            feed_all(&mut sq, &km, "x", b"x", true),
            KeyOutcome {
                action: Some(Action::Ignore),
                ..Default::default()
            }
        );
        assert!(!sq.waiting());
        // 순서 밖의 키는 묶이지 않았으니 그대로 간다
        assert_eq!(feed_all(&mut sq, &km, "x", b"x", true).send, b"x".to_vec());
    }

    /// Ghostty 소스(Config.zig)의 설명과 맞춘 규칙들(주 세션이 더함)
    #[test]
    fn prefix_rules_follow_ghostty_source() {
        let key = |s: &str| Some(trig(s));
        // ctrl+catch_all이 없으면 수식 키 없는 catch_all로 넘어간다
        let (km, _) = Keymap::build(&["clear".into(), "catch_all=ignore".into()]);
        let mut sq = Sequencer::default();
        assert_eq!(
            sq.feed(&km, key("ctrl+x"), vec![24]).action,
            Some(Action::Ignore),
            "수식이 붙은 키도 수식 없는 catch_all에 맞는다"
        );
        // all: 묶음에는 키 순서를 쓸 수 없다(경고)
        let (km, w) = Keymap::build(&["clear".into(), "all:ctrl+a>n=new_tab".into()]);
        assert_eq!(w.len(), 1, "{w:?}");
        assert!(km.seqs.is_empty());
        // all: 묶음은 unconsumed가 있어도 키를 먹고, performable은 효과가 없다
        let (km, _) = Keymap::build(&[
            "clear".into(),
            "all:unconsumed:performable:ctrl+u=copy_to_clipboard".into(),
        ]);
        let out = Sequencer::default().feed_with(&km, key("ctrl+u"), vec![21], &mut |_| false);
        assert_eq!(out.action, Some(Action::Copy), "all:이면 performable을 보지 않는다");
        assert!(out.send.is_empty(), "all:이면 unconsumed여도 키를 먹는다");
        // 순서 중 묶이지 않은 키가 catch_all=ignore에 맞으면 순서 전체를 버린다
        let (km, _) = Keymap::build(&["clear".into(), "catch_all=ignore".into(), "ctrl+a>n=new_window".into()]);
        let mut sq = Sequencer::default();
        sq.feed(&km, key("ctrl+a"), vec![1]);
        assert_eq!(
            sq.feed(&km, key("x"), b"x".to_vec()),
            KeyOutcome::default(),
            "앞 키(^A)도 보내지 않는다"
        );
        assert!(!sq.waiting());
    }

    #[test]
    fn sequence_prefix_wins_over_a_single_catch_all() {
        let (km, _) = Keymap::build(&["clear".into(), "catch_all=ignore".into(), "ctrl+a>n=new_window".into()]);
        let mut sq = Sequencer::default();
        feed_all(&mut sq, &km, "ctrl+a", &[1], true);
        assert!(sq.waiting(), "ctrl+a는 순서의 앞 키이지 catch_all이 아니다");
        assert_eq!(feed_all(&mut sq, &km, "n", b"n", true).action, Some(Action::NewWindow));
    }

    #[test]
    fn clear_removes_all_defaults() {
        let (km, _) = Keymap::build(&["clear".into(), "super+q=new_tab".into()]);
        assert_eq!(km.map.len(), 1);
    }

    #[test]
    fn title_actions_parse() {
        assert_eq!(parse_action("prompt_tab_title").unwrap(), Some(Action::PromptTabTitle));
        assert_eq!(
            parse_action("prompt_window_title").unwrap(),
            Some(Action::PromptWindowTitle)
        );
        assert_eq!(
            parse_action("copy_title_to_clipboard").unwrap(),
            Some(Action::CopyTitle)
        );
        assert_eq!(
            parse_action("set_tab_title:빌드 서버").unwrap(),
            Some(Action::SetTabTitle("빌드 서버".into()))
        );
        assert_eq!(
            parse_action("set_tab_title").unwrap(),
            Some(Action::SetTabTitle(String::new())),
            "글이 없으면 지우기"
        );
        assert_eq!(
            parse_action("set_window_title:창").unwrap(),
            Some(Action::SetWindowTitle("창".into()))
        );
        assert_eq!(
            parse_action("set_window_title").unwrap(),
            Some(Action::SetWindowTitle(String::new()))
        );
    }
}
