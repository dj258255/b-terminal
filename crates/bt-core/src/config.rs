//! 설정 파일. Ghostty와 같은 문법(`키 = 값`, `#` 주석, 같은 키 여러 번)을 쓴다.
//!
//! 읽는 순서(뒤가 앞을 덮는다):
//! 1. 기본값(JetBrains Mono 13, 바탕 #282c34, 글자 #ffffff, Tomorrow Night 팔레트)
//! 2. Ghostty 설정 파일 — `import-ghostty = true`일 때만(옮겨 오는 사람을 위한 선택, 기본 끔, ADR-006)
//! 3. b-terminal 설정 파일 `~/.config/b-terminal/config`
//!
//! 모르는 키는 버리고 경고로 모은다(`b-terminal --show-config`가 보여 준다)

use std::path::{Path, PathBuf};

use crate::theme::{Rgb, Theme};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CursorStyle {
    Block,
    Bar,
    Underline,
    BlockHollow,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OptionAsAlt {
    None,
    Left,
    Right,
    Both,
}

/// 셀 크기 보정: 퍼센트 또는 픽셀
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Adjust {
    Percent(f32),
    Points(f32),
}

impl Adjust {
    fn parse(v: &str) -> Option<Self> {
        let v = v.trim();
        if let Some(p) = v.strip_suffix('%') {
            return p.trim().parse().ok().map(Adjust::Percent);
        }
        v.parse().ok().map(Adjust::Points)
    }

    /// 기준 크기에 보정을 더한 값
    pub fn apply(self, base: f32) -> f32 {
        match self {
            Adjust::Percent(p) => base * (1.0 + p / 100.0),
            Adjust::Points(px) => base + px,
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum WorkingDirectory {
    Home,
    Inherit,
    Path(PathBuf),
}

/// 읽어 낸 설정
#[derive(Debug, Clone, PartialEq)]
pub struct Config {
    /// 앞이 기본 글꼴, 뒤는 대체 글꼴(그 뒤에 넣어 둔 기본 대체 글꼴이 붙는다)
    pub font_family: Vec<String>,
    pub font_family_bold: Option<String>,
    pub font_family_italic: Option<String>,
    pub font_family_bold_italic: Option<String>,
    pub font_size: f32,
    pub adjust_cell_height: Option<Adjust>,
    pub adjust_cell_width: Option<Adjust>,
    pub theme: Theme,
    pub cursor_style: CursorStyle,
    pub cursor_blink: Option<bool>,
    /// (왼, 오른) 포인트
    pub padding_x: (f32, f32),
    /// (위, 아래) 포인트
    pub padding_y: (f32, f32),
    /// 처음 창 크기(셀)
    pub window_width: Option<u16>,
    pub window_height: Option<u16>,
    pub option_as_alt: OptionAsAlt,
    pub scrollback_lines: usize,
    pub copy_on_select: bool,
    pub mouse_hide_while_typing: bool,
    pub command: Option<String>,
    pub working_directory: WorkingDirectory,
    /// 정해 둔 창 제목(셸이 보낸 제목보다 먼저)
    pub title: Option<String>,
    /// `keybind = 트리거=동작`. 앱이 해석한다
    pub keybinds: Vec<String>,
    /// 셸이 마우스 모드를 켠 채 끝났을 때 셸로 돌아오면 끈다(#24)
    pub mouse_reset_on_shell: bool,
    /// OSC 52로 프로그램이 클립보드에 쓰는 것을 허용(Ghostty 기본 allow)
    pub clipboard_write: bool,
    /// OSC 52로 프로그램이 클립보드를 읽는 것을 허용(기본 금지. Ghostty 기본은 ask)
    pub clipboard_read: bool,
    pub warnings: Vec<String>,
    /// 실제로 읽은 파일
    pub sources: Vec<PathBuf>,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            font_family: Vec::new(),
            font_family_bold: None,
            font_family_italic: None,
            font_family_bold_italic: None,
            font_size: 13.0,
            adjust_cell_height: None,
            adjust_cell_width: None,
            theme: Theme::ghostty_default(),
            cursor_style: CursorStyle::Block,
            cursor_blink: None,
            padding_x: (2.0, 2.0),
            padding_y: (2.0, 2.0),
            window_width: None,
            window_height: None,
            option_as_alt: OptionAsAlt::None,
            scrollback_lines: 10_000,
            copy_on_select: true,
            mouse_hide_while_typing: false,
            command: None,
            working_directory: WorkingDirectory::Inherit,
            title: None,
            keybinds: Vec::new(),
            mouse_reset_on_shell: true,
            clipboard_write: true,
            clipboard_read: false,
            warnings: Vec::new(),
            sources: Vec::new(),
        }
    }
}

/// 설정 파일 한 줄
#[derive(Debug, Clone, PartialEq)]
pub struct Entry {
    pub key: String,
    pub value: String,
    pub file: PathBuf,
    pub line: usize,
}

/// 파일을 줄 단위로 읽는다. `config-file = 경로`는 그 자리에서 읽어 넣는다(`?`로 시작하면 없어도 된다)
pub fn read_entries(path: &Path, depth: usize, warnings: &mut Vec<String>, sources: &mut Vec<PathBuf>) -> Vec<Entry> {
    // 서로 부르는 설정 파일이 끝없이 돌지 않게
    if depth > 8 {
        warnings.push(format!("{}: config-file이 너무 깊게 이어집니다", path.display()));
        return Vec::new();
    }
    let Ok(text) = std::fs::read_to_string(path) else {
        return Vec::new();
    };
    sources.push(path.to_path_buf());
    let base = path.parent().unwrap_or(Path::new("."));
    let mut out = Vec::new();
    for (i, raw) in text.lines().enumerate() {
        let line = raw.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let Some((k, v)) = line.split_once('=') else {
            warnings.push(format!("{}:{}: `키 = 값` 꼴이 아닙니다: {line}", path.display(), i + 1));
            continue;
        };
        let key = k.trim().to_string();
        let value = unquote(v.trim()).to_string();
        if key == "config-file" {
            let (optional, p) = match value.strip_prefix('?') {
                Some(rest) => (true, rest),
                None => (false, value.as_str()),
            };
            let target = expand_home(p, base);
            if !target.exists() {
                if !optional {
                    warnings.push(format!(
                        "{}:{}: config-file {}이 없습니다",
                        path.display(),
                        i + 1,
                        target.display()
                    ));
                }
                continue;
            }
            out.extend(read_entries(&target, depth + 1, warnings, sources));
            continue;
        }
        out.push(Entry {
            key,
            value,
            file: path.to_path_buf(),
            line: i + 1,
        });
    }
    out
}

fn unquote(v: &str) -> &str {
    if v.len() >= 2 && v.starts_with('"') && v.ends_with('"') {
        &v[1..v.len() - 1]
    } else {
        v
    }
}

fn expand_home(p: &str, base: &Path) -> PathBuf {
    if let Some(rest) = p.strip_prefix("~/") {
        return home().join(rest);
    }
    let pb = PathBuf::from(p);
    if pb.is_absolute() { pb } else { base.join(pb) }
}

fn home() -> PathBuf {
    std::env::var_os("HOME").map(PathBuf::from).unwrap_or_default()
}

fn parse_bool(v: &str) -> Option<bool> {
    match v.trim() {
        "true" => Some(true),
        "false" => Some(false),
        _ => None,
    }
}

/// `2` 또는 `2,4`
fn parse_pair(v: &str) -> Option<(f32, f32)> {
    let mut it = v.split(',').map(|s| s.trim().parse::<f32>());
    let a = it.next()?.ok()?;
    let b = match it.next() {
        Some(b) => b.ok()?,
        None => a,
    };
    Some((a.max(0.0), b.max(0.0)))
}

/// Ghostty 설정 파일 자리(먼저 찾은 것 하나). macOS는 Application Support도 본다
pub fn ghostty_config_paths() -> Vec<PathBuf> {
    let h = home();
    let xdg = std::env::var_os("XDG_CONFIG_HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|| h.join(".config"));
    let candidates = [
        xdg.join("ghostty/config"),
        xdg.join("ghostty/config.ghostty"),
        h.join("Library/Application Support/com.mitchellh.ghostty/config"),
        h.join("Library/Application Support/com.mitchellh.ghostty/config.ghostty"),
    ];
    candidates.into_iter().filter(|p| p.is_file()).collect()
}

pub fn bterminal_config_path() -> PathBuf {
    let xdg = std::env::var_os("XDG_CONFIG_HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|| home().join(".config"));
    xdg.join("b-terminal/config")
}

include!(concat!(env!("OUT_DIR"), "/themes.rs"));

/// 사용자 테마 폴더. 넣어 둔 테마보다 먼저 본다(같은 이름이면 사용자 것이 이긴다)
pub fn theme_dirs() -> Vec<PathBuf> {
    let xdg = std::env::var_os("XDG_CONFIG_HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|| home().join(".config"));
    vec![xdg.join("b-terminal/themes")]
}

/// 넣어 둔 테마 이름 목록(`b-terminal --list-themes`)
pub fn bundled_theme_names() -> impl Iterator<Item = &'static str> {
    BUNDLED_THEMES.iter().map(|(n, _)| *n)
}

impl Config {
    /// 기본값 → Ghostty 설정 → b-terminal 설정 → 덧붙인 파일 순서로 읽는다. `dark`는 `theme = light:…,dark:…`에서 고를 쪽
    pub fn load(extra: Option<&Path>, dark: bool) -> Self {
        let mut cfg = Config::default();
        let mut warnings = Vec::new();
        let mut sources = Vec::new();
        let own = read_entries(&bterminal_config_path(), 0, &mut warnings, &mut sources);
        let extra_entries = extra
            .map(|p| read_entries(p, 0, &mut warnings, &mut sources))
            .unwrap_or_default();
        let import = own.iter().chain(&extra_entries).rfind(|e| e.key == "import-ghostty");
        // Ghostty 설정은 옮겨 오는 사람만 켠다(기본 끔, ADR-006)
        let import = import.map(|e| parse_bool(&e.value) == Some(true)).unwrap_or(false);
        let mut entries = Vec::new();
        if import {
            if let Some(g) = ghostty_config_paths().into_iter().next() {
                let mut gs = Vec::new();
                entries.extend(read_entries(&g, 0, &mut warnings, &mut gs));
                // Ghostty 파일을 먼저 읽은 것으로 적는다
                gs.extend(sources);
                sources = gs;
            }
        }
        entries.extend(own);
        entries.extend(extra_entries);
        cfg.apply_all(&entries, dark, &mut warnings);
        cfg.warnings = warnings;
        cfg.sources = sources;
        cfg
    }

    /// 글로 된 설정을 읽는다(테스트와 `--show-config`)
    pub fn from_text(text: &str, dark: bool) -> Self {
        let mut cfg = Config::default();
        let mut warnings = Vec::new();
        let entries: Vec<Entry> = text
            .lines()
            .enumerate()
            .filter_map(|(i, l)| {
                let l = l.trim();
                if l.is_empty() || l.starts_with('#') {
                    return None;
                }
                let (k, v) = l.split_once('=')?;
                Some(Entry {
                    key: k.trim().into(),
                    value: unquote(v.trim()).into(),
                    file: PathBuf::from("<글>"),
                    line: i + 1,
                })
            })
            .collect();
        cfg.apply_all(&entries, dark, &mut warnings);
        cfg.warnings = warnings;
        cfg
    }

    fn apply_all(&mut self, entries: &[Entry], dark: bool, warnings: &mut Vec<String>) {
        // 테마를 먼저 깔고, 그 위에 색 키를 덮는다(Ghostty와 같은 순서). 마지막 theme 값이 이긴다
        if let Some(theme) = entries.iter().rfind(|e| e.key == "theme") {
            if !theme.value.trim().is_empty() {
                let name = pick_theme_name(&theme.value, dark);
                match load_theme(&name) {
                    Some(t) => {
                        let mut tw = Vec::new();
                        for e in &t {
                            self.apply_color(e, &mut tw);
                        }
                    }
                    None => warnings.push(format!(
                        "{}:{}: 테마 '{name}'을 찾지 못했습니다{}",
                        theme.file.display(),
                        theme.line,
                        match similar_themes(&name) {
                            v if v.is_empty() => String::new(),
                            v => format!(" (비슷한 이름: {})", v.join(", ")),
                        }
                    )),
                }
            }
        }
        // font-family는 파일마다 처음부터 다시 쌓는다(Ghostty: 빈 값이면 지움)
        let mut families_reset = false;
        for e in entries {
            if e.key == "font-family" && !families_reset {
                self.font_family.clear();
                families_reset = true;
            }
            self.apply(e, warnings);
        }
    }

    fn apply_color(&mut self, e: &Entry, warnings: &mut Vec<String>) -> bool {
        let v = e.value.trim();
        let color = || Rgb::parse_hex(v).or_else(|| named_color(v));
        let t = &mut self.theme;
        match e.key.as_str() {
            "background" => t.background = color().unwrap_or(t.background),
            "foreground" => t.foreground = color().unwrap_or(t.foreground),
            "cursor-color" => t.cursor = color().unwrap_or(t.cursor),
            "cursor-text" => t.cursor_text = color().unwrap_or(t.cursor_text),
            "selection-background" => t.selection_background = color().unwrap_or(t.selection_background),
            "selection-foreground" => t.selection_foreground = color(),
            "palette" => {
                let parsed = v.split_once('=').and_then(|(n, c)| {
                    Some((
                        n.trim().parse::<usize>().ok()?,
                        Rgb::parse_hex(c).or_else(|| named_color(c.trim()))?,
                    ))
                });
                match parsed {
                    Some((n, c)) if n < 16 => t.palette[n] = c,
                    // 16~255번은 xterm 표를 그대로 쓴다(바꾼 값은 아직 받지 않는다)
                    Some(_) => {}
                    None => warnings.push(format!(
                        "{}:{}: palette 값을 읽지 못했습니다: {v}",
                        e.file.display(),
                        e.line
                    )),
                }
            }
            "bold-is-bright" => t.bold_is_bright = parse_bool(v).unwrap_or(t.bold_is_bright),
            _ => return false,
        }
        true
    }

    fn apply(&mut self, e: &Entry, warnings: &mut Vec<String>) {
        if self.apply_color(e, warnings) {
            return;
        }
        let v = e.value.trim();
        let bad = |warnings: &mut Vec<String>| {
            warnings.push(format!(
                "{}:{}: {} 값을 읽지 못했습니다: {v}",
                e.file.display(),
                e.line,
                e.key
            ))
        };
        match e.key.as_str() {
            "theme" | "import-ghostty" => {}
            "font-family" => {
                if v.is_empty() {
                    self.font_family.clear();
                } else {
                    self.font_family.push(v.to_string());
                }
            }
            "font-family-bold" => self.font_family_bold = Some(v.to_string()).filter(|s| !s.is_empty()),
            "font-family-italic" => self.font_family_italic = Some(v.to_string()).filter(|s| !s.is_empty()),
            "font-family-bold-italic" => self.font_family_bold_italic = Some(v.to_string()).filter(|s| !s.is_empty()),
            "font-size" => match v.parse::<f32>() {
                Ok(s) if (4.0..=200.0).contains(&s) => self.font_size = s,
                _ => bad(warnings),
            },
            "adjust-cell-height" => {
                self.adjust_cell_height = if v.is_empty() {
                    None
                } else {
                    Adjust::parse(v).or_else(|| {
                        bad(warnings);
                        None
                    })
                }
            }
            "adjust-cell-width" => {
                self.adjust_cell_width = if v.is_empty() {
                    None
                } else {
                    Adjust::parse(v).or_else(|| {
                        bad(warnings);
                        None
                    })
                }
            }
            "cursor-style" => match v {
                "block" => self.cursor_style = CursorStyle::Block,
                "bar" => self.cursor_style = CursorStyle::Bar,
                "underline" => self.cursor_style = CursorStyle::Underline,
                "block_hollow" => self.cursor_style = CursorStyle::BlockHollow,
                _ => bad(warnings),
            },
            "cursor-style-blink" => self.cursor_blink = if v.is_empty() { None } else { parse_bool(v) },
            "window-padding-x" => match parse_pair(v) {
                Some(p) => self.padding_x = p,
                None => bad(warnings),
            },
            "window-padding-y" => match parse_pair(v) {
                Some(p) => self.padding_y = p,
                None => bad(warnings),
            },
            "window-width" => self.window_width = v.parse::<u16>().ok().filter(|n| *n >= 10),
            "window-height" => self.window_height = v.parse::<u16>().ok().filter(|n| *n >= 4),
            "macos-option-as-alt" => match v {
                "true" => self.option_as_alt = OptionAsAlt::Both,
                "false" => self.option_as_alt = OptionAsAlt::None,
                "left" => self.option_as_alt = OptionAsAlt::Left,
                "right" => self.option_as_alt = OptionAsAlt::Right,
                _ => bad(warnings),
            },
            // Ghostty는 바이트 수다. 줄 하나를 대략 1KB로 보고 줄 수로 바꾼다
            "scrollback-limit" => match v.parse::<u64>() {
                Ok(bytes) => self.scrollback_lines = ((bytes / 1000) as usize).clamp(100, 200_000),
                Err(_) => bad(warnings),
            },
            // b-terminal 키: 줄 수로 바로
            "scrollback-lines" => match v.parse::<usize>() {
                Ok(n) => self.scrollback_lines = n.clamp(0, 1_000_000),
                Err(_) => bad(warnings),
            },
            "copy-on-select" => match v {
                "true" | "clipboard" => self.copy_on_select = true,
                "false" => self.copy_on_select = false,
                _ => bad(warnings),
            },
            "mouse-hide-while-typing" => {
                self.mouse_hide_while_typing = parse_bool(v).unwrap_or(self.mouse_hide_while_typing)
            }
            "command" => self.command = Some(v.to_string()).filter(|s| !s.is_empty()),
            "working-directory" => {
                self.working_directory = match v {
                    "" | "inherit" => WorkingDirectory::Inherit,
                    "home" => WorkingDirectory::Home,
                    p => WorkingDirectory::Path(expand_home(p, e.file.parent().unwrap_or(Path::new(".")))),
                }
            }
            "title" => self.title = Some(v.to_string()).filter(|s| !s.is_empty()),
            "keybind" => self.keybinds.push(v.to_string()),
            "clipboard-write" => match v {
                "allow" | "true" => self.clipboard_write = true,
                "deny" | "false" => self.clipboard_write = false,
                _ => bad(warnings),
            },
            // ask는 아직 물을 창이 없어 금지로 본다
            "clipboard-read" => match v {
                "allow" | "true" => self.clipboard_read = true,
                "deny" | "false" | "ask" => self.clipboard_read = false,
                _ => bad(warnings),
            },
            "mouse-reset-on-shell" => self.mouse_reset_on_shell = parse_bool(v).unwrap_or(true),
            other => {
                // Ghostty에만 있는 키는 조용히 넘기지 않고 알린다
                if !KNOWN_IGNORED.contains(&other) {
                    warnings.push(format!(
                        "{}:{}: b-terminal이 아직 모르는 설정입니다: {other}",
                        e.file.display(),
                        e.line
                    ));
                }
            }
        }
    }

    /// 지금 설정을 Ghostty 문법으로 적는다(`--show-config`)
    pub fn render(&self) -> String {
        let t = &self.theme;
        let mut out = Vec::new();
        let mut push = |k: &str, v: String| out.push(format!("{k} = {v}"));
        if self.font_family.is_empty() {
            push("font-family", "JetBrains Mono  # 넣어 둔 기본 글꼴".into());
        }
        for f in &self.font_family {
            push("font-family", f.clone());
        }
        push("font-size", format!("{}", self.font_size));
        push("background", hex(t.background));
        push("foreground", hex(t.foreground));
        push("cursor-color", hex(t.cursor));
        push("cursor-text", hex(t.cursor_text));
        push("selection-background", hex(t.selection_background));
        if let Some(c) = t.selection_foreground {
            push("selection-foreground", hex(c));
        }
        for (i, c) in t.palette.iter().enumerate() {
            push("palette", format!("{i}={}", hex(*c)));
        }
        push("bold-is-bright", t.bold_is_bright.to_string());
        push(
            "cursor-style",
            match self.cursor_style {
                CursorStyle::Block => "block",
                CursorStyle::Bar => "bar",
                CursorStyle::Underline => "underline",
                CursorStyle::BlockHollow => "block_hollow",
            }
            .into(),
        );
        push("window-padding-x", format!("{},{}", self.padding_x.0, self.padding_x.1));
        push("window-padding-y", format!("{},{}", self.padding_y.0, self.padding_y.1));
        push(
            "macos-option-as-alt",
            match self.option_as_alt {
                OptionAsAlt::None => "false",
                OptionAsAlt::Left => "left",
                OptionAsAlt::Right => "right",
                OptionAsAlt::Both => "true",
            }
            .into(),
        );
        push("scrollback-lines", self.scrollback_lines.to_string());
        push("copy-on-select", self.copy_on_select.to_string());
        push("mouse-hide-while-typing", self.mouse_hide_while_typing.to_string());
        for k in &self.keybinds {
            push("keybind", k.clone());
        }
        out.join("\n")
    }
}

/// Ghostty에 있지만 b-terminal이 일부러 쓰지 않는 키(경고하지 않는다). 화면 모양·창 장식처럼 지금 그릴 수 없는 것
const KNOWN_IGNORED: &[&str] = &[
    "font-feature",
    "font-thicken",
    "font-thicken-strength",
    "font-synthetic-style",
    "background-opacity",
    "background-blur",
    "background-blur-radius",
    "window-decoration",
    "window-theme",
    "window-save-state",
    "window-colorspace",
    "macos-titlebar-style",
    "macos-titlebar-proxy-icon",
    "macos-icon",
    "macos-window-shadow",
    "confirm-close-surface",
    "quit-after-last-window-closed",
    "shell-integration",
    "shell-integration-features",
    "auto-update",
    "auto-update-channel",
    "unfocused-split-opacity",
    "minimum-contrast",
    "link-url",
    "clipboard-paste-protection",
    "resize-overlay",
    "gtk-titlebar",
    "gtk-single-instance",
];

fn hex(c: Rgb) -> String {
    format!("#{:02x}{:02x}{:02x}", c.r, c.g, c.b)
}

/// X11 색 이름 몇 개(Ghostty 테마가 가끔 쓴다)
fn named_color(v: &str) -> Option<Rgb> {
    Some(match v.to_ascii_lowercase().as_str() {
        "black" => Rgb::new(0, 0, 0),
        "white" => Rgb::new(255, 255, 255),
        "red" => Rgb::new(255, 0, 0),
        "green" => Rgb::new(0, 255, 0),
        "blue" => Rgb::new(0, 0, 255),
        _ => return None,
    })
}

/// `light:A,dark:B`에서 고르고, 그냥 이름이면 그대로
fn pick_theme_name(v: &str, dark: bool) -> String {
    let mut light = None;
    let mut darkn = None;
    for part in v.split(',') {
        let part = part.trim();
        if let Some(n) = part.strip_prefix("light:") {
            light = Some(n.trim().to_string());
        } else if let Some(n) = part.strip_prefix("dark:") {
            darkn = Some(n.trim().to_string());
        }
    }
    match (light, darkn) {
        (None, None) => v.trim().to_string(),
        (l, d) => if dark { d.or(l) } else { l.or(d) }.unwrap_or_default(),
    }
}

/// 테마 이름이나 경로로 테마 파일을 읽는다
fn load_theme(name: &str) -> Option<Vec<Entry>> {
    let direct = PathBuf::from(name);
    let path = if direct.is_absolute() && direct.is_file() {
        Some(direct)
    } else {
        // 이름에 경로 구분자가 있으면 폴더를 빠져나갈 수 있으므로 받지 않는다
        if name.contains('/') || name.contains("..") {
            return None;
        }
        theme_dirs().into_iter().map(|d| d.join(name)).find(|p| p.is_file())
    };
    if let Some(path) = path {
        let mut w = Vec::new();
        let mut s = Vec::new();
        return Some(read_entries(&path, 0, &mut w, &mut s));
    }
    // 넣어 둔 테마: 이름이 같은 것, 없으면 띄어쓰기·대소문자·-·_를 무시하고(`Tokyo Night` = `TokyoNight`)
    let key = theme_key(name);
    let text = BUNDLED_THEMES
        .iter()
        .find(|(n, _)| *n == name)
        .or_else(|| BUNDLED_THEMES.iter().find(|(n, _)| theme_key(n) == key))
        .map(|(_, t)| *t)?;
    Some(parse_text(text, Path::new(name)))
}

fn theme_key(n: &str) -> String {
    n.chars()
        .filter(|c| c.is_alphanumeric())
        .flat_map(|c| c.to_lowercase())
        .collect()
}

/// 못 찾은 테마 이름과 비슷한 넣어 둔 테마(경고에 보인다)
pub fn similar_themes(name: &str) -> Vec<&'static str> {
    let key = theme_key(name);
    if key.len() < 3 {
        return Vec::new();
    }
    let head: String = key.chars().take(5).collect();
    BUNDLED_THEMES
        .iter()
        .map(|(n, _)| *n)
        .filter(|n| theme_key(n).contains(&head))
        .take(5)
        .collect()
}

/// 글로 된 설정을 줄로 나눈다(넣어 둔 테마처럼 파일이 아닌 것)
fn parse_text(text: &str, origin: &Path) -> Vec<Entry> {
    text.lines()
        .enumerate()
        .filter_map(|(i, l)| {
            let l = l.trim();
            if l.is_empty() || l.starts_with('#') {
                return None;
            }
            let (k, v) = l.split_once('=')?;
            Some(Entry {
                key: k.trim().into(),
                value: unquote(v.trim()).into(),
                file: origin.to_path_buf(),
                line: i + 1,
            })
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_match_ghostty() {
        let c = Config::default();
        assert_eq!(c.font_size, 13.0);
        assert_eq!(c.theme.background, Rgb::new(0x28, 0x2c, 0x34));
        assert_eq!(c.theme.foreground, Rgb::new(0xff, 0xff, 0xff));
        assert_eq!(c.theme.palette[1], Rgb::new(0xcc, 0x66, 0x66));
        assert!(!c.theme.bold_is_bright);
        assert_eq!(c.option_as_alt, OptionAsAlt::None);
    }

    #[test]
    fn reads_ghostty_syntax() {
        let c = Config::from_text(
            r##"
            # 주석
            font-family = "D2Coding"
            font-family = Apple SD Gothic Neo
            font-size = 15
            background = #101010
            foreground = eeeeee
            palette = 1=#ff0000
            cursor-style = bar
            window-padding-x = 4,8
            window-padding-y = 6
            macos-option-as-alt = left
            adjust-cell-height = 10%
            keybind = super+t=new_tab
            keybind = super+shift+comma=reload_config
            background-opacity = 0.9
            what-is-this = 1
            "##,
            true,
        );
        assert_eq!(c.font_family, vec!["D2Coding", "Apple SD Gothic Neo"]);
        assert_eq!(c.font_size, 15.0);
        assert_eq!(c.theme.background, Rgb::new(0x10, 0x10, 0x10));
        assert_eq!(c.theme.foreground, Rgb::new(0xee, 0xee, 0xee));
        assert_eq!(c.theme.palette[1], Rgb::new(255, 0, 0));
        assert_eq!(c.cursor_style, CursorStyle::Bar);
        assert_eq!(c.padding_x, (4.0, 8.0));
        assert_eq!(c.padding_y, (6.0, 6.0));
        assert_eq!(c.option_as_alt, OptionAsAlt::Left);
        assert_eq!(c.adjust_cell_height, Some(Adjust::Percent(10.0)));
        assert_eq!(c.keybinds.len(), 2);
        assert_eq!(
            c.warnings.len(),
            1,
            "background-opacity는 알려진 무시 키, what-is-this만 경고: {:?}",
            c.warnings
        );
        assert!(c.warnings[0].contains("what-is-this"));
    }

    #[test]
    fn bad_values_warn_and_keep_defaults() {
        let c = Config::from_text("font-size = huge\ncursor-style = triangle\npalette = x\n", true);
        assert_eq!(c.font_size, 13.0);
        assert_eq!(c.cursor_style, CursorStyle::Block);
        assert_eq!(c.warnings.len(), 3);
    }

    #[test]
    fn theme_light_dark_pick() {
        assert_eq!(
            pick_theme_name("light:GitHub Light Default,dark:GitHub Dark", true),
            "GitHub Dark"
        );
        assert_eq!(
            pick_theme_name("light:GitHub Light Default,dark:GitHub Dark", false),
            "GitHub Light Default"
        );
        assert_eq!(pick_theme_name("Dracula", true), "Dracula");
        assert!(
            load_theme("../../etc/passwd").is_none(),
            "경로를 빠져나가는 이름은 받지 않는다"
        );
    }

    #[test]
    fn theme_file_then_overrides() {
        let dir = std::env::temp_dir().join(format!("bt-theme-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let theme = dir.join("Mine");
        std::fs::write(
            &theme,
            "background = #000001\nforeground = #000002\npalette = 2=#000003\n",
        )
        .unwrap();
        let c = Config::from_text(&format!("theme = {}\nforeground = #abcdef\n", theme.display()), true);
        assert_eq!(c.theme.background, Rgb::new(0, 0, 1), "테마 바탕");
        assert_eq!(c.theme.foreground, Rgb::new(0xab, 0xcd, 0xef), "설정이 테마를 덮는다");
        assert_eq!(c.theme.palette[2], Rgb::new(0, 0, 3));
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn config_file_include_and_cycle_guard() {
        let dir = std::env::temp_dir().join(format!("bt-cfg-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(
            dir.join("a"),
            "font-size = 20\nconfig-file = b\nconfig-file = ?missing\n",
        )
        .unwrap();
        std::fs::write(dir.join("b"), "background = #111111\nconfig-file = a\n").unwrap();
        let mut w = Vec::new();
        let mut s = Vec::new();
        let entries = read_entries(&dir.join("a"), 0, &mut w, &mut s);
        assert!(entries.iter().any(|e| e.key == "background"));
        assert!(
            w.iter().any(|m| m.contains("너무 깊게")),
            "서로 부르는 파일은 멈춘다: {w:?}"
        );
        assert!(!w.iter().any(|m| m.contains("missing")), "?가 붙은 파일은 없어도 된다");
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn bundled_themes_work_without_ghostty() {
        assert!(BUNDLED_THEMES.len() > 800, "넣어 둔 테마 {}개", BUNDLED_THEMES.len());
        let c = Config::from_text("theme = Dracula\n", true);
        assert_eq!(c.theme.background, Rgb::new(0x28, 0x2a, 0x36));
        assert_eq!(c.theme.selection_foreground, Some(Rgb::new(255, 255, 255)));
        let c = Config::from_text("theme = catppuccin mocha\n", true);
        assert_eq!(
            c.theme.background,
            Rgb::new(0x1e, 0x1e, 0x2e),
            "대소문자를 무시하고 찾는다"
        );
    }
}
