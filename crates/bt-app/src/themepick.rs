//! `b-terminal +list-themes`의 대화형 화면(Ghostty 1.4.0 `src/cli/list_themes.zig`의 `Preview`)
//!
//! 표준 입출력이 모두 터미널이고 `--plain`이 아닐 때 목록 대신 이 화면을 띄운다.
//! 왼쪽에 테마 이름 목록, 오른쪽에 고른 테마의 색 미리보기(팔레트 16색, 예시 코드, 글 모양)를 보인다.
//!
//! 순수 부분과 터미널 부분을 나눈다.
//! - 순수: `parse_keys`(바이트 → 키), `Picker::update`(상태 + 키 → 상태), `Picker::render`(상태 → 바이트).
//!   터미널을 건드리지 않아 단위 시험으로 확인한다.
//! - 터미널: `run`이 원시 모드로 바꾸고 그리고 읽는다. 끝나면(정상·패닉·SIGTERM·SIGHUP) 원래대로 돌려 놓는다.
//!
//! 테마 이름·파일 글은 믿지 않는 입력이다. 사용자 테마 폴더의 파일 이름에 ESC가 있을 수 있어서,
//! 화면에 찍는 모든 글은 격자에 놓을 때 제어 문자를 지운다(`Grid::put`). 설정 파일은 고치지 않는다.
//! 'w'를 눌렀을 때만 자동 테마 파일(`auto/theme.ghostty`)을 쓴다(Ghostty와 같다).

use bt_core::theme::{Rgb, Theme};
use std::io::Write;
use std::path::{Path, PathBuf};

/// 목록 칸의 너비(Ghostty `theme_list` 너비)
pub const LIST_W: usize = 32;
/// PgUp·PgDn·Ctrl-D·Ctrl-U가 옮기는 칸 수
const PAGE: usize = 20;
/// 걸러낸 테마가 이만큼 이하이면 창을 맨 위로 되돌려 결과를 다 보인다
const SMALL_LIST_THRESHOLD: usize = 10;
/// 자동 테마 파일을 불러오는 설정 줄(설정 폴더 기준)
pub const AUTO_INCLUDE: &str = "config-file = ?auto/theme.ghostty";
/// 설정 폴더 아래 자동 테마 파일 경로
pub const AUTO_FILE: &str = "auto/theme.ghostty";

const LOREM: &str = include_str!("themepick_lorem.txt");

// ---------------------------------------------------------------- 믿지 않는 글 닦기

/// 화면에 찍기 전에 제어 문자(ESC·C1·DEL·줄바꿈 등)와 글자 방향을 바꾸는 문자를 지운다
pub fn sanitize(s: &str) -> String {
    s.chars().filter(|&c| !is_unsafe_char(c)).collect()
}

fn is_unsafe_char(c: char) -> bool {
    c.is_control() || matches!(c, '\u{202a}'..='\u{202e}' | '\u{2066}'..='\u{2069}' | '\u{2028}' | '\u{2029}')
}

/// 칸 너비(0, 1, 2). 한글·CJK·이모지는 2칸. 이 화면 안에서만 쓰는 근사표다
fn cell_width(c: char) -> usize {
    match c as u32 {
        0 => 0,
        0x300..=0x36f | 0x200b..=0x200f | 0xfe00..=0xfe0f | 0x20d0..=0x20ff => 0,
        0x1100..=0x115f
        | 0x2e80..=0xa4cf
        | 0xac00..=0xd7a3
        | 0xf900..=0xfaff
        | 0xfe30..=0xfe6f
        | 0xff00..=0xff60
        | 0xffe0..=0xffe6
        | 0x1f300..=0x1faff
        | 0x20000..=0x3fffd => 2,
        _ => 1,
    }
}

fn text_width(s: &str) -> usize {
    s.chars().filter(|&c| !is_unsafe_char(c)).map(cell_width).sum()
}

// ---------------------------------------------------------------- 테마 하나

/// 밝음/어둠 거르기(`--color`, 화면의 `f`)
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum ColorFilter {
    All,
    Dark,
    Light,
}

impl ColorFilter {
    fn next(self) -> Self {
        match self {
            Self::All => Self::Dark,
            Self::Dark => Self::Light,
            Self::Light => Self::All,
        }
    }

    fn admits(self, dark: bool) -> bool {
        match self {
            Self::All => true,
            Self::Dark => dark,
            Self::Light => !dark,
        }
    }
}

/// 미리보기에 쓰는 색(Ghostty `drawPreview`가 읽는 키만)
#[derive(Clone, Debug)]
pub struct ThemeColors {
    pub fg: Rgb,
    pub bg: Rgb,
    pub cursor_fg: Rgb,
    pub cursor_bg: Rgb,
    pub sel_fg: Rgb,
    pub sel_bg: Rgb,
    /// 0~255번 색
    pub palette: Vec<Rgb>,
}

impl ThemeColors {
    /// 테마 파일 글을 읽는다. 없는 키는 Ghostty 기본값을 쓴다. 모르는 줄·읽지 못한 색은 넘긴다
    pub fn parse(text: &str) -> Self {
        let base = Theme::ghostty_default();
        let mut palette: Vec<Rgb> = (0..=255u8).map(|i| base.indexed(i)).collect();
        let (mut fg, mut bg) = (base.foreground, base.background);
        let (mut cursor, mut cursor_text, mut sel_bg, mut sel_fg) = (None, None, None, None);
        for line in text.lines() {
            let Some((k, v)) = line.split_once('=') else { continue };
            let v = v.trim();
            match k.trim() {
                "background" => bg = Rgb::parse_color(v).unwrap_or(bg),
                "foreground" => fg = Rgb::parse_color(v).unwrap_or(fg),
                "cursor-color" => cursor = Rgb::parse_color(v).or(cursor),
                "cursor-text" => cursor_text = Rgb::parse_color(v).or(cursor_text),
                "selection-background" => sel_bg = Rgb::parse_color(v).or(sel_bg),
                "selection-foreground" => sel_fg = Rgb::parse_color(v).or(sel_fg),
                "palette" => {
                    if let Ok((i, c)) = bt_core::theme::parse_palette_entry(v) {
                        palette[i as usize] = c;
                    }
                }
                _ => {}
            }
        }
        Self {
            fg,
            bg,
            cursor_fg: cursor_text.unwrap_or(bg),
            cursor_bg: cursor.unwrap_or(fg),
            sel_fg: sel_fg.unwrap_or(bg),
            sel_bg: sel_bg.unwrap_or(fg),
            palette,
        }
    }

    /// 배경의 밝기(0.2126 R + 0.7152 G + 0.0722 B, 0~1). 0.5보다 작으면 어둡다(Ghostty `shouldIncludeTheme`)
    fn is_dark(&self) -> bool {
        let c = |v: u8| f64::from(v) / 255.0;
        0.2126 * c(self.bg.r) + 0.7152 * c(self.bg.g) + 0.0722 * c(self.bg.b) < 0.5
    }
}

/// 목록의 테마 하나
#[derive(Clone, Debug)]
pub struct Entry {
    /// 테마 이름(파일 이름 그대로. `theme = 이름`에 쓰는 값)
    pub name: String,
    /// 화면에 찍는 이름(제어 문자를 지운 것)
    pub label: String,
    /// 사용자 테마 폴더의 파일이면 그 경로, 넣어 둔 테마면 `None`
    pub path: Option<PathBuf>,
    /// 파일을 읽지 못했을 때 까닭
    pub error: Option<String>,
    pub colors: ThemeColors,
}

impl Entry {
    /// `text`가 테마 파일의 글(읽지 못했으면 `Err`에 까닭)
    pub fn new(name: &str, path: Option<PathBuf>, text: Result<&str, String>) -> Self {
        let (colors, error) = match text {
            Ok(t) => (ThemeColors::parse(t), None),
            Err(e) => (ThemeColors::parse(""), Some(e)),
        };
        Self {
            name: name.to_string(),
            label: sanitize(name),
            path,
            error,
            colors,
        }
    }

    fn path_label(&self) -> String {
        match &self.path {
            Some(p) => sanitize(&p.display().to_string()),
            None => "(앱에 넣어 둔 테마)".to_string(),
        }
    }
}

// ---------------------------------------------------------------- 키

/// 화면이 받는 키(원시 모드로 읽은 바이트를 `parse_keys`가 바꾼다)
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Key {
    Char(char),
    /// Ctrl과 함께 친 글자(`Ctrl('c')`). Ctrl-/ 는 `Ctrl('/')`
    Ctrl(char),
    Esc,
    Enter,
    Backspace,
    Up,
    Down,
    PageUp,
    PageDown,
    Home,
    End,
    F1,
    WheelUp,
    WheelDown,
    /// 왼쪽 단추를 뗐다(0부터 센 칸 줄·칸 열)
    Click {
        row: usize,
        col: usize,
    },
}

