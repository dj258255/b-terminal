//! b-terminal(#634): AppleScript support through Cocoa Scripting (as the Ghostty macOS app does). winit 0.30 has no
//! scripting API; this module is the only place that touches it. The app sees safe functions in `platform::macos`.
//!
//! How it fits together:
//! - The app bundle's `Info.plist` names a scripting definition (`b-terminal.sdef`). Cocoa Scripting reads it and maps
//!   the dictionary's classes and keys to Objective-C classes and methods by name.
//! - `NSApplication` gets the application-level keys (`scriptWindows`, `terminals`, `btFrontWindow`, `name`, `version`)
//!   and the `new window` / `new tab` handlers through `class_addMethod` (Ghostty does the same with a Swift extension).
//!   `class_addMethod` does not replace a method AppKit already has, so the keys avoid AppKit's names (`frontWindow` is
//!   a private `NSApplication` method that returns an `NSWindow`; the sdef names `btFrontWindow` instead, #645). `quit`
//!   uses AppKit's own `handleQuitScriptCommand:`, which terminates the app as the plain Apple Event handler did (#644).
//! - `window`, `tab` and `terminal` objects are small classes holding only an id string (`BTScriptWindow`, ...). Their
//!   properties are looked up in a snapshot of the app's state. The snapshot is refreshed (one `Snapshot` command to the
//!   app) when a read finds it older than `SNAPSHOT_FRESH`, so a script that reads many properties asks the app once.
//!   Each object builds an `NSUniqueIDSpecifier` so Cocoa can name it again.
//! - Commands that change something go to a handler the app registers. The handler runs on the main thread (Apple
//!   Events are delivered there) but the app's state lives behind its event loop, so the handler only queues the
//!   request; this module spins the run loop until the app answers (or a timeout passes).
//!
//! - `send key`, `split`, `close`, `close tab` and `close window` (#650) follow the same pattern: `send key` is a command
//!   class (`BTScriptSendKeyCommand`, it names its terminal in an argument); the others are `handle...Command:` methods
//!   of the object they act on. This module only parses the Apple Event; the key names, the key encoding and the close
//!   policy belong to the app.
//!
//! - `perform action` (#658) is also a command class (`BTScriptPerformActionCommand`: the action string is the direct
//!   parameter, the terminal is the `on` argument) so no selector is added to `NSApplication` or `NSObject`. Its answer is
//!   a boolean (`ScriptTarget::Performed`, returned as an `NSNumber`); the action string is read by the app.
//!
//! - `send mouse button`, `send mouse position` and `send mouse scroll` (#662) are command classes too
//!   (`BTScriptMouseButtonCommand`, `BTScriptMousePosCommand`, `BTScriptMouseScrollCommand`); they answer nothing (as in
//!   Ghostty). The mouse button is the direct parameter (an enumerator); positions and scroll deltas are `real` arguments.
//!   This module only parses the Apple Event. Where the point lands, what it does and what the mouse state is belong to the app.
//!
//! - `new surface configuration` and the `with configuration` argument of `new window`, `new tab` and `split` (#666) carry the
//!   `surface configuration` record (`GScf`). `new surface configuration` is a command class
//!   (`BTScriptNewSurfaceConfigurationCommand`), so no selector is added to `NSApplication` or `NSObject`; the other three read
//!   the `configuration` argument in the handlers they already have. This module reads the record by the sdef's cocoa keys
//!   (`fontSize`, `workingDirectory`, `command`, `initialInput`, `waitAfterCommand`, `environmentVariables`) into a
//!   `ScriptConfig`, checking the kind and the size of every value before it is copied, and builds the answer record. What the values
//!   mean (finite font size, `KEY=VALUE`, which folder is usable) and how they are applied is the app's. A bad record sets
//!   `errAECoercionFail` before any command is queued. The values may be secrets, so `Debug` shows only lengths and counts.
//!
//! Nothing here is installed in a process without a bundle identifier (a plain `cargo run`): there is no sdef to read.
//! When the app turns scripting off (`macos-applescript = false`), every command and every object lookup sets a
//! script error on the current command.

use std::ffi::CStr;
use std::sync::atomic::{AtomicBool, AtomicU8, Ordering};
use std::sync::mpsc;
use std::sync::{Arc, Mutex, Once};
use std::time::{Duration, Instant};

use objc2::rc::Retained;
use objc2::runtime::{AnyClass, AnyObject, Bool, Imp, Sel};
use objc2::{declare_class, msg_send_id, mutability, sel, ClassType, DeclaredClass};
use objc2_app_kit::NSApplication;
use objc2_foundation::{
    NSArray, NSBundle, NSDate, NSDefaultRunLoopMode, NSDictionary, NSInteger, NSNumber, NSObject, NSRunLoop,
    NSScriptClassDescription, NSScriptCommand, NSScriptObjectSpecifier, NSString, NSUniqueIDSpecifier,
};

/// `errAEEventNotPermitted`: scripting is switched off.
const ERR_NOT_PERMITTED: NSInteger = -1743;
/// `errAEEventFailed`: the command could not be carried out.
const ERR_FAILED: NSInteger = -10000;
/// `errAEParamMissed`: a required parameter is missing or of the wrong kind.
const ERR_PARAM: NSInteger = -1715;
/// `errAECoercionFail`: a `surface configuration` record is wrong (Ghostty answers a bad record with this too).
const ERR_COERCE: NSInteger = -1700;
/// How long a command waits for the app to answer.
const REPLY_TIMEOUT: Duration = Duration::from_secs(5);
/// A snapshot younger than this is used as it is.
const SNAPSHOT_FRESH: Duration = Duration::from_millis(100);
/// After a failed snapshot ask, wait this long before asking again.
const SNAPSHOT_RETRY: Duration = Duration::from_secs(2);
/// `input text` is refused above this many UTF-8 bytes (checked before the text is copied into a Rust string).
const MAX_TEXT_BYTES: usize = 1 << 20;
/// `send key` refuses a key name or a modifier list above this many UTF-8 bytes (the longest key name is 20 bytes).
const MAX_KEY_BYTES: usize = 256;
/// `perform action` refuses an action string above this many UTF-8 bytes (checked before the text is copied into a Rust string).
const MAX_ACTION_BYTES: usize = 4096;
/// A `surface configuration` text value (`command`, `initial working directory`, one environment variable) is refused above
/// this many UTF-8 bytes (checked before the text is copied into a Rust string, #666).
pub const MAX_CONFIG_TEXT_BYTES: usize = 64 << 10;
/// `initial input` of a `surface configuration` (the same as `input text`).
pub const MAX_CONFIG_INPUT_BYTES: usize = 1 << 20;
/// How many environment variables a `surface configuration` may hold (checked before any of them is copied).
pub const MAX_CONFIG_ENV_COUNT: usize = 256;
/// A `name override` a script writes is refused above this many UTF-8 bytes (checked before the text is copied into a Rust
/// string, #672). The app cuts a title to 200 characters (at most 800 bytes), so this only keeps a huge text from being copied.
pub const MAX_TITLE_BYTES: usize = 4096;

/// The four-character codes of the `split direction` and `input action` enumerations in the sdef (the same as Ghostty's).
const fn fourcc(code: &[u8; 4]) -> u32 {
    u32::from_be_bytes(*code)
}

/// The side a `split` puts the new terminal on.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ScriptSplit {
    Right,
    Left,
    Down,
    Up,
}

impl ScriptSplit {
    /// The enumerator of the `split direction` enumeration for this four-character code (`GSrt`, `GSlf`, `GSdn`, `GSup`).
    pub fn from_code(code: u32) -> Option<Self> {
        match code {
            c if c == fourcc(b"GSrt") => Some(Self::Right),
            c if c == fourcc(b"GSlf") => Some(Self::Left),
            c if c == fourcc(b"GSdn") => Some(Self::Down),
            c if c == fourcc(b"GSup") => Some(Self::Up),
            _ => None,
        }
    }

    /// The word `btctl new --split` takes.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Right => "right",
            Self::Left => "left",
            Self::Down => "down",
            Self::Up => "up",
        }
    }
}

/// Whether the `input action` enumerator (`GIpr` press, `GIrl` release) is a release. Anything else is a press, as in
/// Ghostty's `send key`.
pub fn input_action_is_release(code: u32) -> bool {
    code == fourcc(b"GIrl")
}

/// The `mouse button` enumeration (`GMBt`): `GMlf`, `GMrt`, `GMmd`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ScriptMouseButton {
    Left,
    Right,
    Middle,
}

impl ScriptMouseButton {
    /// The enumerator for this four-character code. Anything else is `None` (an error for the command, as in Ghostty).
    pub fn from_code(code: u32) -> Option<Self> {
        match code {
            c if c == fourcc(b"GMlf") => Some(Self::Left),
            c if c == fourcc(b"GMrt") => Some(Self::Right),
            c if c == fourcc(b"GMmd") => Some(Self::Middle),
            _ => None,
        }
    }
}

/// The `scroll momentum` enumeration (`GSMo`). Ghostty reads it and passes it to its core, whose scroll callback does not
/// look at it (Surface.zig `scrollCallback`, v1.4.0); the app does the same.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ScriptMomentum {
    None,
    Began,
    Changed,
    Ended,
    Cancelled,
    MayBegin,
    Stationary,
}

impl ScriptMomentum {
    /// The enumerator for this four-character code. An unknown value is `None` (Ghostty's default).
    pub fn from_code(code: u32) -> Self {
        match code {
            c if c == fourcc(b"SMbg") => Self::Began,
            c if c == fourcc(b"SMch") => Self::Changed,
            c if c == fourcc(b"SMen") => Self::Ended,
            c if c == fourcc(b"SMcn") => Self::Cancelled,
            c if c == fourcc(b"SMmb") => Self::MayBegin,
            c if c == fourcc(b"SMst") => Self::Stationary,
            _ => Self::None,
        }
    }
}

