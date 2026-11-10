//! Softbuffer implementation using CoreGraphics.
use crate::backend_interface::*;
use crate::error::InitError;
use crate::{util, Rect, SoftBufferError};
use objc2::rc::Retained;
use objc2::runtime::{AnyObject, Bool};
use objc2::{define_class, msg_send, AllocAnyThread, DefinedClass, MainThreadMarker, Message};
use objc2_core_foundation::{CFDictionary, CFNumber, CFRetained, CFString, CFType, CGPoint};
use objc2_core_graphics::{
    CGBitmapInfo, CGColorRenderingIntent, CGColorSpace, CGDataProvider, CGImage, CGImageAlphaInfo,
    CGImageByteOrderInfo, CGImageComponentInfo, CGImagePixelFormatInfo,
};
// b-terminal(#346)
use crate::iosurface_pool::{self, Pick, SlotInfo};
use objc2_core_graphics::kCGColorSpaceSRGB;
use objc2_io_surface::{
    kIOSurfaceBytesPerElement, kIOSurfaceColorSpace, kIOSurfaceHeight, kIOSurfacePixelFormat,
    kIOSurfaceWidth, IOSurfaceLockOptions, IOSurfaceRef,
};
use objc2_foundation::{
    ns_string, NSDictionary, NSKeyValueChangeKey, NSKeyValueChangeNewKey,
    NSKeyValueObservingOptions, NSNumber, NSObject, NSObjectNSKeyValueObserverRegistration,
    NSString, NSValue,
};
use objc2_quartz_core::{kCAGravityTopLeft, CALayer, CATransaction};
use raw_window_handle::{HasDisplayHandle, HasWindowHandle, RawWindowHandle};

use std::ffi::c_void;
use std::marker::PhantomData;
use std::mem::size_of;
use std::num::NonZeroU32;
use std::ops::Deref;
use std::ptr::{self, slice_from_raw_parts_mut, NonNull};

define_class!(
    #[unsafe(super(NSObject))]
    #[name = "SoftbufferObserver"]
    #[ivars = SendCALayer]
    #[derive(Debug)]
    struct Observer;

    /// NSKeyValueObserving
    impl Observer {
        #[unsafe(method(observeValueForKeyPath:ofObject:change:context:))]
        fn observe_value(
            &self,
            key_path: Option<&NSString>,
            _object: Option<&AnyObject>,
            change: Option<&NSDictionary<NSKeyValueChangeKey, AnyObject>>,
            _context: *mut c_void,
        ) {
            self.update(key_path, change);
        }
    }
);

impl Observer {
    fn new(layer: &CALayer) -> Retained<Self> {
        let this = Self::alloc().set_ivars(SendCALayer(layer.retain()));
        unsafe { msg_send![super(this), init] }
    }

    fn update(
        &self,
        key_path: Option<&NSString>,
        change: Option<&NSDictionary<NSKeyValueChangeKey, AnyObject>>,
    ) {
        let layer = self.ivars();

        let change =
            change.expect("requested a change dictionary in `addObserver`, but none was provided");
        let new = change
            .objectForKey(unsafe { NSKeyValueChangeNewKey })
            .expect("requested change dictionary did not contain `NSKeyValueChangeNewKey`");

        // NOTE: Setting these values usually causes a quarter second animation to occur, which is
        // undesirable.
        //
        // However, since we're setting them inside an observer, there already is a transaction
        // ongoing, and as such we don't need to wrap this in a `CATransaction` ourselves.

        if key_path == Some(ns_string!("contentsScale")) {
            let new = new.downcast::<NSNumber>().unwrap();
            let scale_factor = new.as_cgfloat();

            // Set the scale factor of the layer to match the root layer when it changes (e.g. if
            // moved to a different monitor, or monitor settings changed).
            layer.setContentsScale(scale_factor);
        } else if key_path == Some(ns_string!("bounds")) {
            let new = new.downcast::<NSValue>().unwrap();
            let bounds = new.get_rect().expect("new bounds value was not CGRect");

            // Set `bounds` and `position` so that the new layer is inside the superlayer.
            //
            // This differs from just setting the `bounds`, as it also takes into account any
            // translation that the superlayer may have that we'd want to preserve.
            layer.setFrame(bounds);
        } else {
            panic!("unknown observed keypath {key_path:?}");
        }
    }
}

