//! 고른 영역을 색·꾸밈이 든 글로 만든다: `copy_to_clipboard`의 `vt`·`html`·`mixed`(#550).
//!
//! 규칙은 Ghostty `src/terminal/formatter.zig`(`PageFormatter`)와 `style.zig`(`formatterVt`·`formatterHtml`)를
//! 7fa26e8b675f 판에서 그대로 옮겼다. 복사 길(`Surface.zig copySelectionToClipboards`)은 `unwrap = true`이고
//! 색을 팔레트 번호로 두지 않고 지금 팔레트의 RGB로 쓴다(`palette` 옵션을 줌). 점 지도(point_map)는 옮기지 않았다. 코드포인트 맵(`clipboard-codepoint-map`)은 `Options::codepoint_map`으로 건다(#787).
//!
//! alacritty 격자와 Ghostty 페이지가 다른 곳(아래 "칸 종류")은 `WRITTEN` 표시로 맞춘다

use alacritty_terminal::event::EventListener;
use alacritty_terminal::grid::Dimensions;
use alacritty_terminal::index::{Column, Line, Point};
use alacritty_terminal::term::Term;
use alacritty_terminal::term::cell::{Cell, Flags, Hyperlink};
use alacritty_terminal::vte::ansi::{Color, NamedColor};
use bt_core::config::CodepointRule;
use bt_core::{Rgb, Theme};

use crate::color;

/// `copy_to_clipboard`의 형식 인자(Ghostty `Binding.Action.CopyToClipboard`). 기본은 `Mixed`
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum CopyFormat {
    /// 글만
    Plain,
    /// 이스케이프(색·꾸밈)가 든 글. text/plain으로 넣는다
    Vt,
    /// HTML만. 바탕·글자 기본색도 넣는다
    Html,
    /// 글(text/plain) + HTML(바탕·글자 기본색 없이)
    #[default]
    Mixed,
}

impl CopyFormat {
    /// `copy_to_clipboard:<이름>`의 이름
    pub fn parse(s: &str) -> Option<Self> {
        Some(match s {
            "plain" => Self::Plain,
            "vt" => Self::Vt,
            "html" => Self::Html,
            "mixed" => Self::Mixed,
            _ => return None,
        })
    }
}

/// 만들 꼴
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Emit {
    /// 글만(꾸밈·링크 없음). 복사의 `plain`과 `write_*_file`의 `plain`(#785)
    Plain,
    Vt,
    Html,
}

/// 옵션(Ghostty `formatter.Options` 중 쓰는 것)
#[derive(Debug, Clone, Copy)]
pub struct Options<'a> {
    pub emit: Emit,
    /// `clipboard-trim-trailing-spaces`. Ghostty에서는 꾸밈 있는 꼴에 거의 영향이 없고, VT에서 끝의 빈 줄을 남길지만 정한다
    pub trim: bool,
    /// 바탕·글자 기본색. 있으면 HTML 감싸개의 style, VT 앞머리의 OSC 11·10으로 넣는다
    pub background: Option<Rgb>,
    pub foreground: Option<Rgb>,
    /// `clipboard-codepoint-map`. 글자 칸의 글자(결합 글자 포함)마다 적용하고, vt·html 모두에 건다(Ghostty `writeCodepointWithReplacement`).
    /// 하이퍼링크 주소와 빈 칸에는 걸지 않는다. 겹치면 나중 규칙이 이긴다. 비면 바꾸지 않는다
    pub codepoint_map: &'a [CodepointRule],
}

/// 색. Ghostty `Style.Color`처럼 팔레트 번호와 RGB를 구분해 둔다(같은 RGB여도 번호와 값은 다른 스타일이다)
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
enum Col {
    #[default]
    None,
    Palette(u8),
    Rgb(Rgb),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
enum Underline {
    #[default]
    None,
    Single,
    Double,
    Curly,
    Dotted,
    Dashed,
}

/// 칸의 꾸밈(Ghostty `Style`)
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
struct Style {
    fg: Col,
    bg: Col,
    underline_color: Col,
    bold: bool,
    italic: bool,
    faint: bool,
    blink: bool,
    inverse: bool,
    invisible: bool,
    strikethrough: bool,
    overline: bool,
    underline: Underline,
}

/// 칸 종류. alacritty는 Ghostty처럼 "글자 없는 칸"과 "빈칸을 쓴 칸"을 `WRITTEN` 표시로 가른다:
/// 글자를 찍은 칸(찍은 빈칸 포함)만 `WRITTEN`이 있고, 지우기·밀기·새 줄이 만든 칸에는 없다.
/// 앞의 칸을 비운 뒤 바탕색만 남은 칸이 Ghostty의 `bg_color_*` 칸이다
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Kind {
    /// 넓은 글자의 뒤 칸·줄 끝의 앞 칸 자리표시. 아무것도 내지 않는다
    Spacer,
    /// 글자가 있다
    Text,
    /// 글자 없이 바탕색만
    Background,
    /// 아무것도 없다
    Empty,
}

fn kind(cell: &Cell) -> Kind {
    if cell
        .flags
        .intersects(Flags::WIDE_CHAR_SPACER | Flags::LEADING_WIDE_CHAR_SPACER)
    {
        Kind::Spacer
    } else if cell.flags.contains(Flags::WRITTEN) {
        Kind::Text
    } else if cell.bg != Color::Named(NamedColor::Background) {
        Kind::Background
    } else {
        Kind::Empty
    }
}

/// 칸 종류를 `emit`에 맞춰 본다. 글만 낼 때(`Plain`)는 바탕색만 가진 칸도 빈 칸이다(Ghostty `formatStyled`가 거짓이면 `hasText`만 본다).
/// 탭 글자(`\t`)는 이동한 칸을 내지 않으려고 글자로 둔다: 복사는 지금까지 탭 글자를 남겼고 이동한 칸은 내지 않았다(Ghostty는 공백으로 낸다. 이번 범위 밖)
fn kind_for(cell: &Cell, emit: Emit) -> Kind {
    let k = kind(cell);
    if emit != Emit::Plain {
        return k;
    }
    match k {
        Kind::Background | Kind::Empty if cell.c == '\t' => Kind::Text,
        Kind::Background => Kind::Empty,
        k => k,
    }
}

fn col_of(c: Color) -> Col {
    match c {
        Color::Spec(v) => Col::Rgb(Rgb::new(v.r, v.g, v.b)),
        Color::Indexed(i) => Col::Palette(i),
        Color::Named(NamedColor::Foreground | NamedColor::Background) => Col::None,
        Color::Named(n) if (n as usize) < 16 => Col::Palette(n as u8),
        // 흐린 이름 색 등은 칸에 놓이지 않는다(흐림은 DIM 표시). 놓였다면 RGB로 푼다
        Color::Named(_) => Col::None,
    }
}

fn style_of(cell: &Cell) -> Style {
    match kind(cell) {
        Kind::Text => {
            let f = cell.flags;
            let underline = if f.contains(Flags::UNDERCURL) {
                Underline::Curly
            } else if f.contains(Flags::DOUBLE_UNDERLINE) {
                Underline::Double
            } else if f.contains(Flags::DOTTED_UNDERLINE) {
                Underline::Dotted
            } else if f.contains(Flags::DASHED_UNDERLINE) {
                Underline::Dashed
            } else if f.contains(Flags::UNDERLINE) {
                Underline::Single
            } else {
                Underline::None
            };
            Style {
                fg: col_of(cell.fg),
                bg: col_of(cell.bg),
                underline_color: cell.underline_color().map_or(Col::None, col_of),
                bold: f.contains(Flags::BOLD),
                italic: f.contains(Flags::ITALIC),
                faint: f.contains(Flags::DIM),
                blink: f.contains(Flags::BLINK),
                inverse: f.contains(Flags::INVERSE),
                invisible: f.contains(Flags::HIDDEN),
                strikethrough: f.contains(Flags::STRIKEOUT),
                overline: f.contains(Flags::OVERLINE),
                underline,
            }
        }
        // 바탕색 칸은 바탕색만 가진다(Ghostty `cellStyle`)
        Kind::Background => Style {
            bg: col_of(cell.bg),
            ..Style::default()
        },
        Kind::Spacer | Kind::Empty => Style::default(),
    }
}

struct Writer<'a, T> {
    out: String,
    term: &'a Term<T>,
    theme: &'a Theme,
    emit: Emit,
    map: &'a [CodepointRule],
    /// HTML 앵커가 열려 있다(막은 링크는 열지 않으므로 닫지도 않는다)
    anchor_open: bool,
}

