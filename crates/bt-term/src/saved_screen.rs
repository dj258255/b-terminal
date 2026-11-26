//! 창 상태 복원(#861)이 칸마다 저장하는 글과 색(SGR), 그리고 되살릴 때 엔진 격자에 직접 놓는 길.
//!
//! 보안 경계(ADR-092): 저장한 것은 바이트가 아니라 셀(글자·전경·배경·꾸밈)이다. 되살릴 때는 VT 파서를 거치지 않고
//! 격자의 셀에 직접 쓰므로 파일에 OSC 52·DCS·BEL이 들어 있어도 해석·실행되지 않는다. 글자는 제어 문자와
//! 방향 제어 글자를 지우고, 꾸밈 비트는 허용한 것만 남긴다.

use alacritty_terminal::event::EventListener;
use alacritty_terminal::grid::Dimensions;
use alacritty_terminal::index::{Column, Line};
use alacritty_terminal::term::Term;
use alacritty_terminal::term::cell::{Cell, Flags};
use alacritty_terminal::vte::ansi::{Color, NamedColor, Rgb};
use serde::{Deserialize, Serialize};

/// 한 줄(접힌 줄은 이어 붙인 논리 줄)의 셀 수 상한. 파일에서 읽은 줄은 여기서 자른다
pub const MAX_LINE_CELLS: usize = 8192;
/// 한 칸 글자에 붙는 결합 문자 수 상한(엔진의 `MAX_ZEROWIDTH`와 같다)
const MAX_ZW: usize = 64;

/// 저장하고 되살리는 꾸밈: 반전·굵게·기울임·밑줄 종류·흐림·숨김·취소선·윗줄·넓은 글자. 이 밖의 비트(보호 칸, 프롬프트 표시 등)는 읽을 때 버린다
fn allowed_flags() -> Flags {
    Flags::INVERSE
        | Flags::BOLD
        | Flags::ITALIC
        | Flags::DIM
        | Flags::HIDDEN
        | Flags::STRIKEOUT
        | Flags::OVERLINE
        | Flags::WIDE_CHAR
        | Flags::ALL_UNDERLINES
}

/// 한 칸짜리 묶음 표시(결합 문자가 붙은 칸). `Flags`의 비트와 겹치지 않는 맨 위 비트
const ZW: u32 = 1 << 31;

/// 저장하는 색. 팔레트 번호는 번호 그대로 두어 되살린 뒤 테마를 따른다
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum SavedColor {
    /// 기본 글자색·바탕색
    #[default]
    D,
    /// 팔레트 번호(0~255)
    I(u8),
    /// 트루컬러
    R(u8, u8, u8),
}

fn color_is_default(c: &SavedColor) -> bool {
    *c == SavedColor::D
}

fn flags_is_zero(f: &u32) -> bool {
    *f == 0
}

/// 같은 꾸밈이 이어지는 글 한 토막. 보통은 글자 하나가 한 칸이다. `f`에 넓은 글자(`WIDE_CHAR`)나 `ZW`가 있으면 `t` 전체가 한 칸이다
/// (넓은 글자 하나, 또는 글자 하나와 그 결합 문자들)
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SavedRun {
    pub t: String,
    #[serde(default, skip_serializing_if = "color_is_default")]
    pub fg: SavedColor,
    #[serde(default, skip_serializing_if = "color_is_default")]
    pub bg: SavedColor,
    #[serde(default, skip_serializing_if = "flags_is_zero")]
    pub f: u32,
}

/// 한 줄. 화면 폭에서 접힌 줄은 이어 붙여 한 줄로 두고, 되살릴 때 새 폭에 맞춰 다시 접는다
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(transparent)]
pub struct SavedLine {
    pub runs: Vec<SavedRun>,
}

impl SavedLine {
    /// 꾸밈 없는 한 줄(시험·안내 줄용)
    pub fn plain(text: &str) -> Self {
        Self {
            runs: vec![SavedRun {
                t: text.to_string(),
                fg: SavedColor::D,
                bg: SavedColor::D,
                f: 0,
            }],
        }
    }

    /// 글만 이어 붙인다(시험·점검용)
    pub fn text(&self) -> String {
        self.runs.iter().map(|r| r.t.as_str()).collect()
    }
}

