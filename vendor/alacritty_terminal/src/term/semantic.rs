//! b-terminal(#329): OSC 133 의미 있는 프롬프트의 엔진 쪽 상태(Ghostty `Terminal.semanticPrompt`, Screen의 `cursorSetSemanticContent`).
//!
//! 줄의 표시(프롬프트·이어진 프롬프트)는 그 줄 0번 칸의 `Flags::ROW_PROMPT`·`ROW_PROMPT_CONTINUATION`에 둔다.
//! 칸이 줄과 함께 스크롤백으로 가고 다시 감싸일 때도 따라가고, 글자를 덮어써도 남는다(`write_at_cursor`).
//! 칸을 지우면(EL·ED·ICH 등) 표시도 함께 사라진다(Ghostty는 줄의 표시가 남는다. 다시 그리기가 지울 때는 남긴다).

use super::*;

/// 커서가 지금 쓰는 글의 종류(Ghostty `cursor.semantic_content`)
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum SemanticContent {
    #[default]
    Output,
    Prompt,
    Input,
}

/// `133;A`·`P`의 `k=`(Ghostty `PromptKind`)
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PromptKind {
    Initial,
    Right,
    Continuation,
    Secondary,
}

/// `redraw=`: 크기가 바뀔 때 셸이 프롬프트를 다시 그리는가(Ghostty `Redraw`)
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Redraw {
    #[default]
    True,
    False,
    Last,
}

/// `click_events=`(Ghostty `semantic_prompt.ClickEvents`): 1은 화면 좌표, 2는 프롬프트 기준 줄
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ClickEvents {
    Absolute,
    Relative,
}

/// `cl=`(Ghostty `semantic_prompt.Click`)
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Click {
    Line,
    Multiple,
    ConservativeVertical,
    SmartVertical,
}

/// 화면이 기억하는 프롬프트 클릭 방식(Ghostty `Screen.SemanticClick`). 셸이 밝히기 전에는 `None`이고 클릭은 아무것도 하지 않는다
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum SemanticClick {
    #[default]
    None,
    ClickEvents(ClickEvents),
    Cl(Click),
}

/// 줄의 프롬프트 표시(Ghostty `row.semantic_prompt`)
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RowPrompt {
    None,
    Prompt,
    Continuation,
}

#[derive(Debug, Clone, Copy, Default)]
pub(super) struct SemanticState {
    pub(super) content: SemanticContent,
    pub(super) clear_eol: bool,
    /// 이 터미널이 프롬프트·입력 표지를 받은 적이 있는가(없으면 ED 2는 그냥 지운다. Ghostty `semantic_prompt.seen`)
    pub(super) seen: bool,
    pub(super) redraw: Redraw,
    pub(super) click: SemanticClick,
}

const ROW_MARKS: Flags = Flags::ROW_PROMPT.union(Flags::ROW_PROMPT_CONTINUATION);

fn mark_of(flags: Flags) -> RowPrompt {
    if flags.contains(Flags::ROW_PROMPT) {
        RowPrompt::Prompt
    } else if flags.contains(Flags::ROW_PROMPT_CONTINUATION) {
        RowPrompt::Continuation
    } else {
        RowPrompt::None
    }
}

impl<T> Term<T> {
    pub fn semantic_content(&self) -> SemanticContent {
        self.semantic.content
    }

    pub fn semantic_seen(&self) -> bool {
        self.semantic.seen
    }

    /// 셸이 `133;A`의 `click_events=`·`cl=`로 밝힌 클릭 방식
    pub fn semantic_click(&self) -> SemanticClick {
        self.semantic.click
    }

    pub fn semantic_redraw(&self) -> Redraw {
        self.semantic.redraw
    }

    /// 줄(스크롤백은 음수)의 프롬프트 표시. 보고 있는 화면(대체 화면이면 대체 화면)의 줄이다
    pub fn row_prompt(&self, line: Line) -> RowPrompt {
        mark_of(self.grid[line][Column(0)].flags)
    }

    fn set_row_prompt(&mut self, line: Line, mark: RowPrompt) {
        let flags = &mut self.grid[line][Column(0)].flags;
        flags.remove(ROW_MARKS);
        match mark {
            RowPrompt::None => (),
            RowPrompt::Prompt => flags.insert(Flags::ROW_PROMPT),
            RowPrompt::Continuation => flags.insert(Flags::ROW_PROMPT_CONTINUATION),
        }
    }