impl<T> Writer<'_, T> {
    fn rgb(&self, c: Col) -> Option<Rgb> {
        match c {
            Col::None => None,
            Col::Rgb(v) => Some(v),
            Col::Palette(i) => Some(color::resolve(self.theme, self.term.colors(), Color::Indexed(i))),
        }
    }

    /// 스타일을 연다. VT는 늘 `ESC[0m`으로 시작하고(스타일은 스스로 완결), 꾸밈마다 따로 낸다(style.zig `VTFormatter`)
    fn style_open(&mut self, s: &Style) {
        match self.emit {
            Emit::Vt => {
                self.out.push_str("\x1b[0m");
                for (on, seq) in [
                    (s.bold, "\x1b[1m"),
                    (s.faint, "\x1b[2m"),
                    (s.italic, "\x1b[3m"),
                    (s.blink, "\x1b[5m"),
                    (s.inverse, "\x1b[7m"),
                    (s.invisible, "\x1b[8m"),
                    (s.strikethrough, "\x1b[9m"),
                    (s.overline, "\x1b[53m"),
                ] {
                    if on {
                        self.out.push_str(seq);
                    }
                }
                self.out.push_str(match s.underline {
                    Underline::None => "",
                    Underline::Single => "\x1b[4m",
                    Underline::Double => "\x1b[4:2m",
                    Underline::Curly => "\x1b[4:3m",
                    Underline::Dotted => "\x1b[4:4m",
                    Underline::Dashed => "\x1b[4:5m",
                });
                for (prefix, c) in [(38, s.fg), (48, s.bg), (58, s.underline_color)] {
                    if let Some(v) = self.rgb(c) {
                        self.out.push_str(&format!("\x1b[{prefix};2;{};{};{}m", v.r, v.g, v.b));
                    }
                }
            }
            // `display: inline`이라 div가 배치에 영향을 주지 않는다(CSS 감싸개일 뿐)
            Emit::Html => {
                self.out.push_str("<div style=\"display: inline;");
                for (prop, c) in [
                    ("color", s.fg),
                    ("background-color", s.bg),
                    ("text-decoration-color", s.underline_color),
                ] {
                    if let Some(v) = self.rgb(c) {
                        self.out.push_str(&format!("{prop}: rgb({}, {}, {});", v.r, v.g, v.b));
                    }
                }
                if s.underline != Underline::None || s.strikethrough || s.overline || s.blink {
                    self.out.push_str("text-decoration-line:");
                    if s.underline != Underline::None {
                        self.out.push_str(" underline");
                    }
                    if s.strikethrough {
                        self.out.push_str(" line-through");
                    }
                    if s.overline {
                        self.out.push_str(" overline");
                    }
                    if s.blink {
                        self.out.push_str(" blink");
                    }
                    self.out.push(';');
                }
                self.out.push_str(match s.underline {
                    Underline::None => "",
                    Underline::Single => "text-decoration-style: solid;",
                    Underline::Double => "text-decoration-style: double;",
                    Underline::Curly => "text-decoration-style: wavy;",
                    Underline::Dotted => "text-decoration-style: dotted;",
                    Underline::Dashed => "text-decoration-style: dashed;",
                });
                for (on, css) in [
                    (s.bold, "font-weight: bold;"),
                    (s.italic, "font-style: italic;"),
                    (s.faint, "opacity: 0.5;"),
                    (s.invisible, "visibility: hidden;"),
                    (s.inverse, "filter: invert(100%);"),
                ] {
                    if on {
                        self.out.push_str(css);
                    }
                }
                self.out.push_str("\">");
            }
            Emit::Plain => {}
        }
    }

    fn style_close(&mut self) {
        self.out.push_str(match self.emit {
            Emit::Vt => "\x1b[0m",
            Emit::Html => "</div>",
            Emit::Plain => "",
        });
    }

    /// 글자 하나. HTML은 특수 문자를 이스케이프하고 ASCII 밖은 숫자 참조로 낸다(인코딩 판별 문제를 피함, Ghostty #9426)
    fn codepoint(&mut self, c: char) {
        match self.emit {
            Emit::Plain | Emit::Vt => self.out.push(c),
            Emit::Html => match c {
                '<' => self.out.push_str("&lt;"),
                '>' => self.out.push_str("&gt;"),
                '&' => self.out.push_str("&amp;"),
                '"' => self.out.push_str("&quot;"),
                '\'' => self.out.push_str("&#39;"),
                c if (c as u32) < 0x80 => self.out.push(c),
                c => self.out.push_str(&format!("&#{};", c as u32)),
            },
        }
    }

    /// 칸의 글자 하나. 규칙이 있으면 바꾼 글(여러 글자일 수 있다)을 글자마다 `codepoint`로 낸다. 그래서 HTML에서는 바꾼 글도
    /// 이스케이프·숫자 참조가 되고, 스타일은 칸의 것을 그대로 쓴다(Ghostty `writeCodepointWithReplacement`)
    fn mapped_codepoint(&mut self, c: char) {
        let cp = c as u32;
        match self.map.iter().rev().find(|r| r.range.0 <= cp && cp <= r.range.1) {
            Some(r) => {
                for rc in r.replacement.chars() {
                    self.codepoint(rc);
                }
            }
            None => self.codepoint(c),
        }
    }

    fn link_open(&mut self, link: &Hyperlink) {
        match self.emit {
            Emit::Plain => {}
            // 명시한 id가 있으면 함께. 엔진이 id 없는 링크에 붙인 번호(`<n>_alacritty`)는 명시한 것이 아니다
            Emit::Vt => {
                let id = link.id();
                if is_implicit_id(id) {
                    self.out.push_str(&format!("\x1b]8;;{}\x1b\\", link.uri()));
                } else {
                    self.out.push_str(&format!("\x1b]8;id={id};{}\x1b\\", link.uri()));
                }
            }
            // 안전한 scheme(http·https·mailto)일 때만 앵커를 만든다. 아니면 링크 글만 낸다.
            // 주소는 글자마다 이스케이프한다(Ghostty는 바이트마다 해서 비ASCII가 깨지고 scheme도 거르지 않는다)
            Emit::Html => {
                if is_safe_scheme(link.uri()) {
                    self.out.push_str("<a href=\"");
                    for c in link.uri().chars() {
                        self.codepoint(c);
                    }
                    self.out.push_str("\">");
                    self.anchor_open = true;
                }
            }
        }
    }

    fn link_close(&mut self) {
        match self.emit {
            Emit::Plain => {}
            Emit::Vt => self.out.push_str("\x1b]8;;\x1b\\"),
            Emit::Html => {
                if self.anchor_open {
                    self.out.push_str("</a>");
                    self.anchor_open = false;
                }
            }
        }
    }
}

