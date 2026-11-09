//! OSC 22로 프로그램이 정하는 마우스 포인터 모양(#239). 이름 표는 Ghostty의 `src/terminal/mouse.zig`와 같다
//! (W3C CSS cursor 이름과 xterm·foot 별칭). 그림은 그리지 않고 이름만 해석한다. 창 포인터로 옮기는 일은 앱의 몫이다.

/// Ghostty `MouseShape`와 같은 34가지. `Default`는 "프로그램이 기본 모양을 골랐다"는 뜻이다
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum MouseShape {
    Default,
    ContextMenu,
    Help,
    Pointer,
    Progress,
    Wait,
    Cell,
    Crosshair,
    Text,
    VerticalText,
    Alias,
    Copy,
    Move,
    NoDrop,
    NotAllowed,
    Grab,
    Grabbing,
    AllScroll,
    ColResize,
    RowResize,
    NResize,
    EResize,
    SResize,
    WResize,
    NeResize,
    NwResize,
    SeResize,
    SwResize,
    EwResize,
    NsResize,
    NeswResize,
    NwseResize,
    ZoomIn,
    ZoomOut,
}

/// 이름 표(Ghostty `string_map`): W3C 이름, 그다음 xterm·foot 별칭
const NAMES: &[(&str, MouseShape)] = &[
    ("default", MouseShape::Default),
    ("context-menu", MouseShape::ContextMenu),
    ("help", MouseShape::Help),
    ("pointer", MouseShape::Pointer),
    ("progress", MouseShape::Progress),
    ("wait", MouseShape::Wait),
    ("cell", MouseShape::Cell),
    ("crosshair", MouseShape::Crosshair),
    ("text", MouseShape::Text),
    ("vertical-text", MouseShape::VerticalText),
    ("alias", MouseShape::Alias),
    ("copy", MouseShape::Copy),
    ("move", MouseShape::Move),
    ("no-drop", MouseShape::NoDrop),
    ("not-allowed", MouseShape::NotAllowed),
    ("grab", MouseShape::Grab),
    ("grabbing", MouseShape::Grabbing),
    ("all-scroll", MouseShape::AllScroll),
    ("col-resize", MouseShape::ColResize),
    ("row-resize", MouseShape::RowResize),
    ("n-resize", MouseShape::NResize),
    ("e-resize", MouseShape::EResize),
    ("s-resize", MouseShape::SResize),
    ("w-resize", MouseShape::WResize),
    ("ne-resize", MouseShape::NeResize),
    ("nw-resize", MouseShape::NwResize),
    ("se-resize", MouseShape::SeResize),
    ("sw-resize", MouseShape::SwResize),
    ("ew-resize", MouseShape::EwResize),
    ("ns-resize", MouseShape::NsResize),
    ("nesw-resize", MouseShape::NeswResize),
    ("nwse-resize", MouseShape::NwseResize),
    ("zoom-in", MouseShape::ZoomIn),
    ("zoom-out", MouseShape::ZoomOut),
    // xterm/foot
    ("left_ptr", MouseShape::Default),
    ("question_arrow", MouseShape::Help),
    ("hand", MouseShape::Pointer),
    ("left_ptr_watch", MouseShape::Progress),
    ("watch", MouseShape::Wait),
    ("cross", MouseShape::Crosshair),
    ("xterm", MouseShape::Text),
    ("dnd-link", MouseShape::Alias),
    ("dnd-copy", MouseShape::Copy),
    ("dnd-move", MouseShape::Move),
    ("dnd-no-drop", MouseShape::NoDrop),
    ("crossed_circle", MouseShape::NotAllowed),
    ("hand1", MouseShape::Grab),
    ("right_side", MouseShape::EResize),
    ("top_side", MouseShape::NResize),
    ("top_right_corner", MouseShape::NeResize),
    ("top_left_corner", MouseShape::NwResize),
    ("bottom_side", MouseShape::SResize),
    ("bottom_right_corner", MouseShape::SeResize),
    ("bottom_left_corner", MouseShape::SwResize),
    ("left_side", MouseShape::WResize),
    ("fleur", MouseShape::AllScroll),
];