    /// 프롬프트 글 시작(`A`·`N`·`P`): 커서 줄을 표시하고 이후 글을 프롬프트로 본다(Ghostty `cursorSetSemanticContent(.prompt)`)
    pub fn semantic_prompt_start(&mut self, kind: Option<PromptKind>) {
        self.semantic.seen = true;
        self.semantic.content = SemanticContent::Prompt;
        self.semantic.clear_eol = false;
        let mark = match kind.unwrap_or(PromptKind::Initial) {
            PromptKind::Initial | PromptKind::Right => RowPrompt::Prompt,
            PromptKind::Continuation | PromptKind::Secondary => RowPrompt::Continuation,
        };
        let line = self.grid.cursor.point.line;
        self.set_row_prompt(line, mark);
    }

    /// 입력 시작(`B`는 `clear_eol`이 거짓, `I`는 참). 이어지는 줄은 줄바꿈 때 표시한다(`semantic_after_index`)
    pub fn semantic_input(&mut self, clear_eol: bool) {
        self.semantic.seen = true;
        self.semantic.content = SemanticContent::Input;
        self.semantic.clear_eol = clear_eol;
    }

    fn semantic_output(&mut self) {
        self.semantic.content = SemanticContent::Output;
        self.semantic.clear_eol = false;
    }

    /// `C`: 출력 시작. 커서 줄이 프롬프트로 표시돼 있고 0번 칸이면 프롬프트를 덮어쓰는 것으로 보고 표시를 지운다
    /// (Ghostty 2878~2891줄, fish를 위한 휴리스틱)
    pub fn semantic_end_input_start_output(&mut self) {
        self.semantic_output();
        let line = self.grid.cursor.point.line;
        if self.row_prompt(line) != RowPrompt::None && self.grid.cursor.point.column.0 == 0 {
            self.set_row_prompt(line, RowPrompt::None);
        }
    }

    /// `D`: 명령 끝. 출력 상태로 돌아간다
    pub fn semantic_end_command(&mut self) {
        self.semantic_output();
    }

    /// 줄바꿈(LF·IND·NEL·fresh-line)이 끝난 뒤(Ghostty `index`의 defer, 3023~3050줄).
    /// 입력 끝이 줄 끝이면 출력으로 돌아가고, 아니면 새 줄을 이어진 프롬프트로 표시한다
    /// (OSC 133;k=s를 보내지 않는 셸을 위한 것이다)
    pub(super) fn semantic_after_index(&mut self) {
        if self.semantic.content == SemanticContent::Output {
            return;
        }
        if self.semantic.clear_eol {
            self.semantic_output();
        } else {
            let line = self.grid.cursor.point.line;
            self.set_row_prompt(line, RowPrompt::Continuation);
        }
    }

    /// 글자가 줄 끝에서 넘어갈 때의 상태를 줄 넘김 앞뒤로 지킨다(Ghostty `printWrap` 2450~2480줄)
    pub(super) fn semantic_state(&self) -> SemanticState {
        self.semantic
    }

    pub(super) fn semantic_after_wrap(&mut self, old: SemanticState) {
        self.semantic.content = old.content;
        self.semantic.clear_eol = old.clear_eol;
        if old.content == SemanticContent::Prompt {
            let line = self.grid.cursor.point.line;
            self.set_row_prompt(line, RowPrompt::Continuation);
        }
    }

    /// 초기화(RIS)
    pub(super) fn semantic_reset(&mut self) {
        self.semantic = SemanticState::default();
    }

    /// 크기를 바꾼 뒤, 커서가 프롬프트·입력에 있으면 프롬프트 줄을 비워 셸이 다시 그리게 한다
    /// (Ghostty `Screen.clearPromptForRedraw`, Screen.zig 2447~2505줄. 주 화면만 한다).
    /// `redraw=1`(기본)이면 커서를 따라 올라가 프롬프트 첫 줄부터 화면 맨 아래까지, `last`면 커서 줄만, `0`이면 아무것도 하지 않는다.
    /// 칸만 비우고 줄의 표시와 줄 이음(WRAPLINE)은 남긴다(Ghostty는 줄 자체를 지우지 않는다).
    /// 프롬프트 첫 줄을 찾는 규칙은 PageList.promptIterator 원문이 없어 Ghostty 동작을 읽고 추정한 것이다:
    /// 커서 줄에서 위로 올라가며 이어진 줄·표시 없는 줄을 지나 처음 만나는 프롬프트 줄
    pub(super) fn semantic_clear_prompt_for_redraw(&mut self) {
        let redraw = self.semantic.redraw;
        if redraw == Redraw::False || self.semantic.content == SemanticContent::Output {
            return;
        }
        let grid = if self.mode.contains(TermMode::ALT_SCREEN) { &mut self.inactive_grid } else { &mut self.grid };
        let cursor_line = grid.cursor.point.line.0;
        let first = match redraw {
            Redraw::False => return,
            Redraw::Last => cursor_line,
            Redraw::True => {
                let top = -(grid.history_size() as i32);
                let mut line = cursor_line;
                loop {
                    if mark_of(grid[Line(line)][Column(0)].flags) == RowPrompt::Prompt {
                        break line;
                    }
                    if line <= top {
                        return;
                    }
                    line -= 1;
                }
            },
        };
        let last = if redraw == Redraw::Last { cursor_line } else { grid.screen_lines() as i32 - 1 };
        let columns = grid.columns();
        for line in first..=last {
            let row = &mut grid[Line(line)];
            let marks = row[Column(0)].flags & ROW_MARKS;
            let wrap = row[Column(columns - 1)].flags & Flags::WRAPLINE;
            for column in 0..columns {
                row[Column(column)] = Cell::default();
            }
            row[Column(0)].flags.insert(marks);
            row[Column(columns - 1)].flags.insert(wrap);
        }
    }