/// HTML 앵커로 만들어도 되는 주소인가: scheme이 http·https·mailto(대소문자 무시)일 때만.
/// `javascript:`·`data:`·`vbscript:` 같은 주소는 붙여넣은 편집기에서 실행될 수 있다
fn is_safe_scheme(uri: &str) -> bool {
    let lower = uri.to_ascii_lowercase();
    ["http:", "https:", "mailto:"].iter().any(|p| lower.starts_with(p))
}

/// 엔진이 id 없는 OSC 8에 붙이는 번호 id(`<숫자>_alacritty`)
fn is_implicit_id(id: &str) -> bool {
    id.strip_suffix("_alacritty")
        .is_some_and(|n| !n.is_empty() && n.bytes().all(|b| b.is_ascii_digit()))
}

/// 두 칸이 같은 바탕색만 가진 칸인가(Ghostty `sameBackground`)
fn same_background(a: &Cell, b: &Cell) -> bool {
    kind(a) == Kind::Background && kind(b) == Kind::Background && col_of(a.bg) == col_of(b.bg)
}

/// 칸 전체를 만든다(`write_screen_file`·`write_scrollback_file`의 `vt`·`html`, #768). `scrollback`이면 스크롤백 맨 위부터
/// 맨 아래까지, 아니면 지금 보이는 화면. 이어진 줄은 이어 붙이고(`unwrap`) 끝의 빈 줄은 내지 않는다
pub(crate) fn format_lines<T: EventListener>(
    term: &Term<T>,
    theme: &Theme,
    scrollback: bool,
    opts: Options<'_>,
) -> String {
    let grid = term.grid();
    let (first, last) = if scrollback {
        (term.topmost_line(), term.bottommost_line())
    } else {
        let off = grid.display_offset() as i32;
        (Line(-off), Line(term.screen_lines() as i32 - 1 - off))
    };
    let end_col = Column(term.columns().saturating_sub(1));
    format_range(
        term,
        theme,
        Point::new(first, Column(0)),
        Point::new(last, end_col),
        false,
        opts,
    )
}

