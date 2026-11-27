//! vi 복사 모드(#930): 키보드만으로 스크롤백을 돌아다니며 글을 고르고 복사한다.
//! 기준: WezTerm CopyMode(h j k l, 0 $, g G, Ctrl-u/d, v V Ctrl-v, y, Esc·q), Alacritty vi mode, iTerm2 copy mode.
//!
//! 이 모듈은 엔진(`Term`)만 보는 순수한 부분이다(PTY·창이 없다). 키를 어떻게 받아 어떤 `CopyOp`로 바꾸는지는 앱의 몫이다.
//! - 커서와 고르기 시작점은 화면 좌표가 아니라 줄의 안정된 번호(`scrolled_total` + 줄)와 칸으로 쥔다. 출력이 와서 화면이
//!   밀리거나 스크롤백이 가득 차 줄이 밀려나도 같은 줄을 가리키고, 그 줄이 스크롤백에서 사라지면 맨 위 줄로 붙는다
//! - 커서는 넓은 글자의 오른쪽 반(뒤 칸)에 서지 않는다. 늘 앞 칸으로 붙인다
//! - 위아래 움직임은 "바라는 칸"(`Goal`)을 기억해 짧은 줄을 지나도 원래 칸으로 돌아온다. `$`는 줄마다 글자 끝을 바란다

use alacritty_terminal::event::EventListener;
use alacritty_terminal::grid::{Dimensions, Scroll};
use alacritty_terminal::index::{Column, Line, Point, Side};
use alacritty_terminal::selection::{Selection, SelectionType};
use alacritty_terminal::term::Term;
use alacritty_terminal::term::cell::Flags;

/// 복사 모드에서 하는 일 하나(키 테이블의 `copy_mode:<이름>` 동작)
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CopyOp {
    Left,
    Right,
    Up,
    Down,
    /// 줄 맨 앞 칸(`0`)
    LineStart,
    /// 줄의 글자 끝(`$`)
    LineEnd,
    /// 스크롤백 맨 위(`g`)
    Top,
    /// 맨 아래 줄(`G`)
    Bottom,
    HalfPageUp,
    HalfPageDown,
    PageUp,
    PageDown,
    /// 글자 단위 고르기 켜고 끄기(`v`)
    SelectCell,
    /// 줄 단위 고르기(`V`)
    SelectLine,
    /// 사각 고르기(`Ctrl-v`)
    SelectBlock,
}

/// 지금 하는 고르기의 종류
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CopySelect {
    Cell,
    Line,
    Block,
}

/// 위아래로 움직일 때 바라는 칸
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Goal {
    Col(usize),
    /// `$` 뒤: 가는 줄마다 글자 끝
    End,
}

/// 줄의 안정된 번호와 칸
type Pos = (i64, usize);

/// 복사 모드 하나의 상태(칸마다 하나)
#[derive(Debug, Clone)]
pub struct CopyMode {
    cursor: Pos,
    goal: Goal,
    anchor: Option<(Pos, CopySelect)>,
    /// 들어갈 때 맨 아래에서 올라가 있던 줄 수. 나올 때 이 자리로 돌린다
    saved_offset: usize,
}

fn stable_id<T>(term: &Term<T>, line: Line) -> i64 {
    term.grid().scrolled_total() as i64 + line.0 as i64
}

/// 안정된 번호가 가리키는 줄. 스크롤백에서 밀려났으면 맨 위 줄, 화면 아래를 넘으면 맨 아래 줄
fn line_of<T>(term: &Term<T>, id: i64) -> Line {
    let raw = id - term.grid().scrolled_total() as i64;
    let raw = raw.clamp(term.topmost_line().0 as i64, term.bottommost_line().0 as i64);
    Line(raw as i32)
}

fn last_col<T>(term: &Term<T>) -> usize {
    term.columns().saturating_sub(1)
}

/// 넓은 글자의 뒤 칸(과 줄 끝에서 넘어간 글자의 앞 자리 칸)이면 앞 칸으로 붙인다
fn snap<T>(term: &Term<T>, line: Line, col: usize) -> usize {
    let col = col.min(last_col(term));
    if col > 0
        && term.grid()[line][Column(col)]
            .flags
            .intersects(Flags::WIDE_CHAR_SPACER | Flags::LEADING_WIDE_CHAR_SPACER)
    {
        col - 1
    } else {
        col
    }
}

