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
    NSArray, NSBundle, NSDate, NSDefaultRunLoopMode, NSInteger, NSObject, NSRunLoop, NSScriptClassDescription,
    NSScriptCommand, NSScriptObjectSpecifier, NSString, NSUniqueIDSpecifier,
};

/// `errAEEventNotPermitted`: scripting is switched off.
const ERR_NOT_PERMITTED: NSInteger = -1743;
/// `errAEEventFailed`: the command could not be carried out.
const ERR_FAILED: NSInteger = -10000;
/// `errAEParamMissed`: a required parameter is missing or of the wrong kind.
const ERR_PARAM: NSInteger = -1715;
/// How long a command waits for the app to answer.
const REPLY_TIMEOUT: Duration = Duration::from_secs(5);
/// A snapshot younger than this is used as it is.
const SNAPSHOT_FRESH: Duration = Duration::from_millis(100);
/// After a failed snapshot ask, wait this long before asking again.
const SNAPSHOT_RETRY: Duration = Duration::from_secs(2);
/// `input text` is refused above this many UTF-8 bytes (checked before the text is copied into a Rust string).
const MAX_TEXT_BYTES: usize = 1 << 20;

/// One terminal pane as the app reports it.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ScriptTerminal {
    pub id: String,
    pub title: String,
    pub working_directory: Option<String>,
}

/// One tab (a layout of panes).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ScriptTab {
    pub id: String,
    pub title: String,
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

/// A command that changes the app. Ids are the strings from the snapshot. `Debug` leaves out the text of `InputText`
/// (it may hold a password or a secret and commands end up in logs).
#[derive(Clone, PartialEq, Eq)]
pub enum ScriptCommand {
    /// Asks the app to call `set_snapshot` with its current state and then answer `Ok(None)`.
    Snapshot,
    NewWindow,
    NewTab { window: Option<String> },
    Focus { terminal: String },
    ActivateWindow { window: String },
    SelectTab { tab: String },
    InputText { terminal: String, text: String },
}

impl std::fmt::Debug for ScriptCommand {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Snapshot => f.write_str("Snapshot"),
            Self::NewWindow => f.write_str("NewWindow"),
            Self::NewTab { window } => f.debug_struct("NewTab").field("window", window).finish(),
            Self::Focus { terminal } => f.debug_struct("Focus").field("terminal", terminal).finish(),
            Self::ActivateWindow { window } => f.debug_struct("ActivateWindow").field("window", window).finish(),
            Self::SelectTab { tab } => f.debug_struct("SelectTab").field("tab", tab).finish(),
            Self::InputText { terminal, text } => f
                .debug_struct("InputText")
                .field("terminal", terminal)
                .field("text_bytes", &text.len())
                .finish(),
        }
    }
}

/// What a command returns to the script: the object it created, if any.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ScriptTarget {
    Window(String),
    Tab(String),
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
            Ok(reply) => return reply,
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

/// The id of the `T` object in a command argument.
fn argument_id<T: ClassType + 'static>(command: &NSScriptCommand, key: &str, id_of: fn(&T) -> String) -> Option<String> {
    // SAFETY: a property read on the live command; the value is checked for its class.
    let value = unsafe { command.evaluatedArguments() }.and_then(|d| unsafe { d.objectForKey(&ns(key)) })?;
    downcast::<T>(value).map(|o| id_of(&o))
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

        #[method_id(workingDirectory)]
        fn working_directory(&self) -> Option<Retained<NSString>> {
            terminal_working_directory(&self.ivars().0)
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

unsafe extern "C-unwind" fn app_new_window(_this: &AnyObject, _sel: Sel, _command: &NSScriptCommand) -> *mut AnyObject {
    if !gate() {
        return std::ptr::null_mut();
    }
    perform(ScriptCommand::NewWindow)
}

unsafe extern "C-unwind" fn app_new_tab(_this: &AnyObject, _sel: Sel, command: &NSScriptCommand) -> *mut AnyObject {
    if !gate() {
        return std::ptr::null_mut();
    }
    let window = argument_id::<ScriptWindowObj>(command, "window", |o| o.ivars().0.clone());
    perform(ScriptCommand::NewTab { window })
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