/// 한 번 읽은 바이트를 키로 바꾼다. 모르는 시퀀스·Alt 조합은 버린다. 혼자 온 ESC는 `Key::Esc`
pub fn parse_keys(buf: &[u8]) -> Vec<Key> {
    let mut keys = Vec::new();
    let mut i = 0;
    while i < buf.len() {
        let b = buf[i];
        match b {
            0x1b => {
                i += 1;
                match buf.get(i) {
                    None => keys.push(Key::Esc),
                    Some(b'[') => {
                        i += 1;
                        let start = i;
                        while i < buf.len() && !(0x40..=0x7e).contains(&buf[i]) {
                            i += 1;
                        }
                        if i >= buf.len() {
                            break; // 끊긴 시퀀스
                        }
                        if let Some(k) = csi_key(&buf[start..i], buf[i]) {
                            keys.push(k);
                        }
                        i += 1;
                    }
                    Some(b'O') => {
                        i += 1;
                        if let Some(&f) = buf.get(i) {
                            match f {
                                b'P' => keys.push(Key::F1),
                                b'A' => keys.push(Key::Up),
                                b'B' => keys.push(Key::Down),
                                b'H' => keys.push(Key::Home),
                                b'F' => keys.push(Key::End),
                                _ => {}
                            }
                            i += 1;
                        }
                    }
                    // Alt+글자: 버린다(ESC만 읽어 끝내 버리지 않게)
                    Some(_) => i += 1,
                }
            }
            0x0d | 0x0a => {
                keys.push(Key::Enter);
                i += 1;
            }
            0x7f => {
                keys.push(Key::Backspace);
                i += 1;
            }
            0x1f => {
                keys.push(Key::Ctrl('/'));
                i += 1;
            }
            0x01..=0x1a => {
                keys.push(Key::Ctrl((b'a' + b - 1) as char));
                i += 1;
            }
            0x00 | 0x1c..=0x1e => i += 1,
            0x20..=0x7e => {
                keys.push(Key::Char(b as char));
                i += 1;
            }
            _ => {
                // UTF-8 글자. 잘못된 바이트는 버린다
                let len = match b {
                    0xc2..=0xdf => 2,
                    0xe0..=0xef => 3,
                    0xf0..=0xf4 => 4,
                    _ => {
                        i += 1;
                        continue;
                    }
                };
                match buf.get(i..i + len).and_then(|s| std::str::from_utf8(s).ok()) {
                    Some(s) => {
                        if let Some(c) = s.chars().next().filter(|c| !is_unsafe_char(*c)) {
                            keys.push(Key::Char(c));
                        }
                        i += len;
                    }
                    None => i += 1,
                }
            }
        }
    }
    keys
}

fn csi_key(params: &[u8], fin: u8) -> Option<Key> {
    // SGR 마우스: ESC [ < 단추 ; 열 ; 줄 M(누름·돌림) / m(뗌)
    if let Some(rest) = params.strip_prefix(b"<") {
        let s = std::str::from_utf8(rest).ok()?;
        let mut it = s.split(';').map(|p| p.parse::<usize>().ok());
        let (btn, x, y) = (it.next()??, it.next()??, it.next()??);
        // 보조 키(4 시프트·8 Alt·16 Ctrl)와 움직임(32) 비트를 뺀다
        return match (btn & !(4 | 8 | 16 | 32), fin) {
            (64, b'M') => Some(Key::WheelUp),
            (65, b'M') => Some(Key::WheelDown),
            (0, b'm') => Some(Key::Click {
                row: y.saturating_sub(1),
                col: x.saturating_sub(1),
            }),
            _ => None,
        };
    }
    let first = params.split(|&b| b == b';').next().unwrap_or(&[]);
    match fin {
        b'A' => Some(Key::Up),
        b'B' => Some(Key::Down),
        b'H' => Some(Key::Home),
        b'F' => Some(Key::End),
        b'P' => Some(Key::F1),
        b'~' => match first {
            b"1" | b"7" => Some(Key::Home),
            b"4" | b"8" => Some(Key::End),
            b"5" => Some(Key::PageUp),
            b"6" => Some(Key::PageDown),
            b"11" => Some(Key::F1),
            _ => None,
        },
        _ => None,
    }
}

// ---------------------------------------------------------------- 상태와 키 처리

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Mode {
    Normal,
    Help,
    Search,
    Save,
}

/// 키 처리가 바깥에 맡기는 일
#[derive(Clone, PartialEq, Eq, Debug)]
pub enum Effect {
    /// 이 글을 시스템 클립보드에 복사한다(OSC 52)
    Copy(String),
    /// 자동 테마 파일에 `theme = 이름`을 쓴다
    WriteAuto(String),
}

pub struct Picker {
    pub entries: Vec<Entry>,
    /// 보이는 테마(`entries`의 번호). 이름순을 지킨다
    pub filtered: Vec<usize>,
    pub current: usize,
    pub window: usize,
    pub hex: bool,
    pub mode: Mode,
    pub query: String,
    pub filter: ColorFilter,
    /// 이 화면 자체(목록·상자)를 어두운 색으로 그리는가
    pub ui_dark: bool,
    /// (칸 수, 줄 수)
    pub size: (usize, usize),
    pub quit: bool,
    /// 'w'를 누른 결과를 저장 화면에 보인다(`true`면 성공)
    pub notice: Option<(bool, String)>,
    /// 마지막으로 저장 화면을 연 테마 이름(끝난 뒤 `theme = 이름`을 찍는다)
    pub confirmed: Option<String>,
    /// 'w'가 쓸 파일(저장 화면에 보인다)
    pub auto_path: PathBuf,
}

impl Picker {
    pub fn new(entries: Vec<Entry>, filter: ColorFilter, ui_dark: bool, auto_path: PathBuf) -> Self {
        let mut p = Self {
            entries,
            filtered: Vec::new(),
            current: 0,
            window: 0,
            hex: false,
            mode: Mode::Normal,
            query: String::new(),
            filter,
            ui_dark,
            size: (80, 24),
            quit: false,
            notice: None,
            confirmed: None,
            auto_path,
        };
        p.update_filtered();
        p
    }

    /// 지금 고른 테마
    fn selected(&self) -> Option<&Entry> {
        self.filtered.get(self.current).map(|&i| &self.entries[i])
    }

    pub fn set_size(&mut self, cols: usize, rows: usize) {
        self.size = (cols, rows);
        self.layout();
    }

    /// 고른 줄이 목록 안에 보이도록 창을 옮긴다(Ghostty `draw`의 앞머리)
    pub fn layout(&mut self) {
        let h = self.size.1.max(1);
        if self.filtered.is_empty() {
            self.current = 0;
            self.window = 0;
            return;
        }
        let end = self.window + h - 1;
        if self.current > end {
            self.window = self.current + 1 - h;
        }
        if self.current < self.window {
            self.window = self.current;
        }
        if self.window >= self.filtered.len() {
            self.window = self.filtered.len() - 1;
        }
    }

    /// 찾는 글과 밝음/어둠 거르기로 보일 테마를 다시 고른다. 고른 테마가 아직 있으면 그 자리를 지킨다
    fn update_filtered(&mut self) {
        let relative = self.current.saturating_sub(self.window);
        let selected = self.filtered.get(self.current).copied();
        let old = std::mem::take(&mut self.filtered);

        let query = self.query.to_lowercase();
        let tokens: Vec<&str> = query.split(' ').filter(|t| !t.is_empty()).collect();
        for (i, e) in self.entries.iter().enumerate() {
            if !self.filter.admits(e.colors.is_dark()) {
                continue;
            }
            let name = e.label.to_lowercase();
            if tokens.iter().all(|t| is_subsequence(t, &name)) {
                self.filtered.push(i);
            }
        }
        if old == self.filtered {
            return;
        }
        if self.filtered.is_empty() {
            self.current = 0;
            self.window = 0;
            return;
        }
        let at = selected.and_then(|s| self.filtered.iter().position(|&i| i == s));
        match at {
            Some(i) => {
                self.current = i;
                self.window = if self.filtered.len() <= SMALL_LIST_THRESHOLD {
                    0
                } else {
                    i.saturating_sub(relative)
                };
            }
            None => {
                self.current = 0;
                self.window = 0;
            }
        }
    }

    fn up(&mut self, n: usize) {
        if self.filtered.is_empty() {
            self.current = 0;
        } else {
            self.current = self.current.saturating_sub(n);
        }
    }

    fn down(&mut self, n: usize) {
        if self.filtered.is_empty() {
            self.current = 0;
        } else {
            self.current = (self.current + n).min(self.filtered.len() - 1);
        }
    }

    fn clear_query(&mut self) {
        self.query.clear();
        self.update_filtered();
    }

    /// 키 하나를 처리한다(Ghostty `Preview.update`와 같은 키). 바깥에 맡길 일이 있으면 돌려준다
    pub fn update(&mut self, key: Key) -> Option<Effect> {
        if key == Key::Ctrl('c') {
            self.quit = true;
        }
        match self.mode {
            Mode::Normal => self.update_normal(key),
            Mode::Help => {
                match key {
                    Key::Char('q') => self.quit = true,
                    Key::Char('?') | Key::Esc | Key::F1 | Key::Ctrl('h') => self.mode = Mode::Normal,
                    _ => {}
                }
                None
            }
            Mode::Search => {
                match key {
                    Key::Esc | Key::Enter => self.mode = Mode::Normal,
                    Key::Ctrl('x') | Key::Ctrl('/') | Key::Ctrl('u') => self.clear_query(),
                    Key::Backspace | Key::Ctrl('h') => {
                        self.query.pop();
                        self.update_filtered();
                    }
                    Key::Ctrl('w') => {
                        let t = self.query.trim_end().len();
                        let cut = self.query[..t].rfind(' ').map_or(0, |i| i + 1);
                        self.query.truncate(cut);
                        self.update_filtered();
                    }
                    Key::Char(c) if !is_unsafe_char(c) => {
                        self.query.push(c);
                        self.update_filtered();
                    }
                    _ => {}
                }
                None
            }
            Mode::Save => match key {
                Key::Char('q') => {
                    self.quit = true;
                    None
                }
                Key::Esc | Key::Enter => {
                    self.mode = Mode::Normal;
                    None
                }
                Key::Char('w') => self.selected().map(|e| Effect::WriteAuto(e.name.clone())),
                _ => None,
            },
        }
    }

