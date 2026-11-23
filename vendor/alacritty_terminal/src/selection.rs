//! State management for a selection in the grid.
//!
//! A selection should start when the mouse is clicked, and it should be
//! finalized when the button is released. The selection should be cleared
//! when text is added/removed/scrolled on the screen. The selection should
//! also be cleared if the user clicks off of the selection.

use std::cmp::min;
use std::mem;
use std::ops::{Bound, Range, RangeBounds};

use crate::grid::{Dimensions, GridCell, Indexed};
use crate::index::{Boundary, Column, Line, Point, Side};
use crate::term::Term;
use crate::term::cell::{Cell, Flags};
use crate::vte::ansi::CursorShape;

/// b-terminal(#803, #804, #805): Ghostty `Screen.selectLine`·`selectWord`·`selectAll`의 칸 규칙
mod ghostty {
    use super::*;

    #[derive(Clone, Copy, PartialEq, Eq)]
    enum Kind {
        Prompt,
        Input,
        Output,
    }

    /// OSC 133 칸 종류. 표시 없는 칸(쓰지 않은 칸 포함)은 출력이다(Ghostty `Cell.semantic_content` 기본값)
    fn kind(cell: &Cell) -> Kind {
        if cell.flags.contains(Flags::SEMANTIC_INPUT) {
            Kind::Input
        } else if cell.flags.contains(Flags::SEMANTIC_PROMPT) {
            Kind::Prompt
        } else {
            Kind::Output
        }
    }

    /// Ghostty `cell.hasText()`: 글자를 찍은 칸(찍은 빈칸 포함). 넓은 글자의 뒤 칸·줄 끝 자리 칸은 아니다
    pub fn has_text(cell: &Cell) -> bool {
        cell.flags.contains(Flags::WRITTEN)
            && !cell.flags.intersects(Flags::WIDE_CHAR_SPACER | Flags::LEADING_WIDE_CHAR_SPACER)
    }

    /// 그 줄이 다음 줄로 이어지는가(Ghostty `row.wrap`)
    fn wraps<T>(term: &Term<T>, line: Line) -> bool {
        term.grid()[line][term.last_column()].flags.contains(Flags::WRAPLINE)
    }

    fn next<T>(term: &Term<T>, p: Point) -> Option<Point> {
        if p.column < term.last_column() {
            Some(Point::new(p.line, p.column + 1))
        } else if p.line < term.bottommost_line() {
            Some(Point::new(p.line + 1, Column(0)))
        } else {
            None
        }
    }

    fn prev<T>(term: &Term<T>, p: Point) -> Option<Point> {
        if p.column > Column(0) {
            Some(Point::new(p.line, p.column - 1))
        } else if p.line > term.topmost_line() {
            Some(Point::new(p.line - 1, term.last_column()))
        } else {
            None
        }
    }

    /// Ghostty `selectWordCodepoint`: 넓은 글자의 뒤 칸은 앞 칸의 글자, 줄 끝 자리 칸은 다음 줄 첫 칸의 넓은 글자를 따른다.
    /// 글자가 없는 칸은 None
    fn word_char<T>(term: &Term<T>, p: Point) -> Option<char> {
        let grid = term.grid();
        let cell = &grid[p];
        let owner = if cell.flags.contains(Flags::WIDE_CHAR_SPACER) {
            &grid[Point::new(p.line, Column(p.column.0.saturating_sub(1)))]
        } else if cell.flags.contains(Flags::LEADING_WIDE_CHAR_SPACER) {
            if p.line >= term.bottommost_line() {
                return None;
            }
            let owner = &grid[Point::new(p.line + 1, Column(0))];
            if !owner.flags.contains(Flags::WIDE_CHAR) {
                return None;
            }
            owner
        } else {
            cell
        };
        has_text(owner).then_some(owner.c)
    }