fn save_color(c: Color) -> SavedColor {
    match c {
        Color::Named(n) => {
            let i = n as usize;
            if i < 16 { SavedColor::I(i as u8) } else { SavedColor::D }
        }
        Color::Indexed(i) => SavedColor::I(i),
        Color::Spec(Rgb { r, g, b }) => SavedColor::R(r, g, b),
    }
}

fn load_color(c: SavedColor, background: bool) -> Color {
    match c {
        SavedColor::D => Color::Named(if background {
            NamedColor::Background
        } else {
            NamedColor::Foreground
        }),
        SavedColor::I(i) => Color::Indexed(i),
        SavedColor::R(r, g, b) => Color::Spec(Rgb { r, g, b }),
    }
}

/// 격자에서 읽은 칸 하나
struct Snap {
    c: char,
    zw: Vec<char>,
    fg: SavedColor,
    bg: SavedColor,
    f: u32,
}

impl Snap {
    fn is_blank(&self) -> bool {
        (self.c == ' ' || self.c == '\0')
            && self.zw.is_empty()
            && self.fg == SavedColor::D
            && self.bg == SavedColor::D
            // 빈 칸에서도 보이는 꾸밈(반전·밑줄·취소선·윗줄)이 있으면 비지 않은 것이다
            && self.f & (Flags::INVERSE | Flags::ALL_UNDERLINES | Flags::STRIKEOUT | Flags::OVERLINE).bits() == 0
    }
}

fn snap_of(cell: &Cell) -> Snap {
    let mut f = (cell.flags & allowed_flags()).bits();
    let zw: Vec<char> = cell.zerowidth().map(|z| z.to_vec()).unwrap_or_default();
    if !zw.is_empty() {
        f |= ZW;
    }
    Snap {
        c: if cell.c == '\0' { ' ' } else { cell.c },
        zw,
        fg: save_color(cell.fg),
        bg: save_color(cell.bg),
        f,
    }
}

fn encode(cells: Vec<Snap>) -> SavedLine {
    let mut runs: Vec<SavedRun> = Vec::new();
    for s in cells {
        let single = s.f & (Flags::WIDE_CHAR.bits() | ZW) != 0;
        if !single
            && let Some(last) = runs.last_mut()
            && last.f == s.f
            && last.fg == s.fg
            && last.bg == s.bg
        {
            last.t.push(s.c);
            continue;
        }
        let mut t = String::new();
        t.push(s.c);
        t.extend(s.zw.iter());
        runs.push(SavedRun {
            t,
            fg: s.fg,
            bg: s.bg,
            f: s.f,
        });
    }
    // 한 칸짜리 토막(넓은 글자·결합 문자)은 꾸밈 비트에 표시가 있어 다음 보통 글자와 합쳐지지 않는다
    SavedLine { runs }
}

/// 기본 화면(대체 화면이 아닌 쪽)의 스크롤백과 보이는 줄을 끝에서부터 `max_lines` 줄까지 저장 모양으로 꺼낸다.
/// 끝의 빈 줄은 뺀다. 줄 끝의 기본 빈 칸도 뺀다. `max_lines`가 0이면 비어 있다
pub fn capture<T>(term: &Term<T>, max_lines: usize) -> Vec<SavedLine> {
    let grid = term.primary_grid();
    let cols = grid.columns();
    if max_lines == 0 || cols == 0 {
        return Vec::new();
    }
    let top = grid.topmost_line();
    let mut bottom = grid.bottommost_line();
    let wrapped = |line: Line| grid[line][Column(cols - 1)].flags.contains(Flags::WRAPLINE);
    let blank_row = |line: Line| (0..cols).all(|c| snap_of(&grid[line][Column(c)]).is_blank());
    // 끝의 빈 줄(커서 아래 비어 있는 화면)은 저장하지 않는다
    while bottom > top && !wrapped(bottom) && blank_row(bottom) {
        bottom -= 1;
    }
    if bottom == top && !wrapped(bottom) && blank_row(bottom) {
        return Vec::new();
    }
    // 위로 올라가며 `max_lines`개 논리 줄이 되는 첫 행을 찾는다
    let mut start = bottom;
    let mut lines = 1usize;
    while start > top {
        let prev = start - 1;
        if !wrapped(prev) {
            if lines >= max_lines {
                break;
            }
            lines += 1;
        }
        start -= 1;
    }
    let mut out = Vec::new();
    let mut cur: Vec<Snap> = Vec::new();
    let mut line = start;
    while line <= bottom {
        let row = &grid[line];
        for c in 0..cols {
            let cell = &row[Column(c)];
            if cell
                .flags
                .intersects(Flags::WIDE_CHAR_SPACER | Flags::LEADING_WIDE_CHAR_SPACER)
            {
                continue;
            }
            if cur.len() < MAX_LINE_CELLS {
                cur.push(snap_of(cell));
            }
        }
        if !wrapped(line) || line == bottom {
            while cur.last().is_some_and(Snap::is_blank) {
                cur.pop();
            }
            out.push(encode(std::mem::take(&mut cur)));
        }
        line += 1;
    }
    out
}