    fn update_normal(&mut self, key: Key) -> Option<Effect> {
        let has = !self.filtered.is_empty();
        match key {
            Key::Char('q') | Key::Esc => self.quit = true,
            Key::Char('?') | Key::F1 | Key::Ctrl('h') => self.mode = Mode::Help,
            Key::Char('/') => self.mode = Mode::Search,
            Key::Enter if has => {
                self.notice = None;
                self.confirmed = self.selected().map(|e| e.label.clone());
                self.mode = Mode::Save;
            }
            Key::Ctrl('x') | Key::Ctrl('/') => self.clear_query(),
            Key::Home | Key::Char('g') => self.current = 0,
            Key::End | Key::Char('G') => self.current = self.filtered.len().saturating_sub(1),
            Key::Char('j' | '+') | Key::Down | Key::WheelDown => self.down(1),
            Key::PageDown | Key::Ctrl('d') => self.down(PAGE),
            Key::Char('k' | '-') | Key::Up | Key::WheelUp => self.up(1),
            Key::PageUp | Key::Ctrl('u') => self.up(PAGE),
            Key::Char('h' | 'x') => self.hex = true,
            Key::Char('d') => self.hex = false,
            Key::Char('c') if has => return self.selected().map(|e| Effect::Copy(e.label.clone())),
            Key::Char('C') if has => return self.selected().map(|e| Effect::Copy(e.path_label())),
            Key::Char('f') => {
                self.filter = self.filter.next();
                self.update_filtered();
            }
            Key::Click { row, col } if col < LIST_W => {
                let sel = self.window + row;
                if sel < self.filtered.len() {
                    self.current = sel;
                }
            }
            _ => {}
        }
        None
    }
}

/// `needle`의 글자가 `hay`에 차례로 나오는가(흩어져 있어도 됨)
fn is_subsequence(needle: &str, hay: &str) -> bool {
    let mut it = hay.chars();
    needle.chars().all(|c| it.any(|h| h == c))
}

// ---------------------------------------------------------------- 그리기

#[derive(Clone, Copy, PartialEq, Eq)]
enum Ul {
    None,
    Single,
    Double,
    Curly,
    Dotted,
    Dashed,
}

#[derive(Clone, Copy, PartialEq, Eq)]
struct Style {
    fg: Rgb,
    bg: Rgb,
    bold: bool,
    italic: bool,
    ul: Ul,
}

impl Style {
    fn new(fg: Rgb, bg: Rgb) -> Self {
        Self {
            fg,
            bg,
            bold: false,
            italic: false,
            ul: Ul::None,
        }
    }
}

#[derive(Clone, Copy)]
struct Cell {
    ch: char,
    style: Style,
    /// 앞 칸의 넓은 글자가 이 칸까지 차지한다
    cont: bool,
}

#[derive(Clone, Copy)]
struct Rect {
    x: usize,
    y: usize,
    w: usize,
    h: usize,
}

struct Grid {
    w: usize,
    h: usize,
    cells: Vec<Cell>,
}

impl Grid {
    fn new(w: usize, h: usize, style: Style) -> Self {
        Self {
            w,
            h,
            cells: vec![
                Cell {
                    ch: ' ',
                    style,
                    cont: false
                };
                w * h
            ],
        }
    }

    fn full(&self) -> Rect {
        Rect {
            x: 0,
            y: 0,
            w: self.w,
            h: self.h,
        }
    }

    /// `r`을 격자 안으로 자른다
    fn clip(&self, r: Rect) -> Rect {
        let x = r.x.min(self.w);
        let y = r.y.min(self.h);
        Rect {
            x,
            y,
            w: r.w.min(self.w - x),
            h: r.h.min(self.h - y),
        }
    }

    fn fill(&mut self, r: Rect, style: Style) {
        let r = self.clip(r);
        for y in r.y..r.y + r.h {
            for x in r.x..r.x + r.w {
                self.cells[y * self.w + x] = Cell {
                    ch: ' ',
                    style,
                    cont: false,
                };
            }
        }
    }

    fn set(&mut self, x: usize, y: usize, cell: Cell) {
        let i = y * self.w + x;
        // 넓은 글자의 한쪽만 덮어쓰면 남은 반쪽을 빈칸으로 바꾼다
        if self.cells[i].cont && x > 0 {
            self.cells[i - 1].ch = ' ';
        }
        if !self.cells[i].cont && x + 1 < self.w && self.cells[i + 1].cont {
            self.cells[i + 1].cont = false;
            self.cells[i + 1].ch = ' ';
        }
        self.cells[i] = cell;
    }

    /// `r` 안의 (row, col)에서 글을 놓는다. 제어 문자는 지우고 `r` 밖은 자른다. 돌려주는 값은 다음 열
    fn put(&mut self, r: Rect, row: usize, col: usize, text: &str, style: Style) -> usize {
        let r = self.clip(r);
        let mut col = col;
        if row >= r.h {
            return col;
        }
        for ch in text.chars() {
            if is_unsafe_char(ch) {
                continue;
            }
            let w = cell_width(ch);
            if w == 0 {
                continue;
            }
            if col + w > r.w {
                break;
            }
            let (x, y) = (r.x + col, r.y + row);
            self.set(x, y, Cell { ch, style, cont: false });
            if w == 2 {
                self.set(
                    x + 1,
                    y,
                    Cell {
                        ch: ' ',
                        style,
                        cont: true,
                    },
                );
            }
            col += w;
        }
        col
    }

    /// 둥근 테두리 상자를 그리고 안쪽 영역을 돌려준다
    fn boxed(&mut self, r: Rect, style: Style) -> Rect {
        let r = self.clip(r);
        self.fill(r, style);
        if r.w < 2 || r.h < 2 {
            return Rect {
                x: r.x,
                y: r.y,
                w: 0,
                h: 0,
            };
        }
        let line = |n: usize| "─".repeat(n);
        self.put(r, 0, 0, &format!("╭{}╮", line(r.w - 2)), style);
        for row in 1..r.h - 1 {
            self.put(r, row, 0, "│", style);
            self.put(r, row, r.w - 1, "│", style);
        }
        self.put(r, r.h - 1, 0, &format!("╰{}╯", line(r.w - 2)), style);
        Rect {
            x: r.x + 1,
            y: r.y + 1,
            w: r.w - 2,
            h: r.h - 2,
        }
    }

    /// 격자를 이스케이프 시퀀스로 바꾼다. 줄마다 커서를 옮겨 놓으니 어떤 상태에서도 같은 그림이 된다
    fn serialize(&self) -> Vec<u8> {
        let mut out = Vec::with_capacity(self.w * self.h * 4);
        let mut cur: Option<Style> = None;
        for y in 0..self.h {
            let _ = write!(out, "\x1b[{};1H", y + 1);
            for x in 0..self.w {
                let c = self.cells[y * self.w + x];
                if c.cont {
                    continue;
                }
                if cur != Some(c.style) {
                    let s = c.style;
                    let _ = write!(
                        out,
                        "\x1b[0;38;2;{};{};{};48;2;{};{};{}",
                        s.fg.r, s.fg.g, s.fg.b, s.bg.r, s.bg.g, s.bg.b
                    );
                    if s.bold {
                        out.extend_from_slice(b";1");
                    }
                    if s.italic {
                        out.extend_from_slice(b";3");
                    }
                    match s.ul {
                        Ul::None => {}
                        Ul::Single => out.extend_from_slice(b";4"),
                        Ul::Double => out.extend_from_slice(b";4:2"),
                        Ul::Curly => out.extend_from_slice(b";4:3"),
                        Ul::Dotted => out.extend_from_slice(b";4:4"),
                        Ul::Dashed => out.extend_from_slice(b";4:5"),
                    }
                    out.push(b'm');
                    cur = Some(s);
                }
                let mut b = [0u8; 4];
                out.extend_from_slice(c.ch.encode_utf8(&mut b).as_bytes());
            }
        }
        out.extend_from_slice(b"\x1b[0m");
        out
    }
}

/// 예시 코드의 글 모양(Ghostty `drawPreview`)
#[derive(Clone, Copy)]
enum S {
    Std,
    Bold,
    /// 팔레트 번호의 글자색
    C(u8),
    /// 6번 색에 밑줄
    C6Ul,
    /// 고른 칸 색
    Sel,
    /// 커서 색
    Cur,
}

const RULE: u8 = 238;