    /// Ghostty `Screen.selectWord`. 누른 칸이 경계 글자면 이어진 경계 글자 구간만, 아니면 이어진 낱말 글자 구간만 고른다
    pub fn select_word<T>(term: &Term<T>, pin: Point) -> Option<(Point, Point)> {
        let boundaries = term.semantic_escape_chars();
        let expect_boundary = boundaries.contains(word_char(term, pin)?);
        let same_kind = |p: Point| -> Option<()> {
            let c = word_char(term, p)?;
            (boundaries.contains(c) == expect_boundary).then_some(())
        };

        let mut end = pin;
        while let Some(p) = next(term, end) {
            // 줄을 넘는 것은 앞 줄이 이어질 때만
            if end.column == term.last_column() && !wraps(term, end.line) {
                break;
            }
            if same_kind(p).is_none() {
                break;
            }
            end = p;
        }

        let mut start = pin;
        while let Some(p) = prev(term, start) {
            if p.column == term.last_column() && !wraps(term, p.line) {
                break;
            }
            if same_kind(p).is_none() {
                break;
            }
            start = p;
        }
        Some((start, end))
    }

    /// Ghostty `Screen.selectWordBetween`: `from`에서 `to` 쪽으로 가며 처음 낱말이 있는 칸의 낱말
    pub fn select_word_between<T>(term: &Term<T>, from: Point, to: Point) -> Option<(Point, Point)> {
        let mut p = from;
        loop {
            if let Some(w) = select_word(term, p) {
                return Some(w);
            }
            if p == to {
                return None;
            }
            p = if from < to { next(term, p)? } else { prev(term, p)? };
        }
    }

    /// 줄 고르기가 앞뒤에서 빼는 글자(Ghostty `default_line_whitespace`)
    fn is_line_space(c: char) -> bool {
        matches!(c, '\0' | ' ' | '\t')
    }

    /// Ghostty `Screen.selectLine`. 이어진 줄을 한 줄로 보고, OSC 133 칸 종류가 바뀌는 곳에서 끊고, `trim`이면 앞뒤 공백을 뺀다
    pub fn select_line<T>(term: &Term<T>, pin: Point, trim: bool) -> Option<(Point, Point)> {
        let grid = term.grid();
        let last = term.last_column();
        let cols = term.columns();
        let state = kind(&grid[pin]);

        // 줄의 처음: 누른 줄에서 거꾸로 칸 종류가 바뀌는 곳, 없으면 이어진 윗줄들을 거슬러 오른다
        let start_pin = 'start: {
            for x in (0..=pin.column.0).rev() {
                if kind(&grid[pin.line][Column(x)]) != state {
                    break 'start Point::new(pin.line, Column(x + 1));
                }
            }
            let mut it_prev = Point::new(pin.line, Column(0));
            let mut line = pin.line;
            while line > term.topmost_line() {
                let up = line - 1;
                if !wraps(term, up) {
                    break;
                }
                for x in (0..cols).rev() {
                    if kind(&grid[up][Column(x)]) != state {
                        break 'start it_prev;
                    }
                    it_prev = Point::new(up, Column(x));
                }
                line = up;
            }
            Point::new(it_prev.line, Column(0))
        };

        // 줄의 끝: 이어진 아랫줄들을 따라 내려가며 칸 종류가 바뀌는 곳 바로 앞, 없으면 이어지지 않는 줄의 끝 칸
        let end_pin = 'end: {
            let mut line = pin.line;
            while line <= term.bottommost_line() {
                let from = if line == pin.line { pin.column.0 } else { 0 };
                if from == 0 && kind(&grid[line][Column(0)]) != state {
                    // 첫 칸부터 다르면 윗줄 끝. (누른 줄의 첫 칸은 늘 같으므로 윗줄이 있다)
                    break 'end Point::new(line - 1, last);
                }
                for x in from..cols {
                    if kind(&grid[line][Column(x)]) != state {
                        break 'end Point::new(line, Column(x - 1));
                    }
                }
                if !wraps(term, line) {
                    break 'end Point::new(line, last);
                }
                line += 1;
            }
            return None;
        };

        if !trim {
            return Some((start_pin, end_pin));
        }

        let first = |from: Point, to: Point, forward: bool| -> Option<Point> {
            let mut p = from;
            loop {
                if (forward && p > to) || (!forward && p < to) {
                    return None;
                }
                let cell = &grid[p];
                if has_text(cell) && !is_line_space(cell.c) {
                    return Some(p);
                }
                p = if forward { next(term, p)? } else { prev(term, p)? };
            }
        };
        let start = first(start_pin, end_pin, true)?;
        let end = first(end_pin, start_pin, false)?;
        Some((start, end))
    }

    /// Ghostty `Screen.selectAll`: 스크롤백까지 통틀어 글이 있는 첫 칸부터 마지막 칸까지. 앞뒤 공백(널·빈칸·탭)은 뺀다
    pub fn select_all<T>(term: &Term<T>) -> Option<(Point, Point)> {
        let grid = term.grid();
        let top = Point::new(term.topmost_line(), Column(0));
        let bottom = Point::new(term.bottommost_line(), term.last_column());
        let found = |p: Point| {
            let cell = &grid[p];
            has_text(cell) && !is_line_space(cell.c)
        };
        let mut start = top;
        while !found(start) {
            start = next(term, start)?;
        }
        let mut end = bottom;
        while !found(end) {
            end = prev(term, end)?;
        }
        Some((start, end))
    }
}