/// 방향 제어·줄 구분 글자 같은, 화면 글을 속일 수 있는 글자
fn is_spoofing(c: char) -> bool {
    matches!(c, '\u{200E}' | '\u{200F}' | '\u{202A}'..='\u{202E}' | '\u{2066}'..='\u{2069}' | '\u{2028}' | '\u{2029}')
}

/// 글자 하나를 다듬는다: 제어 문자(C0·DEL·C1)와 속임 글자는 공백이 된다
fn clean_char(c: char) -> char {
    if c.is_control() || is_spoofing(c) { ' ' } else { c }
}

/// 되살릴 한 칸
struct Item {
    c: char,
    zw: Vec<char>,
    fg: Color,
    bg: Color,
    flags: Flags,
    wide: bool,
}

fn expand(line: &SavedLine) -> Vec<Item> {
    let allowed = allowed_flags();
    let mut out: Vec<Item> = Vec::new();
    for run in &line.runs {
        let flags = Flags::from_bits_truncate(run.f & !ZW) & allowed;
        let fg = load_color(run.fg, false);
        let bg = load_color(run.bg, true);
        let single = run.f & ZW != 0 || flags.contains(Flags::WIDE_CHAR);
        if single {
            let mut chars = run.t.chars();
            let Some(base) = chars.next() else { continue };
            let zw: Vec<char> = chars
                .filter(|c| !c.is_control() && !is_spoofing(*c))
                .take(MAX_ZW)
                .collect();
            if out.len() < MAX_LINE_CELLS {
                out.push(Item {
                    c: clean_char(base),
                    zw,
                    fg,
                    bg,
                    flags: flags - Flags::WIDE_CHAR,
                    wide: flags.contains(Flags::WIDE_CHAR),
                });
            }
        } else {
            for ch in run.t.chars() {
                if out.len() >= MAX_LINE_CELLS {
                    break;
                }
                out.push(Item {
                    c: clean_char(ch),
                    zw: Vec::new(),
                    fg,
                    bg,
                    flags,
                    wide: false,
                });
            }
        }
    }
    out
}

fn blank_cell() -> Cell {
    Cell::default()
}

fn leading_spacer() -> Cell {
    let mut c = blank_cell();
    c.flags.insert(Flags::LEADING_WIDE_CHAR_SPACER);
    c
}

/// 한 줄을 `cols` 폭 행들로 접는다. 넓은 글자가 줄 끝에 걸리면 앞 칸을 `LEADING_WIDE_CHAR_SPACER`로 채우고 다음 행으로 넘긴다
fn layout(items: Vec<Item>, cols: usize) -> Vec<Vec<Cell>> {
    let mut rows: Vec<Vec<Cell>> = Vec::new();
    let mut row: Vec<Cell> = Vec::new();
    let n = items.len();
    for (i, it) in items.into_iter().enumerate() {
        let w = if it.wide { 2 } else { 1 };
        if w > cols {
            continue;
        }
        if row.len() + w > cols {
            while row.len() < cols {
                row.push(leading_spacer());
            }
            if let Some(last) = row.last_mut() {
                last.flags.insert(Flags::WRAPLINE);
            }
            rows.push(std::mem::take(&mut row));
        }
        let mut cell = blank_cell();
        cell.c = it.c;
        cell.fg = it.fg;
        cell.bg = it.bg;
        // 글자를 놓은 칸 표시(WRITTEN): 복사·화면 읽기가 이 칸을 글 있는 칸으로 센다
        cell.flags = it.flags | Flags::WRITTEN;
        for z in it.zw {
            cell.push_zerowidth(z);
        }
        if it.wide {
            cell.flags.insert(Flags::WIDE_CHAR);
            row.push(cell);
            let mut sp = blank_cell();
            sp.fg = it.fg;
            sp.bg = it.bg;
            sp.flags.insert(Flags::WIDE_CHAR_SPACER);
            row.push(sp);
        } else {
            row.push(cell);
        }
        if row.len() == cols && i + 1 < n {
            if let Some(last) = row.last_mut() {
                last.flags.insert(Flags::WRAPLINE);
            }
            rows.push(std::mem::take(&mut row));
        }
    }
    rows.push(row);
    rows
}