/// 예시 코드 줄(4~24줄이 코드 1~21줄). 머리 줄(0~3)·꼬리 줄(25~27)은 `draw_preview`가 만든다
const CODE: &[&[(S, &str)]] = &[
    &[
        (S::C(RULE), "   1   │ "),
        (S::C(5), "const"),
        (S::Std, " std "),
        (S::C(5), "= @import"),
        (S::Std, "("),
        (S::C(10), "\"std\""),
        (S::Std, ");"),
    ],
    &[(S::C(RULE), "   2   │")],
    &[
        (S::C(RULE), "   3   │ "),
        (S::C(5), "pub "),
        (S::C(12), "fn "),
        (S::C(2), "main"),
        (S::Std, "(init: "),
        (S::C(12), "std.process.Init"),
        (S::Std, ") "),
        (S::C(5), "!"),
        (S::C(12), "void"),
        (S::Std, " {"),
    ],
    &[
        (S::C(RULE), "   4   │     "),
        (S::C(5), "var "),
        (S::Std, "buf:"),
        (S::C(12), " [1024]u8"),
        (S::C(5), " ="),
        (S::Std, " undefined;"),
    ],
    &[
        (S::C(RULE), "   5   │     "),
        (S::C(5), "var "),
        (S::Std, "stdout "),
        (S::C(5), "="),
        (S::Std, " std.Io.File.stdout().writer(init.io, &buf);"),
    ],
    &[
        (S::C(RULE), "   6   │     "),
        (S::C(5), "const "),
        (S::Std, "w "),
        (S::C(5), "="),
        (S::Std, " &stdout.interface;"),
    ],
    &[(S::C(RULE), "   7   │")],
    &[
        (S::C(RULE), "   8   │     "),
        (S::C(5), "var "),
        (S::Std, "i:"),
        (S::C(12), " usize"),
        (S::C(5), " ="),
        (S::C(4), " 1"),
        (S::Std, ";"),
    ],
    &[
        (S::C(RULE), "   9   │     "),
        (S::C(5), "while "),
        (S::Std, "(i "),
        (S::C(5), "<= "),
        (S::C(4), "16"),
        (S::Std, ") : (i "),
        (S::C(5), "+= "),
        (S::C(4), "1"),
        (S::Std, ") {"),
    ],
    &[
        (S::C(RULE), "  10   │         "),
        (S::C(5), "if "),
        (S::Std, "(i "),
        (S::C(5), "% "),
        (S::C(4), "15 "),
        (S::C(5), "== "),
        (S::C(4), "0"),
        (S::Std, ") {"),
    ],
    &[
        (S::C(RULE), "  11   │             "),
        (S::C(5), "try "),
        (S::Std, "w.writeAll("),
        (S::C(10), "\"ZiggZagg"),
        (S::C(12), "\\n"),
        (S::C(10), "\""),
        (S::Std, ");"),
    ],
    &[
        (S::C(RULE), "  12   │         "),
        (S::Std, "} "),
        (S::C(5), "else if "),
        (S::Std, "(i "),
        (S::C(5), "% "),
        (S::C(4), "3 "),
        (S::C(5), "== "),
        (S::C(4), "0"),
        (S::Std, ") {"),
    ],
    &[
        (S::C(RULE), "  13   │             "),
        (S::C(5), "try "),
        (S::Std, "w.writeAll("),
        (S::C(10), "\"Zigg"),
        (S::C(12), "\\n"),
        (S::C(10), "\""),
        (S::Std, ");"),
    ],
    &[
        (S::C(RULE), "  14   │         "),
        (S::Std, "} "),
        (S::C(5), "else if "),
        (S::Std, "(i "),
        (S::C(5), "% "),
        (S::C(4), "5 "),
        (S::C(5), "== "),
        (S::C(4), "0"),
        (S::Std, ") {"),
    ],
    &[
        (S::C(RULE), "  15   │             "),
        (S::C(5), "try "),
        (S::Std, "w.writeAll("),
        (S::C(10), "\"Zagg"),
        (S::C(12), "\\n"),
        (S::C(10), "\""),
        (S::Std, ");"),
    ],
    &[
        (S::C(RULE), "  16   │         "),
        (S::Std, "} "),
        (S::C(5), "else "),
        (S::Std, "{"),
    ],
    &[
        (S::C(RULE), "  17   │             "),
        (S::C(5), "try "),
        (S::Sel, "w.print(\"{d}\\n\", .{i})"),
        (S::Cur, ";"),
    ],
    &[(S::C(RULE), "  18   │         "), (S::Std, "}")],
    &[(S::C(RULE), "  19   │     "), (S::Std, "}")],
    &[(S::C(RULE), "  20   │     "), (S::C(5), "try "), (S::Std, "w.flush();")],
    &[(S::C(RULE), "  21   │ "), (S::Std, "}")],
];

impl Picker {
    fn ui(&self) -> Style {
        if self.ui_dark {
            Style::new(Rgb::new(255, 255, 255), Rgb::new(0, 0, 0))
        } else {
            Style::new(Rgb::new(0, 0, 0), Rgb::new(255, 255, 255))
        }
    }

    fn ui_selected(&self) -> Style {
        let bg = if self.ui_dark {
            Rgb::new(0x33, 0x33, 0x33)
        } else {
            Rgb::new(0xaa, 0xaa, 0xaa)
        };
        Style::new(Rgb::new(0, 0xaa, 0), bg)
    }

    fn ui_err(&self) -> Style {
        Style {
            fg: Rgb::new(0xff, 0, 0),
            ..self.ui()
        }
    }

    /// 화면 전체를 그린다(상태 → 바이트). 이 바이트를 터미널에 쓰면 한 장면이 된다
    pub fn render(&self) -> Vec<u8> {
        let (w, h) = self.size;
        if w == 0 || h == 0 {
            return Vec::new();
        }
        let mut g = Grid::new(w, h, self.ui());
        self.draw_list(&mut g);
        self.draw_preview(&mut g);
        match self.mode {
            Mode::Normal => {}
            Mode::Help => self.draw_help(&mut g),
            Mode::Search => self.draw_search(&mut g),
            Mode::Save => self.draw_save(&mut g),
        }
        g.serialize()
    }

    fn draw_list(&self, g: &mut Grid) {
        let area = g.clip(Rect {
            x: 0,
            y: 0,
            w: LIST_W,
            h: g.h,
        });
        g.fill(area, self.ui());
        for row in 0..area.h {
            let index = self.window + row;
            let Some(&ei) = self.filtered.get(index) else { break };
            let label = &self.entries[ei].label;
            if index == self.current {
                let sel = self.ui_selected();
                g.put(area, row, 0, "❯ ", sel);
                // 이름 뒤를 채우고 오른쪽 끝에 ❮를 놓는다. 이름은 ❮ 앞에서 자른다
                let name_area = Rect {
                    w: LIST_W.saturating_sub(2),
                    ..area
                };
                let end = g.put(name_area, row, 2, label, sel);
                for c in end..LIST_W.saturating_sub(2) {
                    g.put(area, row, c, " ", sel);
                }
                g.put(area, row, LIST_W - 2, " ❮", sel);
            } else {
                g.put(area, row, 2, label, self.ui());
            }
        }
    }