pub use ghostty::select_all as ghostty_select_all;

/// A Point and side within that point.
#[derive(Debug, Copy, Clone, PartialEq, Eq)]
struct Anchor {
    point: Point,
    side: Side,
}

impl Anchor {
    fn new(point: Point, side: Side) -> Anchor {
        Anchor { point, side }
    }
}

/// Represents a range of selected cells.
#[derive(Copy, Clone, Debug, Eq, PartialEq)]
pub struct SelectionRange {
    /// Start point, top left of the selection.
    pub start: Point,
    /// End point, bottom right of the selection.
    pub end: Point,
    /// Whether this selection is a block selection.
    pub is_block: bool,
}

impl SelectionRange {
    pub fn new(start: Point, end: Point, is_block: bool) -> Self {
        assert!(start <= end);
        Self { start, end, is_block }
    }
}

impl SelectionRange {
    /// Check if a point lies within the selection.
    pub fn contains(&self, point: Point) -> bool {
        self.start.line <= point.line
            && self.end.line >= point.line
            && (self.start.column <= point.column
                || (self.start.line != point.line && !self.is_block))
            && (self.end.column >= point.column || (self.end.line != point.line && !self.is_block))
    }

    /// Check if the cell at a point is part of the selection.
    pub fn contains_cell(
        &self,
        indexed: &Indexed<&Cell>,
        point: Point,
        shape: CursorShape,
    ) -> bool {
        // Do not invert block cursor at selection boundaries.
        if shape == CursorShape::Block
            && point == indexed.point
            && (self.start == indexed.point
                || self.end == indexed.point
                || (self.is_block
                    && ((self.start.line == indexed.point.line
                        && self.end.column == indexed.point.column)
                        || (self.end.line == indexed.point.line
                            && self.start.column == indexed.point.column))))
        {
            return false;
        }

        // Point itself is selected.
        if self.contains(indexed.point) {
            return true;
        }

        // Check if a wide char's trailing spacer is selected.
        indexed.cell.flags().contains(Flags::WIDE_CHAR)
            && self.contains(Point::new(indexed.point.line, indexed.point.column + 1))
    }
}

/// Different kinds of selection.
#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub enum SelectionType {
    Simple,
    Block,
    Semantic,
    Lines,
}

/// Describes a region of a 2-dimensional area.
///
/// Used to track a text selection. There are four supported modes, each with its own constructor:
/// [`simple`], [`block`], [`semantic`], and [`lines`]. The [`simple`] mode precisely tracks which
/// cells are selected without any expansion. [`block`] will select rectangular regions.
/// [`semantic`] mode expands the initial selection to the nearest semantic escape char in either
/// direction. [`lines`] will always select entire lines.
///
/// Calls to [`update`] operate different based on the selection kind. The [`simple`] and [`block`]
/// mode do nothing special, simply track points and sides. [`semantic`] will continue to expand
/// out to semantic boundaries as the selection point changes. Similarly, [`lines`] will always
/// expand the new point to encompass entire lines.
///
/// [`simple`]: enum.Selection.html#method.simple
/// [`block`]: enum.Selection.html#method.block
/// [`semantic`]: enum.Selection.html#method.semantic
/// [`lines`]: enum.Selection.html#method.lines
/// [`update`]: enum.Selection.html#method.update
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Selection {
    pub ty: SelectionType,
    region: Range<Anchor>,
}

impl Selection {
    pub fn new(ty: SelectionType, location: Point, side: Side) -> Selection {
        Self {
            region: Range { start: Anchor::new(location, side), end: Anchor::new(location, side) },
            ty,
        }
    }

    /// b-terminal(#433): 고르기를 시작한 칸. 스크롤로 옮겨진 뒤의 값이다(사각을 다시 만들 때 쓴다)
    pub fn start_point(&self) -> Point {
        self.region.start.point
    }

