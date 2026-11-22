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

/// 포인터를 정하는 데 쓰는 상태(#250)
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct PointerInput {
    /// Cmd(Linux는 Ctrl)를 누른 채 링크 위에 있다
    pub over_link: bool,
    /// 마우스 아래 칸이 프로그램이 OSC 22로 정한 모양. 정한 것이 없으면 `None`
    pub program: Option<MouseShape>,
    /// Shift를 누르고 있다
    pub shift: bool,
    /// 칸의 마우스 보고 모드(X10·버튼·움직임)가 켜져 있다. Shift 잡기나 `mouse-reporting`, 읽기 전용과 상관없이
    /// 모드 자체만 본다(Ghostty `terminal.flags.mouse_event != .none`). 정한 모양이 없을 때의 기본과, 십자·I자에
    /// Shift가 필요한지가 이것으로 갈린다
    pub mouse_mode: bool,
    /// 사각 고르기 수식 키를 누르고 있다(macOS Option, 그 밖에는 Ctrl+Alt. `mouse::is_rectangle_select`)
    pub rectangle: bool,
    /// 마우스 포인터를 숨겼다(입력 중 숨김)
    pub hidden: bool,
    /// 칸 끌기 손잡이와 마우스의 관계(`drag-handle`)
    pub handle: HandleCursor,
    /// 분할 경계선과 마우스의 관계. 경계선 위나 끄는 중이면 그 방향의 크기 조절 모양
    pub divider: DividerCursor,
}

/// 분할 경계선 위의 포인터 상태. 좌우로 나란한 칸 사이의 선은 `Col`(좌우 화살표), 위아래 칸 사이의 선은 `Row`
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum DividerCursor {
    #[default]
    None,
    Col,
    Row,
}

/// 칸 끌기 손잡이 위의 포인터 상태
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum HandleCursor {
    /// 손잡이와 상관없다
    #[default]
    None,
    /// 잡을 수 있는 손잡이 위
    Over,
    /// 칸을 끄는 중
    Dragging,
}