/// 격자의 `start`~`end`(양 끝 포함)를 만든다. `block`이면 사각 고르기. 이어진 줄은 이어 붙인다(`unwrap`).
/// 줄 처리는 `PageFormatter.formatWithStateEmit`을 그대로 따른다
pub(crate) fn format_range<T: EventListener>(
    term: &Term<T>,
    theme: &Theme,
    start: Point,
    end: Point,
    block: bool,
    opts: Options<'_>,
) -> String {
    let cols = term.columns();
    let grid = term.grid();
    let (first, last) = (start.line, end.line);
    let start_x = start.column.0.min(cols - 1);
    let mut end_x = end.column.0.min(cols - 1);
    let mut last = last;
    // 끝 칸이 줄 끝의 앞 칸 자리표시면 다음 줄 첫 글자까지 넣는다(Ghostty `rightWrap`)
    if !block
        && grid[last][Column(end_x)]
            .flags
            .contains(Flags::LEADING_WIDE_CHAR_SPACER)
        && last < grid.bottommost_line()
    {
        last += 1;
        end_x = 0;
    }
    if first > last || (first == last && start_x > end_x) {
        return String::new();
    }

    let mut w = Writer {
        out: String::new(),
        term,
        theme,
        emit: opts.emit,
        map: opts.codepoint_map,
        anchor_open: false,
    };

    // 앞머리: 바탕·글자 기본색
    match opts.emit {
        Emit::Plain => {}
        Emit::Html => {
            w.out.push_str("<div style=\"font-family: monospace; white-space: pre;");
            if let Some(bg) = opts.background {
                w.out
                    .push_str(&format!("background-color: #{:02x}{:02x}{:02x};", bg.r, bg.g, bg.b));
            }
            if let Some(fg) = opts.foreground {
                w.out
                    .push_str(&format!("color: #{:02x}{:02x}{:02x};", fg.r, fg.g, fg.b));
            }
            w.out.push_str("\">");
        }
        Emit::Vt => {
            if let Some(fg) = opts.foreground {
                w.out
                    .push_str(&format!("\x1b]10;rgb:{:02x}/{:02x}/{:02x}\x1b\\", fg.r, fg.g, fg.b));
            }
            if let Some(bg) = opts.background {
                w.out
                    .push_str(&format!("\x1b]11;rgb:{:02x}/{:02x}/{:02x}\x1b\\", bg.r, bg.g, bg.b));
            }
        }
    }
    let newline = match opts.emit {
        Emit::Vt => "\r\n",
        Emit::Html | Emit::Plain => "\n",
    };
    let plain = opts.emit == Emit::Plain;

    let mut blank_rows = 0usize;
    let mut blank_cells = 0usize;
    let mut style = Style::default();
    let mut link: Option<Hyperlink> = None;

    let mut line = first;
    while line <= last {
        let row = &grid[line];
        let wrapped = row[Column(cols - 1)].flags.contains(Flags::WRAPLINE);
        let continuation =
            line > grid.topmost_line() && grid[Line(line.0 - 1)][Column(cols - 1)].flags.contains(Flags::WRAPLINE);
        let is_first = line == first;

        // 이 줄에서 읽을 칸의 범위
        let row_end = if block || line == last { end_x + 1 } else { cols };
        let mut row_start = 0;
        if start_x > 0 && (block || is_first) {
            match kind_for(&row[Column(start_x)], opts.emit) {
                // 넓은 글자의 뒤 칸에서 시작하면 앞 칸부터
                Kind::Spacer if row[Column(start_x)].flags.contains(Flags::WIDE_CHAR_SPACER) => row_start = start_x - 1,
                // 줄 끝의 앞 칸 자리표시에서 시작하면 이 줄은 건너뛴다
                Kind::Spacer => {
                    line += 1;
                    continue;
                }
                _ => row_start = start_x,
            }
        }
        let cells: Vec<&Cell> = (row_start..row_end).map(|c| &row[Column(c)]).collect();

        // 글자 없는 줄은 모았다가, 뒤에 글자 있는 줄이 오면 줄바꿈으로 낸다. VT는 바탕색만 가진 줄도 글자 줄처럼 둔다
        let has_text = cells.iter().any(|c| kind_for(c, opts.emit) == Kind::Text);
        let has_bg = cells.iter().any(|c| kind_for(c, opts.emit) == Kind::Background);
        let keeps = has_text || (opts.emit == Emit::Vt && has_bg);
        if !keeps {
            blank_rows += 1;
            line += 1;
            continue;
        }

        if blank_rows > 0 {
            // 줄바꿈 앞에서 스타일을 닫는다: 바탕색이 다음 줄의 앞 칸으로 번지지 않게
            if style != Style::default() {
                w.style_close();
                style = Style::default();
            }
            for _ in 0..blank_rows {
                w.out.push_str(newline);
            }
            blank_rows = 0;
        }

        // 이어진 줄이 아니면 줄바꿈을 더한다
        if !wrapped {
            blank_rows += 1;
        }
        if !continuation {
            blank_cells = 0;
        }

        let mut i = 0;
        // 글만 낼 때 탭 글자 바로 뒤의 빈 칸은 탭이 건넌 칸이라 내지 않는다
        let mut after_tab = false;
        while i < cells.len() {
            let cell = cells[i];
            let k = kind_for(cell, opts.emit);
            if k == Kind::Spacer {
                i += 1;
                continue;
            }
            // 글자 없는 칸은 센다. 뒤에 글자가 오면 그때 낸다
            if k == Kind::Empty {
                if !after_tab {
                    blank_cells += 1;
                }
                i += 1;
                continue;
            }
            after_tab = plain && cell.c == '\t' && !cell.flags.contains(Flags::WRITTEN);
            // 끝 공백은 `trim`이면 빈 칸으로 센다(Ghostty: 뒤에 글자가 오면 그때 공백으로 내고, 끝이면 버린다)
            if plain && opts.trim && k == Kind::Text && cell.c == ' ' && cell.zerowidth().is_none() {
                blank_cells += 1;
                i += 1;
                continue;
            }

            if blank_cells > 0 {
                if opts.emit == Emit::Vt {
                    if blank_cells <= i {
                        // 이 줄 안의 빈 칸은 커서로 건너뛴다. 공백으로 쓰면 앞 글자의 펜으로 글자가 된다
                        w.out.push_str(&format!("\x1b[{blank_cells}C"));
                        blank_cells = 0;
                    } else if style != Style::default() {
                        // 앞 줄에서 이어진 빈 칸은 CUF가 줄 끝에서 멈춰 공백으로 쓴다. 앞 글자의 펜 없이
                        w.style_close();
                        style = Style::default();
                    }
                }
                if blank_cells > 0 {
                    // 빈 칸에는 하이퍼링크가 없다. VT에서 열린 링크는 공백에도 걸리므로 먼저 닫는다
                    if opts.emit == Emit::Vt && link.is_some() {
                        w.link_close();
                        link = None;
                    }
                    for _ in 0..blank_cells {
                        w.out.push(' ');
                    }
                    blank_cells = 0;
                }
            }

            // 스타일이 바뀌었나
            let cell_style = if plain { Style::default() } else { style_of(cell) };
            if cell_style != style {
                if style != Style::default() {
                    match opts.emit {
                        Emit::Html => w.style_close(),
                        Emit::Plain => {}
                        // VT는 기본 스타일로 돌아갈 때만 닫는다. 다른 스타일의 시작이 `ESC[0m`이다
                        Emit::Vt => {
                            if cell_style == Style::default() {
                                w.style_close();
                            }
                        }
                    }
                }
                style = cell_style;
                if style != Style::default() {
                    w.style_open(&style);
                }
            }

            // 하이퍼링크가 바뀌었나
            let cell_link = if k == Kind::Text && !plain {
                cell.hyperlink()
            } else {
                None
            };
            if cell_link != link {
                // HTML은 늘 닫고 연다. VT는 새 링크를 열면 열려 있던 것이 바뀌므로 링크가 끝날 때만 닫는다
                if link.is_some() && (opts.emit == Emit::Html || cell_link.is_none()) {
                    w.link_close();
                }
                if let Some(l) = &cell_link {
                    w.link_open(l);
                }
                link = cell_link;
            }

            match k {
                Kind::Text => {
                    w.mapped_codepoint(cell.c);
                    for z in cell.zerowidth().into_iter().flatten() {
                        w.mapped_codepoint(*z);
                    }
                }
                Kind::Background => {
                    if opts.emit == Emit::Vt {
                        // VT는 바탕색으로 지운 칸을 그대로 되살린다(ECH로 지우고 CUF로 건넘). 같은 바탕색이 이어지면 한 번에
                        let mut run = 1;
                        while i + run < cells.len() && same_background(cell, cells[i + run]) {
                            run += 1;
                        }
                        w.out.push_str(&format!("\x1b[{run}X\x1b[{run}C"));
                        i += run;
                        continue;
                    }
                    // HTML은 바탕색 스타일 안의 공백
                    w.out.push(' ');
                }
                Kind::Spacer | Kind::Empty => {}
            }
            i += 1;
        }
        line += 1;
    }

    if style != Style::default() {
        w.style_close();
    }
    if link.is_some() {
        w.link_close();
    }
    if opts.emit == Emit::Html {
        w.out.push_str("</div>");
    } else if opts.emit == Emit::Vt && !opts.trim && blank_rows > 1 {
        // VT는 끝의 빈 줄을 자리로 남긴다(`trim = false`)
        for _ in 0..blank_rows - 1 {
            w.out.push_str("\r\n");
        }
    }
    w.out
}