/// 모든 모양(번호 ↔ 모양 변환에 쓴다)
const ALL: [MouseShape; 34] = [
    MouseShape::Default,
    MouseShape::ContextMenu,
    MouseShape::Help,
    MouseShape::Pointer,
    MouseShape::Progress,
    MouseShape::Wait,
    MouseShape::Cell,
    MouseShape::Crosshair,
    MouseShape::Text,
    MouseShape::VerticalText,
    MouseShape::Alias,
    MouseShape::Copy,
    MouseShape::Move,
    MouseShape::NoDrop,
    MouseShape::NotAllowed,
    MouseShape::Grab,
    MouseShape::Grabbing,
    MouseShape::AllScroll,
    MouseShape::ColResize,
    MouseShape::RowResize,
    MouseShape::NResize,
    MouseShape::EResize,
    MouseShape::SResize,
    MouseShape::WResize,
    MouseShape::NeResize,
    MouseShape::NwResize,
    MouseShape::SeResize,
    MouseShape::SwResize,
    MouseShape::EwResize,
    MouseShape::NsResize,
    MouseShape::NeswResize,
    MouseShape::NwseResize,
    MouseShape::ZoomIn,
    MouseShape::ZoomOut,
];

impl MouseShape {
    /// OSC 22의 이름(정확히 일치, 대소문자 구분: Ghostty와 같다). 모르는 이름은 `None`
    pub fn from_name(name: &str) -> Option<MouseShape> {
        NAMES.iter().find(|(n, _)| *n == name).map(|(_, s)| *s)
    }

    /// 칸의 원자값(0은 "정한 모양 없음")으로 담는 번호. 1부터
    pub fn to_code(self) -> u8 {
        ALL.iter().position(|s| *s == self).map_or(0, |i| i as u8 + 1)
    }

    /// `to_code`의 거꾸로. 0이거나 범위 밖이면 `None`
    pub fn from_code(code: u8) -> Option<MouseShape> {
        (code as usize).checked_sub(1).and_then(|i| ALL.get(i)).copied()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn w3c_and_xterm_names_map_like_ghostty() {
        assert_eq!(MouseShape::from_name("pointer"), Some(MouseShape::Pointer));
        assert_eq!(MouseShape::from_name("vertical-text"), Some(MouseShape::VerticalText));
        assert_eq!(MouseShape::from_name("zoom-out"), Some(MouseShape::ZoomOut));
        // xterm·foot 별칭
        assert_eq!(MouseShape::from_name("left_ptr"), Some(MouseShape::Default));
        assert_eq!(MouseShape::from_name("hand"), Some(MouseShape::Pointer));
        assert_eq!(MouseShape::from_name("xterm"), Some(MouseShape::Text));
        assert_eq!(MouseShape::from_name("fleur"), Some(MouseShape::AllScroll));
        assert_eq!(MouseShape::from_name("top_left_corner"), Some(MouseShape::NwResize));
    }

    #[test]
    fn unknown_empty_and_cased_names_are_none() {
        assert_eq!(MouseShape::from_name(""), None);
        assert_eq!(MouseShape::from_name("banana"), None);
        // 대소문자를 구분한다(Ghostty의 StaticStringMap과 같다)
        assert_eq!(MouseShape::from_name("Pointer"), None);
        assert_eq!(MouseShape::from_name("pointer "), None);
    }

    #[test]
    fn every_name_in_the_table_resolves_and_codes_round_trip() {
        for (n, s) in NAMES {
            assert_eq!(MouseShape::from_name(n), Some(*s), "{n}");
        }
        for s in ALL {
            assert_eq!(MouseShape::from_code(s.to_code()), Some(s));
        }
        assert_eq!(MouseShape::from_code(0), None);
        assert_eq!(MouseShape::from_code(35), None);
        assert_eq!(NAMES.len(), 56);
    }
}
