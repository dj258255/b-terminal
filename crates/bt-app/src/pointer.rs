//! 창 마우스 포인터 모양을 정한다(#239). 프로그램이 OSC 22로 정한 모양(칸마다)과, 링크 위에서 b-terminal이 고르는 손가락 모양 중
//! 무엇을 보일지 판단한다. 순수 함수라 창 없이 시험한다.

use bt_term::MouseShape;
use winit::window::CursorIcon;

/// OSC 22 모양 → winit 포인터. winit(cursor-icon)이 W3C 34가지를 모두 가지므로 빠지는 이름이 없다
pub fn icon_for(shape: MouseShape) -> CursorIcon {
    match shape {
        MouseShape::Default => CursorIcon::Default,
        MouseShape::ContextMenu => CursorIcon::ContextMenu,
        MouseShape::Help => CursorIcon::Help,
        MouseShape::Pointer => CursorIcon::Pointer,
        MouseShape::Progress => CursorIcon::Progress,
        MouseShape::Wait => CursorIcon::Wait,
        MouseShape::Cell => CursorIcon::Cell,
        MouseShape::Crosshair => CursorIcon::Crosshair,
        MouseShape::Text => CursorIcon::Text,
        MouseShape::VerticalText => CursorIcon::VerticalText,
        MouseShape::Alias => CursorIcon::Alias,
        MouseShape::Copy => CursorIcon::Copy,
        MouseShape::Move => CursorIcon::Move,
        MouseShape::NoDrop => CursorIcon::NoDrop,
        MouseShape::NotAllowed => CursorIcon::NotAllowed,
        MouseShape::Grab => CursorIcon::Grab,
        MouseShape::Grabbing => CursorIcon::Grabbing,
        MouseShape::AllScroll => CursorIcon::AllScroll,
        MouseShape::ColResize => CursorIcon::ColResize,
        MouseShape::RowResize => CursorIcon::RowResize,
        MouseShape::NResize => CursorIcon::NResize,
        MouseShape::EResize => CursorIcon::EResize,
        MouseShape::SResize => CursorIcon::SResize,
        MouseShape::WResize => CursorIcon::WResize,
        MouseShape::NeResize => CursorIcon::NeResize,
        MouseShape::NwResize => CursorIcon::NwResize,
        MouseShape::SeResize => CursorIcon::SeResize,
        MouseShape::SwResize => CursorIcon::SwResize,
        MouseShape::EwResize => CursorIcon::EwResize,
        MouseShape::NsResize => CursorIcon::NsResize,
        MouseShape::NeswResize => CursorIcon::NeswResize,
        MouseShape::NwseResize => CursorIcon::NwseResize,
        MouseShape::ZoomIn => CursorIcon::ZoomIn,
        MouseShape::ZoomOut => CursorIcon::ZoomOut,
    }
}

/// 지금 보일 포인터. Ghostty처럼 링크 위의 손가락이 프로그램이 정한 모양보다 앞서고, 링크가 아니면 프로그램이 정한 모양을,
/// 정한 것이 없으면(OSC 22를 받은 적 없거나 되돌렸으면) 기본 화살표를 쓴다
pub fn decide(over_link: bool, program: Option<MouseShape>) -> CursorIcon {
    if over_link {
        return CursorIcon::Pointer;
    }
    program.map_or(CursorIcon::Default, icon_for)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn names_map_to_the_matching_winit_icons() {
        assert_eq!(icon_for(MouseShape::Pointer), CursorIcon::Pointer);
        assert_eq!(icon_for(MouseShape::Text), CursorIcon::Text);
        assert_eq!(icon_for(MouseShape::Crosshair), CursorIcon::Crosshair);
        assert_eq!(icon_for(MouseShape::NwseResize), CursorIcon::NwseResize);
        assert_eq!(icon_for(MouseShape::ZoomOut), CursorIcon::ZoomOut);
        assert_eq!(icon_for(MouseShape::Default), CursorIcon::Default);
    }

    #[test]
    fn no_shape_is_dropped_for_the_default_icon() {
        // 기본 모양이 아닌 모든 모양은 기본 화살표로 떨어지지 않는다(대응 표에서 빠진 것을 찾는다)
        for code in 1..=34u8 {
            let s = MouseShape::from_code(code).unwrap();
            if s != MouseShape::Default {
                assert_ne!(icon_for(s), CursorIcon::Default, "{s:?}");
            }
        }
    }

    #[test]
    fn link_hover_beats_the_program_shape() {
        assert_eq!(decide(true, Some(MouseShape::Crosshair)), CursorIcon::Pointer);
        assert_eq!(decide(true, None), CursorIcon::Pointer);
    }

    #[test]
    fn program_shape_applies_off_links_and_default_without_one() {
        assert_eq!(decide(false, Some(MouseShape::Crosshair)), CursorIcon::Crosshair);
        assert_eq!(decide(false, Some(MouseShape::Default)), CursorIcon::Default);
        assert_eq!(decide(false, None), CursorIcon::Default);
    }
}