    /// ED 2를 받았을 때 프롬프트에 있는가: 주 화면이고 표지를 받은 적이 있고, 보이는 화면 맨 아래에서 올라가며 처음 만나는
    /// 글 있는 줄이 프롬프트·이어진 프롬프트면 참이다. 출력(표시 없는 글 줄)을 먼저 만나면 거짓이다.
    /// (Ghostty `eraseDisplay`의 `.complete`, Terminal.zig 4482~4513줄. 표지를 받은 적이 없으면 줄을 읽지도 않는다)
    fn semantic_cursor_at_prompt_for_clear(&self) -> bool {
        if self.mode.contains(TermMode::ALT_SCREEN) || !self.semantic.seen {
            return false;
        }
        for line in (0..self.screen_lines() as i32).rev() {
            let row = &self.grid[Line(line)];
            if mark_of(row[Column(0)].flags) != RowPrompt::None {
                return true;
            }
            if row[Column(0)..].iter().any(|cell| cell.flags.contains(Flags::WRITTEN)) {
                return false;
            }
        }
        false
    }

    /// ED 2(`CSI 2 J`)가 프롬프트에서 왔으면 지우기 전에 화면을 스크롤백으로 올린다. 올렸으면 참
    /// (Ghostty `scrollClear`). 올리지 않으면 거짓이고 부르는 쪽이 그냥 지운다 — 프롬프트가 아니면 스크롤백에 쌓지 않는다
    pub(super) fn semantic_scroll_clear_at_prompt(&mut self) -> bool {
        if !self.semantic_cursor_at_prompt_for_clear() {
            return false;
        }
        self.scroll_clear_viewport();
        true
    }

    /// 보이는 화면의 글을 스크롤백으로 올려 화면을 비운다(Ghostty `Screen.scrollClear`). 프롬프트의 ED 2와 ED 22(#327)가 함께 쓴다
    pub(super) fn scroll_clear_viewport(&mut self) {
        let old_offset = self.grid.display_offset();
        self.grid.clear_viewport();
        // 올린 줄 수만큼 vi 커서가 따라간다
        let lines = self.grid.display_offset().saturating_sub(old_offset);
        self.vi_mode_cursor.point.line = (self.vi_mode_cursor.point.line - lines).grid_clamp(self, Boundary::Grid);
    }
}

impl<T: EventListener> Term<T> {
    /// `L`: fresh-line. 커서가 줄의 처음이 아니면 줄을 바꾼다(Ghostty `semanticPromptFreshLine`, 2905~2922줄).
    /// 커서가 왼쪽 여백 왼쪽에 있으면 0번 칸을, 아니면 왼쪽 여백을 줄의 처음으로 본다
    pub fn semantic_fresh_line(&mut self) {
        let column = self.grid.cursor.point.column.0;
        let left = if column < self.margin_left { 0 } else { self.margin_left };
        if column == left {
            return;
        }
        Handler::carriage_return(self);
        Handler::linefeed(self);
    }

    /// `A`·`N`: fresh-line을 한 뒤 프롬프트 글을 시작한다. `redraw=`가 있으면 기억한다
    pub fn semantic_new_prompt(
        &mut self,
        kind: Option<PromptKind>,
        redraw: Option<Redraw>,
        click: Option<SemanticClick>,
    ) {
        self.semantic_fresh_line();
        self.semantic_prompt_start(kind);
        if let Some(redraw) = redraw {
            self.semantic.redraw = redraw;
        }
        // click_events가 먼저이고 없으면 cl이다. 둘 다 없으면 지난 값을 그대로 둔다(Ghostty `semanticPrompt`의 `click:` 블록)
        if let Some(click) = click {
            self.semantic.click = click;
        }
    }
}
