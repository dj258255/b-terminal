//! 명령줄 정보 동작(Ghostty `+list-fonts` `+list-keybinds` `+list-actions` `+list-colors` `+show-face` `+explain-config`, #231).
//! 창을 띄우지 않고 글을 찍고 끝낸다. 글을 만드는 일은 순수 함수로 빼 두고 `run`은 읽고 찍기만 한다

use std::collections::BTreeMap;
use std::io::Write;

use bt_render::{FaceEntry, FaceSource, FontStyle};

use crate::keymap::{self, Mods};

/// 설정 문서. `+explain-config`가 키 표에서 설명을 뽑는다(키 설명 데이터가 따로 없다)
const CONFIG_DOC: &str = include_str!("../../../docs/configuration.md");

/// 키 묶음 동작 이름(`keymap::parse_action`이 아는 것). 인자는 `:` 뒤에 붙는다
const ACTIONS: &[&str] = &[
    "ignore",
    "unbind",
    "csi",
    "esc",
    "text",
    "reset",
    "copy_to_clipboard",
    "paste_from_clipboard",
    "paste_from_selection",
    "copy_url_to_clipboard",
    "copy_title_to_clipboard",
    "increase_font_size",
    "decrease_font_size",
    "reset_font_size",
    "set_font_size",
    "clear_screen",
    "select_all",
    "scroll_to_top",
    "scroll_to_bottom",
    "scroll_to_selection",
    "scroll_to_row",
    "scroll_page_up",
    "scroll_page_down",
    "scroll_page_fractional",
    "scroll_page_lines",
    "adjust_selection",
    "jump_to_prompt",
    "write_scrollback_file",
    "write_screen_file",
    "write_selection_file",
    "start_search",
    "search",
    "search_selection",
    "navigate_search",
    "end_search",
    "new_window",
    "new_tab",
    "previous_tab",
    "next_tab",
    "last_tab",
    "goto_tab",
    "move_tab",
    "move_tab_to_new_window",
    "toggle_tab_overview",
    "prompt_surface_title",
    "prompt_tab_title",
    "prompt_window_title",
    "set_surface_title",
    "set_tab_title",
    "set_window_title",
    "new_split",
    "goto_split",
    "toggle_split_zoom",
    "resize_split",
    "equalize_splits",
    "reset_window_size",
    "inspector",
    "show_gtk_inspector",
    "show_on_screen_keyboard",
    "open_config",
    "reload_config",
    "close_surface",
    "close_tab",
    "close_window",
    "close_all_windows",
    "toggle_maximize",
    "toggle_fullscreen",
    "toggle_window_decorations",
    "toggle_window_float_on_top",
    "toggle_secure_input",
    "toggle_mouse_reporting",
    "toggle_command_palette",
    "toggle_quick_terminal",
    "toggle_visibility",
    "toggle_background_opacity",
    "toggle_readonly",
    "toggle_broadcast_group",
    "clear_broadcast_groups",
    "set_window_theme",
    "goto_window",
    "check_for_updates",
    "undo",
    "redo",
    "quit",
    "crash",
    "activate_key_table",
    "activate_key_table_once",
    "deactivate_key_table",
    "deactivate_all_key_tables",
    "end_key_sequence",
];

/// b-terminal이 받는 키 묶음 동작 이름(이름순)
pub fn action_names() -> Vec<&'static str> {
    let mut v = ACTIONS.to_vec();
    v.sort_unstable();
    v
}

/// 트리거 하나를 Ghostty가 쓰는 차례(super, ctrl, alt, shift)로 적는다
fn trigger_text(t: &keymap::Trigger) -> String {
    let Mods { shift, ctrl, alt, sup } = t.mods;
    let mut s = String::new();
    for (on, name) in [(sup, "super+"), (ctrl, "ctrl+"), (alt, "alt+"), (shift, "shift+")] {
        if on {
            s.push_str(name);
        }
    }
    s.push_str(&t.key);
    s
}

/// `a>b`를 같은 차례로 다시 적는다. 읽지 못하면 없다
fn chain_text(t: &str) -> Option<String> {
    let parts: Option<Vec<String>> = t
        .split('>')
        .map(|p| keymap::parse_trigger(p).ok().map(|t| trigger_text(&t)))
        .collect();
    parts.map(|p| p.join(">"))
}

