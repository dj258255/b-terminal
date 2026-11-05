//! 설정, 제어 프로토콜, 에이전트 상태 모델(ADR-003). 창을 모른다

pub mod agent;
pub mod theme;

pub use agent::{AgentKind, AgentState, WaitReason};
pub use theme::{Rgb, Theme};