/// The `surface configuration` record (`GScf`) as it was read from the script, before the app checks its meaning (#666).
/// `None` is a key the record does not have. The values (a command, a folder, input text, environment variables) may hold
/// secrets, so `Debug` shows only lengths and counts.
#[derive(Clone, Default, PartialEq)]
pub struct ScriptConfig {
    pub font_size: Option<f64>,
    pub working_directory: Option<String>,
    pub command: Option<String>,
    pub initial_input: Option<String>,
    pub wait_after_command: Option<bool>,
    /// `KEY=VALUE` items, in the order the script gave them.
    pub environment: Option<Vec<String>>,
}

impl std::fmt::Debug for ScriptConfig {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ScriptConfig")
            .field("font_size", &self.font_size)
            .field("working_directory_bytes", &self.working_directory.as_ref().map(|s| s.len()))
            .field("command_bytes", &self.command.as_ref().map(|s| s.len()))
            .field("initial_input_bytes", &self.initial_input.as_ref().map(|s| s.len()))
            .field("wait_after_command", &self.wait_after_command)
            .field("environment_count", &self.environment.as_ref().map(|e| e.len()))
            .finish()
    }
}

/// One terminal pane as the app reports it.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ScriptTerminal {
    pub id: String,
    pub title: String,
    /// The title a person or a script put on the terminal (`name override`, #672); empty when there is none.
    pub title_override: String,
    pub working_directory: Option<String>,
    /// The foreground process group of the terminal; 0 when unknown (`pid`, #672).
    pub pid: i32,
    /// The PTY device path (`tty`, #672); empty when unknown.
    pub tty: String,
}

/// One tab (a layout of panes).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ScriptTab {
    pub id: String,
    pub title: String,
    /// The title put on the tab (`name override`, #672); empty when there is none.
    pub title_override: String,
    pub selected: bool,
    pub focused_terminal: Option<String>,
    pub terminals: Vec<ScriptTerminal>,
}

/// One window.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ScriptWindow {
    pub id: String,
    pub title: String,
    pub selected_tab: Option<String>,
    pub tabs: Vec<ScriptTab>,
}

/// Everything the dictionary can see.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ScriptSnapshot {
    pub windows: Vec<ScriptWindow>,
    pub front_window: Option<String>,
}

/// A command that changes the app. Ids are the strings from the snapshot. `Debug` leaves out the text of `InputText` and
/// the keys of `SendKey` (they may spell a password or a secret and commands end up in logs) and shows only the length of
/// the modifier text of the mouse commands. (Not `Eq`: the mouse commands carry real numbers.)
#[derive(Clone, PartialEq)]
pub enum ScriptCommand {
    /// Asks the app to call `set_snapshot` with its current state and then answer `Ok(None)`.
    Snapshot,
    /// `new window [with configuration RECORD]` (#666).
    NewWindow { config: Option<ScriptConfig> },
    /// `new tab [in WINDOW] [with configuration RECORD]` (#666).
    NewTab { window: Option<String>, config: Option<ScriptConfig> },
    /// `new surface configuration [from RECORD]` (#666): the app checks the record and answers `ScriptTarget::Configuration`.
    NewSurfaceConfiguration { from: Option<ScriptConfig> },
    Focus { terminal: String },
    ActivateWindow { window: String },
    SelectTab { tab: String },
    InputText { terminal: String, text: String },
    /// `send key`: the key name (Ghostty's raw values), whether it is a release, the comma-separated modifiers.
    SendKey { terminal: String, key: String, release: bool, modifiers: Option<String> },
    /// `perform action "..." on terminal` (#658): the action string, which the app reads as a keybind action.
    PerformAction { terminal: String, action: String },
    Split { terminal: String, direction: ScriptSplit, config: Option<ScriptConfig> },
    Close { terminal: String },
    CloseTab { tab: String },
    CloseWindow { window: String },
    /// `set name override of tab` (#672): the text as the script wrote it (the app cleans it; empty clears the override).
    SetTabTitleOverride { tab: String, title: String },
    /// `set name override of terminal` (#672): as above, for the title a person put on the pane.
    SetTerminalTitleOverride { terminal: String, title: String },
    /// `send mouse button` (#662): the button, whether it is a release, the comma-separated modifiers.
    SendMouseButton { terminal: String, button: ScriptMouseButton, release: bool, modifiers: Option<String> },
    /// `send mouse position` (#662): the position in points from the terminal's top left corner.
    SendMousePosition { terminal: String, x: f64, y: f64, modifiers: Option<String> },
    /// `send mouse scroll` (#662): the deltas, whether the event is a precise (pixel) one and the momentum phase.
    SendMouseScroll { terminal: String, x: f64, y: f64, precision: bool, momentum: ScriptMomentum },
}

impl std::fmt::Debug for ScriptCommand {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Snapshot => f.write_str("Snapshot"),
            Self::NewWindow { config } => f.debug_struct("NewWindow").field("config", config).finish(),
            Self::NewTab { window, config } => {
                f.debug_struct("NewTab").field("window", window).field("config", config).finish()
            }
            Self::NewSurfaceConfiguration { from } => {
                f.debug_struct("NewSurfaceConfiguration").field("from", from).finish()
            }
            Self::Focus { terminal } => f.debug_struct("Focus").field("terminal", terminal).finish(),
            Self::ActivateWindow { window } => f.debug_struct("ActivateWindow").field("window", window).finish(),
            Self::SelectTab { tab } => f.debug_struct("SelectTab").field("tab", tab).finish(),
            Self::InputText { terminal, text } => f
                .debug_struct("InputText")
                .field("terminal", terminal)
                .field("text_bytes", &text.len())
                .finish(),
            Self::SendKey { terminal, key, release, modifiers } => f
                .debug_struct("SendKey")
                .field("terminal", terminal)
                .field("key_bytes", &key.len())
                .field("release", release)
                .field("modifier_bytes", &modifiers.as_ref().map(|m| m.len()))
                .finish(),
            Self::PerformAction { terminal, action } => f
                .debug_struct("PerformAction")
                .field("terminal", terminal)
                .field("action_bytes", &action.len())
                .finish(),
            Self::Split { terminal, direction, config } => f
                .debug_struct("Split")
                .field("terminal", terminal)
                .field("direction", direction)
                .field("config", config)
                .finish(),
            Self::Close { terminal } => f.debug_struct("Close").field("terminal", terminal).finish(),
            Self::CloseTab { tab } => f.debug_struct("CloseTab").field("tab", tab).finish(),
            Self::CloseWindow { window } => f.debug_struct("CloseWindow").field("window", window).finish(),
            // a title may spell something private: only its size is shown
            Self::SetTabTitleOverride { tab, title } => f
                .debug_struct("SetTabTitleOverride")
                .field("tab", tab)
                .field("title_bytes", &title.len())
                .finish(),
            Self::SetTerminalTitleOverride { terminal, title } => f
                .debug_struct("SetTerminalTitleOverride")
                .field("terminal", terminal)
                .field("title_bytes", &title.len())
                .finish(),
            Self::SendMouseButton { terminal, button, release, modifiers } => f
                .debug_struct("SendMouseButton")
                .field("terminal", terminal)
                .field("button", button)
                .field("release", release)
                .field("modifier_bytes", &modifiers.as_ref().map(|m| m.len()))
                .finish(),
            Self::SendMousePosition { terminal, x, y, modifiers } => f
                .debug_struct("SendMousePosition")
                .field("terminal", terminal)
                .field("x", x)
                .field("y", y)
                .field("modifier_bytes", &modifiers.as_ref().map(|m| m.len()))
                .finish(),
            Self::SendMouseScroll { terminal, x, y, precision, momentum } => f
                .debug_struct("SendMouseScroll")
                .field("terminal", terminal)
                .field("x", x)
                .field("y", y)
                .field("precision", precision)
                .field("momentum", momentum)
                .finish(),
        }
    }
}

/// What a command returns to the script: the object it created, if any.
#[derive(Debug, Clone, PartialEq)]
pub enum ScriptTarget {
    Window(String),
    Tab(String),
    Terminal(String),
    /// The result of `new surface configuration`: a record with every key set (a missing key reads as its default).
    Configuration(ScriptConfig),
    /// The result of `perform action`: whether the action was performed.
    Performed(bool),
}

/// The app's answer to a command: the new object, or a message for the script error.
pub type ScriptReply = Result<Option<ScriptTarget>, String>;

/// Decides, once, who owns a request: the app (it runs the command) or the waiting side (it gave up after the
/// timeout). Without it a request still sitting in the app's event queue would run long after the script was told it
/// failed (for example when a modal context menu delayed the queue). Whichever side calls first wins.
#[derive(Debug, Clone, Default)]
pub struct ScriptTicket(Arc<AtomicU8>);

impl ScriptTicket {
    const PENDING: u8 = 0;
    const TAKEN: u8 = 1;
    const CANCELLED: u8 = 2;

    pub fn new() -> Self {
        Self::default()
    }

    /// The app calls this right before it runs the command. `false` means the script already gave up: drop it.
    pub fn take(&self) -> bool {
        self.0.compare_exchange(Self::PENDING, Self::TAKEN, Ordering::AcqRel, Ordering::Acquire).is_ok()
    }

    /// The waiting side calls this on timeout. `false` means the app already started the command, so the answer is
    /// still coming.
    pub fn cancel(&self) -> bool {
        self.0.compare_exchange(Self::PENDING, Self::CANCELLED, Ordering::AcqRel, Ordering::Acquire).is_ok()
    }
}

/// When to ask the app for a fresh snapshot. A snapshot is used for `SNAPSHOT_FRESH`; after a failed ask (the app did
/// not answer) the next ask waits `SNAPSHOT_RETRY`, so a script reading twenty properties does not wait twenty times.
#[derive(Debug, Clone, Default)]
pub struct ScriptRefresh {
    last_ok: Option<Instant>,
    last_fail: Option<Instant>,
}