/// 키 묶음 표를 `keybind = …` 줄로. 기본 묶음 위에 설정의 줄을 차례로 얹고(같은 트리거는 나중 것, `unbind`는 지움,
/// `clear`는 모두 지움) 읽지 못한 줄은 `Keymap::build`와 같이 건너뛴다. `defaults_only`면 기본만
pub fn keybind_lines(user: &[String], defaults_only: bool) -> Vec<String> {
    // 열쇠 = (키 테이블, 전역 여부, 트리거 순서), 값 = 접두사를 뺀 줄 꼬리를 포함한 완성 줄 하나와, 그 뒤에 덧붙은 `chain=` 줄들
    type Key = (String, bool, String);
    let mut entries: Vec<(Key, Vec<String>)> = Vec::new();
    let put = |entries: &mut Vec<(Key, Vec<String>)>, key: Key, line: Option<String>| {
        entries.retain(|(k, _)| *k != key);
        if let Some(l) = line {
            entries.push((key, vec![l]));
        }
    };
    // `chain=`이 덧붙을 앞 묶음의 열쇠(지우기가 끊는다). 앱의 `Keymap::build`와 같은 규칙이다
    let mut chain_parent: Option<Key> = None;
    let swap_super = !cfg!(target_os = "macos");
    for (t, a) in keymap::defaults() {
        let (Ok(mut trig), Ok(Some(act))) = (keymap::parse_trigger(t), keymap::parse_action(a)) else {
            continue;
        };
        if swap_super && trig.mods.sup {
            trig.mods.sup = false;
            trig.mods.ctrl = true;
            trig.mods.shift = true;
        }
        let performable = keymap::default_performable(&act);
        let text = trigger_text(&trig);
        let line = format!("keybind = {}{text}={a}", if performable { "performable:" } else { "" });
        put(&mut entries, (String::new(), false, text), Some(line));
    }
    if !defaults_only {
        // 읽지 못하는 줄은 건너뛴다(앱이 경고하고 버리는 것과 같다). `chain=` 줄은 앞 줄에 달려 있어 줄 전체를 한 번에 본다
        let bad = keymap::Keymap::build_with_status(user).2;
        for (raw, bad) in user.iter().zip(bad) {
            let raw = raw.trim();
            if raw == "clear" {
                entries.clear();
                chain_parent = None;
                continue;
            }
            if bad {
                continue;
            }
            // 덧붙은 동작은 앞 묶음 줄 바로 뒤에 쓴 그대로 보인다
            if raw.starts_with("chain=") {
                if let Some((_, lines)) = chain_parent
                    .as_ref()
                    .and_then(|p| entries.iter_mut().find(|(k, _)| k == p))
                {
                    lines.push(format!("keybind = {raw}"));
                }
                continue;
            }
            let eq_at = raw.find('=').unwrap_or(raw.len());
            let table = raw[..eq_at]
                .find('/')
                .filter(|&i| i > 0 && !raw[..i].contains(['+', '>']))
                .map(|i| (raw[..i].to_string(), &raw[i + 1..]));
            let body = match &table {
                Some((_, "")) => {
                    chain_parent = None;
                    continue;
                }
                Some((_, rest)) => *rest,
                None => raw,
            };
            let mut prefixes = String::new();
            let mut global = false;
            let mut line = body;
            while let Some(p) = ["all:", "unconsumed:", "performable:", "global:"]
                .into_iter()
                .find(|p| line.starts_with(p))
            {
                if p == "global:" {
                    global = true;
                }
                prefixes.push_str(p);
                line = &line[p.len()..];
            }
            let Some(eq) = line.rfind('=') else { continue };
            let (mut t, mut a) = (&line[..eq], &line[eq + 1..]);
            if let Some(first) = line.find('=') {
                if first < eq && keymap::parse_action(&line[first + 1..]).is_ok() {
                    (t, a) = (&line[..first], &line[first + 1..]);
                }
            }
            let Some(chain) = chain_text(t) else { continue };
            let table_name = table.as_ref().map(|(n, _)| n.clone()).unwrap_or_default();
            let key = (table_name.clone(), global, chain.clone());
            let shown = if table_name.is_empty() {
                format!("keybind = {prefixes}{chain}={a}")
            } else {
                format!("keybind = {prefixes}{table_name}/{chain}={a}")
            };
            let unbind = a.trim() == "unbind";
            chain_parent = (!unbind).then(|| key.clone());
            put(&mut entries, key, if unbind { None } else { Some(shown) });
        }
    }
    // 보이는 차례: 기본 묶음(테이블 밖) 먼저, 같은 쪽 안에서는 이름순
    let mut sorted: BTreeMap<(bool, String, String), Vec<String>> = BTreeMap::new();
    for ((table, global, chain), line) in entries {
        sorted.insert((!table.is_empty(), table, format!("{}{chain}", u8::from(global))), line);
    }
    sorted.into_values().flatten().collect()
}