#[derive(Debug)]
pub struct CGImpl<D, W> {
    /// Our layer.
    layer: SendCALayer,
    /// The layer that our layer was created from.
    ///
    /// Can also be retrieved from `layer.superlayer()`.
    root_layer: SendCALayer,
    observer: Retained<Observer>,
    color_space: CFRetained<CGColorSpace>,
    /// b-terminal(#97): 색 공간을 가져온 화면 번호. 창이 다른 화면으로 가면 그 화면의 색 공간으로 바꾼다
    display: u32,
    /// b-terminal(#97): 마지막으로 본 창의 NSScreen 객체 자리. 같으면 화면 번호를 다시 묻지 않는다
    screen: usize,
    /// b-terminal(#346): IOSurface 묶음(최대 3개)과 각 칸의 색 공간 열쇠
    pool: Vec<(SendSurface, u64)>,
    /// b-terminal(#346): IOSurface로 올리다 실패했으면 이 그림판은 CGImage 길로 간다(프레임을 잃지 않으려고)
    io_failed: bool,
    /// b-terminal(#346): 지금 레이어 `contents`인 IOSurface의 자리(다시 쓰지 않는다)
    current: usize,
    /// The width of the underlying buffer.
    width: usize,
    /// The height of the underlying buffer.
    height: usize,
    window_handle: W,
    _display: PhantomData<D>,
}

impl<D, W> Drop for CGImpl<D, W> {
    fn drop(&mut self) {
        // SAFETY: Registered in `new`, must be removed before the observer is deallocated.
        unsafe {
            self.root_layer
                .removeObserver_forKeyPath(&self.observer, ns_string!("contentsScale"));
            self.root_layer
                .removeObserver_forKeyPath(&self.observer, ns_string!("bounds"));
        }
    }
}

impl<D: HasDisplayHandle, W: HasWindowHandle> SurfaceInterface<D, W> for CGImpl<D, W> {
    type Context = D;
    type Buffer<'a>
        = BufferImpl<'a, D, W>
    where
        Self: 'a;