/// 줄에서 글자가 있는 마지막 칸(없으면 0). 넓은 글자면 그 글자의 앞 칸
fn content_end<T>(term: &Term<T>, line: Line) -> usize {
    let row = &term.grid()[line];
    (0..term.columns())
        .rev()
        .find(|&c| {
            let cell = &row[Column(c)];
            cell.c != ' '
                && cell.c != '\0'
                && !cell
                    .flags
                    .intersects(Flags::WIDE_CHAR_SPACER | Flags::LEADING_WIDE_CHAR_SPACER)
        })
        .map_or(0, |c| snap(term, line, c))
}

fn col_for<T>(term: &Term<T>, line: Line, goal: Goal) -> usize {
    match goal {
        Goal::Col(c) => snap(term, line, c),
        Goal::End => content_end(term, line),
    }
}

/// 움직임 하나: (줄, 칸, 바라는 칸)을 새로 정한다. 모든 결과는 격자 안이고 넓은 글자의 앞 칸이다.
/// `page`는 화면 줄 수
fn motion<T>(term: &Term<T>, line: Line, col: usize, goal: Goal, op: CopyOp, page: usize) -> (Line, usize, Goal) {
    let (top, bottom) = (term.topmost_line(), term.bottommost_line());
    let vertical = |to: i32| {
        let l = Line(to.clamp(top.0, bottom.0));
        (l, col_for(term, l, goal), goal)
    };
    let half = (page / 2).max(1) as i32;
    match op {
        CopyOp::Left => {
            let c = if col == 0 { 0 } else { snap(term, line, col - 1) };
            (line, c, Goal::Col(c))
        }
        CopyOp::Right => {
            let width = if term.grid()[line][Column(col)].flags.contains(Flags::WIDE_CHAR) {
                2
            } else {
                1
            };
            // 끝 칸을 넘으면 그대로(넓은 글자가 끝에 걸치면 앞 칸에 선다)
            let c = if col + width > last_col(term) {
                snap(term, line, col)
            } else {
                col + width
            };
            (line, c, Goal::Col(c))
        }
        CopyOp::Up => vertical(line.0 - 1),
        CopyOp::Down => vertical(line.0 + 1),
        CopyOp::HalfPageUp => vertical(line.0 - half),
        CopyOp::HalfPageDown => vertical(line.0 + half),
        CopyOp::PageUp => vertical(line.0 - page.max(1) as i32),
        CopyOp::PageDown => vertical(line.0 + page.max(1) as i32),
        CopyOp::LineStart => (line, 0, Goal::Col(0)),
        CopyOp::LineEnd => (line, content_end(term, line), Goal::End),
        CopyOp::Top => (top, 0, Goal::Col(0)),
        CopyOp::Bottom => (bottom, 0, Goal::Col(0)),
        CopyOp::SelectCell | CopyOp::SelectLine | CopyOp::SelectBlock => (line, col, goal),
    }
}

/// 커서가 보이게 스크롤한다(넘은 만큼만. 이미 보이면 그대로)
fn reveal<T: EventListener>(term: &mut Term<T>, line: Line) {
    let top = -(term.grid().display_offset() as i32);
    let bottom = top + term.screen_lines() as i32 - 1;
    if line.0 < top {
        term.scroll_display(Scroll::Delta(top - line.0));
    } else if line.0 > bottom {
        term.scroll_display(Scroll::Delta(-(line.0 - bottom)));
    }
}

fn point(line: Line, col: usize) -> Point {
    Point::new(line, Column(col))
}

/// 두 칸(둘 다 품는다)을 `kind`로 고른 고르기. 거꾸로 고르면 가장자리 쪽을 맞춰 두 칸이 빠지지 않게 한다
fn selection_between(kind: CopySelect, anchor: Point, cursor: Point) -> Selection {
    let ty = match kind {
        CopySelect::Cell => SelectionType::Simple,
        CopySelect::Line => SelectionType::Lines,
        CopySelect::Block => SelectionType::Block,
    };
    let backwards = if kind == CopySelect::Block {
        cursor.column < anchor.column
    } else {
        cursor < anchor
    };
    let (from, to) = if backwards {
        (Side::Right, Side::Left)
    } else {
        (Side::Left, Side::Right)
    };
    let mut sel = Selection::new(ty, anchor, from);
    sel.update(cursor, to);
    sel
}

