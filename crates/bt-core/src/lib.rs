//! 설정, 제어 프로토콜, 테마, 클립보드 정책(ADR-003). 에이전트 상태 모델은 ADR-005로 걷어냈다. 창을 모른다

pub mod clipboard;
pub mod config;
pub mod kitty_geom;
pub mod paneborder;
pub mod proto;
pub mod tabstyle;
pub mod tabtitle;
pub mod text_composition;
pub mod theme;
pub mod visual_bell;

pub use config::{AlphaBlending, Config, LinkPreviews, PaletteEntryConf};
pub use text_composition::TextComposition;
pub use theme::{Rgb, Theme};
