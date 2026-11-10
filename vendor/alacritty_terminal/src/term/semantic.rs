//! b-terminal(#329): OSC 133 의미 있는 프롬프트의 엔진 쪽 상태(Ghostty `Terminal.semanticPrompt`, Screen의 `cursorSetSemanticContent`).
//!
//! 줄의 표시(프롬프트·이어진 프롬프트)는 그 줄 0번 칸의 `Flags::ROW_PROMPT`·`ROW_PROMPT_CONTINUATION`에 둔다.
//! 칸이 줄과 함께 스크롤백으로 가고 다시 감싸일 때도 따라가고, 글자를 덮어써도 남는다(`write_at_cursor`).

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
    /// 이 터미널이 프롬프트·입력 표지를 받은 적이 있는가(없으면 ED 2는 그냥 지운다)
    pub(super) seen: bool,
    pub(super) redraw: Redraw,
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
    /// 줄(스크롤백은 음수)의 프롬프트 표시
    pub fn row_prompt(&self, _line: Line) -> RowPrompt {
        RowPrompt::None
    }
    pub fn semantic_prompt_start(&mut self, _kind: Option<PromptKind>) {}
    pub fn semantic_input(&mut self, _clear_eol: bool) {}
    pub fn semantic_end_input_start_output(&mut self) {}
    pub fn semantic_end_command(&mut self) {}
}

impl<T: EventListener> Term<T> {
    pub fn semantic_fresh_line(&mut self) {}
    pub fn semantic_new_prompt(&mut self, _kind: Option<PromptKind>, _redraw: Option<Redraw>) {}
}