    fn draw_preview(&self, g: &mut Grid) {
        let x0 = LIST_W.min(g.w);
        let pw = g.w - x0;
        if pw == 0 {
            return;
        }
        let Some(e) = self.selected() else {
            let full = g.full();
            g.fill(full, self.ui());
            g.put(
                full,
                (g.h / 2).saturating_sub(1),
                (g.w / 2).saturating_sub(7),
                "테마가 없습니다",
                self.ui(),
            );
            return;
        };
        let c = &e.colors;
        let std = Style::new(c.fg, c.bg);
        let bold = Style { bold: true, ..std };
        let col = |i: u8| c.palette[i as usize];
        let at = |y: usize, h: usize| Rect { x: x0, y, w: pw, h };
        let center = |text: &str| (pw / 2).saturating_sub(text_width(text) / 2);

        if let Some(err) = &e.error {
            let area = at(0, g.h);
            g.fill(area, self.ui());
            let mid = g.h / 2;
            let lines = [
                format!("{}을(를) 열 수 없습니다:", e.label),
                e.path_label(),
                sanitize(err),
            ];
            for (i, t) in lines.iter().enumerate() {
                g.put(area, (mid + i).saturating_sub(1), center(t), t, self.ui_err());
            }
            return;
        }

        // 머리: 이름(굵은 기울임)과 경로, 4줄
        let head = at(0, 4);
        g.fill(head, std);
        let name_style = Style {
            bold: true,
            italic: true,
            ..std
        };
        g.put(head, 1, center(&e.label), &e.label, name_style);
        let path = e.path_label();
        g.put(head, 2, center(&path), &path, std);

        // 팔레트 16색, 6줄
        let pal = at(4, 6);
        g.fill(pal, std);
        for i in 0..16usize {
            let (r, cc) = (i / 8, i % 8);
            let label = if self.hex {
                format!(" {i:02x}")
            } else {
                format!("{i:3}")
            };
            g.put(pal, 3 * r, cc * 8, &label, std);
            let swatch = Style::new(col(i as u8), c.bg);
            g.put(pal, 3 * r, 4 + cc * 8, "████", swatch);
            g.put(pal, 3 * r + 1, 4 + cc * 8, "████", swatch);
        }

        // 예시 코드, 28줄
        let code = at(10, 28);
        g.fill(code, std);
        let st = |s: S| -> Style {
            match s {
                S::Std => std,
                S::Bold => bold,
                S::C(i) => Style::new(col(i), c.bg),
                S::C6Ul => Style {
                    ul: Ul::Single,
                    ..Style::new(col(6), c.bg)
                },
                S::Sel => Style::new(c.sel_fg, c.sel_bg),
                S::Cur => Style::new(c.cursor_fg, c.cursor_bg),
            }
        };
        let rule = |g: &mut Grid, row: usize, joint: &str| {
            g.put(code, row, 2, joint, st(S::C(RULE)));
            for x in 10..pw {
                g.put(code, row, x, "─", st(S::C(RULE)));
            }
        };
        let line = |g: &mut Grid, row: usize, segs: &[(S, &str)]| {
            let mut x = 2;
            for (s, t) in segs {
                x = g.put(code, row, x, t, st(*s));
            }
        };
        line(
            g,
            0,
            &[
                (S::C(2), "→"),
                (S::Std, " "),
                (S::C(4), "bat"),
                (S::Std, " "),
                (S::C6Ul, "ziggzagg.zig"),
            ],
        );
        rule(g, 1, "───────┬");
        line(
            g,
            2,
            &[(S::C(RULE), "       │ "), (S::Std, "File: "), (S::Bold, "ziggzagg.zig")],
        );
        rule(g, 3, "───────┼");
        for (i, segs) in CODE.iter().enumerate() {
            line(g, 4 + i, segs);
        }
        rule(g, 25, "───────┴");
        line(
            g,
            26,
            &[
                (S::C(6), "b-terminal "),
                (S::Std, "on "),
                (S::C(4), "\u{f418} main "),
                (S::C(1), "[+] "),
                (S::Std, "via "),
                (S::C(3), "\u{e6a9} v0.16.0 "),
                (S::Std, "via "),
                (S::C(4), "\u{f313} impure (b-terminal-env)"),
            ],
        );
        line(
            g,
            27,
            &[(S::C(4), "✦ "), (S::Std, "at "), (S::C(3), "10:36:15 "), (S::C(2), "→")],
        );

        // 남는 자리: 글 모양을 보이는 문단
        if 38 < g.h {
            let rest = at(38, g.h - 38);
            g.fill(rest, std);
            let (mut row, mut col_x) = (1usize, 2usize);
            let mut words = LOREM.split([' ', '\n']).cycle();
            while row < rest.h {
                let word = words.next().unwrap_or("");
                let wl = text_width(word);
                if col_x + wl > rest.w {
                    row += 1;
                    col_x = 2;
                }
                let style = match word {
                    "ipsum" => Style::new(col(2), c.bg),
                    "consectetur" => bold,
                    "reprehenderit" => Style { italic: true, ..std },
                    "Praesent" => Style {
                        bold: true,
                        italic: true,
                        ..std
                    },
                    "auctor" => Style { ul: Ul::Single, ..std },
                    "dui" => Style { ul: Ul::Double, ..std },
                    "erat" => Style { ul: Ul::Dashed, ..std },
                    "enim" => Style { ul: Ul::Dotted, ..std },
                    "odio" => Style { ul: Ul::Curly, ..std },
                    _ => std,
                };
                g.put(rest, row, col_x, word, style);
                col_x += wl + 1;
            }
        }
    }

    fn draw_help(&self, g: &mut Grid) {
        const KEYS: &[(&str, &str)] = &[
            ("^C, q, ESC", "끝냅니다."),
            ("F1, ?, ^H", "도움말을 켜고 끕니다."),
            ("f", "밝음/어둠 거르기를 돌아가며 바꿉니다."),
            ("k, ↑", "한 칸 위로."),
            ("휠 위", "한 칸 위로."),
            ("PgUp, ^U", "20칸 위로."),
            ("j, ↓", "한 칸 아래로."),
            ("휠 아래", "한 칸 아래로."),
            ("PgDown, ^D", "20칸 아래로."),
            ("h, x", "팔레트 번호를 16진수로 봅니다."),
            ("d", "팔레트 번호를 10진수로 봅니다."),
            ("c", "테마 이름을 클립보드에 복사합니다."),
            ("C", "테마 파일 경로를 클립보드에 복사합니다."),
            ("Home, g", "목록 맨 위로."),
            ("End, G", "목록 맨 아래로."),
            ("/", "찾기를 시작합니다."),
            ("^X, ^/", "찾는 글을 지웁니다."),
            ("⏎", "테마를 고릅니다(적을 줄을 보임). 찾기 창은 닫습니다."),
            ("w", "고른 테마를 자동 테마 파일에 씁니다."),
        ];
        let (w, h) = (60, 22);
        let r = Rect {
            x: (g.w / 2).saturating_sub(w / 2),
            y: (g.h / 2).saturating_sub(h / 2),
            w,
            h,
        };
        let inner = g.boxed(r, self.ui());
        // 안쪽 영역 기준 줄 i+1, 열 2·15·17(Ghostty와 같은 자리)
        for (i, (k, t)) in KEYS.iter().enumerate() {
            g.put(inner, i + 1, 2, k, self.ui());
            g.put(inner, i + 1, 15, "—", self.ui());
            g.put(inner, i + 1, 17, t, self.ui());
        }
    }

    fn draw_search(&self, g: &mut Grid) {
        if g.w < 42 || g.h < 5 {
            return;
        }
        let r = Rect {
            x: 20,
            y: g.h - 5,
            w: g.w - 40,
            h: 3,
        };
        let inner = g.boxed(r, self.ui());
        let end = g.put(inner, 0, 0, &self.query, self.ui());
        g.put(inner, 0, end, "▏", self.ui());
    }

    fn draw_save(&self, g: &mut Grid) {
        let Some(e) = self.selected() else { return };
        let (w, h) = (92, 17);
        let r = Rect {
            x: (g.w / 2).saturating_sub(w / 2),
            y: (g.h / 2).saturating_sub(h / 2),
            w,
            h,
        };
        let inner = g.boxed(r, self.ui());
        let lines = [
            "이 테마를 쓰려면 설정 파일에 다음 줄을 더하세요:".to_string(),
            String::new(),
            format!("theme = {}", e.label),
            String::new(),
            "설정 파일을 저장하고 다시 읽으면 새 테마가 적용됩니다.".to_string(),
            String::new(),
            "아니면 'w'를 눌러 자동 테마 파일을 씁니다(설정 파일은 그대로입니다).".to_string(),
            "그다음 설정 파일에 다음 줄을 더하고 다시 읽으세요:".to_string(),
            String::new(),
            AUTO_INCLUDE.to_string(),
            String::new(),
            format!("쓸 파일: {}", sanitize(&self.auto_path.display().to_string())),
        ];
        for (i, t) in lines.iter().enumerate() {
            g.put(inner, i + 1, 2, t, self.ui());
        }
        if let Some((ok, msg)) = &self.notice {
            let style = if *ok { self.ui_selected() } else { self.ui_err() };
            g.put(
                inner,
                lines.len() + 2,
                2,
                msg,
                Style {
                    bg: self.ui().bg,
                    ..style
                },
            );
        }
    }
}

// ---------------------------------------------------------------- 자동 테마 파일

/// 자동 테마 파일에 `theme = 이름`을 쓴다. 이름에 제어 문자가 있으면 줄을 끼워 넣을 수 있으므로 쓰지 않는다
pub fn write_auto_theme(path: &Path, name: &str) -> std::io::Result<()> {
    if name.is_empty() || name.chars().any(is_unsafe_char) {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidInput,
            "테마 이름에 제어 문자가 있어 쓰지 않습니다",
        ));
    }
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir)?;
    }
    std::fs::write(path, format!("theme = {name}\n"))
}

/// 'w'를 처리한다. 결과 글은 저장 화면에 보인다
fn apply_write(p: &mut Picker, name: &str) {
    p.notice = Some(match write_auto_theme(&p.auto_path, name) {
        Ok(()) => (true, format!("썼습니다: theme = {}", sanitize(name))),
        Err(e) => (false, format!("쓰지 못했습니다: {}", sanitize(&e.to_string()))),
    });
}

/// 복사할 글을 OSC 52로 감싼다
fn osc52(text: &str) -> Vec<u8> {
    format!("\x1b]52;c;{}\x07", bt_term::kitty_clip::b64_encode(text.as_bytes())).into_bytes()
}

// ---------------------------------------------------------------- 터미널

/// 대화형 화면이 끝났을 때 돌려주는 값
pub struct Outcome {
    /// 종료 코드
    pub code: i32,
    /// 마지막으로 저장 화면을 연 테마 이름(제어 문자를 지운 것)
    pub picked: Option<String>,
}

/// 화면을 띄우는 함수의 꼴(시험에서 바꿔 끼운다)
pub type PickFn = fn(Vec<Entry>, ColorFilter, bool, PathBuf) -> std::io::Result<Outcome>;

