//! 앱 아이콘(Ghostty `macos-icon = custom`·`macos-custom-icon`, #196).
//!
//! `NSApplication.setApplicationIconImage`는 objc2에서 `unsafe fn`이라(작업 공간은 unsafe 금지) 쓰지 않는다.
//! 대신 Dock 아이콘(`NSDockTile`)에 그림 뷰를 올린다. 그래서 바뀌는 곳은 Dock 아이콘뿐이고
//! Cmd-Tab 전환기·Finder의 아이콘은 앱 꾸러미의 것 그대로다

use std::path::Path;

use bt_core::config::MacosIcon;

#[cfg_attr(not(target_os = "macos"), allow(dead_code))]
/// 그림을 바꿔야 하는 파일. `custom`이고 경로가 있을 때만 있다(Ghostty처럼 `custom`이 아니면 `macos-custom-icon`은 쓰지 않는다)
pub fn custom_icon_file(icon: MacosIcon, path: Option<&Path>) -> Option<&Path> {
    if icon == MacosIcon::Custom { path } else { None }
}

#[cfg(target_os = "macos")]
thread_local! {
    /// 지금 Dock에 올린 사용자 그림(없으면 앱 꾸러미의 아이콘). 진행 막대(`dock`)가 같은 그림 위에 막대를 얹는다
    static CUSTOM: std::cell::RefCell<Option<objc2::rc::Retained<objc2_app_kit::NSImage>>> =
        const { std::cell::RefCell::new(None) };
}

/// 지금 Dock에 올린 사용자 그림. `custom`이 아니면 없다
#[cfg(target_os = "macos")]
pub fn custom_image() -> Option<objc2::rc::Retained<objc2_app_kit::NSImage>> {
    CUSTOM.with(|c| c.borrow().clone())
}

/// 설정대로 Dock 아이콘을 정한다. 바꿨던 것을 되돌릴 때는 앱 꾸러미의 아이콘으로 돌아간다. macOS 밖에서는 하는 일이 없다
#[cfg(target_os = "macos")]
pub fn apply(icon: MacosIcon, path: Option<&Path>) {
    use objc2::AnyThread;
    use objc2_app_kit::{NSApplication, NSImage, NSImageScaling, NSImageView};
    use std::sync::atomic::{AtomicBool, Ordering};
    // 우리가 올린 그림이 있는가(없는데 비우기를 되풀이하지 않는다)
    static SET: AtomicBool = AtomicBool::new(false);
    let Some(mtm) = objc2::MainThreadMarker::new() else {
        return;
    };
    let tile = NSApplication::sharedApplication(mtm).dockTile();
    match custom_icon_file(icon, path) {
        Some(file) => {
            let Some(name) = file.to_str().map(objc2_foundation::NSString::from_str) else {
                log::warn!("macos-custom-icon 경로를 읽을 수 없습니다: {}", file.display());
                return;
            };
            let Some(image) = NSImage::initWithContentsOfFile(NSImage::alloc(), &name) else {
                log::warn!(
                    "macos-custom-icon을 읽지 못했습니다(그림 파일이 아니거나 없습니다): {}",
                    file.display()
                );
                return;
            };
            let view = NSImageView::imageViewWithImage(&image, mtm);
            view.setImageScaling(NSImageScaling::ScaleProportionallyUpOrDown);
            tile.setContentView(Some(&view));
            tile.display();
            CUSTOM.with(|c| *c.borrow_mut() = Some(image));
            SET.store(true, Ordering::Relaxed);
        }
        None => {
            CUSTOM.with(|c| *c.borrow_mut() = None);
            if SET.swap(false, Ordering::Relaxed) {
                tile.setContentView(None);
                tile.display();
            }
        }
    }
}

#[cfg(not(target_os = "macos"))]
pub fn apply(_icon: MacosIcon, _path: Option<&Path>) {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_custom_icon_with_a_path_changes_the_image() {
        let p = Path::new("/tmp/i.icns");
        assert_eq!(custom_icon_file(MacosIcon::Custom, Some(p)), Some(p));
        assert_eq!(custom_icon_file(MacosIcon::Custom, None), None);
        for icon in [MacosIcon::Official, MacosIcon::Builtin, MacosIcon::CustomStyle] {
            assert_eq!(custom_icon_file(icon, Some(p)), None, "{icon:?}");
        }
    }
}
