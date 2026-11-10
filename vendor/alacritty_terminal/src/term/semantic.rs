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
    pub fn semantic_new_prompt(&mut self, kind: Option<PromptKind>, redraw: Option<Redraw>) {
        self.semantic_fresh_line();
        self.semantic_prompt_start(kind);
        if let Some(redraw) = redraw {
            self.semantic.redraw = redraw;
        }
    }
}