impl ScriptRefresh {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn due(&self, now: Instant) -> bool {
        let within = |at: Option<Instant>, span: Duration| at.is_some_and(|t| now.saturating_duration_since(t) < span);
        !within(self.last_ok, SNAPSHOT_FRESH) && !within(self.last_fail, SNAPSHOT_RETRY)
    }

    pub fn record_ok(&mut self, now: Instant) {
        self.last_ok = Some(now);
        self.last_fail = None;
    }

    pub fn record_fail(&mut self, now: Instant) {
        self.last_fail = Some(now);
    }
}

type Handler = Arc<dyn Fn(ScriptCommand, ScriptTicket, mpsc::Sender<ScriptReply>) + Send + Sync>;

static HANDLER: Mutex<Option<Handler>> = Mutex::new(None);
static SNAPSHOT: Mutex<(ScriptRefresh, ScriptSnapshot)> = Mutex::new((
    ScriptRefresh { last_ok: None, last_fail: None },
    ScriptSnapshot { windows: Vec::new(), front_window: None },
));
static ENABLED: AtomicBool = AtomicBool::new(true);
static INSTALL: Once = Once::new();

/// Whether the process runs from an app bundle (has a bundle identifier). Without one there is no sdef and nothing is
/// installed.
pub fn supported() -> bool {
    // SAFETY: `bundleIdentifier` has no preconditions; `mainBundle` always returns a bundle object.
    unsafe { NSBundle::mainBundle().bundleIdentifier() }.is_some()
}

/// Turns scripting on or off (`macos-applescript`). Safe to call from any thread, any time.
pub fn set_enabled(enabled: bool) {
    ENABLED.store(enabled, Ordering::Relaxed);
    if !enabled {
        // Nothing the app showed earlier may stay readable.
        *SNAPSHOT.lock().unwrap_or_else(|e| e.into_inner()) = Default::default();
    }
}

fn enabled() -> bool {
    ENABLED.load(Ordering::Relaxed)
}

/// Replaces the state that property reads see.
pub fn set_snapshot(snapshot: ScriptSnapshot) {
    let mut g = SNAPSHOT.lock().unwrap_or_else(|e| e.into_inner());
    g.0.record_ok(Instant::now());
    g.1 = snapshot;
}

/// The app's state as of at most `SNAPSHOT_FRESH` ago (asks the app when the stored one is older and the last ask did
/// not just fail). Empty while scripting is off, so every object getter reads nothing then.
fn snapshot() -> ScriptSnapshot {
    if !enabled() {
        return ScriptSnapshot::default();
    }
    let due = SNAPSHOT.lock().unwrap_or_else(|e| e.into_inner()).0.due(Instant::now());
    if due {
        let result = run_command(ScriptCommand::Snapshot);
        let mut g = SNAPSHOT.lock().unwrap_or_else(|e| e.into_inner());
        match result {
            Ok(_) => g.0.record_ok(Instant::now()),
            Err(message) => {
                g.0.record_fail(Instant::now());
                tracing::debug!("scripting: no snapshot from the app: {message}");
            }
        }
    }
    SNAPSHOT.lock().unwrap_or_else(|e| e.into_inner()).1.clone()
}

/// Registers the function that receives commands and installs the scripting classes. Does nothing but store the handler
/// when the process has no bundle identifier.
pub fn set_handler(handler: Box<dyn Fn(ScriptCommand, ScriptTicket, mpsc::Sender<ScriptReply>) + Send + Sync>) {
    *HANDLER.lock().unwrap_or_else(|e| e.into_inner()) = Some(handler.into());
    if !supported() {
        return;
    }
    INSTALL.call_once(install);
}

// ---------------------------------------------------------------------------------------------------------------
// Errors and dispatch

fn set_script_error(number: NSInteger, message: &str) {
    // SAFETY: `currentCommand` has no preconditions; the setters take a number and a string the framework copies.
    unsafe {
        if let Some(cmd) = NSScriptCommand::currentCommand() {
            cmd.setScriptErrorNumber(number);
            cmd.setScriptErrorString(Some(&NSString::from_str(message)));
        }
    }
}

const DISABLED_MESSAGE: &str = "AppleScript는 설정 macos-applescript = false로 꺼져 있습니다";

/// `true` when scripting is on. Otherwise sets the script error on the current command.
fn gate() -> bool {
    if enabled() {
        return true;
    }
    set_script_error(ERR_NOT_PERMITTED, DISABLED_MESSAGE);
    false
}

/// Puts an empty application-defined event in the queue. The wait loop below spins only the run loop, never AppKit's
/// event queue, so work AppKit defers to its next event (`NSApp hide:` from `perform action "toggle_visibility"`) would
/// otherwise wait for the user's next key or mouse event. b-terminal(#718)
fn wake_event_queue() {
    let Some(mtm) = objc2_foundation::MainThreadMarker::new() else { return };
    if let Some(event) = super::event::dummy_event() {
        NSApplication::sharedApplication(mtm).postEvent_atStart(&event, false);
    }
}

/// Sends the command to the app and waits for the answer, spinning the run loop so the app's event loop can run.
fn run_command(cmd: ScriptCommand) -> ScriptReply {
    if !enabled() {
        return Err(DISABLED_MESSAGE.into());
    }
    let handler = HANDLER.lock().unwrap_or_else(|e| e.into_inner()).clone();
    let Some(handler) = handler else {
        return Err("터미널이 스크립트 명령을 받을 준비가 되지 않았습니다".into());
    };
    let (tx, rx) = mpsc::channel();
    let ticket = ScriptTicket::new();
    handler(cmd, ticket.clone(), tx);
    let mut deadline = Instant::now() + REPLY_TIMEOUT;
    // The app took the command before the first deadline (it may be running it)
    let mut taken = false;
    loop {
        match rx.try_recv() {
            Ok(reply) => {
                wake_event_queue();
                return reply;
            }
            Err(mpsc::TryRecvError::Disconnected) => return Err("터미널이 끝나는 중입니다".into()),
            Err(mpsc::TryRecvError::Empty) => {}
        }
        if Instant::now() >= deadline {
            // Still queued: cancel it so the app drops it when it gets to it. Already running: the answer is on its
            // way, so wait once more (bounded) instead of leaving the command half done.
            if taken {
                return Err("터미널이 답하지 않습니다(명령이 실행 중이었을 수 있습니다)".into());
            }
            if !ticket.cancel() {
                taken = true;
                deadline = Instant::now() + REPLY_TIMEOUT;
                continue;
            }
            return Err("터미널이 답하지 않습니다(명령은 실행되지 않고 버려졌습니다)".into());
        }
        // SAFETY: called on the main thread (Apple Events arrive there); the mode constant is valid for the process.
        unsafe {
            let until = NSDate::dateWithTimeIntervalSinceNow(0.01);
            NSRunLoop::currentRunLoop().runMode_beforeDate(NSDefaultRunLoopMode, &until);
        }
    }
}

/// Runs a command on behalf of an object's `handle...` method and turns the reply into the method's return value.
fn perform(cmd: ScriptCommand) -> *mut AnyObject {
    match run_command(cmd) {
        Ok(None) => std::ptr::null_mut(),
        Ok(Some(ScriptTarget::Window(id))) => Retained::autorelease_return(ScriptWindowObj::new(&id)).cast(),
        Ok(Some(ScriptTarget::Tab(id))) => Retained::autorelease_return(ScriptTabObj::new(&id)).cast(),
        Ok(Some(ScriptTarget::Terminal(id))) => Retained::autorelease_return(ScriptTerminalObj::new(&id)).cast(),
        Ok(Some(ScriptTarget::Performed(done))) => Retained::autorelease_return(NSNumber::numberWithBool(done)).cast(),
        Ok(Some(ScriptTarget::Configuration(config))) => Retained::autorelease_return(config_dictionary(&config)).cast(),
        Err(message) => {
            set_script_error(ERR_FAILED, &message);
            std::ptr::null_mut()
        }
    }
}

// ---------------------------------------------------------------------------------------------------------------
// Snapshot lookups (pure; the object classes below only call these)

fn find_window<'a>(snap: &'a ScriptSnapshot, id: &str) -> Option<&'a ScriptWindow> {
    snap.windows.iter().find(|w| w.id == id)
}

/// The tab with this id and the window that holds it.
fn find_tab<'a>(snap: &'a ScriptSnapshot, id: &str) -> Option<(&'a ScriptWindow, &'a ScriptTab)> {
    snap.windows.iter().find_map(|w| w.tabs.iter().find(|t| t.id == id).map(|t| (w, t)))
}

/// The terminal with this id and the tab that holds it.
fn find_terminal<'a>(snap: &'a ScriptSnapshot, id: &str) -> Option<(&'a ScriptTab, &'a ScriptTerminal)> {
    snap.windows
        .iter()
        .flat_map(|w| w.tabs.iter())
        .find_map(|t| t.terminals.iter().find(|x| x.id == id).map(|x| (t, x)))
}

fn ns(s: &str) -> Retained<NSString> {
    NSString::from_str(s)
}

/// `[object isKindOfClass:T]`: Cocoa hands arguments over as untyped objects, so every one is checked before it is used.
fn kind_of<T: ClassType>(object: &AnyObject) -> bool {
    // SAFETY: `isKindOfClass:` is defined for every object and takes a class.
    let yes: Bool = unsafe { objc2::msg_send![object, isKindOfClass: T::class()] };
    yes.as_bool()
}

/// UTF-8 size of the string, without copying it.
fn utf8_len(s: &NSString) -> usize {
    s.lengthOfBytesUsingEncoding(objc2_foundation::NSUTF8StringEncoding)
}

/// The object as a `T`, when it is one.
fn downcast<T: ClassType>(object: Retained<AnyObject>) -> Option<Retained<T>> {
    if kind_of::<T>(&object) {
        // SAFETY: the class was just checked.
        Some(unsafe { Retained::cast::<T>(object) })
    } else {
        None
    }
}