    /// Update the end of the selection.
    pub fn update(&mut self, point: Point, side: Side) {
        self.region.end = Anchor::new(point, side);
    }

    pub fn rotate<D: Dimensions>(
        mut self,
        dimensions: &D,
        range: &Range<Line>,
        delta: i32,
    ) -> Option<Selection> {
        let bottommost_line = dimensions.bottommost_line();
        let range_bottom = range.end;
        let range_top = range.start;

        let (mut start, mut end) = (&mut self.region.start, &mut self.region.end);
        if start.point > end.point {
            mem::swap(&mut start, &mut end);
        }

        // Rotate start of selection.
        if (start.point.line >= range_top || range_top == 0) && start.point.line < range_bottom {
            start.point.line = min(start.point.line - delta, bottommost_line);

            // If end is within the same region, delete selection once start rotates out.
            if start.point.line >= range_bottom && end.point.line < range_bottom {
                return None;
            }

            // Clamp selection to start of region.
            if start.point.line < range_top && range_top != 0 {
                if self.ty != SelectionType::Block {
                    start.point.column = Column(0);
                    start.side = Side::Left;
                }
                start.point.line = range_top;
            }
        }

        // Rotate end of selection.
        if (end.point.line >= range_top || range_top == 0) && end.point.line < range_bottom {
            end.point.line = min(end.point.line - delta, bottommost_line);

            // Delete selection if end has overtaken the start.
            if end.point.line < start.point.line {
                return None;
            }

            // Clamp selection to end of region.
            if end.point.line >= range_bottom {
                if self.ty != SelectionType::Block {
                    end.point.column = dimensions.last_column();
                    end.side = Side::Right;
                }
                end.point.line = range_bottom - 1;
            }
        }

        Some(self)
    }

    pub fn is_empty(&self) -> bool {
        match self.ty {
            SelectionType::Simple => {
                let (mut start, mut end) = (self.region.start, self.region.end);
                if start.point > end.point {
                    mem::swap(&mut start, &mut end);
                }

                // Simple selection is empty when the points are identical
                // or two adjacent cells have the sides right -> left.
                start == end
                    || (start.side == Side::Right
                        && end.side == Side::Left
                        && (start.point.line == end.point.line)
                        && start.point.column + 1 == end.point.column)
            },
            SelectionType::Block => {
                let (start, end) = (self.region.start, self.region.end);

                // Block selection is empty when the points' columns and sides are identical
                // or two cells with adjacent columns have the sides right -> left,
                // regardless of their lines
                (start.point.column == end.point.column && start.side == end.side)
                    || (start.point.column + 1 == end.point.column
                        && start.side == Side::Right
                        && end.side == Side::Left)
                    || (end.point.column + 1 == start.point.column
                        && start.side == Side::Left
                        && end.side == Side::Right)
            },
            SelectionType::Semantic | SelectionType::Lines => false,
        }
    }

    /// Check whether selection contains any point in a given range.
    pub fn intersects_range<R: RangeBounds<Line>>(&self, range: R) -> bool {
        let mut start = self.region.start.point.line;
        let mut end = self.region.end.point.line;

        if start > end {
            mem::swap(&mut start, &mut end);
        }

        let range_top = match range.start_bound() {
            Bound::Included(&range_start) => range_start,
            Bound::Excluded(&range_start) => range_start + 1,
            Bound::Unbounded => Line(i32::MIN),
        };

        let range_bottom = match range.end_bound() {
            Bound::Included(&range_end) => range_end,
            Bound::Excluded(&range_end) => range_end - 1,
            Bound::Unbounded => Line(i32::MAX),
        };

        range_bottom >= start && range_top <= end
    }

    /// Expand selection sides to include all cells.
    pub fn include_all(&mut self) {
        let (start, end) = (self.region.start.point, self.region.end.point);
        let (start_side, end_side) = match self.ty {
            SelectionType::Block
                if start.column > end.column
                    || (start.column == end.column && start.line > end.line) =>
            {
                (Side::Right, Side::Left)
            },
            SelectionType::Block => (Side::Left, Side::Right),
            _ if start > end => (Side::Right, Side::Left),
            _ => (Side::Left, Side::Right),
        };

        self.region.start.side = start_side;
        self.region.end.side = end_side;
    }