impl CopyMode {
    /// 복사 모드에 들어간다. 맨 아래를 보고 있으면 커서가 있는 칸에서, 위로 올라가 있으면 보이는 화면의 맨 위 왼쪽에서 시작한다
    pub fn enter<T>(term: &Term<T>) -> Self {
        let offset = term.grid().display_offset();
        let (line, col) = if offset == 0 {
            let p = term.grid().cursor.point;
            (p.line, p.column.0)
        } else {
            (Line(-(offset as i32)), 0)
        };
        let line = Line(line.0.clamp(term.topmost_line().0, term.bottommost_line().0));
        let col = snap(term, line, col);
        Self {
            cursor: (stable_id(term, line), col),
            goal: Goal::Col(col),
            anchor: None,
            saved_offset: offset,
        }
    }

    /// 커서의 지금 자리(엔진 격자 좌표)
    pub fn cursor_point<T>(&self, term: &Term<T>) -> Point {
        let line = line_of(term, self.cursor.0);
        point(line, snap(term, line, self.cursor.1))
    }

    /// 지금 하는 고르기. 시작하지 않았으면 없다
    pub fn selecting(&self) -> Option<CopySelect> {
        self.anchor.map(|(_, k)| k)
    }

    /// 동작 하나를 하고, 커서가 보이게 스크롤하고, 고른 것을 엔진에 반영한다
    pub fn apply<T: EventListener>(&mut self, term: &mut Term<T>, op: CopyOp) {
        let line = line_of(term, self.cursor.0);
        let col = snap(term, line, self.cursor.1);
        match op {
            CopyOp::SelectCell | CopyOp::SelectLine | CopyOp::SelectBlock => {
                let kind = match op {
                    CopyOp::SelectCell => CopySelect::Cell,
                    CopyOp::SelectLine => CopySelect::Line,
                    _ => CopySelect::Block,
                };
                self.anchor = match self.anchor {
                    // 같은 종류를 다시 누르면 끈다(WezTerm). 다른 종류면 시작점은 두고 종류만 바꾼다
                    Some((_, k)) if k == kind => None,
                    Some((a, _)) => Some((a, kind)),
                    None => Some(((stable_id(term, line), col), kind)),
                };
            }
            _ => {
                let page = term.screen_lines();
                let (l, c, g) = motion(term, line, col, self.goal, op, page);
                self.cursor = (stable_id(term, l), c);
                self.goal = g;
            }
        }
        self.sync(term);
    }

    /// 커서가 보이게 하고 고른 것을 엔진에 반영한다. 고르기를 시작하지 않았으면 커서 칸 하나만 칠해 자리를 보인다
    /// (`has_selection`은 이 한 칸을 고른 것으로 치지 않는다)
    pub fn sync<T: EventListener>(&self, term: &mut Term<T>) {
        let cur = self.cursor_point(term);
        reveal(term, cur.line);
        let (anchor, kind) = match self.anchor {
            Some((a, k)) => (point(line_of(term, a.0), snap(term, line_of(term, a.0), a.1)), k),
            None => (cur, CopySelect::Cell),
        };
        term.selection = Some(selection_between(kind, anchor, cur));
    }

    /// 복사할 고른 글이 있는가(고르기를 시작했을 때만)
    pub fn has_selection(&self) -> bool {
        self.anchor.is_some()
    }

