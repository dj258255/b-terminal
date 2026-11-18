//! b-terminal(#631): Finder Services "New b-terminal Tab Here" / "New b-terminal Window Here" (as Ghostty has). The
//! system calls `openTab:userData:error:` / `openWindow:userData:error:` on the object set with
//! `-[NSApplication setServicesProvider:]` and hands over a pasteboard holding the selected paths. This module only
//! pulls the paths out of the pasteboard and passes them to a function the app registered; the app decides what to open.
//!
//! The provider is installed only when the process has a bundle identifier (it runs from an app bundle). Without one
//! the system cannot offer the services, so nothing is installed.

use std::path::PathBuf;
use std::sync::{Arc, Mutex, Once};

use objc2::rc::Retained;
use objc2::runtime::AnyObject;
use objc2::{declare_class, msg_send_id, mutability, ClassType, DeclaredClass};
use objc2_app_kit::{
    NSApplication, NSFilenamesPboardType, NSPasteboard, NSPasteboardTypeFileURL, NSPasteboardTypeString,
};
use objc2_foundation::{MainThreadMarker, NSArray, NSObject, NSObjectProtocol, NSString, NSURL};

use crate::platform::macos::ServiceTarget;

type Handler = Arc<dyn Fn(ServiceTarget, Vec<PathBuf>) + Send + Sync>;

struct State {
    handler: Option<Handler>,
    pending: Vec<(ServiceTarget, Vec<PathBuf>)>,
}

static STATE: Mutex<State> = Mutex::new(State { handler: None, pending: Vec::new() });
static PROVIDER: Once = Once::new();

/// Upper bound on the paths taken from one request (a pasteboard can hold anything the sender put there).
const MAX_PATHS: usize = 256;
/// Upper bound on the plain-text fallback that is split into lines.
const MAX_TEXT_BYTES: usize = 64 * 1024;

declare_class!(
    struct ServicesProvider;

    unsafe impl ClassType for ServicesProvider {
        type Super = NSObject;
        // Services are called on the main thread; the provider holds no state.
        type Mutability = mutability::InteriorMutable;
        const NAME: &'static str = "BTServicesProvider";
    }

    impl DeclaredClass for ServicesProvider {}

    unsafe impl ServicesProvider {
        #[method(openTab:userData:error:)]
        fn open_tab(&self, pasteboard: &NSPasteboard, _user_data: Option<&NSString>, error: *mut *mut NSString) {
            handle(ServiceTarget::Tab, pasteboard, error);
        }

        #[method(openWindow:userData:error:)]
        fn open_window(&self, pasteboard: &NSPasteboard, _user_data: Option<&NSString>, error: *mut *mut NSString) {
            handle(ServiceTarget::Window, pasteboard, error);
        }
    }
);

impl ServicesProvider {
    fn new() -> Retained<Self> {
        let this = Self::alloc().set_ivars(());
        // SAFETY: `NSObject`'s designated initialiser.
        unsafe { msg_send_id![super(this), init] }
    }
}

fn handle(target: ServiceTarget, pasteboard: &NSPasteboard, error: *mut *mut NSString) {
    let paths = paths_from(pasteboard);
    if paths.is_empty() {
        // The system shows this to the user. Ghostty answers the same way when nothing usable came.
        if !error.is_null() {
            let msg = NSString::from_str("Could not find any path in the selection.");
            // SAFETY: `error` is the out-parameter the system passed (checked non-null); the message is autoreleased,
            // as the services contract requires for it.
            unsafe { *error = Retained::autorelease_ptr(msg) };
        }
        return;
    }
    deliver(target, paths);
}