/// 시험은 Ghostty `formatter.zig`의 시험을 같은 입력 바이트·같은 기대 출력으로 옮긴 것이다(시험 이름 위에 원본 이름).
/// 원본은 팔레트를 주지 않으면 `var(--vt-palette-N)`·`38;5;N`을 쓰지만 복사 길은 늘 팔레트를 주므로 RGB로 바꿔 적었다.
/// 격자는 `Term`을 직접 만들어 읽는다(PTY 없음)
#[cfg(test)]
mod tests {
    use super::*;
    use alacritty_terminal::index::Column;
    use alacritty_terminal::term::Config;
    use alacritty_terminal::vte::ansi::Processor;

    struct Dims(usize, usize);
    impl Dimensions for Dims {
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

    #[derive(Clone, Copy)]
    struct Quiet;
    impl EventListener for Quiet {}

    fn term(cols: usize, rows: usize, input: &str) -> Term<Quiet> {
        let mut t = Term::new(Config::default(), &Dims(cols, rows), Quiet);
        let mut p: Processor = Processor::new();
        p.advance(&mut t, input.as_bytes());
        t
    }

    fn opts(emit: Emit) -> Options<'static> {
        Options {
            emit,
            trim: true,
            background: None,
            foreground: None,
            codepoint_map: &[],
        }
    }

    /// 화면 전체(80x24)를 만든다
    fn all(input: &str, o: Options<'_>) -> String {
        all_in(80, 24, input, o)
    }

    fn all_in(cols: usize, rows: usize, input: &str, o: Options<'_>) -> String {
        let t = term(cols, rows, input);
        format_range(
            &t,
            &Theme::default(),
            Point::new(Line(0), Column(0)),
            Point::new(Line(rows as i32 - 1), Column(cols - 1)),
            false,
            o,
        )
    }

    fn html(input: &str) -> String {
        all(input, opts(Emit::Html))
    }

    /// `write_screen_file:…,vt|html`·`write_scrollback_file:…,vt|html`(#768): 칸 전체와 스크롤백 전체를 꾸밈과 함께 만든다
    #[test]
    fn format_lines_covers_the_screen_and_the_scrollback() {
        // 3줄 화면에 5줄을 쓰면 윗 2줄은 스크롤백으로 밀린다
        let t = term(10, 3, "l1\r\nl2\r\n\x1b[1ml3\x1b[0m\r\nl4\r\nl5");
        let o = |emit| Options {
            trim: false,
            ..opts(emit)
        };
        let screen = format_lines(&t, &Theme::default(), false, o(Emit::Vt));
        assert!(screen.contains("l4") && screen.contains("l5"), "{screen:?}");
        assert!(
            !screen.contains("l1") && !screen.contains("l2"),
            "보이는 화면만: {screen:?}"
        );
        let all = format_lines(&t, &Theme::default(), true, o(Emit::Vt));
        assert!(all.contains("l1") && all.contains("l5"), "{all:?}");
        assert!(all.contains("\x1b[1m"), "꾸밈이 든다: {all:?}");
        let h = format_lines(&t, &Theme::default(), true, o(Emit::Html));
        assert!(h.contains("<div") && h.contains("l1") && h.contains("l5"), "{h:?}");
    }

    fn vt(input: &str) -> String {
        all(input, opts(Emit::Vt))
    }

    const WRAP: &str = "<div style=\"font-family: monospace; white-space: pre;\">";

    /// 기본 테마 팔레트 i번의 `r;g;b`
    fn semi(i: usize) -> String {
        let c = Theme::default().palette[i];
        format!("{};{};{}", c.r, c.g, c.b)
    }

    /// 기본 테마 팔레트 i번의 `r, g, b`
    fn comma(i: usize) -> String {
        let c = Theme::default().palette[i];
        format!("{}, {}, {}", c.r, c.g, c.b)
    }

    // "Page html with multiple styles"
    #[test]
    fn html_multiple_styles() {
        assert_eq!(
            html("\x1b[1mbold\x1b[3mitalic\x1b[0mnormal"),
            format!(
                "{WRAP}<div style=\"display: inline;font-weight: bold;\">bold</div>\
                 <div style=\"display: inline;font-weight: bold;font-style: italic;\">italic</div>normal</div>"
            )
        );
    }

    // "Page html plain text"·"Page html ascii characters unchanged"
    #[test]
    fn html_plain_text() {
        assert_eq!(html("hello, world"), format!("{WRAP}hello, world</div>"));
        assert_eq!(html("hello world"), format!("{WRAP}hello world</div>"));
    }

    /// Ghostty `Page codepoint_map with styled formats`(`formatter.zig`): 'e'→'X'. 원본 기대는 `\x1b[0m\x1b[38;5;1mrXd tXxt\x1b[0m`(팔레트를 안 줌).
    /// 복사 길은 팔레트를 주므로 RGB로 바꿔 적었다. 10열 화면(#787)
    #[test]
    fn codepoint_map_applies_to_vt_keeping_styles() {
        let rules = [rule('e', 'e', "X")];
        let o = Options {
            codepoint_map: &rules,
            ..opts(Emit::Vt)
        };
        assert_eq!(
            all_in(10, 24, "\x1b[31mred text\x1b[0m", o),
            format!("\x1b[0m\x1b[38;2;{}mrXd tXxt\x1b[0m", semi(1))
        );
    }

    /// 같은 입력을 html로: 스타일은 그대로, 글자만 바뀐다
    #[test]
    fn codepoint_map_applies_to_html_keeping_styles() {
        let rules = [rule('e', 'e', "X")];
        let o = Options {
            codepoint_map: &rules,
            ..opts(Emit::Html)
        };
        assert_eq!(
            all_in(10, 24, "\x1b[31mred text\x1b[0m", o),
            format!(
                "{WRAP}<div style=\"display: inline;color: rgb({});\">rXd tXxt</div></div>",
                comma(1)
            )
        );
    }