    /// Convert selection to grid coordinates.
    pub fn to_range<T>(&self, term: &Term<T>) -> Option<SelectionRange> {
        let grid = term.grid();
        let columns = grid.columns();

        // Order start above the end.
        let mut start = self.region.start;
        let mut end = self.region.end;

        if start.point > end.point {
            mem::swap(&mut start, &mut end);
        }

        // Clamp selection to within grid boundaries.
        if end.point.line < term.topmost_line() {
            return None;
        }
        start.point = start.point.grid_clamp(term, Boundary::Grid);
        end.point = end.point.grid_clamp(term, Boundary::Grid);

        match self.ty {
            SelectionType::Simple => self.range_simple(start, end, columns),
            SelectionType::Block => self.range_block(start, end),
            // b-terminal(#803, #804): 누른 칸과 끈 칸은 정렬하기 전의 것을 쓴다(Ghostty는 누른 쪽과 끈 쪽을 따로 본다)
            SelectionType::Semantic => Self::range_semantic(
                term,
                self.region.start.point.grid_clamp(term, Boundary::Grid),
                self.region.end.point.grid_clamp(term, Boundary::Grid),
            ),
            SelectionType::Lines => Self::range_lines(
                term,
                self.region.start.point.grid_clamp(term, Boundary::Grid),
                self.region.end.point.grid_clamp(term, Boundary::Grid),
            ),
        }
    }

    /// b-terminal(#804): 두 번 누르기·낱말 단위 끌기. Ghostty `Screen.selectWord`·`SelectionGesture.dragSelectionWord`와 같다.
    /// 누른 칸(`region.start`)과 끈 칸(`region.end`)에서 각각 낱말을 구해 이어 붙인다. 쓰지 않은 칸만 있으면 None
    fn range_semantic<T>(term: &Term<T>, click: Point, drag: Point) -> Option<SelectionRange> {
        let word_start = ghostty::select_word_between(term, click, drag)?;
        let word_current = ghostty::select_word_between(term, drag, click)?;
        let (start, end) = if drag < click {
            (word_current.0, word_start.1)
        } else {
            (word_start.0, word_current.1)
        };
        Some(SelectionRange { start, end, is_block: false })
    }

    /// b-terminal(#803): 세 번 누르기·줄 단위 끌기. Ghostty `Screen.selectLine`·`SelectionGesture.dragSelectionLine`과 같다.
    /// 앞뒤 공백을 빼고 OSC 133 상태가 바뀌는 곳에서 끊는다. 누른 칸에 줄이 없으면 None
    fn range_lines<T>(term: &Term<T>, click: Point, drag: Point) -> Option<SelectionRange> {
        // 끈 줄에 글이 없으면 Ghostty는 이번 움직임을 버리고 앞서 고른 것을 둔다. 고르기가 상태 없이 범위를 셈하므로
        // (그것을 흉내 낼 앞선 값이 없다) 끌고 있을 때는 그 줄 전체를 쓴다. 누른 줄 하나만이면(끌지 않았으면) 고르지 않는다
        let line = ghostty::select_line(term, drag, true).or_else(|| {
            (drag != click).then(|| ghostty::select_line(term, drag, false)).flatten()
        })?;
        let (mut start, mut end) = ghostty::select_line(term, click, true)
            .or_else(|| ghostty::select_line(term, click, false))?;
        if drag < click {
            start = line.0;
        } else {
            end = line.1;
        }
        Some(SelectionRange { start, end, is_block: false })
    }

    fn range_simple(
        &self,
        mut start: Anchor,
        mut end: Anchor,
        columns: usize,
    ) -> Option<SelectionRange> {
        if self.is_empty() {
            return None;
        }

        // Remove last cell if selection ends to the left of a cell.
        if end.side == Side::Left && start.point != end.point {
            // Special case when selection ends to left of first cell.
            if end.point.column == 0 {
                end.point.column = Column(columns - 1);
                end.point.line -= 1;
            } else {
                end.point.column -= 1;
            }
        }

        // Remove first cell if selection starts at the right of a cell.
        if start.side == Side::Right && start.point != end.point {
            start.point.column += 1;

            // Wrap to next line when selection starts to the right of last column.
            if start.point.column == columns {
                start.point.column = Column(0);
                start.point.line += 1;
            }
        }

        Some(SelectionRange { start: start.point, end: end.point, is_block: false })
    }