/// `+list-colors` 줄(Ghostty `list_colors.zig`): `이름 = #rrggbb`. `rgb.txt` 782개를 원래 대소문자로, 대소문자 무시 이름순(#837)
pub fn color_lines() -> Vec<String> {
    let mut all: Vec<_> = bt_core::theme::x11_colors().collect();
    all.sort_by_key(|(n, _)| n.to_ascii_lowercase());
    all.into_iter()
        .map(|(n, c)| format!("{n} = #{:02x}{:02x}{:02x}", c.r, c.g, c.b))
        .collect()
}

/// `--cp` 값: 10진수, `0x` 16진수, `0o` 8진수, `0b` 2진수
pub fn parse_codepoint(s: &str) -> Option<u32> {
    let s = s.trim();
    let n = if let Some(h) = s.strip_prefix("0x") {
        u32::from_str_radix(h, 16).ok()?
    } else if let Some(o) = s.strip_prefix("0o") {
        u32::from_str_radix(o, 8).ok()?
    } else if let Some(b) = s.strip_prefix("0b") {
        u32::from_str_radix(b, 2).ok()?
    } else {
        s.parse().ok()?
    };
    char::from_u32(n).map(|_| n)
}

/// `+show-face` 한 줄(Ghostty 문구를 따른다)
pub fn face_report(ch: char, src: &FaceSource) -> String {
    let head = format!("U+{:04X} « {ch} »", ch as u32);
    match src {
        FaceSource::Sprite => format!("{head} is handled by b-terminal's internal box drawing."),
        FaceSource::NotFound => format!("{head} not found."),
        FaceSource::ColorEmoji(n)
        | FaceSource::Mapped(n)
        | FaceSource::Primary(n)
        | FaceSource::PrimaryRegular(n)
        | FaceSource::Fallback(n) => format!("{head} found in face “{n}”."),
    }
}

/// `+list-fonts` 걸러내기
#[derive(Debug, Default, Clone)]
pub struct FontListOpts {
    pub family: Option<String>,
    pub style: Option<String>,
    pub bold: bool,
    pub italic: bool,
}

/// `+list-fonts` 글: 집안 이름, 들여쓴 얼굴 이름들, 빈 줄. `--family`가 없으면 고정폭만, 집안·얼굴 모두 이름순
pub fn list_fonts_text(faces: &[FaceEntry], o: &FontListOpts) -> String {
    let mut by_family: BTreeMap<&str, Vec<&str>> = BTreeMap::new();
    for f in faces {
        let keep = match &o.family {
            Some(fam) => f.family.eq_ignore_ascii_case(fam),
            None => f.monospaced,
        } && (!o.bold || f.bold)
            && (!o.italic || f.italic)
            && o.style
                .as_ref()
                .is_none_or(|s| f.name.to_ascii_lowercase().contains(&s.to_ascii_lowercase()));
        if keep {
            by_family.entry(&f.family).or_default().push(&f.name);
        }
    }
    let mut out = String::new();
    for (fam, mut names) in by_family {
        names.sort_unstable();
        names.dedup();
        out.push_str(fam);
        out.push('\n');
        for n in names {
            out.push_str("  ");
            out.push_str(n);
            out.push('\n');
        }
        out.push('\n');
    }
    out
}

/// 설정 문서의 "지원하는 키" 표에서 `name` 줄을 찾아 풀어 쓴다. 표에 없으면 없다
pub fn explain_option(doc: &str, name: &str) -> Option<String> {
    let needle = format!("`{name}`");
    let mut in_table = false;
    for l in doc.lines() {
        if l.starts_with("## ") {
            in_table = l.starts_with("## 지원하는 키");
            continue;
        }
        if !in_table || !l.starts_with('|') {
            continue;
        }
        let cells: Vec<&str> = l.trim_start_matches('|').splitn(4, '|').collect();
        let [keys, value, default, memo] = cells[..] else {
            continue;
        };
        if !keys.contains(&needle) {
            continue;
        }
        let memo = memo.trim().trim_end_matches('|').trim();
        let mut out = format!("{name}\n  값: {}\n  기본: {}\n", value.trim(), default.trim());
        if !memo.is_empty() {
            out.push_str(&format!("  메모: {memo}\n"));
        }
        return Some(out);
    }
    None
}

