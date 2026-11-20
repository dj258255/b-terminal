//! 명령 출력 고르기(#700): ⌘(macOS)·Ctrl+세 번 누르기로 누른 자리의 명령 출력 구간 전체를 고르고, 끌면 구간 단위로 늘린다.
//! 기준: Ghostty v1.4.0 `Screen.selectOutput`(3558~3610줄), `PageList.highlightSemanticContent`의 `.output`,
//! `PageList.PromptIterator`(6247~6394줄), `SelectionGesture.dragSelectionOutput`(906~925줄).
//!
//! 칸의 의미는 엔진 표시로 가른다: `SEMANTIC_PROMPT`는 프롬프트, `SEMANTIC_INPUT`은 입력, 나머지는 출력(Ghostty 기본값 `.output`).
//! 줄의 프롬프트 표시는 0번 칸의 `ROW_PROMPT`·`ROW_PROMPT_CONTINUATION`이다. 좌표는 엔진 격자 좌표(스크롤백은 음수 줄)이고 보고 있는 화면을 본다

use alacritty_terminal::grid::Dimensions;
use alacritty_terminal::index::{Column, Line, Point};
use alacritty_terminal::term::Term;
use alacritty_terminal::term::cell::{Cell, Flags};
use alacritty_terminal::term::semantic::RowPrompt;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Kind {
    Prompt,
    Input,
    Output,
}

fn kind(cell: &Cell) -> Kind {
    if cell.flags.contains(Flags::SEMANTIC_INPUT) {
        Kind::Input
    } else if cell.flags.contains(Flags::SEMANTIC_PROMPT) {
        Kind::Prompt
    } else {
        Kind::Output
    }
}

/// Ghostty `cell.hasText()`: 글자를 찍은 칸(찍은 빈칸 포함). 넓은 글자의 뒤 칸·줄 끝 자리 칸은 아니다(`xt_checksum`과 같은 판정)
fn has_text(cell: &Cell) -> bool {
    cell.flags.contains(Flags::WRITTEN)
        && !cell
            .flags
            .intersects(Flags::WIDE_CHAR_SPACER | Flags::LEADING_WIDE_CHAR_SPACER)
}

fn cell<T>(term: &Term<T>, p: Point) -> &Cell {
    &term.grid()[p.line][p.column]
}

fn last_col<T>(term: &Term<T>) -> Column {
    Column(term.columns().saturating_sub(1))
}

/// `from` 줄(포함)에서 위로 올라가며 찾은 프롬프트 첫 줄(Ghostty `PromptIterator.nextLeftUp`)
fn prompt_up<T>(term: &Term<T>, from: Line) -> Option<Line> {
    let top = term.topmost_line();
    let mut line = from;
    loop {
        match term.row_prompt(line) {
            RowPrompt::None => {}
            RowPrompt::Prompt => return Some(line),
            // 이어진 줄이면 프롬프트 첫 줄이나 표시 없는 줄을 만날 때까지 올라간다. 표시 없는 줄을 만나면 그 바로 아래 이어진 줄이 첫 줄이다
            RowPrompt::Continuation => {
                let mut end = line;
                while end > top {
                    let prior = end - 1;
                    match term.row_prompt(prior) {
                        RowPrompt::None => return Some(end),
                        RowPrompt::Continuation => end = prior,
                        RowPrompt::Prompt => return Some(prior),
                    }
                }
                // 위에 줄이 없다(잘린 스크롤백)
                return Some(line);
            }
        }
        if line <= top {
            return None;
        }
        line -= 1;
    }
}

/// `from` 줄(포함)에서 아래로 내려가며 찾은 프롬프트 첫 줄과, 그 프롬프트에 딸린 이어진 줄들 바로 다음 줄
/// (Ghostty `PromptIterator.nextRightDown`)
fn prompt_down<T>(term: &Term<T>, from: Line) -> Option<(Line, Line)> {
    let bottom = term.bottommost_line();
    let mut line = from;
    while line <= bottom {
        if term.row_prompt(line) != RowPrompt::None {
            let mut next = line + 1;
            while next <= bottom && term.row_prompt(next) == RowPrompt::Continuation {
                next += 1;
            }
            return Some((line, next));
        }
        line += 1;
    }
    None
}