mod tty {
    //! 원시 모드와 복구. `unsafe` 없이 rustix의 안전한 termios 호출만 쓴다
    use rustix::termios::{self, OptionalActions, SpecialCodeIndex, Termios};
    use std::io::Write;
    use std::sync::Mutex;

    static SAVED: Mutex<Option<Termios>> = Mutex::new(None);

    /// 화면을 닫는 시퀀스: 마우스 끔, 커서 보임, 원래 화면으로
    const LEAVE: &[u8] = b"\x1b[0m\x1b[?1006l\x1b[?1000l\x1b[?25h\x1b[?1049l";
    const ENTER: &[u8] = b"\x1b[?1049h\x1b[?25l\x1b[?1000h\x1b[?1006h";

    pub struct Raw;

    impl Raw {
        /// 원시 모드로 바꾸고 대체 화면을 연다. 읽기는 0.5초까지 기다렸다 돌아온다(크기 변화를 보려고)
        pub fn enter() -> std::io::Result<Raw> {
            let stdin = rustix::stdio::stdin();
            let orig = termios::tcgetattr(stdin)?;
            let mut raw = orig.clone();
            raw.make_raw();
            raw.special_codes[SpecialCodeIndex::VMIN] = 0;
            raw.special_codes[SpecialCodeIndex::VTIME] = 5;
            *SAVED.lock().unwrap_or_else(|e| e.into_inner()) = Some(orig);
            termios::tcsetattr(stdin, OptionalActions::Flush, &raw)?;
            let mut out = std::io::stdout();
            out.write_all(ENTER)?;
            out.flush()?;
            Ok(Raw)
        }
    }

    impl Drop for Raw {
        fn drop(&mut self) {
            restore();
        }
    }

    /// 터미널을 원래대로 돌린다. 여러 번 불러도 된다(패닉 훅·Drop·종료 신호가 겹쳐도)
    pub fn restore() {
        let saved = SAVED.lock().unwrap_or_else(|e| e.into_inner()).take();
        if let Some(orig) = saved {
            let mut out = std::io::stdout();
            let _ = out.write_all(LEAVE);
            let _ = out.flush();
            let _ = termios::tcsetattr(rustix::stdio::stdin(), OptionalActions::Flush, &orig);
        }
    }

    /// (칸 수, 줄 수). 얻지 못하면 80x24
    pub fn size() -> (usize, usize) {
        match termios::tcgetwinsize(rustix::stdio::stdout()) {
            Ok(w) if w.ws_col > 0 && w.ws_row > 0 => (usize::from(w.ws_col), usize::from(w.ws_row)),
            _ => (80, 24),
        }
    }

    pub fn is_tty() -> bool {
        termios::isatty(rustix::stdio::stdin()) && termios::isatty(rustix::stdio::stdout())
    }
}

/// 두 입출력이 모두 터미널인가(대화형으로 갈지 정한다)
pub fn is_interactive() -> bool {
    tty::is_tty() && std::env::var("TERM").map(|t| t != "dumb").unwrap_or(true)
}