/// `+show-config --docs`: 설정 글(`키 = 값` 줄들)의 키마다 위에 설명 주석을 단다.
/// 설명은 `+explain-config`와 같은 표·같은 파서에서 온다. 표에 없는 키는 그대로 두고,
/// 같은 키가 여러 줄이어도 설명은 첫 줄 위에 한 번만 단다
pub fn annotate_with_docs(text: &str) -> String {
    annotate_with(text, CONFIG_DOC)
}

fn annotate_with(text: &str, doc: &str) -> String {
    let mut seen: Vec<&str> = Vec::new();
    let mut out = Vec::new();
    for l in text.lines() {
        if let Some((key, _)) = l.split_once(" = ") {
            if !l.starts_with('#') && !seen.contains(&key) {
                seen.push(key);
                if let Some(t) = explain_option(doc, key) {
                    out.push(String::new());
                    out.extend(t.lines().map(|d| format!("# {}", d.trim())));
                }
            }
        }
        out.push(l.to_string());
    }
    out.join("\n")
}

/// 읽은 인자
#[derive(Debug, Default)]
pub struct Flags {
    pairs: Vec<(String, Option<String>)>,
    pub rest: Vec<String>,
}

impl Flags {
    pub fn has(&self, k: &str) -> bool {
        self.pairs.iter().any(|(n, _)| n == k)
    }

    pub fn value(&self, k: &str) -> Option<&str> {
        self.pairs
            .iter()
            .rev()
            .find(|(n, _)| n == k)
            .and_then(|(_, v)| v.as_deref())
    }
}

/// `--이름`(값 없음), `--이름=값`·`--이름 값`(값 있음)을 읽는다. 모르는 `--` 인자와 값 빠진 인자는 오류
pub fn parse_flags(args: &[String], bools: &[&str], vals: &[&str]) -> Result<Flags, String> {
    let mut f = Flags::default();
    let mut it = args.iter();
    while let Some(a) = it.next() {
        let Some(body) = a.strip_prefix("--") else {
            f.rest.push(a.clone());
            continue;
        };
        let (name, inline) = match body.split_once('=') {
            Some((n, v)) => (n, Some(v.to_string())),
            None => (body, None),
        };
        if bools.contains(&name) && inline.is_none() {
            f.pairs.push((name.to_string(), None));
        } else if vals.contains(&name) {
            let v = match inline {
                Some(v) => v,
                None => it
                    .next()
                    .cloned()
                    .ok_or_else(|| format!("--{name} 뒤에 값이 필요합니다"))?,
            };
            f.pairs.push((name.to_string(), Some(v)));
        } else {
            return Err(format!("모르는 인자입니다: {a}"));
        }
    }
    Ok(f)
}

/// 도움말 요청(`-h`·`--help`)인가
fn wants_help(args: &[String]) -> bool {
    args.iter().any(|a| a == "-h" || a == "--help")
}

fn bad(action: &str, msg: &str) -> Option<i32> {
    eprintln!("+{action}: {msg}");
    Some(1)
}

