#[macro_use]
mod util;

mod app;
mod app_state;
mod cursor;
mod event;
mod event_handler;
mod event_loop;
mod ffi;
mod menu;
mod monitor;
mod observer;
mod view;
mod window;
mod window_delegate;

use std::fmt;

pub(crate) use self::event::{physicalkey_to_scancode, scancode_to_physicalkey, KeyEventExtra};
pub(crate) use self::event_loop::{
    ActiveEventLoop, EventLoop, EventLoopProxy, OwnedDisplayHandle,
    PlatformSpecificEventLoopAttributes,
};
pub(crate) use self::monitor::{MonitorHandle, VideoModeHandle};
pub(crate) use self::window::WindowId;
pub(crate) use self::window_delegate::PlatformSpecificWindowAttributes;
use crate::event::DeviceId as RootDeviceId;

pub(crate) use self::cursor::CustomCursor as PlatformCustomCursor;
pub(crate) use self::window::Window;
pub(crate) use crate::cursor::OnlyCursorImageSource as PlatformCustomCursorSource;
pub(crate) use crate::icon::NoIcon as PlatformIcon;
pub(crate) use crate::platform_impl::Fullscreen;

/// b-terminal(#258): Carbon secure event input is process-global, so it is a free function rather than a window method.
pub fn set_secure_event_input(enable: bool) -> bool {
    // SAFETY: both functions take no arguments and have no preconditions.
    let status = unsafe {
        if enable {
            ffi::EnableSecureEventInput()
        } else {
            ffi::DisableSecureEventInput()
        }
    };
    status == 0
}

/// b-terminal(#278): shows an `NSMenu` at the mouse pointer and returns when it is dismissed (tracking is modal).
/// The menu is owned by the caller (muda's `ContextMenu::ns_menu()`); it is only borrowed for the call.
/// Returns `false` off the main thread, for a null pointer, or when the menu was dismissed without a choice.
pub fn popup_context_menu(ns_menu: *mut std::ffi::c_void) -> bool {
    use objc2_app_kit::{NSEvent, NSMenu};
    if ns_menu.is_null() || objc2_foundation::MainThreadMarker::new().is_none() {
        return false;
    }
    // SAFETY: the caller passes a live `NSMenu` (documented contract, checked non-null above) and we are on the
    // main thread, which AppKit menu tracking requires. With no view, the location is in screen coordinates.
    unsafe {
        let menu: &NSMenu = &*ns_menu.cast::<NSMenu>();
        menu.popUpMenuPositioningItem_atLocation_inView(None, NSEvent::mouseLocation(), None)
    }
}

#[derive(Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct DeviceId;

impl DeviceId {
    pub const fn dummy() -> Self {
        DeviceId
    }
}

// Constant device ID; to be removed when if backend is updated to report real device IDs.
pub(crate) const DEVICE_ID: RootDeviceId = RootDeviceId(DeviceId);

#[derive(Debug)]
pub enum OsError {
    CGError(core_graphics::base::CGError),
    CreationError(&'static str),
}

impl fmt::Display for OsError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            OsError::CGError(e) => f.pad(&format!("CGError {e}")),
            OsError::CreationError(e) => f.pad(e),
        }
    }
}