    /// Ghostty는 바꾼 글도 `writeCodepoint`로 내므로 HTML에서 이스케이프·숫자 참조가 되고, 여러 글자도 된다.
    /// 겹치면 나중 규칙이 이기고(`prefers last`), 범위도 먹고(`range`·`multiple ranges`), 글자 아닌 칸에는 걸리지 않는다
    #[test]
    fn codepoint_map_replacement_is_escaped_in_html_and_last_rule_wins() {
        let rules = [
            rule('a', 'm', "A"),
            rule('o', 'o', "x"),
            rule('o', 'o', "<&>"),
            rule('\u{26a1}', '\u{26a1}', "\u{1f525}"),
        ];
        let h = Options {
            codepoint_map: &rules,
            ..opts(Emit::Html)
        };
        assert_eq!(
            all("hello \u{26a1} world", h),
            format!("{WRAP}AAAA&lt;&amp;&gt; &#128293; w&lt;&amp;&gt;rAA</div>")
        );
        let v = Options {
            codepoint_map: &rules,
            ..opts(Emit::Vt)
        };
        assert_eq!(all("hello \u{26a1} world", v), "AAAA<&> \u{1f525} w<&>rAA");
    }

    /// 비어 있는 맵은 바꾸지 않는다(`empty map`). 빈 글로 바꾸면 그 글자가 지워진다
    #[test]
    fn codepoint_map_empty_changes_nothing_and_empty_replacement_deletes() {
        assert_eq!(all("a e", opts(Emit::Vt)), "a e");
        let rules = [rule('e', 'e', "")];
        let v = Options {
            codepoint_map: &rules,
            ..opts(Emit::Vt)
        };
        assert_eq!(all("a e", v), "a ");
    }

    /// 결합 글자에도 걸리고, 하이퍼링크 주소에는 걸리지 않는다
    #[test]
    fn codepoint_map_applies_to_combining_marks_but_not_link_uris() {
        let rules = [rule('e', 'e', "X"), rule('\u{301}', '\u{301}', "!")];
        let h = Options {
            codepoint_map: &rules,
            ..opts(Emit::Html)
        };
        assert_eq!(
            all("e\u{301}\x1b]8;;http://e.com\x1b\\e\x1b]8;;\x1b\\", h),
            format!("{WRAP}X!<a href=\"http://e.com\">X</a></div>")
        );
    }

    fn rule(a: char, b: char, to: &str) -> CodepointRule {
        CodepointRule {
            range: (a as u32, b as u32),
            replacement: to.to_string(),
        }
    }

    // "Page html with colors": 원본은 var(--vt-palette-1)·var(--vt-palette-4)
    #[test]
    fn html_colors_are_palette_rgb() {
        assert_eq!(
            html("\x1b[31;44mcolored"),
            format!(
                "{WRAP}<div style=\"display: inline;color: rgb({});background-color: rgb({});\">colored</div></div>",
                comma(1),
                comma(4)
            )
        );
    }

    // "Page html with palette option emits RGB": OSC 4로 바꾼 팔레트가 따라온다
    #[test]
    fn html_palette_follows_osc4() {
        assert_eq!(
            html("\x1b]4;1;rgb:ab/cd/ef\x1b\\\x1b[31mred"),
            format!("{WRAP}<div style=\"display: inline;color: rgb(171, 205, 239);\">red</div></div>")
        );
    }

    // "Page html with background and foreground colors"
    #[test]
    fn html_background_and_foreground_header() {
        let o = Options {
            background: Some(Rgb::new(0x12, 0x34, 0x56)),
            foreground: Some(Rgb::new(0xab, 0xcd, 0xef)),
            ..opts(Emit::Html)
        };
        assert_eq!(
            all("hello", o),
            "<div style=\"font-family: monospace; white-space: pre;background-color: #123456;color: #abcdef;\">hello</div>"
        );
    }

    // "Page html with escaping"
    #[test]
    fn html_escapes_special_characters() {
        assert_eq!(
            html("<tag>&\"'text"),
            format!("{WRAP}&lt;tag&gt;&amp;&quot;&#39;text</div>")
        );
    }

    // "Page html with unicode as numeric entities"·"Page html mixed ascii and unicode"
    #[test]
    fn html_unicode_is_numeric_entities() {
        assert_eq!(html("╰─ ❯"), format!("{WRAP}&#9584;&#9472; &#10095;</div>"));
        assert_eq!(
            html("test ╰─❯ ok"),
            format!("{WRAP}test &#9584;&#9472;&#10095; ok</div>")
        );
    }

    // "Page html trailing blank lines"
    #[test]
    fn html_trailing_blank_lines_are_dropped() {
        assert_eq!(html("hello\r\nworld\r\n\r\n"), format!("{WRAP}hello\nworld</div>"));
    }

    // "Page html with hyperlinks"
    #[test]
    fn html_hyperlink() {
        assert_eq!(
            html("\x1b]8;;https://example.com\x1b\\link text\x1b]8;;\x1b\\ normal"),
            format!("{WRAP}<a href=\"https://example.com\">link text</a> normal</div>")
        );
    }

    // "Page HTML with multiple hyperlinks"
    #[test]
    fn html_multiple_hyperlinks() {
        assert_eq!(
            html(
                "\x1b]8;;https://first.com\x1b\\first\x1b]8;;\x1b\\ \x1b]8;;https://second.com\x1b\\second\x1b]8;;\x1b\\"
            ),
            format!("{WRAP}<a href=\"https://first.com\">first</a> <a href=\"https://second.com\">second</a></div>")
        );
    }

    // "Page HTML with hyperlink escaping"
    #[test]
    fn html_hyperlink_escapes_the_uri() {
        assert_eq!(
            html("\x1b]8;;https://example.com?a=1&b=2\x1b\\link\x1b]8;;\x1b\\"),
            format!("{WRAP}<a href=\"https://example.com?a=1&amp;b=2\">link</a></div>")
        );
    }

    // "Page HTML with styled hyperlink"·"Page HTML hyperlink closes style before anchor"
    // (Ghostty 그대로 태그가 겹쳐 닫힌다: 스타일은 링크 안에서 닫힌다)
    #[test]
    fn html_styled_hyperlink_closes_style_inside_the_anchor() {
        assert_eq!(
            html("\x1b]8;;https://example.com\x1b\\\x1b[1mbold link\x1b[0m\x1b]8;;\x1b\\"),
            format!(
                "{WRAP}<div style=\"display: inline;font-weight: bold;\"><a href=\"https://example.com\">bold link</div></a></div>"
            )
        );
        assert_eq!(
            html("\x1b]8;;https://example.com\x1b\\\x1b[1mbold\x1b[0m plain"),
            format!(
                "{WRAP}<div style=\"display: inline;font-weight: bold;\"><a href=\"https://example.com\">bold</div> plain</a></div>"
            )
        );
    }

