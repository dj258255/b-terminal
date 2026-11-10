//! 프롬프트 클릭(#338). 기준: Ghostty `Surface.maybePromptClick`(gh7/Surface.zig 4300~4420줄)과
//! `Screen.promptClickMove`·`promptClickLine`(gh10/Screen.zig 3639~3820줄).
//!
//! 셸이 OSC 133;A에 `click_events=`나 `cl=`을 밝힌 화면에서만 한다. `click_events`면 SGR 왼쪽 단추 누름 하나를,
//! `cl`이면 입력 칸(OSC 133;B 뒤에 쓴 칸)만 세어 ←·→를 보낸다. 보내는 바이트는 숫자와 고정 글자뿐이고 화면 글은 들어가지 않는다.

use alacritty_terminal::event::EventListener;
use alacritty_terminal::grid::Dimensions;
use alacritty_terminal::index::{Column, Line};
use alacritty_terminal::term::cell::Flags;
use alacritty_terminal::term::semantic::{ClickEvents, RowPrompt, SemanticClick, SemanticContent};
use alacritty_terminal::term::{Term, TermMode};

/// 눌러서 PTY에 보낼 바이트를 정한다. `row`는 보이는 화면의 줄(맨 위가 0), `col`은 칸.
/// 셸이 `cl=`·`click_events=`를 밝히지 않았거나 조건이 맞지 않거나 보낼 것이 없으면 None
pub fn prompt_click<T: EventListener>(term: &Term<T>, col: usize, row: usize) -> Option<Vec<u8>> {
    let click = term.semantic_click();
    if click == SemanticClick::None {
        return None;
    }
    // 대체 화면에서는 프롬프트가 아니다(Ghostty `cursorIsAtPrompt`)
    if term.mode().contains(TermMode::ALT_SCREEN) {
        return None;
    }
    let grid = term.grid();
    let cursor = grid.cursor.point;
    let content = term.semantic_content();
    let at_prompt = term.row_prompt(cursor.line) != RowPrompt::None
        || matches!(content, SemanticContent::Input | SemanticContent::Prompt);
    if !at_prompt {
        return None;
    }
    if col >= term.columns() || row >= term.screen_lines() {
        return None;
    }
    let click_line = Line(row as i32 - grid.display_offset() as i32);

    // 커서의 프롬프트: 커서 줄에서 위로 올라가며 처음 만나는 프롬프트 줄. 클릭이 그보다 앞이면 하지 않는다(뒤는 허용)
    let prompt_line = prompt_line_of(term, cursor.line)?;
    if click_line < prompt_line {
        return None;
    }

    let bytes = match click {
        SemanticClick::None => return None,
        SemanticClick::ClickEvents(ClickEvents::Absolute) => sgr_click(col + 1, row + 1),
        SemanticClick::ClickEvents(ClickEvents::Relative) => sgr_click(col + 1, (click_line.0 - prompt_line.0 + 1) as usize),
        // 네 값(line·m·v·w) 모두 지금은 한 줄 이동이다(Ghostty `promptClickMove`)
        SemanticClick::Cl(_) => {
            let (left, right) = prompt_click_line(term, click_line, col);
            move_bytes(left, right, term.mode().contains(TermMode::APP_CURSOR))
        }
    };
    (!bytes.is_empty()).then_some(bytes)
}

/// 커서 줄에서 위로(스크롤백까지) 올라가며 처음 만나는 프롬프트 첫 줄
fn prompt_line_of<T>(term: &Term<T>, from: Line) -> Option<Line> {
    let top = -(term.grid().history_size() as i32);
    let mut line = from.0;
    loop {
        if term.row_prompt(Line(line)) == RowPrompt::Prompt {
            return Some(Line(line));
        }
        if line <= top {
            return None;
        }
        line -= 1;
    }
}

fn is_input(cell: &alacritty_terminal::term::cell::Cell) -> bool {
    cell.flags.contains(Flags::SEMANTIC_INPUT)
}

fn is_spacer(cell: &alacritty_terminal::term::cell::Cell) -> bool {
    cell.flags.intersects(Flags::WIDE_CHAR_SPACER | Flags::LEADING_WIDE_CHAR_SPACER)
}