    fn new(window_src: W, _display: &D) -> Result<Self, InitError<W>> {
        // `NSView`/`UIView` can only be accessed from the main thread.
        let _mtm = MainThreadMarker::new().ok_or(SoftBufferError::PlatformError(
            Some("can only access Core Graphics handles from the main thread".to_string()),
            None,
        ))?;

        let root_layer = match window_src.window_handle()?.as_raw() {
            RawWindowHandle::AppKit(handle) => {
                // SAFETY: The pointer came from `WindowHandle`, which ensures that the
                // `AppKitWindowHandle` contains a valid pointer to an `NSView`.
                //
                // We use `NSObject` here to avoid importing `objc2-app-kit`.
                let view: &NSObject = unsafe { handle.ns_view.cast().as_ref() };

                // Force the view to become layer backed
                let _: () = unsafe { msg_send![view, setWantsLayer: Bool::YES] };

                // SAFETY: `-[NSView layer]` returns an optional `CALayer`
                let layer: Option<Retained<CALayer>> = unsafe { msg_send![view, layer] };
                layer.expect("failed making the view layer-backed")
            }
            RawWindowHandle::UiKit(handle) => {
                // SAFETY: The pointer came from `WindowHandle`, which ensures that the
                // `UiKitWindowHandle` contains a valid pointer to an `UIView`.
                //
                // We use `NSObject` here to avoid importing `objc2-ui-kit`.
                let view: &NSObject = unsafe { handle.ui_view.cast().as_ref() };

                // SAFETY: `-[UIView layer]` returns `CALayer`
                let layer: Retained<CALayer> = unsafe { msg_send![view, layer] };
                layer
            }
            _ => return Err(InitError::Unsupported(window_src)),
        };

        // Add a sublayer, to avoid interfering with the root layer, since setting the contents of
        // e.g. a view-controlled layer is brittle.
        let layer = CALayer::new();
        root_layer.addSublayer(&layer);

        // Set the anchor point and geometry. Softbuffer's uses a coordinate system with the origin
        // in the top-left corner.
        //
        // NOTE: This doesn't really matter unless we start modifying the `position` of our layer
        // ourselves, but it's nice to have in place.
        layer.setAnchorPoint(CGPoint::new(0.0, 0.0));
        layer.setGeometryFlipped(true);

        // Do not use auto-resizing mask.
        //
        // This is done to work around a bug in macOS 14 and above, where views using auto layout
        // may end up setting fractional values as the bounds, and that in turn doesn't propagate
        // properly through the auto-resizing mask and with contents gravity.
        //
        // Instead, we keep the bounds of the layer in sync with the root layer using an observer,
        // see below.
        //
        // layer.setAutoresizingMask(kCALayerHeightSizable | kCALayerWidthSizable);

        let observer = Observer::new(&layer);
        // Observe changes to the root layer's bounds and scale factor, and apply them to our layer.
        //
        // The previous implementation updated the scale factor inside `resize`, but this works
        // poorly with transactions, and is generally inefficient. Instead, we update the scale
        // factor only when needed because the super layer's scale factor changed.
        //
        // Note that inherent in this is an explicit design decision: We control the `bounds` and
        // `contentsScale` of the layer directly, and instead let the `resize` call that the user
        // controls only be the size of the underlying buffer.
        //
        // SAFETY: Observer deregistered in `Drop` before the observer object is deallocated.
        unsafe {
            root_layer.addObserver_forKeyPath_options_context(
                &observer,
                ns_string!("contentsScale"),
                NSKeyValueObservingOptions::New | NSKeyValueObservingOptions::Initial,
                ptr::null_mut(),
            );
            root_layer.addObserver_forKeyPath_options_context(
                &observer,
                ns_string!("bounds"),
                NSKeyValueObservingOptions::New | NSKeyValueObservingOptions::Initial,
                ptr::null_mut(),
            );
        }

        // Set the content so that it is placed in the top-left corner if it does not have the same
        // size as the surface itself.
        //
        // TODO(madsmtm): Consider changing this to `kCAGravityResize` to stretch the content if
        // resized to something that doesn't fit, see #177.
        layer.setContentsGravity(unsafe { kCAGravityTopLeft });

        // Initialize color space here, to reduce work later on.
        // b-terminal(ADR-009): 화면 자신의 색 공간으로 넘기면 Core Animation이 프레임마다 색 맞춤 변환을 하지 않는다
        // (실험 E3: 큰 창 present 14ms → 0.1ms). 색은 맞추지 않고 그대로 나간다. 끄면 원래대로 기기 RGB
        // b-terminal(#97): 주 화면이 아니라 창이 있는 화면(못 구하면 주 화면)
        let display = display_of(&window_src).unwrap_or_else(|| objc2_core_graphics::CGMainDisplayID());
        let color_space = if crate::native_color_space() {
            objc2_core_graphics::CGDisplayCopyColorSpace(display)
        } else {
            CGColorSpace::new_device_rgb().unwrap()
        };

        // Grab initial width and height from the layer (whose properties have just been initialized
        // by the observer using `NSKeyValueObservingOptionInitial`).
        let size = layer.bounds().size;
        let scale_factor = layer.contentsScale();
        let width = (size.width * scale_factor) as usize;
        let height = (size.height * scale_factor) as usize;

        Ok(Self {
            layer: SendCALayer(layer),
            root_layer: SendCALayer(root_layer),
            observer,
            color_space,
            display,
            screen: 0,
            pool: Vec::new(),
            io_failed: false,
            current: 0,
            width,
            height,
            _display: PhantomData,
            window_handle: window_src,
        })
    }

    #[inline]
    fn window(&self) -> &W {
        &self.window_handle
    }

    fn resize(&mut self, width: NonZeroU32, height: NonZeroU32) -> Result<(), SoftBufferError> {
        self.width = width.get() as usize;
        self.height = height.get() as usize;
        Ok(())
    }