/// `+동작`을 실행한다. 이 모듈의 동작이 아니면 `None`, 맞으면 실행하고 종료 코드를 돌려준다
pub fn run(action: &str, args: &[String]) -> Option<i32> {
    let mut out = std::io::stdout().lock();
    let code = match action {
        "list-keybinds" => {
            if wants_help(args) {
                return Some(help(action, "[--default] [--docs] [--plain]"));
            }
            let f = match parse_flags(args, &["default", "docs", "plain"], &[]) {
                Ok(f) => f,
                Err(e) => return bad(action, &e),
            };
            let user = if f.has("default") {
                Vec::new()
            } else {
                bt_core::Config::load_with(None, false, true).keybinds
            };
            for l in keybind_lines(&user, f.has("default")) {
                let _ = writeln!(out, "{l}");
            }
            0
        }
        "list-actions" => {
            if wants_help(args) {
                return Some(help(action, "[--docs] [--plain]"));
            }
            if let Err(e) = parse_flags(args, &["docs", "plain"], &[]) {
                return bad(action, &e);
            }
            for n in action_names() {
                let _ = writeln!(out, "{n}");
            }
            0
        }
        "list-colors" => {
            if wants_help(args) {
                return Some(help(action, "[--plain]"));
            }
            if let Err(e) = parse_flags(args, &["plain"], &[]) {
                return bad(action, &e);
            }
            for l in color_lines() {
                let _ = writeln!(out, "{l}");
            }
            0
        }
        "list-fonts" => {
            if wants_help(args) {
                return Some(help(action, "[--family=<이름>] [--style=<이름>] [--bold] [--italic]"));
            }
            let f = match parse_flags(args, &["bold", "italic"], &["family", "style"]) {
                Ok(f) => f,
                Err(e) => return bad(action, &e),
            };
            let opts = FontListOpts {
                family: f.value("family").map(str::to_string),
                style: f.value("style").map(str::to_string),
                bold: f.has("bold"),
                italic: f.has("italic"),
            };
            let _ = write!(out, "{}", list_fonts_text(&bt_render::list_system_faces(), &opts));
            0
        }
        "show-face" => {
            if wants_help(args) {
                return Some(help(
                    action,
                    "[--cp=<번호>] [--string=<글>] [--style=regular|bold|italic|bold_italic] [--presentation=text|emoji]",
                ));
            }
            let f = match parse_flags(args, &[], &["cp", "string", "style", "presentation"]) {
                Ok(f) => f,
                Err(e) => return bad(action, &e),
            };
            show_face(&f, &mut out)
        }
        "explain-config" => {
            if wants_help(args) {
                return Some(help(action, "<키> | --option=<키> | --keybind=<동작> [--no-pager]"));
            }
            explain_config(args, &mut out)
        }
        _ => return None,
    };
    let _ = out.flush();
    Some(code)
}

fn help(action: &str, usage: &str) -> i32 {
    println!("사용법: b-terminal +{action} {usage}");
    0
}

fn show_face(f: &Flags, out: &mut impl Write) -> i32 {
    let mut chars: Vec<char> = Vec::new();
    if let Some(cp) = f.value("cp") {
        match parse_codepoint(cp).and_then(char::from_u32) {
            Some(c) => chars.push(c),
            None => {
                eprintln!("--cp 값을 읽지 못했습니다: {cp}");
                return 1;
            }
        }
    }
    if let Some(s) = f.value("string") {
        chars.extend(s.chars());
    }
    if f.value("cp").is_none() && f.value("string").is_none() {
        eprintln!("--cp로 글자 번호를, 또는 --string으로 글을 줘야 합니다");
        return 1;
    }
    let style = match f.value("style").unwrap_or("regular") {
        "regular" => FontStyle::Regular,
        "bold" => FontStyle::Bold,
        "italic" => FontStyle::Italic,
        "bold_italic" => FontStyle::BoldItalic,
        other => {
            eprintln!("--style은 regular·bold·italic·bold_italic입니다: {other}");
            return 1;
        }
    };
    let presentation = match f.value("presentation") {
        None => None,
        Some("text") => Some(false),
        Some("emoji") => Some(true),
        Some(other) => {
            eprintln!("--presentation은 text·emoji입니다: {other}");
            return 1;
        }
    };
    // 앱이 쓰는 글꼴 설정 그대로(설정 파일 반영)
    let config = bt_core::Config::load_with(None, false, true);
    let fonts = match bt_render::Fonts::new(&crate::app::font_config(&config, config.font_size, 1.0)) {
        Ok(f) => f,
        Err(e) => {
            eprintln!("글꼴을 읽지 못했습니다: {e}");
            return 1;
        }
    };
    for c in chars {
        let _ = writeln!(out, "{}", face_report(c, &fonts.face_source(c, style, presentation)));
    }
    0
}