    // 보안: 프로그램이 심은 주소가 붙여넣은 편집기에서 실행되지 않게 http·https·mailto만 앵커로 만든다(Ghostty는 거르지 않는다)
    #[test]
    fn html_anchor_only_for_safe_schemes() {
        for uri in [
            "http://a.example/x",
            "https://a.example/x",
            "mailto:a@b.example",
            "HTTPS://A.EXAMPLE",
            "MailTo:a@b",
        ] {
            let out = html(&format!("\x1b]8;;{uri}\x1b\\link\x1b]8;;\x1b\\"));
            assert_eq!(out, format!("{WRAP}<a href=\"{uri}\">link</a></div>"), "{uri}");
        }
        for uri in [
            "javascript:alert(1)",
            "JaVaScRiPt:alert(1)",
            "data:text/html,<b>x</b>",
            "vbscript:x",
            "file:///etc/passwd",
            "//evil.example/x",
            "/relative",
            " javascript:alert(1)",
        ] {
            let out = html(&format!("\x1b]8;;{uri}\x1b\\link\x1b]8;;\x1b\\ ok"));
            assert_eq!(out, format!("{WRAP}link ok</div>"), "앵커 없이 글만: {uri}");
        }
        // 막힌 링크가 스타일과 섞여도 짝이 맞는다
        assert_eq!(
            html("\x1b]8;;javascript:x\x1b\\\x1b[1mb\x1b[0m\x1b]8;;\x1b\\"),
            format!("{WRAP}<div style=\"display: inline;font-weight: bold;\">b</div></div>")
        );
    }

    // 주소가 비ASCII여도 글자가 깨지지 않고, 속성값 안의 특수 문자는 이스케이프된다
    #[test]
    fn html_anchor_uri_keeps_unicode_and_escapes_attribute_characters() {
        assert_eq!(
            html("\x1b]8;;https://a.example/한글\x1b\\x\x1b]8;;\x1b\\"),
            format!("{WRAP}<a href=\"https://a.example/&#54620;&#44544;\">x</a></div>")
        );
        assert_eq!(
            html("\x1b]8;;https://a.example/?q=\"><script>'&\x1b\\x\x1b]8;;\x1b\\"),
            format!("{WRAP}<a href=\"https://a.example/?q=&quot;&gt;&lt;script&gt;&#39;&amp;\">x</a></div>")
        );
    }

    // "Page VT single line plain text"
    #[test]
    fn vt_plain_text() {
        assert_eq!(vt("hello"), "hello");
    }

    // "Page VT single line with bold"·"Page VT style reset properly closes styles"
    #[test]
    fn vt_bold() {
        assert_eq!(vt("\x1b[1mhello\x1b[0m"), "\x1b[0m\x1b[1mhello\x1b[0m");
        assert_eq!(vt("\x1b[1mbold\x1b[0mnormal"), "\x1b[0m\x1b[1mbold\x1b[0mnormal");
    }

    // "Page VT multiple styles"
    #[test]
    fn vt_multiple_styles() {
        assert_eq!(
            vt("\x1b[1mhello \x1b[3mworld\x1b[0m"),
            "\x1b[0m\x1b[1mhello \x1b[0m\x1b[1m\x1b[3mworld\x1b[0m"
        );
    }

    // "Page VT with foreground color": 원본은 \x1b[38;5;1m
    #[test]
    fn vt_foreground_is_palette_rgb() {
        assert_eq!(
            vt("\x1b[31mred\x1b[0m"),
            format!("\x1b[0m\x1b[38;2;{}mred\x1b[0m", semi(1))
        );
        // "Page VT with palette option emits RGB"
        assert_eq!(
            vt("\x1b]4;1;rgb:ab/cd/ef\x1b\\\x1b[31mred"),
            "\x1b[0m\x1b[38;2;171;205;239mred\x1b[0m"
        );
    }

    // "Page VT with background and foreground colors"
    #[test]
    fn vt_background_and_foreground_header() {
        let o = Options {
            background: Some(Rgb::new(0x12, 0x34, 0x56)),
            foreground: Some(Rgb::new(0xab, 0xcd, 0xef)),
            ..opts(Emit::Vt)
        };
        assert_eq!(
            all("hello", o),
            "\x1b]10;rgb:ab/cd/ef\x1b\\\x1b]11;rgb:12/34/56\x1b\\hello"
        );
    }

    // "Page VT multi-line with styles": 줄바꿈 앞에서 스타일을 닫는다
    #[test]
    fn vt_multi_line_resets_style_before_newline() {
        assert_eq!(
            vt("\x1b[1mfirst\x1b[0m\r\n\x1b[3msecond\x1b[0m"),
            "\x1b[0m\x1b[1mfirst\x1b[0m\r\n\x1b[0m\x1b[3msecond\x1b[0m"
        );
    }

    // "Page VT duplicate style not emitted twice"
    #[test]
    fn vt_same_style_is_not_emitted_twice() {
        assert_eq!(vt("\x1b[1mhel\x1b[1mlo\x1b[0m"), "\x1b[0m\x1b[1mhello\x1b[0m");
    }

    // "Page VT background color on trailing blank cells"·"Page VT erases cells that only have a background"
    // 바탕색으로 지운 칸(글자 없음)은 ECH로 지우고 CUF로 건너뛴다
    #[test]
    fn vt_background_only_cells_are_erased_then_stepped_over() {
        assert_eq!(
            vt("CPU:\x1b[41m\x1b[K\x1b[0m"),
            format!("CPU:\x1b[0m\x1b[48;2;{}m\x1b[76X\x1b[76C\x1b[0m", semi(1))
        );
    }

    // "Page VT steps over a gap": 커서로 건너뛴 빈 칸은 CUF
    #[test]
    fn vt_steps_over_gap() {
        assert_eq!(
            vt("\x1b[41mred\x1b[0m\x1b[10Gplain"),
            format!("\x1b[0m\x1b[48;2;{}mred\x1b[6C\x1b[0mplain", semi(1))
        );
    }

    // "Page VT keeps rows that only have a background"
    #[test]
    fn vt_keeps_rows_that_only_have_a_background() {
        assert_eq!(
            all_in(5, 2, "\x1b[44m\x1b[2J\x1b[Hab\x1b[0m", opts(Emit::Vt)),
            format!(
                "\x1b[0m\x1b[48;2;{0}mab\x1b[3X\x1b[3C\x1b[0m\r\n\x1b[0m\x1b[48;2;{0}m\x1b[5X\x1b[5C\x1b[0m",
                semi(4)
            )
        );
    }

    // "Page VT unwrap writes blanks before a wrapped wide character"(폭 10)
    #[test]
    fn vt_unwrap_writes_blanks_before_a_wrapped_wide_character() {
        assert_eq!(
            all_in(10, 2, "\x1b[41mabc\x1b[0m\x1b[10G中", opts(Emit::Vt)),
            format!("\x1b[0m\x1b[48;2;{}mabc\x1b[0m      中", semi(1))
        );
    }