    fn buffer_mut(&mut self) -> Result<BufferImpl<'_, D, W>, SoftBufferError> {
        // b-terminal(#79): Core Animation이 다 쓰고 돌려준 버퍼를 다시 쓴다. 새로 잡으면 0으로 채운 쪽마다
        // 처음 쓸 때 쪽 부재가 나 큰 창에서 약 2ms가 든다(실험 E3·E4). 내용은 정해지지 않는다(age 0, 원래와 같음)
        let n = self.width * self.height;
        // b-terminal(#346): IOSurface 길은 프레임을 곧장 IOSurface에 옮기므로 버퍼를 잡지 않는다(`present_frame`).
        // 픽셀을 직접 쓰는 쪽(`pixels_mut`)이 있으면 그때 잡는다
        let reused = if crate::iosurface() {
            Some(Vec::new())
        } else if crate::reuse_buffers() {
            take_spare(n)
        } else {
            None
        };
        Ok(BufferImpl {
            buffer: util::PixelBuffer(reused.unwrap_or_else(|| vec![0; n])),
            imp: self,
        })
    }
}

impl<D: HasDisplayHandle, W: HasWindowHandle> CGImpl<D, W> {
    /// b-terminal(#97): 창이 다른 화면으로 옮겨 갔으면 그 화면의 색 공간으로 바꾼다(화면 색 공간을 쓸 때만).
    /// NSScreen 객체가 그대로면 번호를 묻지 않는다(deviceDescription은 프레임마다 묻기에 비싸다)
    fn refresh_color_space(&mut self) {
        if crate::native_color_space() {
            if let Some(screen) = screen_of(&self.window_handle) {
                let at = Retained::as_ptr(&screen) as usize;
                if at != self.screen {
                    self.screen = at;
                    if let Some(d) = screen_number(&screen).filter(|d| *d != self.display) {
                        self.color_space = objc2_core_graphics::CGDisplayCopyColorSpace(d);
                        self.display = d;
                    }
                }
            }
        }
    }

