#![allow(clippy::unnecessary_cast)]

use objc2::rc::{autoreleasepool, Retained};
use objc2::{declare_class, msg_send, mutability, ClassType, DeclaredClass};
use objc2_app_kit::{NSResponder, NSScreen, NSWindow, NSWindowStyleMask};
use objc2_foundation::{MainThreadBound, MainThreadMarker, NSObject, NSRect};

use super::event_loop::ActiveEventLoop;
use super::window_delegate::WindowDelegate;
use crate::error::OsError as RootOsError;
use crate::window::WindowAttributes;

pub(crate) struct Window {
    window: MainThreadBound<Retained<WinitWindow>>,
    /// The window only keeps a weak reference to this, so we must keep it around here.
    delegate: MainThreadBound<Retained<WindowDelegate>>,
}

impl Drop for Window {
    fn drop(&mut self) {
        self.window.get_on_main(|window| autoreleasepool(|_| window.close()))
    }
}

impl Window {
    pub(crate) fn new(
        window_target: &ActiveEventLoop,
        attributes: WindowAttributes,
    ) -> Result<Self, RootOsError> {
        let mtm = window_target.mtm;
        let delegate = autoreleasepool(|_| {
            WindowDelegate::new(window_target.app_delegate(), attributes, mtm)
        })?;
        Ok(Window {
            window: MainThreadBound::new(delegate.window().retain(), mtm),
            delegate: MainThreadBound::new(delegate, mtm),
        })
    }

    pub(crate) fn maybe_queue_on_main(&self, f: impl FnOnce(&WindowDelegate) + Send + 'static) {
        // For now, don't actually do queuing, since it may be less predictable
        self.maybe_wait_on_main(f)
    }

    pub(crate) fn maybe_wait_on_main<R: Send>(
        &self,
        f: impl FnOnce(&WindowDelegate) -> R + Send,
    ) -> R {
        self.delegate.get_on_main(|delegate| f(delegate))
    }

    #[cfg(feature = "rwh_06")]
    #[inline]
    pub(crate) fn raw_window_handle_rwh_06(
        &self,
    ) -> Result<rwh_06::RawWindowHandle, rwh_06::HandleError> {
        if let Some(mtm) = MainThreadMarker::new() {
            Ok(self.delegate.get(mtm).raw_window_handle_rwh_06())
        } else {
            Err(rwh_06::HandleError::Unavailable)
        }
    }

    #[cfg(feature = "rwh_06")]
    #[inline]
    pub(crate) fn raw_display_handle_rwh_06(
        &self,
    ) -> Result<rwh_06::RawDisplayHandle, rwh_06::HandleError> {
        Ok(rwh_06::RawDisplayHandle::AppKit(rwh_06::AppKitDisplayHandle::new()))
    }
}

#[derive(Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct WindowId(pub usize);

impl WindowId {
    pub const fn dummy() -> Self {
        Self(0)
    }
}

impl From<WindowId> for u64 {
    fn from(window_id: WindowId) -> Self {
        window_id.0 as u64
    }
}

impl From<u64> for WindowId {
    fn from(raw_id: u64) -> Self {
        Self(raw_id as usize)
    }
}

declare_class!(
    #[derive(Debug)]
    pub struct WinitWindow;

    unsafe impl ClassType for WinitWindow {
        #[inherits(NSResponder, NSObject)]
        type Super = NSWindow;
        type Mutability = mutability::MainThreadOnly;
        const NAME: &'static str = "WinitWindow";
    }

    impl DeclaredClass for WinitWindow {}

    unsafe impl WinitWindow {
        #[method(canBecomeMainWindow)]
        fn can_become_main_window(&self) -> bool {
            trace_scope!("canBecomeMainWindow");
            true
        }

        #[method(canBecomeKeyWindow)]
        fn can_become_key_window(&self) -> bool {
            trace_scope!("canBecomeKeyWindow");
            true
        }

        // b-terminal 패치(#110): 테두리 없는 창(퀵 터미널)은 macOS가 메뉴 막대 아래로 밀어 넣지 않게 둔다.
        // 화면 위쪽 바깥에서 미끄러져 들어오는 움직임에 필요하다. 제목 막대가 있는 창은 원래대로
        #[method(constrainFrameRect:toScreen:)]
        fn constrain_frame_rect(&self, frame: NSRect, screen: Option<&NSScreen>) -> NSRect {
            if self.styleMask().contains(NSWindowStyleMask::Titled) {
                unsafe { msg_send![super(self), constrainFrameRect: frame, toScreen: screen] }
            } else {
                frame
            }
        }
    }
);

impl WinitWindow {
    pub(super) fn id(&self) -> WindowId {
        WindowId(self as *const Self as usize)
    }
}