    /// 나간다: 고른 것을 지우고 스크롤 자리를 들어가기 전으로 돌린다
    pub fn exit<T: EventListener>(self, term: &mut Term<T>) {
        term.selection = None;
        let now = term.grid().display_offset() as i32;
        term.scroll_display(Scroll::Delta(self.saved_offset as i32 - now));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use alacritty_terminal::term::Config;
    use alacritty_terminal::vte::ansi::Processor;

    struct Size(usize, usize);
    impl Dimensions for Size {
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

    fn term_with(cols: usize, rows: usize, text: &str) -> Term<Quiet> {
        let mut t = Term::new(Config::default(), &Size(cols, rows), Quiet);
        let mut p: Processor = Processor::new();
        p.advance(&mut t, text.as_bytes());
        t
    }

    fn at(t: &Term<Quiet>, cm: &CopyMode) -> (i32, usize) {
        let p = cm.cursor_point(t);
        (p.line.0, p.column.0)
    }

    fn goto(t: &mut Term<Quiet>, cm: &mut CopyMode, ops: &[CopyOp]) {
        for &o in ops {
            cm.apply(t, o);
        }
    }

    fn text(t: &Term<Quiet>) -> Option<String> {
        let range = t.selection.as_ref()?.to_range(t)?;
        let opts = crate::copy_format::Options {
            emit: crate::copy_format::Emit::Plain,
            trim: false,
            background: None,
            foreground: None,
            codepoint_map: &[],
        };
        Some(crate::copy_format::format_range(
            t,
            &bt_core::Theme::default(),
            range.start,
            range.end,
            range.is_block,
            opts,
        ))
    }

    use CopyOp::*;

    /// 5칸 x 3줄 화면에 "abcde"·"fg"·"hijkl"
    fn small() -> Term<Quiet> {
        term_with(8, 3, "abcdefgh\r\nij\r\nklmnopqr")
    }

    #[test]
    fn hjkl_move_one_cell_and_stop_at_the_edges() {
        let mut t = small();
        let mut cm = CopyMode::enter(&t);
        goto(&mut t, &mut cm, &[Top]);
        assert_eq!(at(&t, &cm), (-0, 0));
        goto(&mut t, &mut cm, &[Left, Up]);
        assert_eq!(at(&t, &cm), (0, 0), "왼쪽 끝·맨 위에서는 그대로");
        goto(&mut t, &mut cm, &[Right, Right, Down]);
        assert_eq!(at(&t, &cm), (1, 2));
        goto(&mut t, &mut cm, &[Down, Down, Down]);
        assert_eq!(at(&t, &cm), (2, 2), "맨 아래 줄에서 멈춘다");
        goto(&mut t, &mut cm, &[Right; 20]);
        assert_eq!(at(&t, &cm), (2, 7), "오른쪽 끝에서 멈춘다");
    }

    #[test]
    fn line_start_and_end_follow_the_text() {
        let mut t = small();
        let mut cm = CopyMode::enter(&t);
        goto(&mut t, &mut cm, &[Top, LineEnd]);
        assert_eq!(at(&t, &cm), (0, 7));
        // `$` 뒤에는 줄마다 글자 끝으로 간다
        goto(&mut t, &mut cm, &[Down]);
        assert_eq!(at(&t, &cm), (1, 1), "짧은 줄의 글자 끝");
        goto(&mut t, &mut cm, &[Down]);
        assert_eq!(at(&t, &cm), (2, 7));
        goto(&mut t, &mut cm, &[LineStart]);
        assert_eq!(at(&t, &cm), (2, 0));
    }

    #[test]
    fn vertical_moves_remember_the_wanted_column() {
        let mut t = small();
        let mut cm = CopyMode::enter(&t);
        goto(&mut t, &mut cm, &[Top, Right, Right, Right, Right]);
        goto(&mut t, &mut cm, &[Down]);
        // 짧은 줄도 바라는 칸을 지키되 격자 안(빈 칸 포함)이다
        assert_eq!(at(&t, &cm), (1, 4));
        goto(&mut t, &mut cm, &[Down]);
        assert_eq!(at(&t, &cm), (2, 4));
        goto(&mut t, &mut cm, &[Left, Up]);
        assert_eq!(at(&t, &cm), (1, 3), "옆으로 움직이면 바라는 칸이 새로 정해진다");
    }

    #[test]
    fn top_bottom_and_half_pages_scroll_through_history() {
        let mut feed = String::new();
        for i in 0..30 {
            feed.push_str(&format!("line{i:02}\r\n"));
        }
        let mut t = term_with(10, 5, &feed);
        let mut cm = CopyMode::enter(&t);
        goto(&mut t, &mut cm, &[Top]);
        assert_eq!(at(&t, &cm).0, -(t.grid().history_size() as i32), "스크롤백 맨 위");
        assert_eq!(
            t.grid().display_offset(),
            t.grid().history_size(),
            "맨 위가 보이게 스크롤"
        );
        goto(&mut t, &mut cm, &[HalfPageDown]);
        assert_eq!(
            at(&t, &cm).0,
            -(t.grid().history_size() as i32) + 2,
            "화면 반(5줄이면 2줄)"
        );
        goto(&mut t, &mut cm, &[Bottom]);
        assert_eq!(at(&t, &cm).0, 4);
        assert_eq!(t.grid().display_offset(), 0, "맨 아래가 보이게 돌아온다");
        goto(&mut t, &mut cm, &[HalfPageUp, HalfPageUp]);
        assert_eq!(at(&t, &cm).0, 0);
        goto(&mut t, &mut cm, &[Up]);
        assert_eq!(t.grid().display_offset(), 1, "화면 위를 넘으면 한 줄 스크롤");
    }

    #[test]
    fn wide_chars_take_two_cells_and_the_cursor_never_sits_on_the_back_half() {
        // 칸: 한(0,1) 글(2,3) a(4) 자(5,6)
        let mut t = term_with(8, 2, "한글a자");
        let mut cm = CopyMode::enter(&t);
        goto(&mut t, &mut cm, &[Top]);
        goto(&mut t, &mut cm, &[Right]);
        assert_eq!(at(&t, &cm), (0, 2), "넓은 글자는 두 칸을 건너뛴다");
        goto(&mut t, &mut cm, &[Right, Right]);
        assert_eq!(at(&t, &cm), (0, 5));
        goto(&mut t, &mut cm, &[Left]);
        assert_eq!(at(&t, &cm), (0, 4));
        goto(&mut t, &mut cm, &[Left]);
        assert_eq!(at(&t, &cm), (0, 2), "뒤 칸(3)이 아니라 앞 칸");
        goto(&mut t, &mut cm, &[LineEnd]);
        assert_eq!(at(&t, &cm), (0, 5), "줄 끝 글자(넓은 글자)의 앞 칸");
        // 위아래로 움직일 때 바라는 칸이 뒤 칸이면 앞 칸으로 붙는다
        let mut t = term_with(8, 2, "abcdefgh\r\n한글a자");
        let mut cm = CopyMode::enter(&t);
        goto(&mut t, &mut cm, &[Top, Right, Right, Right, Down]);
        assert_eq!(at(&t, &cm), (1, 2));
        goto(&mut t, &mut cm, &[Up]);
        assert_eq!(at(&t, &cm), (0, 3), "바라는 칸 3은 윗줄에서 그대로");
    }

    #[test]
    fn selection_covers_both_ends_and_wide_chars_whole() {
        let mut t = term_with(8, 2, "한글a자");
        let mut cm = CopyMode::enter(&t);
        goto(&mut t, &mut cm, &[Top, SelectCell]);
        assert!(cm.has_selection());
        goto(&mut t, &mut cm, &[Right]);
        assert_eq!(text(&t).as_deref(), Some("한글"));
        goto(&mut t, &mut cm, &[Right, Right]);
        assert_eq!(text(&t).as_deref(), Some("한글a자"));
        // 거꾸로 골라도 두 끝을 모두 품는다
        goto(&mut t, &mut cm, &[SelectCell, SelectCell]);
        assert_eq!(cm.selecting(), Some(CopySelect::Cell));
        goto(&mut t, &mut cm, &[Left]);
        assert_eq!(text(&t).as_deref(), Some("a자"));
    }

    #[test]
    fn pressing_the_same_select_key_again_turns_selection_off() {
        let mut t = small();
        let mut cm = CopyMode::enter(&t);
        goto(&mut t, &mut cm, &[Top, SelectCell, Right]);
        assert!(cm.has_selection());
        goto(&mut t, &mut cm, &[SelectCell]);
        assert!(!cm.has_selection(), "복사할 것이 없다(커서 칸 표시는 고른 것이 아니다)");
        goto(&mut t, &mut cm, &[SelectCell, SelectLine]);
        assert_eq!(
            cm.selecting(),
            Some(CopySelect::Line),
            "다른 종류는 시작점을 두고 종류만 바꾼다"
        );
    }

    #[test]
    fn line_select_takes_whole_lines_and_joins_wrapped_rows() {
        // 8칸 화면에 12글자: 줄 바꿈으로 이어진 한 줄 + 직접 친 줄
        let mut t = term_with(8, 4, "abcdefghijkl\r\nxy");
        let mut cm = CopyMode::enter(&t);
        goto(&mut t, &mut cm, &[Top, Right, Right, SelectLine]);
        assert_eq!(text(&t).as_deref(), Some("abcdefghijkl"), "이어진 두 줄이 한 줄로");
        goto(&mut t, &mut cm, &[Down, Down]);
        assert_eq!(text(&t).as_deref(), Some("abcdefghijkl\nxy"));
    }

    #[test]
    fn selection_across_wrapped_rows_has_no_newline_inside() {
        let mut t = term_with(8, 4, "abcdefghijkl\r\nxy");
        let mut cm = CopyMode::enter(&t);
        goto(&mut t, &mut cm, &[Top, Right, SelectCell, Down, Right]);
        assert_eq!(text(&t).as_deref(), Some("bcdefghijk"));
    }

    #[test]
    fn block_select_takes_a_rectangle() {
        let mut t = small();
        let mut cm = CopyMode::enter(&t);
        goto(&mut t, &mut cm, &[Top, Right, SelectBlock, Right, Down, Down]);
        // 사각: 1~2칸 x 세 줄 ("abcdefgh"·"ij"·"klmnopqr")
        assert_eq!(text(&t).as_deref(), Some("bc\nj\nlm"));
        assert_eq!(cm.selecting(), Some(CopySelect::Block));
    }

    #[test]
    fn exit_clears_the_selection_and_restores_the_scroll_position() {
        let mut feed = String::new();
        for i in 0..30 {
            feed.push_str(&format!("line{i:02}\r\n"));
        }
        let mut t = term_with(10, 5, &feed);
        t.scroll_display(Scroll::Delta(7));
        let before = t.grid().display_offset();
        assert_eq!(before, 7);
        let mut cm = CopyMode::enter(&t);
        goto(&mut t, &mut cm, &[Top, SelectCell, Bottom]);
        assert_eq!(t.grid().display_offset(), 0);
        cm.exit(&mut t);
        assert!(t.selection.is_none());
        assert_eq!(t.grid().display_offset(), before, "들어가기 전 자리");
        // 맨 아래에서 들어갔으면 맨 아래로 돌아온다
        t.scroll_display(Scroll::Bottom);
        let mut cm = CopyMode::enter(&t);
        goto(&mut t, &mut cm, &[Top]);
        cm.exit(&mut t);
        assert_eq!(t.grid().display_offset(), 0);
    }

    #[test]
    fn cursor_keeps_pointing_at_the_same_line_while_output_arrives() {
        let mut feed = String::new();
        for i in 0..8 {
            feed.push_str(&format!("line{i}\r\n"));
        }
        let mut t = term_with(10, 4, &feed);
        let mut cm = CopyMode::enter(&t);
        goto(&mut t, &mut cm, &[Top, Down, Right]);
        let id = cm.cursor.0;
        let line = |t: &Term<Quiet>, cm: &CopyMode| {
            let p = cm.cursor_point(t);
            t.grid()[p.line].into_iter().take(5).map(|c| c.c).collect::<String>()
        };
        let before = line(&t, &cm);
        let mut p: Processor = Processor::new();
        p.advance(&mut t, b"more1\r\nmore2\r\nmore3\r\n");
        assert_eq!(cm.cursor.0, id);
        assert_eq!(line(&t, &cm), before, "새 출력이 와 줄이 밀려도 같은 줄");
    }

    #[test]
    fn a_line_pushed_out_of_the_scrollback_pins_the_cursor_to_the_top() {
        let mut t = Term::new(
            Config {
                scrolling_history: 3,
                ..Config::default()
            },
            &Size(10, 3),
            Quiet,
        );
        let mut p: Processor = Processor::new();
        p.advance(&mut t, b"a\r\nb\r\nc\r\nd\r\ne\r\n");
        let mut cm = CopyMode::enter(&t);
        goto(&mut t, &mut cm, &[Top]);
        let top = at(&t, &cm).0;
        p.advance(&mut t, b"f\r\ng\r\nh\r\ni\r\nj\r\n");
        assert_eq!(
            cm.cursor_point(&t).line,
            t.topmost_line(),
            "밀려난 줄은 맨 위 줄로 붙는다 ({top})"
        );
        goto(&mut t, &mut cm, &[Down, Down]);
        assert!(cm.cursor_point(&t).line <= t.bottommost_line());
    }

    #[test]
    fn resizing_smaller_never_leaves_the_cursor_outside_the_grid() {
        let mut t = small();
        let mut cm = CopyMode::enter(&t);
        goto(&mut t, &mut cm, &[Bottom, LineEnd]);
        t.resize(Size(4, 2));
        let p = cm.cursor_point(&t);
        assert!(p.column.0 < 4 && p.line <= t.bottommost_line() && p.line >= t.topmost_line());
        goto(&mut t, &mut cm, &[Right, Down]);
    }
}