fn explain_config(args: &[String], out: &mut impl Write) -> i32 {
    let (mut option, mut keybind, mut positional) = (None, None, None);
    for a in args {
        if let Some(v) = a.strip_prefix("--option=") {
            option = Some(v.to_string());
        } else if let Some(v) = a.strip_prefix("--keybind=") {
            keybind = Some(v.to_string());
        } else if a == "--no-pager" || a.starts_with('-') {
            // 쪽 나누기는 쓰지 않는다. 모르는 `-` 인자는 Ghostty처럼 무시한다
        } else {
            positional = Some(a.clone());
        }
    }
    let Some(name) = keybind.clone().or(option.clone()).or(positional) else {
        eprintln!("Usage: b-terminal +explain-config <option>");
        eprintln!("       b-terminal +explain-config --option=<option>");
        eprintln!("       b-terminal +explain-config --keybind=<action>");
        return 1;
    };
    let text = if keybind.is_some() {
        explain_action(&name)
    } else if option.is_some() {
        explain_option(CONFIG_DOC, &name)
    } else {
        explain_option(CONFIG_DOC, &name).or_else(|| explain_action(&name))
    };
    match text {
        Some(t) => {
            let _ = writeln!(out, "{t}");
            0
        }
        None => {
            let _ = writeln!(out, "Unknown: '{name}'.");
            1
        }
    }
}