fn window_selected_tab(id: &str) -> Option<Retained<ScriptTabObj>> {
    let snap = snapshot();
    let w = find_window(&snap, id)?;
    w.selected_tab.as_deref().map(ScriptTabObj::new)
}

fn tab_focused_terminal(id: &str) -> Option<Retained<ScriptTerminalObj>> {
    let snap = snapshot();
    let (_, t) = find_tab(&snap, id)?;
    t.focused_terminal.as_deref().map(ScriptTerminalObj::new)
}

fn terminal_working_directory(id: &str) -> Option<Retained<NSString>> {
    let snap = snapshot();
    let (_, x) = find_terminal(&snap, id)?;
    x.working_directory.as_deref().map(ns)
}

fn tab_specifier(id: &str) -> Option<Retained<NSScriptObjectSpecifier>> {
    let snap = snapshot();
    let (w, _) = find_tab(&snap, id)?;
    let window = ScriptWindowObj::new(&w.id);
    // SAFETY: `objectSpecifier` is the method `ScriptWindowObj` declares, called on a live object.
    let window_spec: Option<Retained<NSScriptObjectSpecifier>> = unsafe { msg_send_id![&window, objectSpecifier] };
    specifier(ScriptWindowObj::class(), window_spec.as_deref(), "tabs", id)
}

fn terminal_specifier(id: &str) -> Option<Retained<NSScriptObjectSpecifier>> {
    let snap = snapshot();
    match find_terminal(&snap, id) {
        Some((t, _)) => {
            let tab = ScriptTabObj::new(&t.id);
            // SAFETY: `objectSpecifier` is the method `ScriptTabObj` declares, called on a live object.
            let tab_spec: Option<Retained<NSScriptObjectSpecifier>> = unsafe { msg_send_id![&tab, objectSpecifier] };
            specifier(ScriptTabObj::class(), tab_spec.as_deref(), "terminals", id)
        }
        // A terminal that has closed since: name it from the application so the reference still resolves (to
        // nothing) instead of failing to build.
        None => specifier(NSApplication::class(), None, "terminals", id),
    }
}

/// The text a script wrote to `name override`, with its size checked before it is copied into a Rust string (#672).
fn title_value(value: Option<&AnyObject>) -> Result<String, String> {
    let Some(value) = value.filter(|v| kind_of::<NSString>(v)) else {
        return Err("name override에는 텍스트가 필요합니다".into());
    };
    // SAFETY: the class was just checked.
    let text: &NSString = unsafe { &*(value as *const AnyObject).cast::<NSString>() };
    if utf8_len(text) > MAX_TITLE_BYTES {
        return Err(format!("제목이 너무 깁니다(상한 {MAX_TITLE_BYTES} 바이트)"));
    }
    Ok(text.to_string())
}

/// What Cocoa's setter of `name override` does: checks the switch and the text, asks the app, and sets a script error when
/// any of them fails (the setter itself returns nothing). The value is never logged.
fn write_title(value: Option<&AnyObject>, command: impl FnOnce(String) -> ScriptCommand) {
    if !gate() {
        return;
    }
    let result = title_value(value).and_then(|title| run_command(command(title)).map(|_| ()));
    if let Err(message) = result {
        set_script_error(ERR_FAILED, &message);
    }
}

/// The id of the `T` object in a command argument.
fn argument_id<T: ClassType + 'static>(command: &NSScriptCommand, key: &str, id_of: fn(&T) -> String) -> Option<String> {
    // SAFETY: a property read on the live command; the value is checked for its class.
    let value = unsafe { command.evaluatedArguments() }.and_then(|d| unsafe { d.objectForKey(&ns(key)) })?;
    downcast::<T>(value).map(|o| id_of(&o))
}

/// A number argument (an enumerator arrives as the number of its four-character code).
fn argument_u32(command: &NSScriptCommand, key: &str) -> Option<u32> {
    // SAFETY: a property read on the live command; the value is checked for its class before `unsignedIntValue`.
    let value = unsafe { command.evaluatedArguments() }.and_then(|d| unsafe { d.objectForKey(&ns(key)) })?;
    if !kind_of::<NSNumber>(&value) {
        return None;
    }
    // SAFETY: `value` is an `NSNumber`.
    Some(unsafe { objc2::msg_send![&*value, unsignedIntValue] })
}

/// The enumerator in the command's direct parameter (it arrives as the number of its four-character code).
fn direct_u32(command: &NSScriptCommand) -> Option<u32> {
    // SAFETY: a property read on the live command; the value is checked for its class before `unsignedIntValue`.
    let value = unsafe { command.directParameter() }?;
    if !kind_of::<NSNumber>(&value) {
        return None;
    }
    // SAFETY: `value` is an `NSNumber`.
    Some(unsafe { objc2::msg_send![&*value, unsignedIntValue] })
}

/// A real-number argument; `None` when it is absent or not a number.
fn argument_f64(command: &NSScriptCommand, key: &str) -> Option<f64> {
    // SAFETY: a property read on the live command; the value is checked for its class before `doubleValue`.
    let value = unsafe { command.evaluatedArguments() }.and_then(|d| unsafe { d.objectForKey(&ns(key)) })?;
    if !kind_of::<NSNumber>(&value) {
        return None;
    }
    // SAFETY: `value` is an `NSNumber`.
    Some(unsafe { objc2::msg_send![&*value, doubleValue] })
}

/// A boolean argument; `None` when it is absent or not a number.
fn argument_bool(command: &NSScriptCommand, key: &str) -> Option<bool> {
    // SAFETY: as `argument_f64`, with `boolValue`.
    let value = unsafe { command.evaluatedArguments() }.and_then(|d| unsafe { d.objectForKey(&ns(key)) })?;
    if !kind_of::<NSNumber>(&value) {
        return None;
    }
    // SAFETY: `value` is an `NSNumber`.
    let yes: Bool = unsafe { objc2::msg_send![&*value, boolValue] };
    Some(yes.as_bool())
}

/// A text argument; `None` when it is absent or not text.
fn argument_string(command: &NSScriptCommand, key: &str) -> Option<Retained<NSString>> {
    // SAFETY: as `argument_u32`.
    let value = unsafe { command.evaluatedArguments() }.and_then(|d| unsafe { d.objectForKey(&ns(key)) })?;
    downcast::<NSString>(value)
}

// ---------------------------------------------------------------------------------------------------------------
// `surface configuration` records (#666)

/// The value of `key` in a record, if it has one.
fn record_value(record: &NSDictionary<NSString, AnyObject>, key: &str) -> Option<Retained<AnyObject>> {
    // SAFETY: a read on a live dictionary with a string key.
    unsafe { record.objectForKey(&ns(key)) }
}

/// A text value of a record: `Ok(None)` when the key is absent, an error for another kind or a text above `limit`
/// (the size is read from the `NSString` before it is copied into a Rust string).
fn record_text(
    record: &NSDictionary<NSString, AnyObject>,
    key: &str,
    label: &str,
    limit: usize,
) -> Result<Option<String>, String> {
    let Some(value) = record_value(record, key) else {
        return Ok(None);
    };
    let Some(text) = downcast::<NSString>(value) else {
        return Err(format!("{label}은 텍스트여야 합니다"));
    };
    if utf8_len(&text) > limit {
        return Err(format!("{label}이 너무 깁니다(상한 {limit} 바이트)"));
    }
    Ok(Some(text.to_string()))
}

/// The environment variables of a record: a list of text (one text alone counts as a list of one). The count and every
/// item's size are checked before anything is copied.
fn record_environment(record: &NSDictionary<NSString, AnyObject>) -> Result<Option<Vec<String>>, String> {
    let label = "environment variables";
    let Some(value) = record_value(record, "environmentVariables") else {
        return Ok(None);
    };
    let one = |text: Retained<NSString>| -> Result<String, String> {
        if utf8_len(&text) > MAX_CONFIG_TEXT_BYTES {
            return Err(format!("{label}의 한 항목이 너무 깁니다(상한 {MAX_CONFIG_TEXT_BYTES} 바이트)"));
        }
        Ok(text.to_string())
    };
    if kind_of::<NSString>(&value) {
        // SAFETY: the class was just checked.
        return one(unsafe { Retained::cast::<NSString>(value) }).map(|s| Some(vec![s]));
    }
    let Some(list) = downcast::<NSArray<AnyObject>>(value) else {
        return Err(format!("{label}는 텍스트 목록이어야 합니다"));
    };
    let count = list.len();
    if count > MAX_CONFIG_ENV_COUNT {
        return Err(format!("{label}가 너무 많습니다({count}개, 상한 {MAX_CONFIG_ENV_COUNT})"));
    }
    let mut items = Vec::with_capacity(count);
    for i in 0..count {
        // SAFETY: `i` is below the array's count; the item is checked for its class.
        let item: Option<Retained<AnyObject>> = unsafe { msg_send_id![&*list, objectAtIndex: i] };
        let Some(text) = item.and_then(downcast::<NSString>) else {
            return Err(format!("{label}는 텍스트 목록이어야 합니다"));
        };
        items.push(one(text)?);
    }
    Ok(Some(items))
}

/// Reads a `surface configuration` record (Ghostty reads it by the cocoa keys of the sdef). Unknown keys are ignored. A value of
/// the wrong kind or above a size limit is an error. What the values mean (finite, negative, `KEY=VALUE`) is the app's check.
fn config_from_record(record: &NSDictionary<NSString, AnyObject>) -> Result<ScriptConfig, String> {
    let mut config = ScriptConfig::default();
    if let Some(value) = record_value(record, "fontSize") {
        if !kind_of::<NSNumber>(&value) {
            return Err("font size는 숫자여야 합니다".into());
        }
        // SAFETY: `value` is an `NSNumber`.
        config.font_size = Some(unsafe { objc2::msg_send![&*value, doubleValue] });
    }
    config.working_directory =
        record_text(record, "workingDirectory", "initial working directory", MAX_CONFIG_TEXT_BYTES)?;
    config.command = record_text(record, "command", "command", MAX_CONFIG_TEXT_BYTES)?;
    config.initial_input = record_text(record, "initialInput", "initial input", MAX_CONFIG_INPUT_BYTES)?;
    if let Some(value) = record_value(record, "waitAfterCommand") {
        if !kind_of::<NSNumber>(&value) {
            return Err("wait after command는 true나 false여야 합니다".into());
        }
        // SAFETY: `value` is an `NSNumber`.
        let yes: Bool = unsafe { objc2::msg_send![&*value, boolValue] };
        config.wait_after_command = Some(yes.as_bool());
    }
    config.environment = record_environment(record)?;
    Ok(config)
}

