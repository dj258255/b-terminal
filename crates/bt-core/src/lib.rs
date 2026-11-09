//! 설정, 제어 프로토콜, 에이전트 상태 모델(ADR-003). 창을 모른다

pub mod clipboard;
pub mod config;
pub mod proto;
pub mod theme;

pub use config::{Config, LinkPreviews, PaletteEntryConf};
pub use theme::{Rgb, Theme};