    /// b-terminal(#346): 프레임을 IOSurface에 줄마다 `bytesPerRow`를 지켜 옮기고 레이어 `contents`로 넣는다.
    /// 묶음에서 사용 중이 아닌 같은 크기·색 공간의 것을 고르고(없으면 새로 만든다), 잠그고 쓴 뒤 푼다
    fn present_iosurface(&mut self, frame: &[u32]) -> Result<(), SoftBufferError> {
        self.refresh_color_space();
        let native = crate::native_color_space();
        let space = iosurface_pool::color_space_key(native, self.display);
        let (width, height) = (self.width, self.height);

        // 크기나 색 공간이 바뀌었으면 묶음을 비운다
        let infos: Vec<SlotInfo> = self
            .pool
            .iter()
            .map(|(s, key)| SlotInfo {
                width: s.width(),
                height: s.height(),
                space: *key,
                in_use: false,
            })
            .collect();
        let mut keep = iosurface_pool::retain_mask(&infos, width, height, space).into_iter();
        self.pool.retain(|_| keep.next().unwrap_or(false));

        // 지금 화면에 올라간 것과 Core Animation이 쥔 것은 건드리지 않는다(찢어짐)
        let current = self.current;
        let infos: Vec<SlotInfo> = self
            .pool
            .iter()
            .map(|(s, key)| SlotInfo {
                width: s.width(),
                height: s.height(),
                space: *key,
                in_use: s.is_in_use() || (&**s as *const IOSurfaceRef as usize) == current,
            })
            .collect();
        let index = match iosurface_pool::pick_slot(&infos, width, height, space) {
            Pick::Reuse(i) => i,
            Pick::New => {
                let surface = self.create_iosurface(width, height, native).ok_or_else(|| {
                    SoftBufferError::PlatformError(Some("IOSurface를 만들지 못했다".to_string()), None)
                })?;
                let evict = iosurface_pool::evict_for_new(self.pool.len());
                self.pool.drain(..evict);
                self.pool.push((SendSurface(surface), space));
                self.pool.len() - 1
            }
        };
        let surface = self.pool[index].0 .0.clone();

        // SAFETY: 잠금(옵션 0 = 읽기·쓰기)을 잡은 동안만 기준 주소를 쓰고, 끝나면 바로 푼다. `alloc_size`가 그 메모리의 크기다.
        // 시드는 쓰지 않으므로 NULL
        let copied = unsafe {
            if surface.lock(IOSurfaceLockOptions(0), ptr::null_mut()) != 0 {
                return Err(SoftBufferError::PlatformError(
                    Some("IOSurface를 잠그지 못했다".to_string()),
                    None,
                ));
            }
            let dst = std::slice::from_raw_parts_mut(
                surface.base_address().as_ptr().cast::<u8>(),
                surface.alloc_size(),
            );
            let ok = iosurface_pool::copy_frame(
                dst,
                surface.bytes_per_row(),
                frame,
                width,
                height,
                // 불투명 창은 알파 바이트를 늘 0xff로 채운다(CGImage 길의 NoneSkipFirst가 알파를 무시하던 것과 같다).
                // 투명 창은 캔버스가 곱한 알파로 그린 그대로
                !crate::transparent(),
            );
            surface.unlock(IOSurfaceLockOptions(0), ptr::null_mut());
            ok
        };
        if !copied {
            return Err(SoftBufferError::PlatformError(
                Some("IOSurface에 프레임을 옮기지 못했다".to_string()),
                None,
            ));
        }
        self.current = (&*surface as *const IOSurfaceRef) as usize;

        // CGImage 길과 같다: 레이어 contents를 바꾸는 암묵 애니메이션(0.25초 페이드)을 끈다
        CATransaction::begin();
        CATransaction::setDisableActions(true);
        // SAFETY: IOSurface는 `contents`로 쓸 수 있는 클래스다(Core Animation이 받는다)
        unsafe { self.layer.setContents(Some(surface.as_ref())) };
        CATransaction::commit();
        Ok(())
    }

    /// b-terminal(#346): 32BGRA IOSurface를 만들고 색 공간을 붙인다. 색 공간은 화면 자신의 것(기본) 또는 sRGB(`window-colorspace = srgb`)
    fn create_iosurface(&self, width: usize, height: usize, native: bool) -> Option<CFRetained<IOSurfaceRef>> {
        let space = if native {
            self.color_space.property_list()
        } else {
            CGColorSpace::with_name(Some(unsafe { kCGColorSpaceSRGB })).and_then(|c| c.property_list())
        };
        new_iosurface(width, height, space.as_deref())
    }
}

/// b-terminal(#346): 32BGRA(`'BGRA'`, 원소 4바이트) IOSurface를 만들고 색 공간 속성을 붙인다(Ghostty의 IOSurface와 같은 꼴)
fn new_iosurface(width: usize, height: usize, space: Option<&CFType>) -> Option<CFRetained<IOSurfaceRef>> {
    let w = CFNumber::new_i64(width as i64);
    let h = CFNumber::new_i64(height as i64);
    let bpe = CFNumber::new_i32(4);
    let format = CFNumber::new_i32(0x4247_5241); // 'BGRA'
    // SAFETY: 키는 CFString 상수, 값은 CFNumber라 CFDictionary의 CFType 콜백이 맞다
    let (keys, values): ([&CFString; 4], [&CFType; 4]) = unsafe {
        (
            [kIOSurfaceWidth, kIOSurfaceHeight, kIOSurfaceBytesPerElement, kIOSurfacePixelFormat],
            [w.as_ref(), h.as_ref(), bpe.as_ref(), format.as_ref()],
        )
    };
    let props = CFDictionary::<CFString, CFType>::from_slices(&keys, &values);
    // SAFETY: 위 딕셔너리는 IOSurface가 받는 키와 CFNumber 값만 있다
    let surface = unsafe { IOSurfaceRef::new(props.as_ref()) }?;
    if let Some(plist) = space {
        // SAFETY: 색 공간 속성 목록은 `kIOSurfaceColorSpace`가 받는 CFPropertyList다
        unsafe { surface.set_value(kIOSurfaceColorSpace, plist) };
    }
    Some(surface)
}

