//! 탭을 끌어 창 밖에 놓으면 새 창으로 떼기(#935, kitty 0.46 드래그로 탭 떼기). 창 없이 시험하려고 좌표 판단만 이 파일에 둔다.
//! 좌표는 모두 화면(물리 픽셀)이다. winit(macOS)는 단추를 누른 채 창 밖으로 끌어도 누른 창에 `CursorMoved`·놓음을 계속 보낸다.

/// 화면 좌표 `cursor`가 창 틀(`frame_pos`, `frame_size`: 제목 막대 포함) 밖인가. 틀 가장자리에서 `slack`px 안쪽까지는 안으로 본다
/// (탭 줄에서 끌다가 모서리를 살짝 벗어난 놓기가 떼기가 되지 않게)
pub fn is_outside(cursor: (f64, f64), frame_pos: (i32, i32), frame_size: (u32, u32), slack: f64) -> bool {
    let (x0, y0) = (frame_pos.0 as f64 - slack, frame_pos.1 as f64 - slack);
    let (x1, y1) = (
        frame_pos.0 as f64 + frame_size.0 as f64 + slack,
        frame_pos.1 as f64 + frame_size.1 as f64 + slack,
    );
    cursor.0 < x0 || cursor.0 >= x1 || cursor.1 < y0 || cursor.1 >= y1
}

/// 떼어 낸 새 창을 놓을 왼쪽 위(화면 좌표): 놓은 자리가 새 창의 탭 줄 근처가 되게 커서의 왼쪽·위에 둔다
pub fn window_origin(cursor: (f64, f64), scale: f64) -> (i32, i32) {
    (
        (cursor.0 - 120.0 * scale).round() as i32,
        (cursor.1 - 24.0 * scale).round() as i32,
    )
}

/// 놓기를 놓친 단추(창 밖에서 뗀 놓음이 오지 않는 플랫폼)에 속지 않게, 끌기를 이 거리(물리 픽셀) 넘게 해야 뗀다
pub const DETACH_SLACK_PX: f64 = 12.0;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn outside_means_beyond_the_frame_plus_slack() {
        let (pos, size) = ((100, 50), (800, 600));
        // 안
        assert!(!is_outside((100.0, 50.0), pos, size, 0.0));
        assert!(!is_outside((899.9, 649.9), pos, size, 0.0));
        // 틀 바로 밖은 0이면 밖, slack 안이면 안
        assert!(is_outside((900.0, 300.0), pos, size, 0.0));
        assert!(is_outside((99.9, 300.0), pos, size, 0.0));
        assert!(!is_outside((905.0, 300.0), pos, size, DETACH_SLACK_PX));
        assert!(!is_outside((90.0, 40.0), pos, size, DETACH_SLACK_PX));
        assert!(is_outside((912.0, 300.0), pos, size, DETACH_SLACK_PX));
        assert!(is_outside((300.0, 37.9), pos, size, DETACH_SLACK_PX));
        assert!(is_outside((300.0, 662.0), pos, size, DETACH_SLACK_PX));
        // 음수 화면 좌표(왼쪽 모니터)도 같은 규칙
        assert!(!is_outside((-500.0, 100.0), (-800, 0), (600, 400), 0.0));
        assert!(is_outside((-100.0, 100.0), (-800, 0), (600, 400), 0.0));
    }

    #[test]
    fn new_window_lands_with_the_cursor_near_its_tab_bar() {
        assert_eq!(window_origin((1000.0, 500.0), 2.0), (760, 452));
        assert_eq!(window_origin((1000.0, 500.0), 1.0), (880, 476));
        assert_eq!(
            window_origin((10.0, 5.0), 1.0),
            (-110, -19),
            "화면 밖은 창 관리자가 맞춘다"
        );
    }
}