/// 칸을 줄 순서로 한 칸 앞으로. 마지막 칸이면 None
fn step_forward<T>(term: &Term<T>, p: Point) -> Option<Point> {
    if p.column < last_col(term) {
        Some(Point::new(p.line, p.column + 1))
    } else if p.line < term.bottommost_line() {
        Some(Point::new(p.line + 1, Column(0)))
    } else {
        None
    }
}

/// 칸을 줄 순서로 한 칸 뒤로. 첫 칸이면 None
fn step_back<T>(term: &Term<T>, p: Point) -> Option<Point> {
    if p.column > Column(0) {
        Some(Point::new(p.line, p.column - 1))
    } else if p.line > term.topmost_line() {
        Some(Point::new(p.line - 1, last_col(term)))
    } else {
        None
    }
}

/// `prompt` 줄의 명령 출력 구간(Ghostty `highlightSemanticContent(prompt, .output)`): 프롬프트 첫 줄부터 다음 프롬프트 바로 위 줄
/// (없으면 화면 끝)까지에서, 프롬프트·입력·글 없는 칸을 건너뛴 첫 출력 글자부터 다음 프롬프트·입력 칸 앞의 마지막 출력 글자까지
fn output_of_prompt<T>(term: &Term<T>, prompt: Line) -> Option<(Point, Point)> {
    let screen_end = Point::new(term.bottommost_line(), last_col(term));
    let end = match prompt_down(term, prompt) {
        Some((_, after)) => match prompt_down(term, after) {
            Some((next, _)) if next > term.topmost_line() => Point::new(next - 1, last_col(term)),
            _ => screen_end,
        },
        None => screen_end,
    };
    let mut at = Some(Point::new(prompt, Column(0)));
    let mut found: Option<(Point, Point)> = None;
    while let Some(p) = at.filter(|p| *p <= end) {
        let c = cell(term, p);
        match (kind(c), found.as_mut()) {
            (Kind::Prompt | Kind::Input, None) => {}
            (Kind::Prompt | Kind::Input, Some(_)) => break,
            (Kind::Output, None) => {
                if has_text(c) {
                    found = Some((p, p));
                }
            }
            (Kind::Output, Some(range)) => {
                if has_text(c) {
                    range.1 = p;
                }
            }
        }
        at = step_forward(term, p);
    }
    found
}

/// `at` 칸이 출력이면 그 명령의 출력 구간(첫 칸, 끝 칸, 둘 다 포함). 출력이 아니거나 고를 것이 없으면 None
pub fn select_output<T>(term: &Term<T>, at: Point) -> Option<(Point, Point)> {
    if kind(cell(term, at)) != Kind::Output {
        return None;
    }
    if let Some(prompt) = prompt_up(term, at.line) {
        // 끝 빈칸 다듬기는 `output_of_prompt`가 이미 글자 칸에서 끝내므로 따로 하지 않는다
        return output_of_prompt(term, prompt);
    }
    // 위에 프롬프트가 없으면 화면 맨 위부터 다음 프롬프트 바로 위까지이고, 끝의 글 없는 칸을 뺀다
    let (next, _) = prompt_down(term, at.line)?;
    if next <= term.topmost_line() {
        return None;
    }
    let start = Point::new(term.topmost_line(), Column(0));
    let mut end = Point::new(next - 1, last_col(term));
    loop {
        if has_text(cell(term, end)) {
            break;
        }
        match step_back(term, end) {
            Some(p) if p >= start => end = p,
            _ => break,
        }
    }
    Some((start, end))
}

/// 출력 고르기를 끄는 중: 누른 자리의 구간에서 끈 자리의 구간까지 늘린다. 끈 자리가 출력이 아니면 누른 자리의 구간 그대로
pub fn drag_output<T>(term: &Term<T>, click: Point, drag: Point) -> Option<(Point, Point)> {
    let mut sel = select_output(term, click)?;
    let Some(current) = select_output(term, drag) else {
        return Some(sel);
    };
    if drag < click {
        sel.0 = current.0;
    } else {
        sel.1 = current.1;
    }
    Some(sel)
}
