//! b-terminal(#632): sets the color space of the `CAMetalLayer` under a window's view to sRGB, so that a GPU renderer
//! that draws sRGB values can leave the surface's color space alone and let the compositor convert sRGB to the display
//! (as Ghostty does for `window-colorspace = srgb`). wgpu/raw-window-metal create the `CAMetalLayer` (it may be a
//! sublayer of the view's root layer), so this walks the layer tree. The app sees a safe function in `platform::macos`.
//!
//! wgpu rewrites the layer's color space on every `Surface::configure`, so the caller applies this after each configure.

use std::ffi::c_void;

use objc2::rc::Retained;
use objc2::runtime::{AnyClass, AnyObject};
use objc2::{msg_send, msg_send_id, Encode, Encoding};
use objc2_foundation::{MainThreadMarker, NSArray};

/// `CGColorSpaceRef`, with the encoding the Objective-C runtime reports for it (`^{CGColorSpace=}`).
#[repr(transparent)]
#[derive(Clone, Copy)]
struct CGColorSpaceRef(*mut c_void);

// SAFETY: `CGColorSpaceRef` is a pointer to the opaque struct `CGColorSpace`, so this is its Objective-C encoding.
unsafe impl Encode for CGColorSpaceRef {
    const ENCODING: Encoding = Encoding::Pointer(&Encoding::Struct("CGColorSpace", &[]));
}

/// How deep the layer tree is searched. raw-window-metal puts the metal layer directly under (or as) the root layer.
const MAX_DEPTH: usize = 4;

/// Sets the color space of every `CAMetalLayer` at or under the layer of `ns_view` to sRGB. Returns `true` when at
/// least one layer was found and set. Returns `false` (and changes nothing) off the main thread, for a null view, or
/// when no metal layer exists yet.
///
/// The caller guarantees that `ns_view` is null or points to a live `NSView` (the pointer comes from the window).
pub fn set_metal_layer_srgb(ns_view: *mut c_void) -> bool {
    if ns_view.is_null() || MainThreadMarker::new().is_none() {
        return false;
    }
    let Some(metal) = AnyClass::get("CAMetalLayer") else {
        return false;
    };
    // SAFETY: `kCGColorSpaceSRGB` is a constant string owned by CoreGraphics; `CGColorSpaceCreateWithName` has no other
    // preconditions and returns a +1 reference (or null), released below.
    let space = unsafe { CGColorSpaceCreateWithName(core_graphics::color_space::kCGColorSpaceSRGB as *const c_void) };
    if space.is_null() {
        return false;
    }
    // SAFETY: the caller guarantees `ns_view` is a live `NSView`; `layer` is a plain getter on the main thread.
    let root: Option<Retained<AnyObject>> = unsafe { msg_send_id![ns_view as *mut AnyObject, layer] };
    let applied = root.is_some_and(|root| apply(&root, metal, CGColorSpaceRef(space), 0));
    // SAFETY: `space` is the +1 reference created above; `setColorspace:` retains what it keeps.
    unsafe { CFRelease(space) };
    applied
}

fn apply(layer: &AnyObject, metal: &AnyClass, space: CGColorSpaceRef, depth: usize) -> bool {
    let mut applied = false;
    // SAFETY: `layer` is a live `CALayer`; `isKindOfClass:` takes a class and returns BOOL.
    let is_metal: bool = unsafe { msg_send![layer, isKindOfClass: metal] };
    if is_metal {
        // SAFETY: `CAMetalLayer` has `setColorspace:(CGColorSpaceRef)`; `space` is a valid color space.
        let () = unsafe { msg_send![layer, setColorspace: space] };
        applied = true;
    }
    if depth < MAX_DEPTH {
        // SAFETY: `sublayers` returns nil or an `NSArray` of `CALayer`s.
        let subs: Option<Retained<NSArray<AnyObject>>> = unsafe { msg_send_id![layer, sublayers] };
        if let Some(subs) = subs {
            for sub in subs.iter() {
                applied |= apply(&sub, metal, space, depth + 1);
            }
        }
    }
    applied
}

#[link(name = "CoreGraphics", kind = "framework")]
extern "C" {
    fn CGColorSpaceCreateWithName(name: *const c_void) -> *mut c_void;
}

#[link(name = "CoreFoundation", kind = "framework")]
extern "C" {
    fn CFRelease(cf: *const c_void);
}