#[cfg(test)]
mod iosurface_tests {
    use super::*;

    /// 창 없이 진짜 IOSurface를 만들어 잠그고, 줄 간격(`bytesPerRow`)을 지켜 옮긴 바이트를 읽어 본다
    #[test]
    fn real_iosurface_copy_respects_bytes_per_row() {
        let (w, h) = (37usize, 5usize); // 폭×4 = 148, 시스템이 줄 간격을 더 키울 수 있는 폭
        let srgb = CGColorSpace::with_name(Some(unsafe { kCGColorSpaceSRGB })).unwrap();
        let plist = srgb.property_list();
        let surface = new_iosurface(w, h, plist.as_deref()).expect("IOSurface 만들기");
        assert_eq!((surface.width(), surface.height()), (w, h));
        assert_eq!(surface.bytes_per_element(), 4);
        assert_eq!(surface.pixel_format(), 0x4247_5241);
        assert!(surface.value(unsafe { kIOSurfaceColorSpace }).is_some(), "색 공간이 붙는다");
        let bpr = surface.bytes_per_row();
        assert!(bpr >= w * 4);
        eprintln!("bytesPerRow = {bpr} (폭×4 = {})", w * 4);
        assert!(!surface.is_in_use());

        let frame: Vec<u32> = (0..(w * h) as u32).map(|i| 0x0010_0000 + i).collect();
        unsafe {
            assert_eq!(surface.lock(IOSurfaceLockOptions(0), ptr::null_mut()), 0);
            let dst = std::slice::from_raw_parts_mut(surface.base_address().as_ptr().cast::<u8>(), surface.alloc_size());
            assert!(iosurface_pool::copy_frame(dst, bpr, &frame, w, h, true));
            surface.unlock(IOSurfaceLockOptions(0), ptr::null_mut());
            assert_eq!(surface.lock(IOSurfaceLockOptions::ReadOnly, ptr::null_mut()), 0);
            let src = std::slice::from_raw_parts(surface.base_address().as_ptr().cast::<u8>(), surface.alloc_size());
            for y in 0..h {
                for x in 0..w {
                    let o = y * bpr + x * 4;
                    let px = u32::from_le_bytes([src[o], src[o + 1], src[o + 2], src[o + 3]]);
                    assert_eq!(px, frame[y * w + x] | 0xff00_0000, "({x},{y})");
                }
            }
            surface.unlock(IOSurfaceLockOptions::ReadOnly, ptr::null_mut());
        }
    }
}

/// b-terminal(#97): 창이 있는 화면(NSScreen). 창이 화면에 없으면 없다.
/// AppKit 뷰만(UIKit은 없다). 메인 스레드에서 부른다(그림판을 만들고 올리는 곳)
fn screen_of<W: HasWindowHandle>(window: &W) -> Option<Retained<NSObject>> {
    let RawWindowHandle::AppKit(handle) = window.window_handle().ok()?.as_raw() else {
        return None;
    };
    // SAFETY: `WindowHandle`이 살아 있는 NSView를 가리킨다고 보장한다. `-[NSView window]`와 `-[NSWindow screen]`은
    // 없을 수 있는 객체를 돌려준다(Option)
    unsafe {
        let view: &NSObject = handle.ns_view.cast().as_ref();
        let window: Option<Retained<NSObject>> = msg_send![view, window];
        msg_send![&*window?, screen]
    }
}

/// b-terminal(#97): 화면의 번호(`NSScreenNumber`, CGDirectDisplayID)
fn screen_number(screen: &NSObject) -> Option<u32> {
    // SAFETY: `-[NSScreen deviceDescription]`은 NSDictionary<NSString, id>를 돌려준다
    let desc: Retained<NSDictionary<NSString, AnyObject>> = unsafe { msg_send![screen, deviceDescription] };
    let number = desc.objectForKey(ns_string!("NSScreenNumber"))?;
    Some(number.downcast::<NSNumber>().ok()?.unsignedIntValue())
}