/// 동작 이름이면 알려 준다. 동작별 설명 데이터는 없어서 문서 위치만 가리킨다
fn explain_action(name: &str) -> Option<String> {
    ACTIONS.contains(&name).then(|| {
        format!("{name}\n  b-terminal이 받는 키 묶음 동작입니다. 쓰는 법은 docs/configuration.md의 \"키 묶음\" 절에 있습니다.\n")
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn s(v: &[&str]) -> Vec<String> {
        v.iter().map(|x| x.to_string()).collect()
    }

    #[test]
    fn 키_묶음은_기본_위에_설정을_얹는다() {
        let sup = if cfg!(target_os = "macos") {
            "super+"
        } else {
            "ctrl+shift+"
        };
        let lines = keybind_lines(&s(&[&format!("{sup}t=unbind"), "ctrl+a>n=new_window"]), false);
        assert!(
            lines.contains(&"keybind = ctrl+a>n=new_window".to_string()),
            "{lines:?}"
        );
        // unbind한 기본은 빠진다
        assert!(
            !lines.iter().any(|l| l.starts_with(&format!("keybind = {sup}t="))),
            "{lines:?}"
        );
        assert!(!lines.iter().any(|l| l.contains("unbind")), "{lines:?}");
        // 건드리지 않은 기본은 남는다
        assert!(
            lines.contains(&format!("keybind = performable:{sup}c=copy_to_clipboard")),
            "{lines:?}"
        );
    }

    #[test]
    fn 같은_트리거는_나중_것이_이기고_clear는_기본을_지운다() {
        let lines = keybind_lines(&s(&["ctrl+q=new_tab", "ctrl+q=close_tab"]), false);
        assert!(lines.contains(&"keybind = ctrl+q=close_tab".to_string()), "{lines:?}");
        assert!(!lines.contains(&"keybind = ctrl+q=new_tab".to_string()), "{lines:?}");
        let lines = keybind_lines(&s(&["clear", "ctrl+x=reset"]), false);
        assert_eq!(lines, vec!["keybind = ctrl+x=reset".to_string()]);
    }

    /// `chain=<동작>` 줄은 앞 묶음 줄 바로 뒤에 쓴 그대로 보이고, 잘못된 chain 줄과 지워진 묶음의 chain 줄은 보이지 않는다(#766)
    #[test]
    fn chain_줄은_앞_묶음_뒤에_보인다() {
        let lines = keybind_lines(
            &s(&[
                "clear",
                "ctrl+a=new_window",
                "chain=new_tab",
                "ctrl+b=new_tab",
                "chain=close_tab",
            ]),
            false,
        );
        assert_eq!(
            lines,
            vec![
                "keybind = ctrl+a=new_window".to_string(),
                "keybind = chain=new_tab".to_string(),
                "keybind = ctrl+b=new_tab".to_string(),
                "keybind = chain=close_tab".to_string(),
            ]
        );
        // 앞 묶음이 없거나, unbind·chain=unbind·접두사 붙은 chain은 경고라 보이지 않는다
        let lines = keybind_lines(
            &s(&[
                "clear",
                "chain=new_tab",
                "ctrl+a=new_window",
                "chain=unbind",
                "global:chain=ignore",
                "ctrl+a=unbind",
                "chain=new_tab",
            ]),
            false,
        );
        assert!(lines.is_empty(), "{lines:?}");
        // 같은 키를 다시 정하면 앞서 덧붙은 chain도 함께 바뀐다
        let lines = keybind_lines(
            &s(&["clear", "ctrl+a=new_window", "chain=new_tab", "ctrl+a=close_tab"]),
            false,
        );
        assert_eq!(lines, vec!["keybind = ctrl+a=close_tab".to_string()]);
        // 키 테이블·전역 묶음의 chain
        let lines = keybind_lines(&s(&["clear", "nav/h=goto_split:left", "chain=text:x"]), false);
        assert_eq!(
            lines,
            vec![
                "keybind = nav/h=goto_split:left".to_string(),
                "keybind = chain=text:x".to_string()
            ]
        );
    }

    #[test]
    fn 기본만_보기는_설정을_무시한다() {
        let a = keybind_lines(&s(&["clear"]), true);
        assert!(a.len() > 20, "{}", a.len());
    }

    #[test]
    fn 읽지_못한_키_묶음_줄은_건너뛴다() {
        let lines = keybind_lines(&s(&["clear", "ctrl+x=없는동작", "ctrl+y=reset"]), false);
        assert_eq!(lines, vec!["keybind = ctrl+y=reset".to_string()]);
    }

    #[test]
    fn 수식_키는_super_ctrl_alt_shift_차례로_쓴다() {
        let lines = keybind_lines(&s(&["clear", "shift+alt+ctrl+cmd+a=reset"]), false);
        assert_eq!(lines, vec!["keybind = super+ctrl+alt+shift+a=reset".to_string()]);
    }

    #[test]
    fn 접두사와_키_테이블을_보인다() {
        let lines = keybind_lines(
            &s(&[
                "clear",
                "global:ctrl+backquote=toggle_quick_terminal",
                "nav/h=goto_split:left",
            ]),
            false,
        );
        assert!(
            lines.contains(&"keybind = global:ctrl+backquote=toggle_quick_terminal".to_string()),
            "{lines:?}"
        );
        assert!(
            lines.contains(&"keybind = nav/h=goto_split:left".to_string()),
            "{lines:?}"
        );
    }

    #[test]
    fn 동작_이름은_모두_키_묶음이_받는_이름이다() {
        let names = action_names();
        assert!(names.len() > 60, "{}", names.len());
        for n in &names {
            if let Err(e) = keymap::parse_action(n) {
                assert!(!e.contains("없는 동작"), "{n}: {e}");
            }
        }
        assert!(names.contains(&"new_tab") && names.contains(&"copy_to_clipboard"));
        let mut sorted = names.clone();
        sorted.dedup();
        assert_eq!(sorted.len(), names.len(), "중복");
    }

    #[test]
    fn 색_목록은_이름순이고_16진수_세_바이트다() {
        let lines = color_lines();
        assert!(lines.len() > 600, "{}", lines.len());
        assert!(lines.contains(&"AliceBlue = #f0f8ff".to_string()));
        assert!(lines.contains(&"gray50 = #7f7f7f".to_string()));
        assert!(lines.contains(&"RebeccaPurple = #663399".to_string()));
        assert!(lines.contains(&"x11 gray = #bebebe".to_string()));
        let names: Vec<&str> = lines.iter().map(|l| l.split(" = ").next().unwrap()).collect();
        let mut sorted = names.clone();
        sorted.sort_by_key(|n| n.to_ascii_lowercase());
        sorted.dedup_by_key(|n| n.to_ascii_lowercase());
        assert_eq!(sorted, names, "정렬과 중복");
    }

    /// #837: Ghostty `+list-colors`처럼 `rgb.txt` 782개를 원래 대소문자로, 대소문자 무시 이름순으로 낸다
    #[test]
    fn 색_목록은_ghostty처럼_782개를_원래_대소문자로_낸다() {
        let lines = color_lines();
        assert_eq!(lines.len(), 782);
        assert_eq!(lines[0], "alice blue = #f0f8ff");
        assert_eq!(lines[1], "AliceBlue = #f0f8ff");
        assert!(lines.contains(&"LawnGreen = #7cfc00".to_string()));
        assert_eq!(lines.last().unwrap(), "YellowGreen = #9acd32");
    }

    #[test]
    fn 글자_번호는_10_16_8_2진수를_받는다() {
        assert_eq!(parse_codepoint("65"), Some(65));
        assert_eq!(parse_codepoint("0x41"), Some(0x41));
        assert_eq!(parse_codepoint("0o101"), Some(0o101));
        assert_eq!(parse_codepoint("0b1000001"), Some(65));
        assert_eq!(parse_codepoint("zz"), None);
        assert_eq!(parse_codepoint("0x110000"), None);
    }

    #[test]
    fn 얼굴_보고는_ghostty_문구를_따른다() {
        use bt_render::FaceSource::*;
        assert_eq!(
            face_report('A', &Primary("JB Regular".into())),
            "U+0041 « A » found in face “JB Regular”."
        );
        assert_eq!(
            face_report('가', &Fallback("Apple SD Gothic Neo".into())),
            "U+AC00 « 가 » found in face “Apple SD Gothic Neo”."
        );
        assert_eq!(face_report('\u{e000}', &NotFound), "U+E000 « \u{e000} » not found.");
        assert_eq!(
            face_report('─', &Sprite),
            "U+2500 « ─ » is handled by b-terminal's internal box drawing."
        );
    }

    #[test]
    fn 글꼴_목록은_집안별로_묶고_이름순이다() {
        let f = |fam: &str, name: &str, mono, bold| FaceEntry {
            family: fam.into(),
            name: name.into(),
            monospaced: mono,
            bold,
            italic: false,
        };
        let faces = vec![
            f("Zed Mono", "ZedMono-Bold", true, true),
            f("Alpha Mono", "AlphaMono-Regular", true, false),
            f("Zed Mono", "ZedMono-Regular", true, false),
            f("Beta Sans", "BetaSans-Regular", false, false),
        ];
        assert_eq!(
            list_fonts_text(&faces, &FontListOpts::default()),
            "Alpha Mono\n  AlphaMono-Regular\n\nZed Mono\n  ZedMono-Bold\n  ZedMono-Regular\n\n"
        );
        let opts = FontListOpts {
            family: Some("beta sans".into()),
            ..Default::default()
        };
        assert_eq!(list_fonts_text(&faces, &opts), "Beta Sans\n  BetaSans-Regular\n\n");
        let opts = FontListOpts {
            bold: true,
            ..Default::default()
        };
        assert_eq!(list_fonts_text(&faces, &opts), "Zed Mono\n  ZedMono-Bold\n\n");
    }

    #[test]
    fn 설정_설명은_문서_표에서_뽑는다() {
        let doc = "## 지원하는 키\n\n| 키 | 값 | 기본 | 메모 |\n|---|---|---|---|\n| `font-size` | 포인트 | `13` | 크기 |\n| `a` / `b` | 값 | 없음 | 메모 | 더 |\n\n## 다음\n| `other` | x | x | x |\n";
        assert_eq!(
            explain_option(doc, "font-size").unwrap(),
            "font-size\n  값: 포인트\n  기본: `13`\n  메모: 크기\n"
        );
        assert!(explain_option(doc, "b").unwrap().contains("메모: 메모 | 더"));
        assert!(explain_option(doc, "other").is_none(), "표 밖의 줄");
        assert!(explain_option(doc, "없음").is_none());
        // 진짜 문서에서도 찾는다
        assert!(explain_option(CONFIG_DOC, "font-size").is_some());
    }

    #[test]
    fn 인자를_읽는다() {
        let p = parse_flags(
            &s(&["--family=Menlo", "--bold", "--style", "Bold", "x"]),
            &["bold"],
            &["family", "style"],
        )
        .unwrap();
        assert_eq!(p.value("family"), Some("Menlo"));
        assert_eq!(p.value("style"), Some("Bold"));
        assert!(p.has("bold"));
        assert_eq!(p.rest, vec!["x".to_string()]);
        assert!(parse_flags(&s(&["--family"]), &["bold"], &["family"]).is_err());
        assert!(parse_flags(&s(&["--nope"]), &["bold"], &["family"]).is_err());
    }

    #[test]
    fn 동작_해석과_종료_코드() {
        assert_eq!(run("version", &[]), None);
        assert_eq!(run("show-face", &[]), Some(1));
        assert_eq!(run("list-actions", &s(&["--plain"])), Some(0));
        assert_eq!(run("list-actions", &s(&["--nope"])), Some(1));
        assert_eq!(run("explain-config", &[]), Some(1));
        assert_eq!(run("explain-config", &s(&["이런키는없음"])), Some(1));
        assert_eq!(run("explain-config", &s(&["font-size"])), Some(0));
    }
}