    fn range_block(&self, mut start: Anchor, mut end: Anchor) -> Option<SelectionRange> {
        if self.is_empty() {
            return None;
        }

        // Always go top-left -> bottom-right.
        if start.point.column > end.point.column {
            mem::swap(&mut start.side, &mut end.side);
            mem::swap(&mut start.point.column, &mut end.point.column);
        }

        // b-terminal(#437): 같은 열에서 시작이 오른쪽, 끝이 왼쪽이면(눌렀다 놓은 칸 안에서 왼쪽으로 조금 움직인 세로 끌기)
        // 아래 두 보정이 겹쳐 열이 뒤집힌다. Ghostty는 그 열 하나를 고른다(`dragSelection`에서 같은 열은 쪽만 본다)
        if start.point.column == end.point.column && start.side == Side::Right && end.side == Side::Left {
            start.side = Side::Left;
            end.side = Side::Right;
        }

        // Remove last cell if selection ends to the left of a cell.
        if end.side == Side::Left && start.point != end.point && end.point.column.0 > 0 {
            end.point.column -= 1;
        }

        // Remove first cell if selection starts at the right of a cell.
        if start.side == Side::Right && start.point != end.point {
            start.point.column += 1;
        }

        Some(SelectionRange { start: start.point, end: end.point, is_block: true })
    }
}

/// Tests for selection.
///
/// There are comments on all of the tests describing the selection. Pictograms
/// are used to avoid ambiguity. Grid cells are represented by a [  ]. Only
/// cells that are completely covered are counted in a selection. Ends are
/// represented by `B` and `E` for begin and end, respectively.  A selected cell
/// looks like [XX], [BX] (at the start), [XB] (at the end), [XE] (at the end),
/// and [EX] (at the start), or [BE] for a single cell. Partially selected cells
/// look like [ B] and [E ].
#[cfg(test)]
mod tests {
    use super::*;

    use crate::index::{Column, Point, Side};
    use crate::term::test::TermSize;
    use crate::term::{Config, Term};

    fn term(height: usize, width: usize) -> Term<()> {
        let size = TermSize::new(width, height);
        Term::new(Config::default(), &size, ())
    }

    /// b-terminal(#803, #804): 줄·낱말 고르기는 글자를 찍은 칸만 고르므로 모든 칸에 'a'를 찍은 격자를 쓴다
    fn written(height: usize, width: usize) -> Term<()> {
        let mut term = term(height, width);
        for line in 0..height {
            for col in 0..width {
                let cell = &mut term.grid_mut()[Line(line as i32)][Column(col)];
                cell.c = 'a';
                cell.flags.insert(Flags::WRITTEN);
            }
        }
        term
    }

    /// Test case of single cell selection.
    ///
    /// 1. [  ]
    /// 2. [B ]
    /// 3. [BE]
    #[test]
    fn single_cell_left_to_right() {
        let location = Point::new(Line(0), Column(0));
        let mut selection = Selection::new(SelectionType::Simple, location, Side::Left);
        selection.update(location, Side::Right);

        assert_eq!(selection.to_range(&term(1, 2)).unwrap(), SelectionRange {
            start: location,
            end: location,
            is_block: false
        });
    }

    /// Test case of single cell selection.
    ///
    /// 1. [  ]
    /// 2. [ B]
    /// 3. [EB]
    #[test]
    fn single_cell_right_to_left() {
        let location = Point::new(Line(0), Column(0));
        let mut selection = Selection::new(SelectionType::Simple, location, Side::Right);
        selection.update(location, Side::Left);

        assert_eq!(selection.to_range(&term(1, 2)).unwrap(), SelectionRange {
            start: location,
            end: location,
            is_block: false
        });
    }

    /// Test adjacent cell selection from left to right.
    ///
    /// 1. [  ][  ]
    /// 2. [ B][  ]
    /// 3. [ B][E ]
    #[test]
    fn between_adjacent_cells_left_to_right() {
        let mut selection =
            Selection::new(SelectionType::Simple, Point::new(Line(0), Column(0)), Side::Right);
        selection.update(Point::new(Line(0), Column(1)), Side::Left);

        assert_eq!(selection.to_range(&term(1, 2)), None);
    }

