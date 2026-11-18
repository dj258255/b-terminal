//! b-terminal(#629, #630): reads and writes that need AppKit/Foundation calls the application cannot make itself
//! (the workspace forbids `unsafe`): a named (private) pasteboard that holds the selection clipboard shared between
//! b-terminal processes, and the system scroll bar settings. Neither needs an app bundle.

use objc2::rc::Retained;
use objc2::{msg_send, msg_send_id, ClassType};
use objc2_app_kit::{NSPasteboard, NSPasteboardTypeString, NSScroller, NSScrollerStyle};
use objc2_foundation::{MainThreadMarker, NSString, NSUserDefaults};

/// The pasteboards of the system itself (general, find, font, ruler, drag) are named `Apple CFPasteboard ...`. These
/// functions are for the app's own pasteboards only and never touch them, so the user's clipboard stays safe.
fn is_system_name(name: &str) -> bool {
    name.is_empty() || name.starts_with("Apple ")
}

fn board(name: &str) -> Option<Retained<NSPasteboard>> {
    if is_system_name(name) {
        return None;
    }
    let name = NSString::from_str(name);
    // SAFETY: `pasteboardWithName:` accepts any string and returns the pasteboard of that name, creating it.
    Some(unsafe { NSPasteboard::pasteboardWithName(&name) })
}

pub fn set_named_pasteboard_string(name: &str, text: &str) -> bool {
    let Some(pb) = board(name) else { return false };
    let text = NSString::from_str(text);
    // SAFETY: plain message sends on a live pasteboard; `NSPasteboardTypeString` is a framework constant.
    unsafe {
        pb.clearContents();
        pb.setString_forType(&text, NSPasteboardTypeString)
    }
}

pub fn named_pasteboard_string(name: &str) -> Option<String> {
    let pb = board(name)?;
    // SAFETY: as above; `stringForType:` returns nil when the pasteboard holds no string.
    unsafe { pb.stringForType(NSPasteboardTypeString) }.map(|s| s.to_string())
}

pub fn release_named_pasteboard(name: &str) {
    let Some(pb) = board(name) else { return };
    // SAFETY: `releaseGlobally` takes no arguments and returns nothing; the pasteboard is not used afterwards.
    unsafe {
        let _: () = msg_send![&*pb, releaseGlobally];
    }
}

/// `(always_visible, jump_to_click)`: the system settings "Show scroll bars: Always" (the preferred scroller style is
/// legacy) and "Click in the scroll bar to: Jump to the spot that's clicked" (`AppleScrollerPagingBehavior`).
/// `None` off the main thread (the style query needs the main thread).
pub fn scroller_preferences() -> Option<(bool, bool)> {
    let mtm = MainThreadMarker::new()?;
    // SAFETY: `preferredScrollerStyle` is a class property read that needs the main thread, which `mtm` proves.
    let style = unsafe { NSScroller::preferredScrollerStyle(mtm) };
    let key = NSString::from_str("AppleScrollerPagingBehavior");
    // SAFETY: `standardUserDefaults` and `boolForKey:` have no preconditions; the key is a valid string. The call
    // only reads (the global domain is part of the standard search list).
    let jump = unsafe {
        let defaults: Retained<NSUserDefaults> = msg_send_id![NSUserDefaults::class(), standardUserDefaults];
        defaults.boolForKey(&key)
    };
    Some((style == NSScrollerStyle::Legacy, jump))
}