/// The record argument `configuration` of a command (`with configuration` or `from`): `Ok(None)` when the script gave none.
fn configuration_argument(command: &NSScriptCommand) -> Result<Option<ScriptConfig>, String> {
    // SAFETY: a property read on the live command; the value is checked for its class.
    let Some(value) = unsafe { command.evaluatedArguments() }.and_then(|d| unsafe { d.objectForKey(&ns("configuration")) })
    else {
        return Ok(None);
    };
    let Some(record) = downcast::<NSDictionary<NSString, AnyObject>>(value) else {
        return Err("configuration에는 surface configuration 레코드가 필요합니다".into());
    };
    config_from_record(&record).map(Some)
}

/// `configuration_argument`, with the script error set on failure (Ghostty answers a bad record with `errAECoercionFail`).
/// `Err(())` means the command must stop: nothing is created.
fn configuration_or_error(command: &NSScriptCommand) -> Result<Option<ScriptConfig>, ()> {
    configuration_argument(command).map_err(|message| set_script_error(ERR_COERCE, &message))
}

/// The record a script gets back from `new surface configuration`: every key is set, a missing value reads as Ghostty's default.
fn config_dictionary(config: &ScriptConfig) -> Retained<NSDictionary<NSString, NSObject>> {
    let font = NSNumber::numberWithDouble(config.font_size.unwrap_or(0.0));
    let wait = NSNumber::numberWithBool(config.wait_after_command.unwrap_or(false));
    let env: Vec<Retained<NSString>> = config.environment.iter().flatten().map(|e| ns(e)).collect();
    let text = |v: &Option<String>| ns(v.as_deref().unwrap_or(""));
    let keys = [
        ns("fontSize"),
        ns("workingDirectory"),
        ns("command"),
        ns("initialInput"),
        ns("waitAfterCommand"),
        ns("environmentVariables"),
    ];
    let values: Vec<Retained<NSObject>> = vec![
        Retained::into_super(Retained::into_super(font)),
        Retained::into_super(text(&config.working_directory)),
        Retained::into_super(text(&config.command)),
        Retained::into_super(text(&config.initial_input)),
        Retained::into_super(Retained::into_super(wait)),
        Retained::into_super(NSArray::from_vec(env)),
    ];
    let key_refs: Vec<&NSString> = keys.iter().map(|k| &**k).collect();
    NSDictionary::from_vec(&key_refs, values)
}

// ---------------------------------------------------------------------------------------------------------------
// Object specifiers

/// A specifier that names an object by its id inside `container` (the application when `container` is `None`).
fn specifier(
    container_class: &AnyClass,
    container: Option<&NSScriptObjectSpecifier>,
    key: &str,
    id: &str,
) -> Option<Retained<NSScriptObjectSpecifier>> {
    // SAFETY: plain Foundation calls with valid arguments; the class description is looked up for a class that the
    // loaded sdef describes, and `None` is handled.
    unsafe {
        let desc = NSScriptClassDescription::classDescriptionForClass(container_class)?;
        let spec = NSUniqueIDSpecifier::initWithContainerClassDescription_containerSpecifier_key_uniqueID(
            NSUniqueIDSpecifier::alloc(),
            &desc,
            container,
            &ns(key),
            &ns(id),
        );
        Some(Retained::into_super(spec))
    }
}

// ---------------------------------------------------------------------------------------------------------------
// Scripting classes

/// The id an object stands for.
pub struct ObjId(String);

declare_class!(
    struct ScriptWindowObj;

    unsafe impl ClassType for ScriptWindowObj {
        type Super = NSObject;
        type Mutability = mutability::InteriorMutable;
        const NAME: &'static str = "BTScriptWindow";
    }

    impl DeclaredClass for ScriptWindowObj {
        type Ivars = ObjId;
    }

    unsafe impl ScriptWindowObj {
        #[method_id(id)]
        fn script_id(&self) -> Retained<NSString> {
            ns(&self.ivars().0)
        }

        #[method_id(uniqueID)]
        fn unique_id(&self) -> Retained<NSString> {
            ns(&self.ivars().0)
        }

        #[method_id(title)]
        fn title(&self) -> Option<Retained<NSString>> {
            find_window(&snapshot(), &self.ivars().0).map(|w| ns(&w.title))
        }

        #[method_id(selectedTab)]
        fn selected_tab(&self) -> Option<Retained<ScriptTabObj>> {
            window_selected_tab(&self.ivars().0)
        }

        #[method_id(tabs)]
        fn tabs(&self) -> Retained<NSArray<ScriptTabObj>> {
            let snap = snapshot();
            let items = find_window(&snap, &self.ivars().0)
                .map(|w| w.tabs.iter().map(|t| ScriptTabObj::new(&t.id)).collect())
                .unwrap_or_default();
            NSArray::from_vec(items)
        }

        #[method_id(terminals)]
        fn terminals(&self) -> Retained<NSArray<ScriptTerminalObj>> {
            let snap = snapshot();
            let items = find_window(&snap, &self.ivars().0)
                .map(|w| {
                    w.tabs.iter().flat_map(|t| t.terminals.iter()).map(|x| ScriptTerminalObj::new(&x.id)).collect()
                })
                .unwrap_or_default();
            NSArray::from_vec(items)
        }

        #[method_id(objectSpecifier)]
        fn object_specifier(&self) -> Option<Retained<NSScriptObjectSpecifier>> {
            specifier(NSApplication::class(), None, "scriptWindows", &self.ivars().0)
        }

        #[method(handleActivateWindowCommand:)]
        fn handle_activate(&self, _command: &NSScriptCommand) -> *mut AnyObject {
            if !gate() {
                return std::ptr::null_mut();
            }
            perform(ScriptCommand::ActivateWindow { window: self.ivars().0.clone() })
        }

        #[method(handleCloseWindowCommand:)]
        fn handle_close_window(&self, _command: &NSScriptCommand) -> *mut AnyObject {
            if !gate() {
                return std::ptr::null_mut();
            }
            perform(ScriptCommand::CloseWindow { window: self.ivars().0.clone() })
        }
    }
);

impl ScriptWindowObj {
    fn new(id: &str) -> Retained<Self> {
        let this = Self::alloc().set_ivars(ObjId(id.into()));
        // SAFETY: `NSObject`'s designated initialiser.
        unsafe { msg_send_id![super(this), init] }
    }
}

declare_class!(
    struct ScriptTabObj;

    unsafe impl ClassType for ScriptTabObj {
        type Super = NSObject;
        type Mutability = mutability::InteriorMutable;
        const NAME: &'static str = "BTScriptTab";
    }

    impl DeclaredClass for ScriptTabObj {
        type Ivars = ObjId;
    }

    unsafe impl ScriptTabObj {
        #[method_id(id)]
        fn script_id(&self) -> Retained<NSString> {
            ns(&self.ivars().0)
        }

        #[method_id(uniqueID)]
        fn unique_id(&self) -> Retained<NSString> {
            ns(&self.ivars().0)
        }

        #[method_id(title)]
        fn title(&self) -> Option<Retained<NSString>> {
            find_tab(&snapshot(), &self.ivars().0).map(|(_, t)| ns(&t.title))
        }

        /// `name override of tab ...`: the title a person or script set (empty when none).
        #[method_id(titleOverride)]
        fn title_override(&self) -> Option<Retained<NSString>> {
            find_tab(&snapshot(), &self.ivars().0).map(|(_, t)| ns(&t.title_override))
        }

        /// `set name override of tab ... to "..."` (an empty text clears it).
        #[method(setTitleOverride:)]
        fn set_title_override(&self, value: Option<&AnyObject>) {
            let tab = self.ivars().0.clone();
            write_title(value, |title| ScriptCommand::SetTabTitleOverride { tab, title });
        }

        /// 1-based position in the window.
        #[method(index)]
        fn index(&self) -> NSInteger {
            let snap = snapshot();
            find_tab(&snap, &self.ivars().0)
                .and_then(|(w, t)| w.tabs.iter().position(|x| x.id == t.id))
                .map_or(0, |i| i as NSInteger + 1)
        }

        #[method(selected)]
        fn selected(&self) -> Bool {
            Bool::new(find_tab(&snapshot(), &self.ivars().0).is_some_and(|(_, t)| t.selected))
        }

        #[method_id(focusedTerminal)]
        fn focused_terminal(&self) -> Option<Retained<ScriptTerminalObj>> {
            tab_focused_terminal(&self.ivars().0)
        }

        #[method_id(terminals)]
        fn terminals(&self) -> Retained<NSArray<ScriptTerminalObj>> {
            let snap = snapshot();
            let items = find_tab(&snap, &self.ivars().0)
                .map(|(_, t)| t.terminals.iter().map(|x| ScriptTerminalObj::new(&x.id)).collect())
                .unwrap_or_default();
            NSArray::from_vec(items)
        }

        #[method_id(objectSpecifier)]
        fn object_specifier(&self) -> Option<Retained<NSScriptObjectSpecifier>> {
            tab_specifier(&self.ivars().0)
        }

        #[method(handleSelectTabCommand:)]
        fn handle_select(&self, _command: &NSScriptCommand) -> *mut AnyObject {
            if !gate() {
                return std::ptr::null_mut();
            }
            perform(ScriptCommand::SelectTab { tab: self.ivars().0.clone() })
        }

        #[method(handleCloseTabCommand:)]
        fn handle_close_tab(&self, _command: &NSScriptCommand) -> *mut AnyObject {
            if !gate() {
                return std::ptr::null_mut();
            }
            perform(ScriptCommand::CloseTab { tab: self.ivars().0.clone() })
        }
    }
);

