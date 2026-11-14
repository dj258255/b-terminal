//! 설정, 제어 프로토콜, 테마, 클립보드 정책(ADR-003). 에이전트 상태 모델은 ADR-005로 걷어냈다. 창을 모른다

pub mod clipboard;
pub mod config;
pub mod proto;
pub mod theme;

pub use config::{AlphaBlending, Config, LinkPreviews, PaletteEntryConf};
pub use theme::{Rgb, Theme};