    /// Test adjacent cell selection from right to left.
    ///
    /// 1. [  ][  ]
    /// 2. [  ][B ]
    /// 3. [ E][B ]
    #[test]
    fn between_adjacent_cells_right_to_left() {
        let mut selection =
            Selection::new(SelectionType::Simple, Point::new(Line(0), Column(1)), Side::Left);
        selection.update(Point::new(Line(0), Column(0)), Side::Right);

        assert_eq!(selection.to_range(&term(1, 2)), None);
    }

    #[rustfmt::skip]
    /// Test selection across adjacent lines.
    ///
    /// 1.  [  ][  ][  ][  ][  ]
    ///     [  ][  ][  ][  ][  ]
    /// 2.  [  ][ B][  ][  ][  ]
    ///     [  ][  ][  ][  ][  ]
    /// 3.  [  ][ B][XX][XX][XX]
    ///     [XX][XE][  ][  ][  ]
    #[test]
    fn across_adjacent_lines_upward_final_cell_exclusive() {
        let mut selection =
            Selection::new(SelectionType::Simple, Point::new(Line(0), Column(1)), Side::Right);
        selection.update(Point::new(Line(1), Column(1)), Side::Right);

        assert_eq!(selection.to_range(&term(2, 5)).unwrap(), SelectionRange {
            start: Point::new(Line(0), Column(2)),
            end: Point::new(Line(1), Column(1)),
            is_block: false,
        });
    }

    #[rustfmt::skip]
    /// Test selection across adjacent lines.
    ///
    /// 1.  [  ][  ][  ][  ][  ]
    ///     [  ][  ][  ][  ][  ]
    /// 2.  [  ][  ][  ][  ][  ]
    ///     [  ][ B][  ][  ][  ]
    /// 3.  [  ][ E][XX][XX][XX]
    ///     [XX][XB][  ][  ][  ]
    /// 4.  [ E][XX][XX][XX][XX]
    ///     [XX][XB][  ][  ][  ]
    #[test]
    fn selection_bigger_then_smaller() {
        let mut selection =
            Selection::new(SelectionType::Simple, Point::new(Line(1), Column(1)), Side::Right);
        selection.update(Point::new(Line(0), Column(1)), Side::Right);
        selection.update(Point::new(Line(0), Column(0)), Side::Right);

        assert_eq!(selection.to_range(&term(2, 5)).unwrap(), SelectionRange {
            start: Point::new(Line(0), Column(1)),
            end: Point::new(Line(1), Column(1)),
            is_block: false,
        });
    }

    #[test]
    fn line_selection() {
        let size = (10, 5);
        let mut selection =
            Selection::new(SelectionType::Lines, Point::new(Line(9), Column(1)), Side::Left);
        selection.update(Point::new(Line(4), Column(1)), Side::Right);
        selection = selection.rotate(&size, &(Line(0)..Line(size.0 as i32)), 4).unwrap();

        assert_eq!(selection.to_range(&written(size.0, size.1)).unwrap(), SelectionRange {
            start: Point::new(Line(0), Column(0)),
            end: Point::new(Line(5), Column(4)),
            is_block: false,
        });
    }

    #[test]
    fn semantic_selection() {
        let size = (10, 5);
        let mut selection =
            Selection::new(SelectionType::Semantic, Point::new(Line(9), Column(3)), Side::Left);
        selection.update(Point::new(Line(4), Column(1)), Side::Right);
        selection = selection.rotate(&size, &(Line(0)..Line(size.0 as i32)), 4).unwrap();

        assert_eq!(selection.to_range(&written(size.0, size.1)).unwrap(), SelectionRange {
            // b-terminal(#804): 경계 글자가 없는 격자라 낱말이 줄 전체다
            start: Point::new(Line(0), Column(0)),
            end: Point::new(Line(5), Column(4)),
            is_block: false,
        });
    }

    #[test]
    fn simple_selection() {
        let size = (10, 5);
        let mut selection =
            Selection::new(SelectionType::Simple, Point::new(Line(9), Column(3)), Side::Right);
        selection.update(Point::new(Line(4), Column(1)), Side::Right);
        selection = selection.rotate(&size, &(Line(0)..Line(size.0 as i32)), 4).unwrap();

        assert_eq!(selection.to_range(&term(size.0, size.1)).unwrap(), SelectionRange {
            start: Point::new(Line(0), Column(2)),
            end: Point::new(Line(5), Column(3)),
            is_block: false,
        });
    }

