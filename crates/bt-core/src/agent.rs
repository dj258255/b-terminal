//! 탭의 에이전트 상태. 훅·OSC·프로세스 이름처럼 관찰한 신호로만 바꾼다(CONTRIBUTING "변경 원칙")

use serde::{Deserialize, Serialize};

/// 에이전트가 무엇을 기다리나
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum WaitReason {
    /// 도구 실행 허락
    Permission,
    /// 사람에게 물음
    Question,
    /// 다음 요청을 기다림(턴 끝)
    Input,
}

/// 탭 하나의 상태
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case", tag = "state", content = "reason")]
pub enum AgentState {
    /// 에이전트가 아니거나 아무 일도 없음
    #[default]
    Idle,
    Working,
    Waiting(WaitReason),
    /// 턴이 끝났고 아직 보지 않음
    Done,
    Error,
}

impl AgentState {
    /// 사람이 봐야 하는 정도. 클수록 급하다(사이드바 정렬, 다음 볼 탭 고르기)
    pub fn urgency(self) -> u8 {
        match self {
            AgentState::Waiting(WaitReason::Permission) | AgentState::Waiting(WaitReason::Question) => 5,
            AgentState::Error => 4,
            AgentState::Waiting(WaitReason::Input) | AgentState::Done => 3,
            AgentState::Working => 1,
            AgentState::Idle => 0,
        }
    }

    /// 사이드바에 쓰는 짧은 말
    pub fn label(self) -> &'static str {
        match self {
            AgentState::Idle => "쉼",
            AgentState::Working => "작업 중",
            AgentState::Waiting(WaitReason::Permission) => "권한 기다림",
            AgentState::Waiting(WaitReason::Question) => "질문 기다림",
            AgentState::Waiting(WaitReason::Input) => "입력 기다림",
            AgentState::Done => "끝남",
            AgentState::Error => "오류",
        }
    }
}

/// 알려진 AI 코딩 CLI
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AgentKind {
    Claude,
    Codex,
    OpenCode,
    Gemini,
    Aider,
    Other,
}

impl AgentKind {
    /// 전경 프로세스 이름(또는 명령줄 첫 낱말)으로 알아본다
    pub fn from_process(name: &str) -> Option<Self> {
        let base = name.rsplit('/').next().unwrap_or(name).to_ascii_lowercase();
        Some(match base.as_str() {
            "claude" => AgentKind::Claude,
            "codex" => AgentKind::Codex,
            "opencode" => AgentKind::OpenCode,
            "gemini" => AgentKind::Gemini,
            "aider" => AgentKind::Aider,
            "cursor-agent" | "amp" | "goose" | "copilot" | "crush" => AgentKind::Other,
            _ => return None,
        })
    }

    pub fn name(self) -> &'static str {
        match self {
            AgentKind::Claude => "Claude",
            AgentKind::Codex => "Codex",
            AgentKind::OpenCode => "OpenCode",
            AgentKind::Gemini => "Gemini",
            AgentKind::Aider => "Aider",
            AgentKind::Other => "AI",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn waiting_for_permission_is_most_urgent() {
        let mut v = [
            AgentState::Working,
            AgentState::Done,
            AgentState::Waiting(WaitReason::Permission),
            AgentState::Idle,
        ];
        v.sort_by_key(|s| std::cmp::Reverse(s.urgency()));
        assert_eq!(v[0], AgentState::Waiting(WaitReason::Permission));
        assert_eq!(v[3], AgentState::Idle);
    }

    #[test]
    fn process_names_map_to_agents() {
        assert_eq!(AgentKind::from_process("claude"), Some(AgentKind::Claude));
        assert_eq!(
            AgentKind::from_process("/opt/homebrew/bin/codex"),
            Some(AgentKind::Codex)
        );
        assert_eq!(AgentKind::from_process("zsh"), None);
    }

    #[test]
    fn state_serializes_with_reason() {
        let s = serde_json::to_string(&AgentState::Waiting(WaitReason::Permission)).unwrap();
        assert_eq!(s, r#"{"state":"waiting","reason":"permission"}"#);
        let back: AgentState = serde_json::from_str(r#"{"state":"working"}"#).unwrap();
        assert_eq!(back, AgentState::Working);
    }
}