impl ScriptTabObj {
    fn new(id: &str) -> Retained<Self> {
        let this = Self::alloc().set_ivars(ObjId(id.into()));
        // SAFETY: `NSObject`'s designated initialiser.
        unsafe { msg_send_id![super(this), init] }
    }
}

declare_class!(
    struct ScriptTerminalObj;

    unsafe impl ClassType for ScriptTerminalObj {
        type Super = NSObject;
        type Mutability = mutability::InteriorMutable;
        const NAME: &'static str = "BTScriptTerminal";
    }

    impl DeclaredClass for ScriptTerminalObj {
        type Ivars = ObjId;
    }

    unsafe impl ScriptTerminalObj {
        #[method_id(id)]
        fn script_id(&self) -> Retained<NSString> {
            ns(&self.ivars().0)
        }

        #[method_id(uniqueID)]
        fn unique_id(&self) -> Retained<NSString> {
            ns(&self.ivars().0)
        }

        #[method_id(title)]
        fn title(&self) -> Option<Retained<NSString>> {
            find_terminal(&snapshot(), &self.ivars().0).map(|(_, x)| ns(&x.title))
        }

        #[method_id(titleOverride)]
        fn title_override(&self) -> Option<Retained<NSString>> {
            find_terminal(&snapshot(), &self.ivars().0).map(|(_, x)| ns(&x.title_override))
        }

        /// `set name override of terminal ... to "..."` (an empty text clears it).
        #[method(setTitleOverride:)]
        fn set_title_override(&self, value: Option<&AnyObject>) {
            let terminal = self.ivars().0.clone();
            write_title(value, |title| ScriptCommand::SetTerminalTitleOverride { terminal, title });
        }

        #[method_id(workingDirectory)]
        fn working_directory(&self) -> Option<Retained<NSString>> {
            terminal_working_directory(&self.ivars().0)
        }

        /// The foreground process of the terminal (0 when unknown, as Ghostty).
        #[method(pid)]
        fn pid(&self) -> NSInteger {
            find_terminal(&snapshot(), &self.ivars().0).map_or(0, |(_, x)| x.pid as NSInteger)
        }

        #[method_id(tty)]
        fn tty(&self) -> Option<Retained<NSString>> {
            find_terminal(&snapshot(), &self.ivars().0).map(|(_, x)| ns(&x.tty))
        }

        #[method_id(objectSpecifier)]
        fn object_specifier(&self) -> Option<Retained<NSScriptObjectSpecifier>> {
            terminal_specifier(&self.ivars().0)
        }

        #[method(handleFocusCommand:)]
        fn handle_focus(&self, _command: &NSScriptCommand) -> *mut AnyObject {
            if !gate() {
                return std::ptr::null_mut();
            }
            perform(ScriptCommand::Focus { terminal: self.ivars().0.clone() })
        }

        #[method(handleSplitCommand:)]
        fn handle_split(&self, command: &NSScriptCommand) -> *mut AnyObject {
            if !gate() {
                return std::ptr::null_mut();
            }
            let Some(direction) = argument_u32(command, "direction").and_then(ScriptSplit::from_code) else {
                set_script_error(ERR_PARAM, "분할 방향(right, left, down, up)이 없거나 모르는 값입니다");
                return std::ptr::null_mut();
            };
            let Ok(config) = configuration_or_error(command) else {
                return std::ptr::null_mut();
            };
            perform(ScriptCommand::Split { terminal: self.ivars().0.clone(), direction, config })
        }

        #[method(handleCloseCommand:)]
        fn handle_close(&self, _command: &NSScriptCommand) -> *mut AnyObject {
            if !gate() {
                return std::ptr::null_mut();
            }
            perform(ScriptCommand::Close { terminal: self.ivars().0.clone() })
        }
    }
);

impl ScriptTerminalObj {
    fn new(id: &str) -> Retained<Self> {
        let this = Self::alloc().set_ivars(ObjId(id.into()));
        // SAFETY: `NSObject`'s designated initialiser.
        unsafe { msg_send_id![super(this), init] }
    }
}

declare_class!(
    /// `input text "..." to terminal`: the terminal and text come from the command's arguments.
    struct ScriptInputTextCommand;

    unsafe impl ClassType for ScriptInputTextCommand {
        type Super = NSScriptCommand;
        type Mutability = mutability::InteriorMutable;
        const NAME: &'static str = "BTScriptInputTextCommand";
    }

    impl DeclaredClass for ScriptInputTextCommand {}

    unsafe impl ScriptInputTextCommand {
        #[method(performDefaultImplementation)]
        fn perform_default(&self) -> *mut AnyObject {
            if !gate() {
                return std::ptr::null_mut();
            }
            // SAFETY: a property read on the live command; the value is checked for its class.
            let text = unsafe { self.directParameter() }.and_then(downcast::<NSString>);
            if text.as_ref().is_some_and(|s| utf8_len(s) > MAX_TEXT_BYTES) {
                set_script_error(ERR_PARAM, "글이 너무 깁니다(상한 1MiB)");
                return std::ptr::null_mut();
            }
            let text = text.map(|s| s.to_string());
            let terminal = argument_id::<ScriptTerminalObj>(self, "terminal", |o| o.ivars().0.clone());
            let (Some(text), Some(terminal)) = (text, terminal) else {
                set_script_error(ERR_PARAM, "input text에는 글과 터미널이 필요합니다");
                return std::ptr::null_mut();
            };
            if let Err(message) = run_command(ScriptCommand::InputText { terminal, text }) {
                set_script_error(ERR_FAILED, &message);
            }
            std::ptr::null_mut()
        }
    }
);

declare_class!(
    /// `send key "enter" modifiers "control" action release to terminal`: the key name is the direct parameter.
    struct ScriptSendKeyCommand;

    unsafe impl ClassType for ScriptSendKeyCommand {
        type Super = NSScriptCommand;
        type Mutability = mutability::InteriorMutable;
        const NAME: &'static str = "BTScriptSendKeyCommand";
    }

    impl DeclaredClass for ScriptSendKeyCommand {}

    unsafe impl ScriptSendKeyCommand {
        #[method(performDefaultImplementation)]
        fn perform_default(&self) -> *mut AnyObject {
            if !gate() {
                return std::ptr::null_mut();
            }
            // SAFETY: a property read on the live command; the value is checked for its class.
            let key = unsafe { self.directParameter() }.and_then(downcast::<NSString>);
            let modifiers = argument_string(self, "modifiers");
            // Sizes first, before anything is copied into a Rust string.
            if key.iter().chain(modifiers.iter()).any(|s| utf8_len(s) > MAX_KEY_BYTES) {
                set_script_error(ERR_PARAM, "키 이름이나 수식 키가 너무 깁니다");
                return std::ptr::null_mut();
            }
            let terminal = argument_id::<ScriptTerminalObj>(self, "terminal", |o| o.ivars().0.clone());
            let (Some(key), Some(terminal)) = (key, terminal) else {
                set_script_error(ERR_PARAM, "send key에는 키 이름과 터미널이 필요합니다");
                return std::ptr::null_mut();
            };
            let release = argument_u32(self, "action").is_some_and(input_action_is_release);
            let command = ScriptCommand::SendKey {
                terminal,
                key: key.to_string(),
                release,
                modifiers: modifiers.map(|m| m.to_string()),
            };
            if let Err(message) = run_command(command) {
                set_script_error(ERR_FAILED, &message);
            }
            std::ptr::null_mut()
        }
    }
);

declare_class!(
    /// `perform action "new_split:right" on terminal`: the action string is the direct parameter. The answer is a boolean.
    struct ScriptPerformActionCommand;

    unsafe impl ClassType for ScriptPerformActionCommand {
        type Super = NSScriptCommand;
        type Mutability = mutability::InteriorMutable;
        const NAME: &'static str = "BTScriptPerformActionCommand";
    }

    impl DeclaredClass for ScriptPerformActionCommand {}

    unsafe impl ScriptPerformActionCommand {
        #[method(performDefaultImplementation)]
        fn perform_default(&self) -> *mut AnyObject {
            if !gate() {
                return std::ptr::null_mut();
            }
            // SAFETY: a property read on the live command; the value is checked for its class.
            let action = unsafe { self.directParameter() }.and_then(downcast::<NSString>);
            // Size first, before anything is copied into a Rust string.
            if action.as_ref().is_some_and(|s| utf8_len(s) > MAX_ACTION_BYTES) {
                set_script_error(ERR_PARAM, "동작 문자열이 너무 깁니다(상한 4096 바이트)");
                return std::ptr::null_mut();
            }
            let terminal = argument_id::<ScriptTerminalObj>(self, "terminal", |o| o.ivars().0.clone());
            let (Some(action), Some(terminal)) = (action, terminal) else {
                set_script_error(ERR_PARAM, "perform action에는 동작 문자열과 터미널(on)이 필요합니다");
                return std::ptr::null_mut();
            };
            // `perform` turns the app's `Performed` answer into an `NSNumber`; a failure sets the script error.
            perform(ScriptCommand::PerformAction { terminal, action: action.to_string() })
        }
    }
);

declare_class!(
    /// `new surface configuration [from RECORD]` (#666): answers a record. The app checks the record's values.
    struct ScriptNewSurfaceConfigurationCommand;

    unsafe impl ClassType for ScriptNewSurfaceConfigurationCommand {
        type Super = NSScriptCommand;
        type Mutability = mutability::InteriorMutable;
        const NAME: &'static str = "BTScriptNewSurfaceConfigurationCommand";
    }

    impl DeclaredClass for ScriptNewSurfaceConfigurationCommand {}

    unsafe impl ScriptNewSurfaceConfigurationCommand {
        #[method(performDefaultImplementation)]
        fn perform_default(&self) -> *mut AnyObject {
            if !gate() {
                return std::ptr::null_mut();
            }
            let Ok(from) = configuration_or_error(self) else {
                return std::ptr::null_mut();
            };
            perform(ScriptCommand::NewSurfaceConfiguration { from })
        }
    }
);