    // "Page VT with hyperlinks"·"Page VT with explicit hyperlink id"
    #[test]
    fn vt_hyperlink() {
        assert_eq!(
            vt("\x1b]8;;https://example.com\x1b\\link text\x1b]8;;\x1b\\ normal"),
            "\x1b]8;;https://example.com\x1b\\link text\x1b]8;;\x1b\\ normal"
        );
        assert_eq!(
            vt("\x1b]8;id=a;https://example.com\x1b\\one\x1b]8;;\x1b\\"),
            "\x1b]8;id=a;https://example.com\x1b\\one\x1b]8;;\x1b\\"
        );
    }

    // "Page VT with hyperlink across rows"·"Page VT closes a hyperlink on a row without hyperlinks"
    #[test]
    fn vt_hyperlink_stays_open_across_rows() {
        assert_eq!(
            vt("\x1b]8;;https://example.com\x1b\\ab\r\ncd\x1b]8;;\x1b\\"),
            "\x1b]8;;https://example.com\x1b\\ab\r\ncd\x1b]8;;\x1b\\"
        );
        assert_eq!(
            vt("\x1b]8;;https://example.com\x1b\\ab\x1b]8;;\x1b\\\r\ncd"),
            "\x1b]8;;https://example.com\x1b\\ab\r\n\x1b]8;;\x1b\\cd"
        );
    }

    // "Page VT steps over blank cells in a hyperlink"
    #[test]
    fn vt_steps_over_blank_cells_in_a_hyperlink() {
        assert_eq!(
            vt("\x1b]8;;https://example.com\x1b\\ab\x1b[5Gcd\x1b]8;;\x1b\\"),
            "\x1b]8;;https://example.com\x1b\\ab\x1b[2Ccd\x1b]8;;\x1b\\"
        );
    }

    // "Page VT unwrap closes a hyperlink before blank cells"(폭 10)
    #[test]
    fn vt_unwrap_closes_a_hyperlink_before_blank_cells() {
        assert_eq!(
            all_in(
                10,
                2,
                "\x1b]8;;https://example.com\x1b\\abc\x1b[10G中\x1b]8;;\x1b\\",
                opts(Emit::Vt)
            ),
            "\x1b]8;;https://example.com\x1b\\abc\x1b]8;;\x1b\\      \x1b]8;;https://example.com\x1b\\中\x1b]8;;\x1b\\"
        );
    }

    // "Page VT switches between hyperlinks without closing"
    #[test]
    fn vt_switches_between_hyperlinks_without_closing() {
        assert_eq!(
            vt("\x1b]8;;https://a.example\x1b\\ab\x1b]8;;https://b.example\x1b\\cd\x1b]8;;\x1b\\"),
            "\x1b]8;;https://a.example\x1b\\ab\x1b]8;;https://b.example\x1b\\cd\x1b]8;;\x1b\\"
        );
    }

    // "PageList VT without trim keeps trailing blank rows"(6줄)·"...keeps a blank screen"(4줄)
    #[test]
    fn vt_without_trim_keeps_trailing_blank_rows() {
        let keep = Options {
            trim: false,
            ..opts(Emit::Vt)
        };
        assert_eq!(all_in(80, 6, "one\r\ntwo", keep), "one\r\ntwo\r\n\r\n\r\n\r\n");
        assert_eq!(
            all_in(80, 6, "one\r\ntwo", opts(Emit::Vt)),
            "one\r\ntwo",
            "trim이면 끝 빈 줄은 뺀다"
        );
        assert_eq!(all_in(80, 4, "", keep), "\r\n\r\n\r\n");
    }

    // 우리 쪽: 꾸밈 하나하나의 모습(style.zig `formatterVt`·`formatterHtml`)
    #[test]
    fn every_attribute_in_vt_and_html() {
        let sgr = "\x1b[1;2;3;5;7;8;9;53;4:3;58;2;1;2;3mx";
        assert_eq!(
            vt(sgr),
            "\x1b[0m\x1b[1m\x1b[2m\x1b[3m\x1b[5m\x1b[7m\x1b[8m\x1b[9m\x1b[53m\x1b[4:3m\x1b[58;2;1;2;3mx\x1b[0m"
        );
        assert_eq!(
            html(sgr),
            format!(
                "{WRAP}<div style=\"display: inline;text-decoration-color: rgb(1, 2, 3);\
                 text-decoration-line: underline line-through overline blink;text-decoration-style: wavy;\
                 font-weight: bold;font-style: italic;opacity: 0.5;visibility: hidden;filter: invert(100%);\">x</div></div>"
            )
        );
        for (code, vt_seq, css) in [
            ("4", "\x1b[4m", "solid"),
            ("4:2", "\x1b[4:2m", "double"),
            ("4:4", "\x1b[4:4m", "dotted"),
            ("4:5", "\x1b[4:5m", "dashed"),
        ] {
            assert_eq!(vt(&format!("\x1b[{code}mx")), format!("\x1b[0m{vt_seq}x\x1b[0m"));
            assert_eq!(
                html(&format!("\x1b[{code}mx")),
                format!(
                    "{WRAP}<div style=\"display: inline;text-decoration-line: underline;text-decoration-style: {css};\">x</div></div>"
                )
            );
        }
        // 24비트 색과 256색
        assert_eq!(vt("\x1b[38;2;10;20;30;48;5;200mx"), {
            let c = Theme::default().indexed(200);
            format!("\x1b[0m\x1b[38;2;10;20;30m\x1b[48;2;{};{};{}mx\x1b[0m", c.r, c.g, c.b)
        });
    }

    // 고른 영역: 한 줄의 가운데, 여러 줄, 사각, 넓은 글자의 뒤 칸에서 시작
    #[test]
    fn selection_ranges() {
        let t = term(10, 4, "abcdef\r\nghijkl\r\nmnopqr\r\n");
        let th = Theme::default();
        let run = |s: (i32, usize), e: (i32, usize), block: bool| {
            format_range(
                &t,
                &th,
                Point::new(Line(s.0), Column(s.1)),
                Point::new(Line(e.0), Column(e.1)),
                block,
                opts(Emit::Html),
            )
        };
        assert_eq!(run((0, 1), (0, 3), false), format!("{WRAP}bcd</div>"));
        assert_eq!(run((0, 4), (2, 1), false), format!("{WRAP}ef\nghijkl\nmn</div>"));
        assert_eq!(run((0, 1), (2, 3), true), format!("{WRAP}bcd\nhij\nnop</div>"));

        let w = term(10, 2, "a中b");
        let r = format_range(
            &w,
            &th,
            Point::new(Line(0), Column(2)),
            Point::new(Line(0), Column(3)),
            false,
            opts(Emit::Html),
        );
        assert_eq!(r, format!("{WRAP}&#20013;b</div>"), "뒤 칸에서 시작해도 글자 전체");
    }
}