/// 지금 보일 포인터. Ghostty(surface_mouse.zig)처럼 링크 위의 손가락이 가장 앞서고, 그다음 사각 고르기 수식 키를 누른 채
/// 마우스가 고르기를 하게 될 때의 십자(`crosshair`), 그다음 Shift를 누른 채 고르기를 하게 될 때의 I자, 아니면 프로그램이 정한
/// 모양, 정한 것이 없으면(OSC 22를 받은 적 없거나 되돌렸으면) 마우스 보고 모드가 꺼져 있을 때 I자(`text`), 켜져 있을 때
/// 화살표를 쓴다(Ghostty `Terminal.mouse_shape` 기본값 `.text`, `stream_handler.zig`가 모드를 켜면 `.default`·끄면 `.text`,
/// `OSC 22` 지움은 모드에 따라 `.default`·`.text`). 마우스를 숨겼으면 키가 모양을 바꾸지 않는다.
/// 마우스 보고 모드가 켜진 칸에서는 Shift를 누를 때만 십자(Shift+사각 수식 키)·I자가 된다. Shift 잡기 설정은 보지 않는다
/// (Ghostty 1.4.0 `surface_mouse.zig keyToMouseShape`의 `isMouseModeOverrideState`가 `mods.shift`만 본다, #725)
pub fn decide(input: PointerInput) -> CursorIcon {
    // 경계선 위(또는 끄는 중)는 칸의 글이 아니라 창의 것이다: 칸을 끄는 중이 아니면 모든 것에 앞선다
    if input.handle != HandleCursor::Dragging {
        match input.divider {
            DividerCursor::Col => return CursorIcon::ColResize,
            DividerCursor::Row => return CursorIcon::RowResize,
            DividerCursor::None => {}
        }
    }
    // 칸을 끌어 옮기는 중이나 손잡이 위에서는 잡는 손이 모든 것에 앞선다(손잡이 위는 칸의 글이 아니다)
    match input.handle {
        HandleCursor::Dragging => return CursorIcon::Grabbing,
        HandleCursor::Over => return CursorIcon::Grab,
        HandleCursor::None => {}
    }
    if input.over_link {
        return CursorIcon::Pointer;
    }
    // 마우스 모드가 켜졌으면 Shift가 있어야 고르기 모양을 보인다(isMouseModeOverrideState)
    let selecting = !input.hidden && (!input.mouse_mode || input.shift);
    if selecting && input.rectangle {
        return CursorIcon::Crosshair;
    }
    if selecting && input.shift {
        return CursorIcon::Text;
    }
    let base = if input.mouse_mode {
        CursorIcon::Default
    } else {
        CursorIcon::Text
    };
    input.program.map_or(base, icon_for)
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

    fn input() -> PointerInput {
        PointerInput::default()
    }

    #[test]
    fn drag_handle_shows_a_grabbing_hand_over_everything() {
        let busy = PointerInput {
            over_link: true,
            program: Some(MouseShape::Crosshair),
            shift: true,
            ..input()
        };
        assert_eq!(
            decide(PointerInput {
                handle: HandleCursor::Over,
                ..busy
            }),
            CursorIcon::Grab
        );
        assert_eq!(
            decide(PointerInput {
                handle: HandleCursor::Dragging,
                ..busy
            }),
            CursorIcon::Grabbing
        );
        assert_eq!(
            decide(PointerInput {
                handle: HandleCursor::None,
                ..busy
            }),
            CursorIcon::Pointer
        );
    }

    #[test]
    fn divider_shows_a_resize_pointer_over_the_program_shape_and_links() {
        let base = PointerInput {
            over_link: true,
            program: Some(MouseShape::Crosshair),
            shift: true,
            hidden: true,
            ..input()
        };
        assert_eq!(
            decide(PointerInput {
                divider: DividerCursor::Col,
                ..base
            }),
            CursorIcon::ColResize
        );
        assert_eq!(
            decide(PointerInput {
                divider: DividerCursor::Row,
                handle: HandleCursor::Over,
                ..base
            }),
            CursorIcon::RowResize
        );
        // 칸을 끄는 중이면 잡는 손이 앞선다
        assert_eq!(
            decide(PointerInput {
                divider: DividerCursor::Col,
                handle: HandleCursor::Dragging,
                ..base
            }),
            CursorIcon::Grabbing
        );
        assert_eq!(decide(base), CursorIcon::Pointer);
    }

    #[test]
    fn link_hover_beats_everything() {
        let i = PointerInput {
            over_link: true,
            program: Some(MouseShape::Crosshair),
            shift: true,
            ..input()
        };
        assert_eq!(decide(i), CursorIcon::Pointer);
        assert_eq!(
            decide(PointerInput {
                over_link: true,
                ..input()
            }),
            CursorIcon::Pointer
        );
    }

    #[test]
    fn program_shape_applies_off_links_and_default_without_one() {
        let with = |p| PointerInput { program: p, ..input() };
        assert_eq!(decide(with(Some(MouseShape::Crosshair))), CursorIcon::Crosshair);
        assert_eq!(decide(with(Some(MouseShape::Default))), CursorIcon::Default);
        // 정한 것이 없으면 마우스 보고가 꺼진 칸은 I자(Ghostty 기본)
        assert_eq!(decide(with(None)), CursorIcon::Text);
    }

    #[test]
    fn mouse_reporting_mode_makes_the_default_an_arrow() {
        // 마우스 보고 모드가 켜져 있으면 정한 모양이 없을 때 화살표다(Ghostty는 모드를 켜면 default, 끄면 text)
        let on = PointerInput {
            mouse_mode: true,
            ..input()
        };
        assert_eq!(decide(on), CursorIcon::Default);
        // 프로그램이 정한 모양은 모드와 상관없이 그대로
        assert_eq!(
            decide(PointerInput {
                program: Some(MouseShape::Crosshair),
                ..on
            }),
            CursorIcon::Crosshair
        );
        assert_eq!(
            decide(PointerInput {
                program: Some(MouseShape::Text),
                ..on
            }),
            CursorIcon::Text
        );
        // Shift를 누르면 I자
        assert_eq!(decide(PointerInput { shift: true, ..on }), CursorIcon::Text);
        // 링크 손가락과 칸 끌기 손잡이는 그대로 앞선다
        assert_eq!(decide(PointerInput { over_link: true, ..on }), CursorIcon::Pointer);
        assert_eq!(
            decide(PointerInput {
                handle: HandleCursor::Over,
                ..on
            }),
            CursorIcon::Grab
        );
    }

    #[test]
    fn rectangle_modifiers_show_a_crosshair_when_the_mouse_selects() {
        // 사각 고르기 수식 키를 누르면 십자다. 프로그램이 정한 모양과 Shift I자보다 앞선다(Ghostty keyToMouseShape)
        let i = PointerInput {
            rectangle: true,
            program: Some(MouseShape::Progress),
            ..input()
        };
        assert_eq!(decide(i), CursorIcon::Crosshair);
        assert_eq!(
            decide(PointerInput { shift: true, ..i }),
            CursorIcon::Crosshair,
            "Shift I자보다 앞"
        );
        assert_eq!(
            decide(PointerInput { rectangle: false, ..i }),
            CursorIcon::Progress,
            "떼면 프로그램 모양"
        );
        // 링크 위 손가락이 앞서고, 칸 끌기 손잡이가 그보다 앞선다
        assert_eq!(decide(PointerInput { over_link: true, ..i }), CursorIcon::Pointer);
        assert_eq!(
            decide(PointerInput {
                handle: HandleCursor::Over,
                ..i
            }),
            CursorIcon::Grab
        );
        // 마우스를 숨겼으면 모양을 바꾸지 않는다
        assert_eq!(decide(PointerInput { hidden: true, ..i }), CursorIcon::Progress);
        // 마우스 모드가 켜진 칸에서는 Shift 없이 바꾸지 않는다
        assert_eq!(decide(PointerInput { mouse_mode: true, ..i }), CursorIcon::Progress);
    }

    #[test]
    fn shift_shows_the_text_beam_over_the_program_shape_when_the_mouse_selects() {
        // Shift는 I자다(마우스 모드가 켜진 칸에서도). 기본 화살표 위에서도 그렇다(Ghostty와 같다)
        let i = PointerInput {
            shift: true,
            program: Some(MouseShape::Progress),
            ..input()
        };
        assert_eq!(decide(i), CursorIcon::Text);
        assert_eq!(decide(PointerInput { shift: true, ..input() }), CursorIcon::Text);
        // 떼면 프로그램 모양으로 돌아간다
        assert_eq!(decide(PointerInput { shift: false, ..i }), CursorIcon::Progress);
    }

    /// Ghostty 1.4.0 `surface_mouse.zig keyToMouseShape`(#725): 프로그램이 마우스 모드를 켰으면 Shift만 본다.
    /// Shift 잡기 설정도, 칸이 실제로 보고하는지(읽기 전용·`mouse-reporting`)도 보지 않는다
    #[test]
    fn mouse_mode_overrides_follow_shift_only_like_ghostty() {
        let vim = PointerInput {
            mouse_mode: true,
            program: Some(MouseShape::Pointer),
            ..input()
        };
        // Shift 잡기로 Shift 클릭도 프로그램이 받는 때도
        assert_eq!(
            decide(PointerInput { shift: true, ..vim }),
            CursorIcon::Text,
            "Shift는 I자"
        );
        assert_eq!(
            decide(PointerInput {
                shift: true,
                rectangle: true,
                ..vim
            }),
            CursorIcon::Crosshair,
            "Shift+Option은 십자"
        );
        // 모드는 켜졌지만 보고하지 않는 칸(읽기 전용·mouse-reporting 끔)에서 Option만
        assert_eq!(
            decide(PointerInput { rectangle: true, ..vim }),
            CursorIcon::Pointer,
            "Shift 없는 Option은 모양을 바꾸지 않는다"
        );
    }

    #[test]
    fn hidden_mouse_is_not_changed_by_shift() {
        let i = PointerInput {
            shift: true,
            hidden: true,
            program: Some(MouseShape::Wait),
            ..input()
        };
        assert_eq!(decide(i), CursorIcon::Wait);
    }
}