fn display_of<W: HasWindowHandle>(window: &W) -> Option<u32> {
    screen_number(&*screen_of(window)?)
}

#[derive(Debug)]
pub struct BufferImpl<'a, D, W> {
    imp: &'a mut CGImpl<D, W>,
    buffer: util::PixelBuffer,
}

impl<D: HasDisplayHandle, W: HasWindowHandle> BufferInterface for BufferImpl<'_, D, W> {
    fn width(&self) -> NonZeroU32 {
        NonZeroU32::new(self.imp.width as u32).unwrap()
    }

    fn height(&self) -> NonZeroU32 {
        NonZeroU32::new(self.imp.height as u32).unwrap()
    }

    #[inline]
    fn pixels(&self) -> &[u32] {
        &self.buffer
    }

    #[inline]
    fn pixels_mut(&mut self) -> &mut [u32] {
        // b-terminal(#346): IOSurface 길은 버퍼를 비워 두었다. 픽셀을 직접 쓰려 하면 그때 잡는다
        if self.buffer.is_empty() {
            self.buffer.0.resize(self.imp.width * self.imp.height, 0);
        }
        &mut self.buffer
    }

    fn age(&self) -> u8 {
        0
    }

    fn present(mut self) -> Result<(), SoftBufferError> {
        // b-terminal(#346): IOSurface 길(기본). 비어 있는 버퍼는 0으로 채운 프레임으로 본다(CGImage 길과 같다)
        if crate::iosurface() && !self.imp.io_failed {
            let n = self.imp.width * self.imp.height;
            let mut buffer = self.buffer.0;
            if buffer.len() != n {
                buffer.clear();
                buffer.resize(n, 0);
            }
            if self.imp.present_iosurface(&buffer).is_ok() {
                return Ok(());
            }
            // 실패: 이 그림판은 이제 CGImage 길로 간다. 같은 프레임을 그 길로 올린다
            self.imp.io_failed = true;
            self.buffer = util::PixelBuffer(buffer);
        }

        unsafe extern "C-unwind" fn release(
            _info: *mut c_void,
            data: NonNull<c_void>,
            size: usize,
        ) {
            let data = data.cast::<u32>();
            let slice = slice_from_raw_parts_mut(data.as_ptr(), size / size_of::<u32>());
            // SAFETY: This is the same slice that we passed to `Box::into_raw` below.
            let boxed = unsafe { Box::from_raw(slice) };
            // b-terminal(#79): 버리지 않고 다시 쓸 자리에 둔다(Core Animation이 다 쓴 뒤 부른다)
            put_spare(boxed.into_vec());
        }

        let data_provider = {
            let len = self.buffer.len() * size_of::<u32>();
            let buffer: *mut [u32] = Box::into_raw(self.buffer.0.into_boxed_slice());
            // Convert slice pointer to thin pointer.
            let data_ptr = buffer.cast::<c_void>();

            // SAFETY: The data pointer and length are valid.
            // The info pointer can safely be NULL, we don't use it in the `release` callback.
            unsafe {
                CGDataProvider::with_data(ptr::null_mut(), data_ptr, len, Some(release)).unwrap()
            }
        };

        // `CGBitmapInfo` consists of a combination of `CGImageAlphaInfo`, `CGImageComponentInfo`
        // `CGImageByteOrderInfo` and `CGImagePixelFormatInfo` (see e.g. `CGBitmapInfoMake`).
        //
        // TODO: Use `CGBitmapInfo::new` once the next version of objc2-core-graphics is released.
        let bitmap_info = CGBitmapInfo(
            // b-terminal(#78): 투명 창이면 곱한 알파를 넘긴다(바탕 칸으로 창 뒤가 비침)
            (if crate::transparent() {
                CGImageAlphaInfo::PremultipliedFirst.0
            } else {
                CGImageAlphaInfo::NoneSkipFirst.0
            })
                | CGImageComponentInfo::Integer.0
                | CGImageByteOrderInfo::Order32Little.0
                | CGImagePixelFormatInfo::Packed.0,
        );

        self.imp.refresh_color_space();

        let image = unsafe {
            CGImage::new(
                self.imp.width,
                self.imp.height,
                8,
                32,
                self.imp.width * 4,
                Some(&self.imp.color_space),
                bitmap_info,
                Some(&data_provider),
                ptr::null(),
                false,
                CGColorRenderingIntent::RenderingIntentDefault,
            )
        }
        .unwrap();

        // The CALayer has a default action associated with a change in the layer contents, causing
        // a quarter second fade transition to happen every time a new buffer is applied. This can
        // be avoided by wrapping the operation in a transaction and disabling all actions.
        CATransaction::begin();
        CATransaction::setDisableActions(true);

        // SAFETY: The contents is `CGImage`, which is a valid class for `contents`.
        unsafe { self.imp.layer.setContents(Some(image.as_ref())) };

        CATransaction::commit();
        Ok(())
    }

    fn present_with_damage(self, _damage: &[Rect]) -> Result<(), SoftBufferError> {
        self.present()
    }

    // b-terminal(#346): 프레임을 IOSurface로 한 번만 옮긴다(버퍼를 거치지 않는다)
    fn present_frame(mut self, frame: &[u32]) -> Result<(), SoftBufferError> {
        if crate::iosurface() && !self.imp.io_failed && frame.len() == self.imp.width * self.imp.height {
            if self.imp.present_iosurface(frame).is_ok() {
                return Ok(());
            }
            self.imp.io_failed = true;
        }
        self.pixels_mut().copy_from_slice(frame);
        self.present()
    }
}

