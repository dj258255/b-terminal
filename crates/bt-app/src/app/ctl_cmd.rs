//! 제어 소켓의 칸 다루기 명령(#858)의 순수 판단: 인자 해석과 거절 까닭. 앱 상태를 건드리는 일은 `App::handle_call`이 한다.

use crate::split::Direction;

/// `scroll`의 움직임
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ScrollMove {
    /// 양수는 위(스크롤백 쪽)로
    Lines(i32),
    PageUp,
    PageDown,
    Top,
    Bottom,
}

/// 한 번에 움직일 줄 수의 상한(스크롤백 상한보다 크다)
const MAX_SCROLL_LINES: u32 = 1_000_000;

pub fn parse_scroll(to: &str, lines: Option<u32>) -> Result<ScrollMove, String> {
    let n = lines.unwrap_or(1).clamp(1, MAX_SCROLL_LINES) as i32;
    match to {
        "up" => Ok(ScrollMove::Lines(n)),
        "down" => Ok(ScrollMove::Lines(-n)),
        "page-up" => Ok(ScrollMove::PageUp),
        "page-down" => Ok(ScrollMove::PageDown),
        "top" => Ok(ScrollMove::Top),
        "bottom" => Ok(ScrollMove::Bottom),
        other => Err(format!(
            "스크롤 방향이 아닙니다: {other}(up·down·page-up·page-down·top·bottom)"
        )),
    }
}

/// `get-text`의 범위
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Extent {
    Screen,
    All,
    Selection,
    LastOutput,
}

pub fn parse_extent(s: &str) -> Result<Extent, String> {
    match s {
        "screen" => Ok(Extent::Screen),
        "all" => Ok(Extent::All),
        "selection" => Ok(Extent::Selection),
        "last-output" => Ok(Extent::LastOutput),
        other => Err(format!("글 범위가 아닙니다: {other}(screen·all·selection·last-output)")),
    }
}

pub fn parse_resize(direction: &str, points: u16) -> Result<(Direction, u16), String> {
    let dir = match direction {
        "left" => Direction::Left,
        "right" => Direction::Right,
        "up" => Direction::Up,
        "down" => Direction::Down,
        other => return Err(format!("경계를 밀 방향이 아닙니다: {other}(left·right·up·down)")),
    };
    if points == 0 {
        return Err("점 수는 1 이상이어야 합니다".into());
    }
    Ok((dir, points))
}

/// 칸에 입력을 만드는 명령(`send-key`)을 거절할 까닭. 읽기 전용 칸은 `send`·`run`처럼 거절한다
pub fn input_refusal(readonly: bool) -> Option<&'static str> {
    readonly.then_some(super::READONLY_REFUSAL)
}

/// 창에 막대(확인·검색·팔레트·제목)가 떠 있으면 키를 보내지 않는 까닭
pub fn key_overlay_refusal(confirm: bool, other_bar: bool) -> Option<&'static str> {
    if confirm {
        Some("그 창에 확인할 일이 있어 키를 보내지 않았습니다")
    } else if other_bar {
        Some("그 창에 검색·팔레트·제목 입력이 열려 있어 키를 보내지 않았습니다")
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scroll_words_and_counts() {
        assert_eq!(parse_scroll("up", None), Ok(ScrollMove::Lines(1)));
        assert_eq!(parse_scroll("up", Some(5)), Ok(ScrollMove::Lines(5)));
        assert_eq!(parse_scroll("down", Some(5)), Ok(ScrollMove::Lines(-5)));
        assert_eq!(parse_scroll("page-up", None), Ok(ScrollMove::PageUp));
        assert_eq!(parse_scroll("page-down", None), Ok(ScrollMove::PageDown));
        assert_eq!(parse_scroll("top", None), Ok(ScrollMove::Top));
        assert_eq!(parse_scroll("bottom", None), Ok(ScrollMove::Bottom));
        assert!(parse_scroll("sideways", None).is_err());
        // 0줄은 1줄로, 터무니없는 줄 수는 상한으로(i32를 넘지 않는다)
        assert_eq!(parse_scroll("up", Some(0)), Ok(ScrollMove::Lines(1)));
        assert_eq!(parse_scroll("down", Some(u32::MAX)), Ok(ScrollMove::Lines(-1_000_000)));
    }

    #[test]
    fn extent_words() {
        assert_eq!(parse_extent("screen"), Ok(Extent::Screen));
        assert_eq!(parse_extent("all"), Ok(Extent::All));
        assert_eq!(parse_extent("selection"), Ok(Extent::Selection));
        assert_eq!(parse_extent("last-output"), Ok(Extent::LastOutput));
        assert!(parse_extent("everything").is_err());
    }

    #[test]
    fn resize_direction_and_points() {
        assert_eq!(parse_resize("left", 20), Ok((Direction::Left, 20)));
        assert_eq!(parse_resize("down", 1), Ok((Direction::Down, 1)));
        assert!(parse_resize("diagonal", 20).is_err());
        assert!(parse_resize("left", 0).is_err());
    }

    /// 읽기 전용 칸에는 키를 보내지 않는다(btctl send·run과 같은 거절 문구)
    #[test]
    fn send_key_is_refused_on_readonly_panes() {
        assert_eq!(input_refusal(true), Some(super::super::READONLY_REFUSAL));
        assert_eq!(input_refusal(false), None);
    }

    #[test]
    fn key_is_refused_while_a_bar_is_open() {
        assert!(key_overlay_refusal(true, false).is_some());
        assert!(key_overlay_refusal(false, true).is_some());
        assert_eq!(key_overlay_refusal(false, false), None);
    }
}
