//! PTY와 VT 해석, 알림 스캐너, 키 → 바이트(ADR-001). 엔진 타입을 밖으로 내보내지 않는다

mod color;
pub mod input;
pub mod notify;
pub mod process;
pub mod screen;
mod session;

pub use input::{Mods, NamedKey, encode_named, encode_paste, encode_text};
pub use notify::{Notification, Signal};
pub use process::Foreground;
pub use screen::{Cell, CellFlags, Cursor, CursorShape, Modes, Screen};
pub use session::{Emulator, EventSink, GridSize, SelectKind, Session, SessionEvent, SpawnOptions};