/// 저장한 줄들을 `term`의 스크롤백 맨 아래에 놓는다. 화면은 비어 있어야 한다(칸을 띄우기 전, 셸이 아무것도 쓰기 전).
/// 줄은 VT 파서를 거치지 않고 격자에 직접 쓴다. `note`가 있으면 맨 끝에 흐린 안내 줄로 붙인다
pub fn restore<T: EventListener>(term: &mut Term<T>, lines: &[SavedLine], note: Option<&str>) {
    let cols = term.columns();
    let screen = term.screen_lines();
    if cols == 0 || screen == 0 || (lines.is_empty() && note.is_none()) {
        return;
    }
    let region = Line(0)..Line(screen as i32);
    // 맨 윗줄에 쓰고 한 줄 올리면 그 줄이 스크롤백으로 들어간다
    let top = Line(0);
    let push_row = |term: &mut Term<T>, row: Vec<Cell>| {
        let grid = term.grid_mut();
        for c in 0..cols {
            grid[top][Column(c)] = row.get(c).cloned().unwrap_or_else(blank_cell);
        }
        grid.scroll_up(&region, 1);
    };
    for line in lines {
        for row in layout(expand(line), cols) {
            push_row(term, row);
        }
    }
    if let Some(note) = note {
        let mut line = SavedLine::plain(note);
        for r in &mut line.runs {
            r.f = Flags::DIM.bits();
        }
        for row in layout(expand(&line), cols) {
            push_row(term, row);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use alacritty_terminal::term::Config;
    use alacritty_terminal::vte::ansi::Processor;

    #[derive(Clone, Copy)]
    struct Quiet;
    impl EventListener for Quiet {}

    struct D(usize, usize);
    impl Dimensions for D {
        fn total_lines(&self) -> usize {
            self.1
        }
        fn screen_lines(&self) -> usize {
            self.1
        }
        fn columns(&self) -> usize {
            self.0
        }
    }

    fn term(cols: usize, rows: usize) -> Term<Quiet> {
        let config = Config {
            scrolling_history: 1000,
            ..Config::default()
        };
        Term::new(config, &D(cols, rows), Quiet)
    }

    fn feed(t: &mut Term<Quiet>, bytes: &[u8]) {
        let mut p: Processor = Processor::new();
        p.advance(t, bytes);
    }

    /// 격자 전체(스크롤백 포함)를 줄 글로 읽는다
    fn all_text(t: &Term<Quiet>) -> Vec<String> {
        let g = t.grid();
        let mut out = Vec::new();
        let mut line = g.topmost_line();
        while line <= g.bottommost_line() {
            let mut s = String::new();
            for c in 0..g.columns() {
                let cell = &g[line][Column(c)];
                if cell.flags.contains(Flags::WIDE_CHAR_SPACER) || cell.flags.contains(Flags::LEADING_WIDE_CHAR_SPACER)
                {
                    continue;
                }
                s.push(cell.c);
            }
            out.push(s.trim_end().to_string());
            line += 1;
        }
        out
    }

    #[test]
    fn capture_keeps_text_colors_and_trims_trailing_blank_lines() {
        let mut t = term(20, 4);
        feed(
            &mut t,
            b"one\r\n\x1b[1;31mred bold\x1b[0m\r\n\x1b[38;2;1;2;3mrgb\x1b[0m\r\nlast\r\nmore\r\nend",
        );
        let lines = capture(&t, 100);
        let texts: Vec<String> = lines.iter().map(|l| l.text()).collect();
        assert_eq!(texts, ["one", "red bold", "rgb", "last", "more", "end"]);
        let red = &lines[1].runs[0];
        assert_eq!(red.fg, SavedColor::I(1));
        assert_ne!(red.f & Flags::BOLD.bits(), 0);
        assert_eq!(lines[2].runs[0].fg, SavedColor::R(1, 2, 3));
        // 줄 수 상한: 끝에서부터
        let two = capture(&t, 2);
        assert_eq!(two.iter().map(|l| l.text()).collect::<Vec<_>>(), ["more", "end"]);
    }

    #[test]
    fn restore_round_trips_into_scrollback_not_screen() {
        let mut t = term(20, 4);
        feed(
            &mut t,
            b"alpha\r\n\x1b[32mbeta\x1b[0m\r\n\xed\x95\x9c\xea\xb8\x80 wide\r\n",
        );
        let saved = capture(&t, 100);
        let mut fresh = term(20, 4);
        restore(&mut fresh, &saved, Some("-- saved --"));
        // 화면은 비어 있고(스크롤백에만 놓임) 커서는 맨 위
        assert_eq!(fresh.grid().history_size(), 4);
        assert_eq!(fresh.grid().cursor.point.line, Line(0));
        let text = all_text(&fresh);
        assert_eq!(&text[0..4], ["alpha", "beta", "한글 wide", "-- saved --"]);
        // 색이 되살아남
        let beta = &fresh.grid()[Line(-3)][Column(0)];
        assert_eq!(beta.fg, Color::Indexed(2));
        // 넓은 글자는 엔진 표시(WIDE_CHAR + 뒤 칸 SPACER)로
        let wide = &fresh.grid()[Line(-2)][Column(0)];
        assert!(wide.flags.contains(Flags::WIDE_CHAR));
        assert!(
            fresh.grid()[Line(-2)][Column(1)]
                .flags
                .contains(Flags::WIDE_CHAR_SPACER)
        );
        // 안내 줄은 흐림
        assert!(fresh.grid()[Line(-1)][Column(0)].flags.contains(Flags::DIM));
        // 되살린 뒤 셸이 평소처럼 쓴다
        feed(&mut fresh, b"$ ");
        assert_eq!(all_text(&fresh)[4], "$");
    }

    #[test]
    fn restore_rewraps_to_new_width() {
        let mut t = term(10, 3);
        feed(&mut t, b"0123456789abcdefghij\r\nx");
        let saved = capture(&t, 100);
        assert_eq!(saved[0].text(), "0123456789abcdefghij", "접힌 줄은 한 줄로 이어 저장");
        let mut narrow = term(6, 3);
        restore(&mut narrow, &saved, None);
        let text = all_text(&narrow);
        assert_eq!(&text[0..4], ["012345", "6789ab", "cdefgh", "ij"]);
        assert!(narrow.grid()[Line(-4)][Column(5)].flags.contains(Flags::WRAPLINE));
        assert!(!narrow.grid()[Line(-1)][Column(1)].flags.contains(Flags::WRAPLINE));
    }

    #[test]
    fn wide_char_at_edge_wraps_with_leading_spacer() {
        let saved = vec![
            SavedLine::plain("abc"),
            SavedLine {
                runs: vec![
                    SavedRun {
                        t: "abcde".into(),
                        fg: SavedColor::D,
                        bg: SavedColor::D,
                        f: 0,
                    },
                    SavedRun {
                        t: "한".into(),
                        fg: SavedColor::D,
                        bg: SavedColor::D,
                        f: Flags::WIDE_CHAR.bits(),
                    },
                ],
            },
        ];
        let mut t = term(6, 3);
        restore(&mut t, &saved, None);
        // 5칸 뒤 넓은 글자(2칸)는 6칸 줄에 안 들어가 다음 행으로 간다
        let g = t.grid();
        assert!(g[Line(-2)][Column(5)].flags.contains(Flags::LEADING_WIDE_CHAR_SPACER));
        assert!(g[Line(-2)][Column(5)].flags.contains(Flags::WRAPLINE));
        assert_eq!(g[Line(-1)][Column(0)].c, '한');
    }

    /// 보안 경계: 저장 파일에 OSC 52·DCS·BEL·CSI가 들어 있어도 되살릴 때 실행되지 않고 글자도 남지 않는다
    #[test]
    fn hostile_saved_text_never_reaches_the_parser_and_is_cleaned() {
        let evil = "\x1b]52;c;ZXZpbA==\x07\x1bPq#0;2;0;0;0\x1b\\\x07\x1b[2J\x1b[31m\u{9b}31m\u{202e}hi\u{85}";
        let saved = vec![SavedLine {
            runs: vec![SavedRun {
                t: evil.into(),
                fg: SavedColor::D,
                bg: SavedColor::D,
                // 읽는 쪽이 허용하지 않는 비트(보호 칸·프롬프트 표시·WRAPLINE)까지 켠 파일
                f: !ZW & !Flags::WIDE_CHAR.bits(),
            }],
        }];
        let events = std::sync::Arc::new(std::sync::Mutex::new(0usize));
        #[derive(Clone)]
        struct Count(std::sync::Arc<std::sync::Mutex<usize>>);
        impl EventListener for Count {
            fn send_event(&self, _e: alacritty_terminal::event::Event) {
                *self.0.lock().unwrap() += 1;
            }
        }
        let config = Config {
            scrolling_history: 100,
            ..Config::default()
        };
        let mut t = Term::new(config, &D(80, 4), Count(events.clone()));
        restore(&mut t, &saved, None);
        assert_eq!(
            *events.lock().unwrap(),
            0,
            "엔진 사건(클립보드·벨·제목 등)이 하나도 없어야 한다"
        );
        let g = t.grid();
        let mut seen = String::new();
        for c in 0..g.columns() {
            let cell = &g[Line(-1)][Column(c)];
            // 허용하지 않은 꾸밈 비트는 남지 않는다
            assert!(
                !cell
                    .flags
                    .intersects(Flags::PROTECTED | Flags::ROW_PROMPT | Flags::SEMANTIC_INPUT | Flags::SEMANTIC_PROMPT)
            );
            seen.push(cell.c);
        }
        assert!(
            seen.chars().all(|c| !c.is_control() && !is_spoofing(c)),
            "제어·방향 글자가 남음: {seen:?}"
        );
        // ESC·BEL·C1이 공백이 되어도 ']52;c;ZXZpbA==' 같은 글자 자체는 그저 글이다
        assert!(seen.contains("hi"));
    }

    #[test]
    fn zero_width_marks_survive_and_are_bounded() {
        let mut t = term(10, 3);
        // e + 결합 악센트
        feed(&mut t, b"e\xcc\x81x\r\n");
        let saved = capture(&t, 10);
        assert_eq!(saved[0].text(), "e\u{301}x");
        let mut fresh = term(10, 3);
        restore(&mut fresh, &saved, None);
        assert_eq!(fresh.grid()[Line(-1)][Column(0)].zerowidth(), Some(&['\u{301}'][..]));
        // 부풀린 결합 문자는 상한에서 자른다
        let big = SavedLine {
            runs: vec![SavedRun {
                t: format!("a{}", "\u{301}".repeat(1000)),
                fg: SavedColor::D,
                bg: SavedColor::D,
                f: ZW,
            }],
        };
        let mut t2 = term(10, 3);
        restore(&mut t2, &[big], None);
        assert!(t2.grid()[Line(-1)][Column(0)].zerowidth().unwrap().len() <= MAX_ZW);
    }

    #[test]
    fn alt_screen_is_not_saved_but_primary_is() {
        let mut t = term(20, 4);
        feed(&mut t, b"primary text\r\n\x1b[?1049h\x1b[Halt screen vim");
        let saved = capture(&t, 100);
        let all: String = saved.iter().map(|l| l.text()).collect::<Vec<_>>().join("|");
        assert!(all.contains("primary text"), "{all}");
        assert!(!all.contains("alt screen"), "{all}");
    }

    #[test]
    fn oversized_line_is_cut_at_load() {
        let huge = SavedLine::plain(&"x".repeat(MAX_LINE_CELLS * 3));
        assert_eq!(expand(&huge).len(), MAX_LINE_CELLS);
    }

    #[test]
    fn empty_input_does_nothing() {
        let mut t = term(10, 3);
        restore(&mut t, &[], None);
        assert_eq!(t.grid().history_size(), 0);
        assert!(capture(&t, 10).is_empty());
        assert!(capture(&t, 0).is_empty());
    }
}