/// The text of an optional `modifiers` argument, checked for its size before it is copied into a Rust string.
/// `Err` is set as the script error by the caller.
fn modifiers_argument(command: &NSScriptCommand) -> Result<Option<String>, &'static str> {
    let modifiers = argument_string(command, "modifiers");
    if modifiers.as_ref().is_some_and(|s| utf8_len(s) > MAX_KEY_BYTES) {
        return Err("수식 키가 너무 깁니다");
    }
    Ok(modifiers.map(|m| m.to_string()))
}

declare_class!(
    /// `send mouse button left button action release modifiers "shift" to terminal`: the button is the direct parameter.
    struct ScriptMouseButtonCommand;

    unsafe impl ClassType for ScriptMouseButtonCommand {
        type Super = NSScriptCommand;
        type Mutability = mutability::InteriorMutable;
        const NAME: &'static str = "BTScriptMouseButtonCommand";
    }

    impl DeclaredClass for ScriptMouseButtonCommand {}

    unsafe impl ScriptMouseButtonCommand {
        #[method(performDefaultImplementation)]
        fn perform_default(&self) -> *mut AnyObject {
            if !gate() {
                return std::ptr::null_mut();
            }
            let Some(button) = direct_u32(self).and_then(ScriptMouseButton::from_code) else {
                set_script_error(ERR_PARAM, "마우스 단추(left button, right button, middle button)가 없거나 모르는 값입니다");
                return std::ptr::null_mut();
            };
            let modifiers = match modifiers_argument(self) {
                Ok(m) => m,
                Err(message) => {
                    set_script_error(ERR_PARAM, message);
                    return std::ptr::null_mut();
                }
            };
            let Some(terminal) = argument_id::<ScriptTerminalObj>(self, "terminal", |o| o.ivars().0.clone()) else {
                set_script_error(ERR_PARAM, "send mouse button에는 터미널(to)이 필요합니다");
                return std::ptr::null_mut();
            };
            let release = argument_u32(self, "action").is_some_and(input_action_is_release);
            if let Err(message) = run_command(ScriptCommand::SendMouseButton { terminal, button, release, modifiers }) {
                set_script_error(ERR_FAILED, &message);
            }
            std::ptr::null_mut()
        }
    }
);

declare_class!(
    /// `send mouse position x 10 y 20 modifiers "shift" to terminal`.
    struct ScriptMousePosCommand;

    unsafe impl ClassType for ScriptMousePosCommand {
        type Super = NSScriptCommand;
        type Mutability = mutability::InteriorMutable;
        const NAME: &'static str = "BTScriptMousePosCommand";
    }

    impl DeclaredClass for ScriptMousePosCommand {}

    unsafe impl ScriptMousePosCommand {
        #[method(performDefaultImplementation)]
        fn perform_default(&self) -> *mut AnyObject {
            if !gate() {
                return std::ptr::null_mut();
            }
            let (Some(x), Some(y)) = (argument_f64(self, "x"), argument_f64(self, "y")) else {
                set_script_error(ERR_PARAM, "send mouse position에는 x와 y가 필요합니다");
                return std::ptr::null_mut();
            };
            let modifiers = match modifiers_argument(self) {
                Ok(m) => m,
                Err(message) => {
                    set_script_error(ERR_PARAM, message);
                    return std::ptr::null_mut();
                }
            };
            let Some(terminal) = argument_id::<ScriptTerminalObj>(self, "terminal", |o| o.ivars().0.clone()) else {
                set_script_error(ERR_PARAM, "send mouse position에는 터미널(to)이 필요합니다");
                return std::ptr::null_mut();
            };
            if let Err(message) = run_command(ScriptCommand::SendMousePosition { terminal, x, y, modifiers }) {
                set_script_error(ERR_FAILED, &message);
            }
            std::ptr::null_mut()
        }
    }
);

declare_class!(
    /// `send mouse scroll x 0 y -3 precision true momentum began to terminal`.
    struct ScriptMouseScrollCommand;

    unsafe impl ClassType for ScriptMouseScrollCommand {
        type Super = NSScriptCommand;
        type Mutability = mutability::InteriorMutable;
        const NAME: &'static str = "BTScriptMouseScrollCommand";
    }

    impl DeclaredClass for ScriptMouseScrollCommand {}

    unsafe impl ScriptMouseScrollCommand {
        #[method(performDefaultImplementation)]
        fn perform_default(&self) -> *mut AnyObject {
            if !gate() {
                return std::ptr::null_mut();
            }
            let (Some(x), Some(y)) = (argument_f64(self, "x"), argument_f64(self, "y")) else {
                set_script_error(ERR_PARAM, "send mouse scroll에는 x와 y가 필요합니다");
                return std::ptr::null_mut();
            };
            let Some(terminal) = argument_id::<ScriptTerminalObj>(self, "terminal", |o| o.ivars().0.clone()) else {
                set_script_error(ERR_PARAM, "send mouse scroll에는 터미널(to)이 필요합니다");
                return std::ptr::null_mut();
            };
            let precision = argument_bool(self, "precision").unwrap_or(false);
            let momentum = argument_u32(self, "momentum").map_or(ScriptMomentum::None, ScriptMomentum::from_code);
            if let Err(message) = run_command(ScriptCommand::SendMouseScroll { terminal, x, y, precision, momentum }) {
                set_script_error(ERR_FAILED, &message);
            }
            std::ptr::null_mut()
        }
    }
);

// ---------------------------------------------------------------------------------------------------------------
// NSApplication additions

type Getter = unsafe extern "C-unwind" fn(&AnyObject, Sel) -> *mut AnyObject;
type CommandHandler = unsafe extern "C-unwind" fn(&AnyObject, Sel, &NSScriptCommand) -> *mut AnyObject;

fn objects_to_array<T: objc2::Message>(items: Vec<Retained<T>>) -> *mut NSArray<T> {
    Retained::autorelease_return(NSArray::from_vec(items))
}

unsafe extern "C-unwind" fn app_script_windows(_this: &AnyObject, _sel: Sel) -> *mut AnyObject {
    if !gate() {
        return objects_to_array::<NSObject>(Vec::new()).cast();
    }
    let items: Vec<_> = snapshot().windows.iter().map(|w| ScriptWindowObj::new(&w.id)).collect();
    objects_to_array(items).cast()
}

unsafe extern "C-unwind" fn app_terminals(_this: &AnyObject, _sel: Sel) -> *mut AnyObject {
    if !gate() {
        return objects_to_array::<NSObject>(Vec::new()).cast();
    }
    let items: Vec<_> = snapshot()
        .windows
        .iter()
        .flat_map(|w| w.tabs.iter())
        .flat_map(|t| t.terminals.iter())
        .map(|x| ScriptTerminalObj::new(&x.id))
        .collect();
    objects_to_array(items).cast()
}

unsafe extern "C-unwind" fn app_front_window(_this: &AnyObject, _sel: Sel) -> *mut AnyObject {
    if !gate() {
        return std::ptr::null_mut();
    }
    match snapshot().front_window {
        Some(id) => Retained::autorelease_return(ScriptWindowObj::new(&id)).cast(),
        None => std::ptr::null_mut(),
    }
}

unsafe extern "C-unwind" fn app_name(_this: &AnyObject, _sel: Sel) -> *mut AnyObject {
    Retained::autorelease_return(ns("b-terminal")).cast()
}

unsafe extern "C-unwind" fn app_version(_this: &AnyObject, _sel: Sel) -> *mut AnyObject {
    // SAFETY: a plain dictionary read on the main bundle.
    let version = unsafe { NSBundle::mainBundle().objectForInfoDictionaryKey(&ns("CFBundleShortVersionString")) }
        .and_then(downcast::<NSString>)
        .unwrap_or_else(|| ns(""));
    Retained::autorelease_return(version).cast()
}

unsafe extern "C-unwind" fn app_new_window(_this: &AnyObject, _sel: Sel, command: &NSScriptCommand) -> *mut AnyObject {
    if !gate() {
        return std::ptr::null_mut();
    }
    let Ok(config) = configuration_or_error(command) else {
        return std::ptr::null_mut();
    };
    perform(ScriptCommand::NewWindow { config })
}

unsafe extern "C-unwind" fn app_new_tab(_this: &AnyObject, _sel: Sel, command: &NSScriptCommand) -> *mut AnyObject {
    if !gate() {
        return std::ptr::null_mut();
    }
    let window = argument_id::<ScriptWindowObj>(command, "window", |o| o.ivars().0.clone());
    let Ok(config) = configuration_or_error(command) else {
        return std::ptr::null_mut();
    };
    perform(ScriptCommand::NewTab { window, config })
}

/// Selectors the scripting object classes (`BTScriptWindow`, `BTScriptTab`, `BTScriptTerminal`) answer for the sdef's
/// commands. The classes are subclasses of `NSObject`, so a name `NSObject` already has would be overridden for every
/// object; `object_key_conflicts` checks that none is (#650).
pub(crate) const OBJECT_KEYS: [&str; 7] = [
    "handleActivateWindowCommand:",
    "handleCloseWindowCommand:",
    "handleSelectTabCommand:",
    "handleCloseTabCommand:",
    "handleFocusCommand:",
    "handleSplitCommand:",
    "handleCloseCommand:",
];

/// The `OBJECT_KEYS` that `NSObject` already implements (empty when the names are chosen well).
pub(crate) fn object_key_conflicts() -> Vec<&'static str> {
    let object = NSObject::class();
    OBJECT_KEYS
        .into_iter()
        .filter(|name| object.instance_method(Sel::register(name)).is_some())
        .collect()
}