/// 화면을 띄우고 닫힐 때까지 돈다. 패닉·Ctrl-C·SIGTERM·SIGHUP에도 터미널을 되돌려 놓는다
pub fn run(entries: Vec<Entry>, filter: ColorFilter, ui_dark: bool, auto_path: PathBuf) -> std::io::Result<Outcome> {
    use std::sync::Arc;
    use std::sync::atomic::{AtomicBool, Ordering};

    let term = Arc::new(AtomicBool::new(false));
    for sig in [signal_hook::consts::SIGTERM, signal_hook::consts::SIGHUP] {
        signal_hook::flag::register(sig, Arc::clone(&term))?;
    }
    // 패닉이 나면 메시지를 찍기 전에 터미널부터 되돌린다
    let prev = Arc::new(std::panic::take_hook());
    {
        let prev = Arc::clone(&prev);
        std::panic::set_hook(Box::new(move |info| {
            tty::restore();
            prev(info);
        }));
    }
    let result = (|| {
        let _raw = tty::Raw::enter()?;
        let mut picker = Picker::new(entries, filter, ui_dark, auto_path);
        let (c, r) = tty::size();
        picker.set_size(c, r);
        let mut out = std::io::stdout();
        out.write_all(&picker.render())?;
        out.flush()?;
        let mut buf = [0u8; 256];
        let mut code = 0;
        while !picker.quit {
            if term.load(Ordering::Relaxed) {
                code = 128 + signal_hook::consts::SIGTERM;
                break;
            }
            let n = match rustix::io::read(rustix::stdio::stdin(), &mut buf) {
                Ok(n) => n,
                Err(rustix::io::Errno::INTR) => 0,
                Err(e) => return Err(e.into()),
            };
            let before = picker.size;
            let (c, r) = tty::size();
            let resized = (c, r) != before;
            if n == 0 && !resized {
                continue;
            }
            picker.size = (c, r);
            for key in parse_keys(&buf[..n]) {
                match picker.update(key) {
                    Some(Effect::Copy(text)) => out.write_all(&osc52(&text))?,
                    Some(Effect::WriteAuto(name)) => apply_write(&mut picker, &name),
                    None => {}
                }
                if picker.quit {
                    break;
                }
            }
            picker.layout();
            out.write_all(&picker.render())?;
            out.flush()?;
        }
        Ok(Outcome {
            code,
            picked: picker.confirmed.clone(),
        })
    })();
    // 화면을 닫은 뒤에 되돌린다(위 `_raw`가 이미 떨어졌다)
    let _ = std::panic::take_hook();
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entry(name: &str, bg: &str) -> Entry {
        Entry::new(
            name,
            None,
            Ok(&format!(
                "background = {bg}\nforeground = #eeeeee\npalette = 1=#ff0000\npalette = 2=#00ff00\n"
            )),
        )
    }

    fn picker(n: usize) -> Picker {
        let entries = (0..n)
            .map(|i| entry(&format!("Theme {i:03}"), if i % 2 == 0 { "#101010" } else { "#f0f0f0" }))
            .collect();
        let mut p = Picker::new(entries, ColorFilter::All, true, PathBuf::from("/nonexistent/auto"));
        p.set_size(100, 40);
        p
    }

    fn names(p: &Picker) -> Vec<&str> {
        p.filtered.iter().map(|&i| p.entries[i].name.as_str()).collect()
    }

    fn text(bytes: &[u8]) -> String {
        String::from_utf8_lossy(bytes).into_owned()
    }

    #[test]
    fn keys_from_bytes() {
        assert_eq!(
            parse_keys(b"jk\x1b[A\x1b[B\x1b[5~\x1b[6~\x1b[H\x1b[F\x1b[1~\x1b[4~\x1bOP\x1b[11~"),
            vec![
                Key::Char('j'),
                Key::Char('k'),
                Key::Up,
                Key::Down,
                Key::PageUp,
                Key::PageDown,
                Key::Home,
                Key::End,
                Key::Home,
                Key::End,
                Key::F1,
                Key::F1
            ]
        );
        assert_eq!(parse_keys(b"\x1b"), vec![Key::Esc], "혼자 온 ESC는 Esc");
        assert_eq!(
            parse_keys(b"\x03\x04\x15\x08\x18\x1f"),
            vec![
                Key::Ctrl('c'),
                Key::Ctrl('d'),
                Key::Ctrl('u'),
                Key::Ctrl('h'),
                Key::Ctrl('x'),
                Key::Ctrl('/')
            ]
        );
        assert_eq!(parse_keys(b"\r\x7f"), vec![Key::Enter, Key::Backspace]);
        assert_eq!(parse_keys("가a".as_bytes()), vec![Key::Char('가'), Key::Char('a')]);
        // Alt+글자는 버린다(Esc로 읽어 끝내지 않는다), 끊긴 시퀀스·잘못된 UTF-8도 죽지 않는다
        assert_eq!(parse_keys(b"\x1bq"), vec![]);
        assert_eq!(parse_keys(b"\x1b[1;"), vec![]);
        assert_eq!(parse_keys(&[0xff, 0xe0, 0x80, b'a']), vec![Key::Char('a')]);
        // 마우스(SGR): 휠, 왼쪽 단추를 뗌
        assert_eq!(
            parse_keys(b"\x1b[<64;5;5M\x1b[<65;5;5M\x1b[<0;3;7M\x1b[<0;3;7m"),
            vec![Key::WheelUp, Key::WheelDown, Key::Click { row: 6, col: 2 }]
        );
    }

    #[test]
    fn move_keys_like_ghostty() {
        let mut p = picker(100);
        for k in [Key::Char('j'), Key::Char('+'), Key::Down] {
            p.update(k);
        }
        assert_eq!(p.current, 3);
        for k in [Key::Char('k'), Key::Char('-'), Key::Up] {
            p.update(k);
        }
        assert_eq!(p.current, 0);
        p.update(Key::Up);
        assert_eq!(p.current, 0, "맨 위에서 더 올라가지 않는다");
        p.update(Key::Ctrl('d'));
        assert_eq!(p.current, 20);
        p.update(Key::PageDown);
        assert_eq!(p.current, 40);
        p.update(Key::Ctrl('u'));
        p.update(Key::PageUp);
        assert_eq!(p.current, 0);
        p.update(Key::Char('G'));
        assert_eq!(p.current, 99);
        p.update(Key::Down);
        assert_eq!(p.current, 99, "맨 아래에서 더 내려가지 않는다");
        p.update(Key::Char('g'));
        assert_eq!(p.current, 0);
        p.update(Key::End);
        assert_eq!(p.current, 99);
        p.update(Key::Home);
        assert_eq!(p.current, 0);
        p.update(Key::WheelDown);
        assert_eq!(p.current, 1);
        p.update(Key::WheelUp);
        assert_eq!(p.current, 0);
    }

    #[test]
    fn window_follows_current() {
        let mut p = picker(100);
        p.set_size(100, 10);
        p.update(Key::Char('G'));
        p.layout();
        assert_eq!(p.window, 90, "맨 아래: 마지막 줄이 보이게");
        p.update(Key::Char('g'));
        p.layout();
        assert_eq!(p.window, 0);
        for _ in 0..10 {
            p.update(Key::Char('j'));
        }
        p.layout();
        assert_eq!(
            (p.current, p.window),
            (10, 1),
            "10줄 창에서 열한 번째 줄은 창을 한 줄 민다"
        );
    }

    #[test]
    fn click_selects_the_row_in_the_list_only() {
        let mut p = picker(100);
        p.set_size(100, 10);
        p.update(Key::Char('G'));
        p.layout();
        p.update(Key::Click { row: 2, col: 5 });
        assert_eq!(p.current, 92);
        p.update(Key::Click { row: 2, col: 40 });
        assert_eq!(p.current, 92, "목록 밖(미리보기)은 무시");
        p.update(Key::Click { row: 99, col: 5 });
        assert_eq!(p.current, 92, "목록 끝 아래는 무시");
    }

    #[test]
    fn search_filters_by_subsequence_and_keeps_order() {
        let mut p = picker(100);
        p.update(Key::Char('/'));
        assert_eq!(p.mode, Mode::Search);
        for c in "t 07".chars() {
            p.update(Key::Char(c));
        }
        // 낱말마다 흩어진 글자 순서가 맞으면 된다(`t`, 그리고 0 뒤에 7). 017·027 같은 것도 든다
        let want: Vec<String> = (0..100)
            .map(|i| format!("Theme {i:03}"))
            .filter(|n| n.to_lowercase().find('0').is_some_and(|z| n[z + 1..].contains('7')))
            .collect();
        assert_eq!(names(&p), want.iter().map(String::as_str).collect::<Vec<_>>());
        assert_eq!(names(&p)[0], "Theme 007");
        p.update(Key::Backspace);
        p.update(Key::Backspace);
        assert_eq!(p.query, "t ");
        assert_eq!(p.filtered.len(), 100);
        p.update(Key::Char('9'));
        p.update(Key::Ctrl('x'));
        assert_eq!(p.query, "");
        assert_eq!(p.filtered.len(), 100);
        p.update(Key::Enter);
        assert_eq!(p.mode, Mode::Normal, "찾기에서 Enter는 창을 닫는다");
        // 찾기 중에는 q·j가 글자다
        p.update(Key::Char('/'));
        p.update(Key::Char('q'));
        assert!(!p.quit);
        assert_eq!(p.query, "q");
        assert!(p.filtered.is_empty());
        p.update(Key::Esc);
        assert_eq!(p.mode, Mode::Normal);
        p.update(Key::Ctrl('/'));
        assert_eq!(p.filtered.len(), 100, "일반 모드에서 Ctrl-/ 도 찾는 글을 지운다");
    }

    #[test]
    fn search_keeps_the_selected_theme_when_it_still_matches() {
        let mut p = picker(100);
        p.update(Key::Char('G'));
        assert_eq!(p.entries[p.filtered[p.current]].name, "Theme 099");
        p.update(Key::Char('/'));
        p.update(Key::Char('9'));
        assert_eq!(p.entries[p.filtered[p.current]].name, "Theme 099", "고른 테마를 지킨다");
        p.update(Key::Char('8'));
        // 099는 `98`에 안 맞으니 맨 위(098)로
        assert_eq!(names(&p), vec!["Theme 098"]);
        assert_eq!(p.current, 0);
    }

    #[test]
    fn small_result_resets_window_to_top() {
        let mut p = picker(100);
        p.set_size(100, 10);
        p.update(Key::Char('G'));
        p.layout();
        assert!(p.window > 0);
        p.update(Key::Char('/'));
        p.update(Key::Char('9'));
        p.update(Key::Char('9'));
        assert!(p.filtered.len() <= SMALL_LIST_THRESHOLD);
        assert_eq!(p.window, 0, "결과가 10개 이하면 맨 위부터 다 보인다");
    }

    #[test]
    fn small_result_resets_window_even_when_the_selection_was_deep() {
        // y0~y9(10개) 뒤에 x가 30개. 5줄 창에서 y9를 고르면 창은 5부터 보인다.
        // `y`로 걸러 10개가 남으면 그 자리를 따라가지 않고 맨 위부터 다 보인다
        let mut entries: Vec<Entry> = (0..10).map(|i| entry(&format!("y{i}"), "#000000")).collect();
        entries.extend((0..30).map(|i| entry(&format!("x{i}"), "#000000")));
        let mut p = Picker::new(entries, ColorFilter::All, true, PathBuf::new());
        p.set_size(100, 5);
        for _ in 0..9 {
            p.update(Key::Char('j'));
            p.layout();
        }
        assert_eq!((p.current, p.window), (9, 5));
        p.update(Key::Char('/'));
        p.update(Key::Char('y'));
        assert_eq!(p.filtered.len(), 10);
        assert_eq!(p.entries[p.filtered[p.current]].name, "y9", "고른 테마는 그대로");
        assert_eq!(p.window, 0, "10개 이하면 맨 위부터");
    }

    #[test]
    fn grid_strips_control_bytes_even_if_a_label_was_not_cleaned() {
        // `Entry::new`가 이름을 닦지만, 그리는 쪽도 믿지 않는다(마지막 방어)
        let mut e = entry("ok", "#000000");
        e.label = "A\u{1b}]0;title\u{7}B\u{1b}[2JC".to_string();
        let mut p = Picker::new(vec![e], ColorFilter::All, true, PathBuf::new());
        p.set_size(100, 30);
        let s = text(&p.render());
        assert!(!s.contains("\u{1b}]") && !s.contains('\u{7}'), "{s:?}");
        assert!(!s.contains("\u{1b}[2J"));
        assert!(s.contains("A]0;titleB[2JC"));
    }

    #[test]
    fn f_cycles_all_dark_light() {
        let mut p = picker(10);
        assert_eq!(p.filtered.len(), 10);
        p.update(Key::Char('f'));
        assert_eq!(p.filter, ColorFilter::Dark);
        assert_eq!(p.filtered.len(), 5);
        assert!(p.filtered.iter().all(|&i| i % 2 == 0));
        p.update(Key::Char('f'));
        assert_eq!(p.filter, ColorFilter::Light);
        assert!(p.filtered.iter().all(|&i| i % 2 == 1));
        p.update(Key::Char('f'));
        assert_eq!((p.filter, p.filtered.len()), (ColorFilter::All, 10));
    }

    #[test]
    fn initial_filter_is_applied() {
        let entries = vec![entry("A", "#000000"), entry("B", "#ffffff"), entry("C", "#808080")];
        let p = Picker::new(entries, ColorFilter::Light, false, PathBuf::new());
        // 0.5 이상이 밝다: #808080은 0.502라 밝다
        assert_eq!(names(&p), vec!["B", "C"]);
    }

    #[test]
    fn enter_opens_save_and_w_writes_only_there() {
        let mut p = picker(5);
        assert_eq!(p.update(Key::Char('w')), None, "일반 화면에서 w는 아무 일도 안 한다");
        assert_eq!(p.update(Key::Enter), None);
        assert_eq!(p.mode, Mode::Save);
        assert_eq!(p.confirmed.as_deref(), Some("Theme 000"));
        assert_eq!(p.update(Key::Char('w')), Some(Effect::WriteAuto("Theme 000".into())));
        assert_eq!(p.update(Key::Enter), None);
        assert_eq!(p.mode, Mode::Normal);
        // 테마가 없으면 저장 화면을 열지 않는다
        let mut e = Picker::new(vec![], ColorFilter::All, true, PathBuf::new());
        e.update(Key::Enter);
        assert_eq!(e.mode, Mode::Normal);
    }

    #[test]
    fn quit_keys() {
        for k in [Key::Char('q'), Key::Esc, Key::Ctrl('c')] {
            let mut p = picker(3);
            p.update(k);
            assert!(p.quit, "{k:?}");
        }
        for mode_keys in [vec![Key::Char('?')], vec![Key::Enter]] {
            let mut p = picker(3);
            for k in mode_keys {
                p.update(k);
            }
            assert!(!p.quit);
            p.update(Key::Char('q'));
            assert!(p.quit, "도움말·저장 화면에서도 q는 끝낸다");
        }
        let mut p = picker(3);
        p.update(Key::Char('/'));
        p.update(Key::Ctrl('c'));
        assert!(p.quit, "찾기 중에도 Ctrl-C는 끝낸다");
    }

    #[test]
    fn help_toggles() {
        let mut p = picker(3);
        for open in [Key::Char('?'), Key::F1, Key::Ctrl('h')] {
            p.update(open);
            assert_eq!(p.mode, Mode::Help, "{open:?}");
            p.update(Key::Esc);
            assert_eq!(p.mode, Mode::Normal);
            assert!(!p.quit);
        }
    }

    #[test]
    fn hex_and_copy() {
        let mut p = picker(3);
        p.update(Key::Char('h'));
        assert!(p.hex);
        p.update(Key::Char('d'));
        assert!(!p.hex);
        p.update(Key::Char('x'));
        assert!(p.hex);
        assert_eq!(p.update(Key::Char('c')), Some(Effect::Copy("Theme 000".into())));
        assert_eq!(
            p.update(Key::Char('C')),
            Some(Effect::Copy("(앱에 넣어 둔 테마)".into()))
        );
    }

    #[test]
    fn render_shows_palette_code_and_colors() {
        let p = picker(3);
        let s = text(&p.render());
        assert!(s.contains("Theme 000"));
        assert!(s.contains("████"));
        assert!(s.contains("ziggzagg.zig") && s.contains("Lorem"), "예시 코드와 글 문단");
        // 테마의 1번 색(#ff0000)이 24비트 SGR로 나간다
        assert!(s.contains("38;2;255;0;0"), "{s:?}");
        assert!(s.contains("38;2;0;255;0"));
        // 배경 #101010
        assert!(s.contains("48;2;16;16;16"));
        // 팔레트 번호: 10진수 → 16진수
        assert!(s.contains("  0"));
        let mut q = picker(3);
        q.update(Key::Char('h'));
        assert!(text(&q.render()).contains(" 0f"));
        // 같은 상태는 같은 바이트
        assert_eq!(p.render(), picker(3).render());
    }

    #[test]
    fn render_marks_selected_row_and_fills_every_cell() {
        let p = picker(3);
        let s = text(&p.render());
        assert!(s.contains('❯') && s.contains('❮'));
        // 줄마다 커서 이동으로 시작한다
        for row in 1..=40 {
            assert!(s.contains(&format!("\x1b[{row};1H")), "줄 {row}");
        }
    }

    #[test]
    fn render_does_not_panic_for_tiny_or_odd_sizes() {
        for (c, r) in [
            (0, 0),
            (1, 1),
            (5, 2),
            (31, 3),
            (32, 4),
            (33, 30),
            (45, 8),
            (200, 3),
            (100, 100),
        ] {
            for mode in [Mode::Normal, Mode::Help, Mode::Search, Mode::Save] {
                let mut p = picker(30);
                p.set_size(c, r);
                p.mode = mode;
                let _ = p.render();
            }
        }
        let mut e = Picker::new(vec![], ColorFilter::All, false, PathBuf::new());
        e.set_size(80, 24);
        assert!(text(&e.render()).contains("테마가 없습니다"));
    }

    #[test]
    fn untrusted_names_are_stripped_before_drawing() {
        let evil = "A\u{1b}[31mB\u{1b}]52;c;AAAA\u{7}C\u{9b}D\nE\u{7f}F\u{202e}G";
        let mut p = Picker::new(
            vec![Entry::new(
                evil,
                Some(PathBuf::from("/tmp/x\u{1b}[2Jy")),
                Ok("background = #000000\n"),
            )],
            ColorFilter::All,
            true,
            PathBuf::from("/tmp/au\u{1b}to"),
        );
        p.set_size(120, 50);
        for mode in [Mode::Normal, Mode::Help, Mode::Search, Mode::Save] {
            p.mode = mode;
            let out = p.render();
            // 우리가 내보내는 이스케이프만 남아야 한다: ESC 뒤는 `[`뿐이고 `[`의 내용은 숫자·;·:·HmM이다
            let s = text(&out);
            assert!(!s.contains("\u{1b}]"), "OSC가 새어 나옴 ({mode:?})");
            assert!(!s.contains('\u{7}') && !s.contains('\u{9b}') && !s.contains('\u{7f}'));
            assert!(!s.contains('\u{202e}'));
            let mut it = s.chars().peekable();
            while let Some(c) = it.next() {
                if c == '\u{1b}' {
                    assert_eq!(it.next(), Some('['));
                    let body: String = it.by_ref().take_while(|c| !c.is_ascii_alphabetic()).collect();
                    assert!(body.chars().all(|c| c.is_ascii_digit() || ";:".contains(c)), "{body:?}");
                }
            }
        }
        assert_eq!(p.entries[0].label, "A[31mB]52;c;AAAACDEFG");
        // 복사도 닦은 이름이다
        p.mode = Mode::Normal;
        assert_eq!(
            p.update(Key::Char('c')),
            Some(Effect::Copy("A[31mB]52;c;AAAACDEFG".into()))
        );
    }

    #[test]
    fn save_screen_shows_the_config_line_and_the_notice() {
        let mut p = picker(3);
        p.update(Key::Enter);
        let s = text(&p.render());
        assert!(s.contains("theme = Theme 000"));
        assert!(s.contains("config-file = ?auto/theme.ghostty"));
        p.notice = Some((false, "쓰지 못했습니다: x".into()));
        assert!(text(&p.render()).contains("쓰지 못했습니다: x"));
    }

    #[test]
    fn search_box_shows_the_query() {
        let mut p = picker(3);
        p.update(Key::Char('/'));
        p.update(Key::Char('z'));
        p.update(Key::Char('가'));
        assert!(text(&p.render()).contains("z가"));
    }

    #[test]
    fn wide_names_do_not_spill_out_of_the_list_column() {
        let name = "한글테마".repeat(10);
        let mut p = Picker::new(vec![entry(&name, "#000000")], ColorFilter::All, true, PathBuf::new());
        p.set_size(80, 5);
        let s = text(&p.render());
        // 첫 줄: 목록 32칸 안에서 끝나고 ❮가 따라온다
        let first = s.split("\x1b[2;1H").next().unwrap();
        assert!(first.contains('❮'));
        let shown: usize = first
            .chars()
            .filter(|&c| (0xac00..=0xd7a3).contains(&(c as u32)))
            .count();
        assert!(shown <= 14, "한글은 두 칸이라 목록에는 14자까지: {shown}");
    }

    #[test]
    fn unreadable_theme_shows_the_error_panel() {
        let e = Entry::new("Gone", Some(PathBuf::from("/x/Gone")), Err("권한이 없습니다".into()));
        let mut p = Picker::new(vec![e], ColorFilter::All, true, PathBuf::new());
        p.set_size(100, 30);
        let s = text(&p.render());
        assert!(s.contains("Gone을(를) 열 수 없습니다") && s.contains("권한이 없습니다") && s.contains("/x/Gone"));
    }

    #[test]
    fn theme_colors_follow_ghostty_defaults() {
        let c = ThemeColors::parse(
            "palette = 238=#010203\nbackground = red\ncursor-color = #112233\nselection-foreground = #445566\nbogus line\n",
        );
        assert_eq!(c.bg, Rgb::new(255, 0, 0), "색 이름도 읽는다");
        assert_eq!(c.palette[238], Rgb::new(1, 2, 3));
        assert_eq!(c.cursor_bg, Rgb::new(0x11, 0x22, 0x33));
        assert_eq!(c.cursor_fg, c.bg, "cursor-text가 없으면 배경색");
        assert_eq!(c.sel_fg, Rgb::new(0x44, 0x55, 0x66));
        assert_eq!(c.sel_bg, c.fg, "selection-background가 없으면 글자색");
        // 읽지 못한 값은 기본을 둔다
        let d = ThemeColors::parse("background = nonsense\n");
        assert_eq!(d.bg, Theme::ghostty_default().background);
    }

    #[test]
    fn write_auto_theme_writes_one_line_and_refuses_injection() {
        let dir = std::env::temp_dir().join(format!("bt-themepick-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let f = dir.join("auto/theme.ghostty");
        write_auto_theme(&f, "Dracula").unwrap();
        assert_eq!(std::fs::read_to_string(&f).unwrap(), "theme = Dracula\n");
        let before = std::fs::read_to_string(&f).unwrap();
        assert!(write_auto_theme(&f, "X\nconfig-file = /etc/passwd").is_err());
        assert!(write_auto_theme(&f, "").is_err());
        assert_eq!(
            std::fs::read_to_string(&f).unwrap(),
            before,
            "거절하면 파일을 건드리지 않는다"
        );
        // 바깥(설정 파일)은 만들지도 않는다
        assert!(!dir.join("config").exists());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn apply_write_reports_success_and_failure() {
        let dir = std::env::temp_dir().join(format!("bt-themepick-w-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let mut p = picker(2);
        p.auto_path = dir.join("auto/theme.ghostty");
        apply_write(&mut p, "Theme 000");
        assert!(
            matches!(&p.notice, Some((true, m)) if m.contains("theme = Theme 000")),
            "{:?}",
            p.notice
        );
        apply_write(&mut p, "bad\u{1b}name");
        assert!(matches!(&p.notice, Some((false, _))));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn osc52_is_base64_of_the_text() {
        assert_eq!(osc52("Dracula"), b"\x1b]52;c;RHJhY3VsYQ==\x07".to_vec());
    }
}