/// File paths on a services pasteboard: the legacy file-name list (what Finder sends for selected items), else file URLs,
/// else plain text, one path per line (a selected path in a text field). Only absolute paths are kept; the app checks
/// the rest.
fn paths_from(pb: &NSPasteboard) -> Vec<PathBuf> {
    let mut out: Vec<PathBuf> = Vec::new();
    // SAFETY: the type constants are valid for the life of the process; `propertyListForType` has no preconditions
    // and returns an object or nil. The result is checked by class before use.
    if let Some(list) = pb.propertyListForType(unsafe { NSFilenamesPboardType }) {
        if kind_of::<NSArray>(&list) {
            // SAFETY: the class check above makes the cast valid; `NSArray` is only read.
            let arr: &NSArray = unsafe { &*(&*list as *const AnyObject).cast::<NSArray>() };
            for item in arr.iter().take(MAX_PATHS) {
                if kind_of::<NSString>(item) {
                    // SAFETY: class checked on the line above.
                    let s: &NSString = unsafe { &*(item as *const AnyObject).cast::<NSString>() };
                    push_abs(&mut out, s.to_string());
                }
            }
        }
    }
    if out.is_empty() {
        // SAFETY: plain read of a string type; a missing type returns nil.
        if let Some(s) = unsafe { pb.stringForType(NSPasteboardTypeFileURL) } {
            if let Some(p) = file_url_path(&s.to_string()) {
                push_abs(&mut out, p);
            }
        }
    }
    if out.is_empty() {
        // SAFETY: as above.
        if let Some(s) = unsafe { pb.stringForType(NSPasteboardTypeString) } {
            let text = s.to_string();
            if text.len() <= MAX_TEXT_BYTES {
                for line in text.lines().take(MAX_PATHS) {
                    let line = line.trim();
                    match file_url_path(line) {
                        Some(p) => push_abs(&mut out, p),
                        None => push_abs(&mut out, line.to_owned()),
                    }
                }
            }
        }
    }
    out
}

/// Whether `obj` is an instance of `T` (or a subclass). Class clusters such as `NSArray` and `NSString` need this check.
fn kind_of<T: ClassType>(obj: &AnyObject) -> bool {
    // SAFETY: every Objective-C object an `NSPasteboard`/`NSArray` hands out is an `NSObject`; `isKindOfClass:` is read-only.
    let obj: &NSObject = unsafe { &*(obj as *const AnyObject).cast::<NSObject>() };
    obj.is_kind_of::<T>()
}

fn push_abs(out: &mut Vec<PathBuf>, s: String) {
    if s.starts_with('/') && !s.contains('\0') {
        out.push(PathBuf::from(s));
    }
}

/// The path of a `file://` URL string, `None` for anything else.
fn file_url_path(s: &str) -> Option<String> {
    if !s.starts_with("file:") {
        return None;
    }
    // SAFETY: `URLWithString:` takes any string and returns nil when it is not a valid URL.
    let url: Retained<NSURL> = unsafe { NSURL::URLWithString(&NSString::from_str(s)) }?;
    // SAFETY: plain property reads on a live URL object.
    unsafe {
        if !url.isFileURL() {
            return None;
        }
        url.path().map(|p| p.to_string())
    }
}

fn deliver(target: ServiceTarget, paths: Vec<PathBuf>) {
    let handler = {
        let mut g = STATE.lock().unwrap_or_else(|e| e.into_inner());
        match g.handler.clone() {
            Some(h) => h,
            None => {
                g.pending.push((target, paths));
                return;
            }
        }
    };
    handler(target, paths);
}

/// Registers the function that receives the paths, installs the provider (with a bundle identifier only) and passes it
/// the requests that arrived earlier.
pub fn set_handler(handler: Box<dyn Fn(ServiceTarget, Vec<PathBuf>) + Send + Sync>) {
    let handler: Handler = handler.into();
    let pending = {
        let mut g = STATE.lock().unwrap_or_else(|e| e.into_inner());
        g.handler = Some(handler.clone());
        std::mem::take(&mut g.pending)
    };
    install();
    for (target, paths) in pending {
        handler(target, paths);
    }
}

fn install() {
    if !super::user_notify::supported() {
        return;
    }
    let Some(mtm) = MainThreadMarker::new() else {
        tracing::warn!("services provider not installed: not on the main thread");
        return;
    };
    PROVIDER.call_once(|| {
        let provider = ServicesProvider::new();
        let app = NSApplication::sharedApplication(mtm);
        // SAFETY: `setServicesProvider:` keeps an unretained reference, so the provider is leaked to outlive the app.
        unsafe { app.setServicesProvider(Some(&*provider)) };
        std::mem::forget(provider);
    });
}