/// 커서에서 눌린 칸까지 움직일 ←·→ 수(Ghostty `promptClickLine`). 소프트 랩으로 이어진 줄은 한 줄로 본다.
/// 입력 칸만 세고, 넓은 글자는 한 번으로 센다(뒤 칸은 세지 않는다)
fn prompt_click_line<T>(term: &Term<T>, click_line: Line, click_col: usize) -> (usize, usize) {
    let grid = term.grid();
    let columns = term.columns();
    let cursor = grid.cursor.point;
    let (cursor_line, cursor_col) = (cursor.line.0, cursor.column.0);
    // 입력 중이 아닌 곳에서는 움직이지 않는다(Ghostty: 커서의 글 종류도 커서 칸도 입력이 아니면 0)
    let cursor_in_input = is_input(&grid[cursor.line][cursor.column]);
    if term.semantic_content() != SemanticContent::Input && !cursor_in_input {
        return (0, 0);
    }
    // 넓은 글자의 뒤 칸을 눌렀으면 그 글자를 누른 것이다
    let mut click_col = click_col;
    if click_col > 0 && grid[click_line][Column(click_col)].flags.contains(Flags::WIDE_CHAR_SPACER) {
        click_col -= 1;
    }
    if (click_line.0, click_col) == (cursor_line, cursor_col) {
        return (0, 0);
    }
    let wraps = |line: i32| grid[Line(line)][Column(columns - 1)].flags.contains(Flags::WRAPLINE);

    if (cursor_line, cursor_col) < (click_line.0, click_col) {
        // 오른쪽: 커서 뒤의 입력 칸을 눌린 칸까지 센다
        let mut count = 0;
        'rows: for line in cursor_line..=click_line.0 {
            let row = &grid[Line(line)];
            let is_cursor_row = line == cursor_line;
            // 커서 줄이 아니면 처음 프롬프트에서 이어진 줄이어야 한다
            if !is_cursor_row && term.row_prompt(Line(line)) != RowPrompt::Continuation {
                break;
            }
            // 이어진 줄은 첫 입력 칸에서 시작한다(셸이 다음 줄 앞으로 화살표를 옮긴다고 본다)
            let start = if is_cursor_row {
                cursor_col + 1
            } else {
                (0..columns).find(|&x| is_input(&row[Column(x)])).unwrap_or(columns)
            };
            for x in start..columns {
                let cell = &row[Column(x)];
                if !is_input(cell) || is_spacer(cell) {
                    continue;
                }
                count += 1;
                if line == click_line.0 && x == click_col {
                    break 'rows;
                }
            }
            if !wraps(line) {
                // 눌린 칸을 못 찾았다면 입력 끝 너머를 누른 것이다. 커서가 입력 칸이면 빈 끝 칸까지 하나 더
                if cursor_in_input {
                    count += 1;
                }
                break;
            }
        }
        (0, count)
    } else {
        // 왼쪽: 커서 앞의 입력 칸을 눌린 칸까지 센다
        let mut count = 0;
        'rows: for line in (click_line.0..=cursor_line).rev() {
            let row = &grid[Line(line)];
            let end = if line == cursor_line { cursor_col } else { columns };
            for x in (0..end).rev() {
                let cell = &row[Column(x)];
                if !is_input(cell) || is_spacer(cell) {
                    continue;
                }
                count += 1;
                if line == click_line.0 && x == click_col {
                    break 'rows;
                }
            }
            // 윗줄이 이어지지 않은 줄이면 여기서 끝이다(`wrap_continuation`)
            let continued = line > -(grid.history_size() as i32) && wraps(line - 1);
            if !continued {
                break;
            }
        }
        (count, 0)
    }
}

/// `cl=`이 정한 이동 바이트: 왼쪽 `left`번, 오른쪽 `right`번. DECCKM이면 `ESC O D`·`ESC O C`, 아니면 `ESC [ D`·`ESC [ C`
pub fn move_bytes(left: usize, right: usize, app_cursor: bool) -> Vec<u8> {
    let (l, r): (&[u8], &[u8]) = if app_cursor { (b"\x1bOD", b"\x1bOC") } else { (b"\x1b[D", b"\x1b[C") };
    let mut out = Vec::with_capacity((left + right) * 3);
    for _ in 0..left {
        out.extend_from_slice(l);
    }
    for _ in 0..right {
        out.extend_from_slice(r);
    }
    out
}

/// `click_events=`가 정한 SGR 왼쪽 단추 누름 `ESC [ < 0 ; x ; y M`. `x`·`y`는 1부터 센다
pub fn sgr_click(x: usize, y: usize) -> Vec<u8> {
    format!("\x1b[<0;{x};{y}M").into_bytes()
}