/// b-terminal(#79): 다시 쓸 버퍼들. release는 Core Animation의 다른 스레드에서 불릴 수 있어 잠근다
static SPARE: std::sync::Mutex<Vec<Vec<u32>>> = std::sync::Mutex::new(Vec::new());
/// 두는 버퍼 수 상한(창 크기가 바뀌면 옛 크기의 것은 버린다)
const MAX_SPARE: usize = 3;

fn take_spare(len: usize) -> Option<Vec<u32>> {
    let mut spare = SPARE.lock().ok()?;
    let i = spare.iter().position(|b| b.len() == len)?;
    Some(spare.swap_remove(i))
}

fn put_spare(buf: Vec<u32>) {
    if let Ok(mut spare) = SPARE.lock() {
        // 다른 크기(창 크기가 바뀐 뒤)의 것은 버리고, 상한을 넘으면 버린다
        spare.retain(|b| b.len() == buf.len());
        if spare.len() < MAX_SPARE {
            spare.push(buf);
        }
    }
}

/// b-terminal(#346): IOSurface는 프로세스 사이에서 공유하려고 만든 스레드 안전한 객체다(Apple 문서). `Surface`가 `Send`여야 해서 감싼다
struct SendSurface(CFRetained<IOSurfaceRef>);

impl std::fmt::Debug for SendSurface {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SendSurface").finish_non_exhaustive()
    }
}

// SAFETY: IOSurfaceRef는 스레드 사이에서 쓰도록 만든 객체다(참조 세기·잠금이 안전하다).
unsafe impl Send for SendSurface {}
// SAFETY: 위와 같다.
unsafe impl Sync for SendSurface {}

impl Deref for SendSurface {
    type Target = IOSurfaceRef;
    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

#[derive(Debug)]
struct SendCALayer(Retained<CALayer>);

// SAFETY: CALayer is dubiously thread safe, like most things in Core Animation.
// But since we make sure to do our changes within a CATransaction, it is
// _probably_ fine for us to use CALayer from different threads.
//
// See also:
// https://developer.apple.com/documentation/quartzcore/catransaction/1448267-lock?language=objc
// https://stackoverflow.com/questions/76250226/how-to-render-content-of-calayer-on-a-background-thread
unsafe impl Send for SendCALayer {}
// SAFETY: Same as above.
unsafe impl Sync for SendCALayer {}

impl Deref for SendCALayer {
    type Target = CALayer;
    fn deref(&self) -> &Self::Target {
        &self.0
    }
}
