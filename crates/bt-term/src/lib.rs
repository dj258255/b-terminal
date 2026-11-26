//! PTY와 VT 해석, 알림 스캐너, 키 → 바이트(ADR-001). 엔진 타입을 밖으로 내보내지 않는다

mod color;
pub mod copy_format;
pub mod csi_query;
pub mod dcs;
pub mod glyph;
pub mod graphics;
pub mod hints;
pub mod input;
pub mod kitty_anim;
pub mod kitty_clip;
pub mod kitty_color;
pub mod kitty_key;
pub mod kitty_notify;
pub mod kitty_unicode;
pub mod latency;
pub mod links;
pub mod mouse_shape;
pub mod notify;
#[doc(hidden)]
pub mod osc133;
pub mod output_select;
pub mod plain;
pub mod process;
pub mod program_status;
pub mod prompt_click;
pub mod saved_screen;
pub mod screen;
pub mod search;
mod session;
pub mod xt_checksum;

pub use copy_format::{CopyFormat, Emit as CopyEmit};
pub use input::{
    KeyModes, KeypadKey, KittyFlags, LARGE_PASTE_BYTES, Mods, NamedKey, TextKey, apply_paste_actions, ctrl_seq,
    encode_keypad, encode_modify_other_char, encode_named, encode_named_modify_other, encode_named_with, encode_paste,
    encode_text, encode_text_key, encode_text_with, keypad_application, paste_is_large, paste_is_safe,
    quote_url_for_shell, replace_newlines, trim_trailing_spaces,
};
pub use kitty_key::{KeyAction, KittyKey, KittyKeyEvent, ModifierKey, encode_kitty_key};
pub use mouse_shape::MouseShape;
pub use notify::{Notification, PromptMark, Signal};
pub use process::Foreground;
pub use screen::{Cell, CellFlags, Cursor, CursorShape, ImageView, Modes, MouseFormat, Screen};
pub use session::{
    CellSide, ClearScreen, ClipboardReply, CommandRecord, Emulator, EventSink, GridSize, SelectKind, SelectionAdjust,
    Session, SessionEvent, SpawnOptions, WordHit, window_resize_target,
};