    #[test]
    fn block_selection() {
        let size = (10, 5);
        let mut selection =
            Selection::new(SelectionType::Block, Point::new(Line(9), Column(3)), Side::Right);
        selection.update(Point::new(Line(4), Column(1)), Side::Right);
        selection = selection.rotate(&size, &(Line(0)..Line(size.0 as i32)), 4).unwrap();

        assert_eq!(selection.to_range(&term(size.0, size.1)).unwrap(), SelectionRange {
            start: Point::new(Line(0), Column(2)),
            end: Point::new(Line(5), Column(3)),
            is_block: true
        });
    }

    #[test]
    fn simple_is_empty() {
        let mut selection =
            Selection::new(SelectionType::Simple, Point::new(Line(1), Column(0)), Side::Right);
        assert!(selection.is_empty());
        selection.update(Point::new(Line(1), Column(1)), Side::Left);
        assert!(selection.is_empty());
        selection.update(Point::new(Line(0), Column(0)), Side::Right);
        assert!(!selection.is_empty());
    }

    #[test]
    fn block_is_empty() {
        let mut selection =
            Selection::new(SelectionType::Block, Point::new(Line(1), Column(0)), Side::Right);
        assert!(selection.is_empty());
        selection.update(Point::new(Line(1), Column(1)), Side::Left);
        assert!(selection.is_empty());
        selection.update(Point::new(Line(1), Column(1)), Side::Right);
        assert!(!selection.is_empty());
        selection.update(Point::new(Line(0), Column(0)), Side::Right);
        assert!(selection.is_empty());
        selection.update(Point::new(Line(0), Column(1)), Side::Left);
        assert!(selection.is_empty());
        selection.update(Point::new(Line(0), Column(1)), Side::Right);
        assert!(!selection.is_empty());
    }

    #[test]
    fn rotate_in_region_up() {
        let size = (10, 5);
        let mut selection =
            Selection::new(SelectionType::Simple, Point::new(Line(7), Column(3)), Side::Right);
        selection.update(Point::new(Line(4), Column(1)), Side::Right);
        selection = selection.rotate(&size, &(Line(1)..Line(size.0 as i32 - 1)), 4).unwrap();

        assert_eq!(selection.to_range(&term(size.0, size.1)).unwrap(), SelectionRange {
            start: Point::new(Line(1), Column(0)),
            end: Point::new(Line(3), Column(3)),
            is_block: false,
        });
    }

    #[test]
    fn rotate_in_region_down() {
        let size = (10, 5);
        let mut selection =
            Selection::new(SelectionType::Simple, Point::new(Line(4), Column(3)), Side::Right);
        selection.update(Point::new(Line(1), Column(1)), Side::Left);
        selection = selection.rotate(&size, &(Line(1)..Line(size.0 as i32 - 1)), -5).unwrap();

        assert_eq!(selection.to_range(&term(size.0, size.1)).unwrap(), SelectionRange {
            start: Point::new(Line(6), Column(1)),
            end: Point::new(Line(8), size.last_column()),
            is_block: false,
        });
    }

    #[test]
    fn rotate_in_region_up_block() {
        let size = (10, 5);
        let mut selection =
            Selection::new(SelectionType::Block, Point::new(Line(7), Column(3)), Side::Right);
        selection.update(Point::new(Line(4), Column(1)), Side::Right);
        selection = selection.rotate(&size, &(Line(1)..Line(size.0 as i32 - 1)), 4).unwrap();

        assert_eq!(selection.to_range(&term(size.0, size.1)).unwrap(), SelectionRange {
            start: Point::new(Line(1), Column(2)),
            end: Point::new(Line(3), Column(3)),
            is_block: true,
        });
    }

    #[test]
    fn range_intersection() {
        let mut selection =
            Selection::new(SelectionType::Lines, Point::new(Line(3), Column(1)), Side::Left);
        selection.update(Point::new(Line(6), Column(1)), Side::Right);

        assert!(selection.intersects_range(..));
        assert!(selection.intersects_range(Line(2)..));
        assert!(selection.intersects_range(Line(2)..=Line(4)));
        assert!(selection.intersects_range(Line(2)..=Line(7)));
        assert!(selection.intersects_range(Line(4)..=Line(5)));
        assert!(selection.intersects_range(Line(5)..Line(8)));

        assert!(!selection.intersects_range(..=Line(2)));
        assert!(!selection.intersects_range(Line(7)..=Line(8)));
    }
}