/// Selectors this module adds to `NSApplication` (the application-level keys and command handlers the sdef names).
/// `install` adds exactly these; `app_key_conflicts` checks them against what AppKit already has.
pub(crate) const APP_KEYS: [&str; 7] = [
    "scriptWindows",
    "terminals",
    "btFrontWindow",
    "name",
    "version",
    "handleNewWindowScriptCommand:",
    "handleNewTabScriptCommand:",
];

/// The `APP_KEYS` that `NSApplication` already implements. `class_addMethod` leaves an existing method in place, so a
/// key listed here would silently run AppKit's method instead of ours. Meant to be called before `install`.
pub(crate) fn app_key_conflicts() -> Vec<&'static str> {
    let app = NSApplication::class();
    APP_KEYS
        .into_iter()
        .filter(|name| {
            app.instance_method(Sel::register(name)).is_some()
        })
        .collect()
}

/// Adds a method to a class that already exists (`NSApplication`), unless it already has one by that name.
fn add_method(class: &AnyClass, sel: Sel, imp: Imp, types: &CStr) {
    // SAFETY: `class` is a live class, `imp` has the C signature `types` describes (see the callers), and the selector
    // is valid. `class_addMethod` returns NO (and changes nothing) when the class already has the method.
    let added = unsafe {
        objc2::ffi::class_addMethod(
            class as *const AnyClass as *mut objc2::ffi::objc_class,
            sel.as_ptr(),
            Some(imp),
            types.as_ptr(),
        )
    };
    if !added {
        tracing::warn!("NSApplication already has {sel:?}; scripting leaves AppKit's method in place");
    }
}

fn install() {
    // Register the classes now: Cocoa Scripting looks them up by name when it first reads the sdef.
    let _ = (
        ScriptWindowObj::class(),
        ScriptTabObj::class(),
        ScriptTerminalObj::class(),
        ScriptInputTextCommand::class(),
        ScriptSendKeyCommand::class(),
        ScriptPerformActionCommand::class(),
        ScriptNewSurfaceConfigurationCommand::class(),
        ScriptMouseButtonCommand::class(),
        ScriptMousePosCommand::class(),
        ScriptMouseScrollCommand::class(),
    );
    let app = NSApplication::class();
    // SAFETY (all `transmute`s below): each function has the C signature of its type alias, which is the shape the
    // type encoding string next to it declares ("@@:" = returns an object, takes self and the selector; "@@:@" = one
    // more object argument). `Imp` is the untyped function pointer `class_addMethod` expects.
    let getter = |f: Getter| -> Imp { unsafe { std::mem::transmute::<Getter, Imp>(f) } };
    let handler = |f: CommandHandler| -> Imp { unsafe { std::mem::transmute::<CommandHandler, Imp>(f) } };
    let g = c"@@:";
    let h = c"@@:@";
    add_method(app, sel!(scriptWindows), getter(app_script_windows), g);
    add_method(app, sel!(terminals), getter(app_terminals), g);
    add_method(app, sel!(btFrontWindow), getter(app_front_window), g);
    add_method(app, sel!(name), getter(app_name), g);
    add_method(app, sel!(version), getter(app_version), g);
    add_method(app, sel!(handleNewWindowScriptCommand:), handler(app_new_window), h);
    add_method(app, sel!(handleNewTabScriptCommand:), handler(app_new_tab), h);
}

#[cfg(test)]
mod config_tests {
    //! `surface configuration` record reading and writing against real Foundation objects (#666). The vendor crate is
    //! outside the workspace, so run these with `cargo test --manifest-path vendor/winit/Cargo.toml --lib config_tests`.
    use super::*;

    fn record(entries: Vec<(&str, Retained<NSObject>)>) -> Retained<NSDictionary<NSString, AnyObject>> {
        let keys: Vec<Retained<NSString>> = entries.iter().map(|(k, _)| ns(k)).collect();
        let key_refs: Vec<&NSString> = keys.iter().map(|k| &**k).collect();
        let values: Vec<Retained<NSObject>> = entries.into_iter().map(|(_, v)| v).collect();
        let dict: Retained<NSDictionary<NSString, NSObject>> = NSDictionary::from_vec(&key_refs, values);
        // SAFETY: the values are `NSObject`s, which is what `AnyObject` stands for here; only reads follow.
        unsafe { Retained::cast::<NSDictionary<NSString, AnyObject>>(dict) }
    }

    fn text(s: &str) -> Retained<NSObject> {
        Retained::into_super(ns(s))
    }

    fn number(n: f64) -> Retained<NSObject> {
        Retained::into_super(Retained::into_super(NSNumber::numberWithDouble(n)))
    }

    fn boolean(b: bool) -> Retained<NSObject> {
        Retained::into_super(Retained::into_super(NSNumber::numberWithBool(b)))
    }

    fn list(items: &[&str]) -> Retained<NSObject> {
        Retained::into_super(NSArray::from_vec(items.iter().map(|s| ns(s)).collect()))
    }

    #[test]
    fn a_full_record_is_read_by_the_cocoa_keys() {
        let r = record(vec![
            ("fontSize", number(14.5)),
            ("workingDirectory", text("/tmp")),
            ("command", text("echo hi")),
            ("initialInput", text("ls\n")),
            ("waitAfterCommand", boolean(true)),
            ("environmentVariables", list(&["A=1", "B=two=2"])),
            ("somethingElse", text("ignored")),
        ]);
        let c = config_from_record(&r).unwrap();
        assert_eq!(c.font_size, Some(14.5));
        assert_eq!(c.working_directory.as_deref(), Some("/tmp"));
        assert_eq!(c.command.as_deref(), Some("echo hi"));
        assert_eq!(c.initial_input.as_deref(), Some("ls\n"));
        assert_eq!(c.wait_after_command, Some(true));
        assert_eq!(c.environment, Some(vec!["A=1".to_string(), "B=two=2".to_string()]));
        // A record with no keys reads as nothing chosen
        assert_eq!(config_from_record(&record(vec![])).unwrap(), ScriptConfig::default());
    }

    #[test]
    fn wrong_kinds_are_errors() {
        let bad = |key: &str, value: Retained<NSObject>| config_from_record(&record(vec![(key, value)])).is_err();
        assert!(bad("fontSize", text("12")));
        assert!(bad("workingDirectory", number(1.0)));
        assert!(bad("command", boolean(true)));
        assert!(bad("initialInput", number(1.0)));
        assert!(bad("waitAfterCommand", text("yes")));
        assert!(bad("environmentVariables", number(1.0)));
        // An item that is not text
        let with_number: Retained<NSObject> =
            Retained::into_super(NSArray::from_vec(vec![NSNumber::numberWithDouble(1.0)]));
        assert!(bad("environmentVariables", with_number));
    }

    #[test]
    fn sizes_are_checked_before_copying() {
        let ok = |key: &str, n: usize| config_from_record(&record(vec![(key, text(&"x".repeat(n)))])).is_ok();
        assert!(ok("command", MAX_CONFIG_TEXT_BYTES));
        assert!(!ok("command", MAX_CONFIG_TEXT_BYTES + 1));
        assert!(ok("workingDirectory", MAX_CONFIG_TEXT_BYTES));
        assert!(!ok("workingDirectory", MAX_CONFIG_TEXT_BYTES + 1));
        assert!(ok("initialInput", MAX_CONFIG_INPUT_BYTES));
        assert!(!ok("initialInput", MAX_CONFIG_INPUT_BYTES + 1));
        // The limit is on UTF-8 bytes: 3 bytes per Hangul syllable
        let hangul = "가".repeat(MAX_CONFIG_TEXT_BYTES / 3 + 1);
        assert!(config_from_record(&record(vec![("command", text(&hangul))])).is_err());
        let many = |n: usize| {
            let items: Vec<String> = (0..n).map(|i| format!("K{i}=v")).collect();
            let refs: Vec<&str> = items.iter().map(|s| s.as_str()).collect();
            config_from_record(&record(vec![("environmentVariables", list(&refs))]))
        };
        assert!(many(MAX_CONFIG_ENV_COUNT).is_ok());
        assert!(many(MAX_CONFIG_ENV_COUNT + 1).is_err());
        let long = format!("K={}", "x".repeat(MAX_CONFIG_TEXT_BYTES));
        assert!(config_from_record(&record(vec![("environmentVariables", list(&[&long]))])).is_err());
    }

    #[test]
    fn one_text_counts_as_a_list_of_one() {
        let c = config_from_record(&record(vec![("environmentVariables", text("A=1"))])).unwrap();
        assert_eq!(c.environment, Some(vec!["A=1".to_string()]));
    }

    #[test]
    fn the_answer_record_has_every_key_and_reads_back() {
        let d = config_dictionary(&ScriptConfig::default());
        // SAFETY: only reads follow; every value is an `NSObject`.
        let d = unsafe { Retained::cast::<NSDictionary<NSString, AnyObject>>(d) };
        let c = config_from_record(&d).unwrap();
        assert_eq!(c.font_size, Some(0.0));
        assert_eq!(c.working_directory.as_deref(), Some(""));
        assert_eq!(c.command.as_deref(), Some(""));
        assert_eq!(c.initial_input.as_deref(), Some(""));
        assert_eq!(c.wait_after_command, Some(false));
        assert_eq!(c.environment, Some(vec![]));
        let full = ScriptConfig {
            font_size: Some(18.0),
            working_directory: Some("/tmp".into()),
            command: Some("ls".into()),
            initial_input: Some("x".into()),
            wait_after_command: Some(true),
            environment: Some(vec!["A=b".into()]),
        };
        let d = config_dictionary(&full);
        // SAFETY: as above.
        let d = unsafe { Retained::cast::<NSDictionary<NSString, AnyObject>>(d) };
        assert_eq!(config_from_record(&d).unwrap(), full);
    }
}
