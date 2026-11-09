//! 시작 창 모양 계산(#148). 창과 상관없는 순수 함수라 여기서 테스트한다

/// 새 창이 처음 뜨는 모양
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StartMode {
    Normal,
    Maximized,
    Fullscreen,
}

/// `maximize`·`fullscreen` 설정과 퀵 창 여부로 시작 모양을 정한다. 퀵 창은 늘 보통
pub fn start_mode(maximize: bool, fullscreen: bool, quick: bool) -> StartMode {
    if quick {
        StartMode::Normal
    } else if fullscreen {
        StartMode::Fullscreen
    } else if maximize {
        StartMode::Maximized
    } else {
        StartMode::Normal
    }
}

/// 전체 화면을 켜고 끄는 방식
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FullscreenKind {
    /// 창 시스템의 전체 화면(새 Space)
    Native,
    /// 새 Space 없는 전체 화면(macOS 전용, `macos-non-native-fullscreen`)
    Simple,
}

/// 설정과 운영체제로 전체 화면 방식을 정한다. macOS가 아니면 늘 Native
pub fn fullscreen_kind(kind: bt_core::config::NonNativeFullscreen, macos: bool) -> FullscreenKind {
    if macos && kind != bt_core::config::NonNativeFullscreen::False {
        FullscreenKind::Simple
    } else {
        FullscreenKind::Native
    }
}

/// `window-step-resize`가 켜졌을 때 창 크기를 맞출 단위(물리 픽셀). 셀 크기를 모르면(0) 쓰지 않는다
pub fn step_increments(enabled: bool, cell_w: u32, cell_h: u32) -> Option<(u32, u32)> {
    (enabled && cell_w > 0 && cell_h > 0).then_some((cell_w, cell_h))
}

/// `window-position-x`·`-y`: 둘 다 있어야 쓴다(Ghostty와 같이 하나만 있으면 무시)
pub fn configured_position(x: Option<i32>, y: Option<i32>) -> Option<(i32, i32)> {
    Some((x?, y?))
}

/// 설정한 점 좌표를 주 화면 왼쪽 위(물리 픽셀 `origin`)에서 잰 물리 픽셀 자리로 바꾼다
pub fn physical_position(pos: (i32, i32), origin: (i32, i32), scale: f64) -> (i32, i32) {
    let at = |p: i32, o: i32| o + (p as f64 * scale).round() as i32;
    (at(pos.0, origin.0), at(pos.1, origin.1))
}

/// 항상 위 켬/끔에 맞는 창 층
pub fn window_level(on: bool) -> winit::window::WindowLevel {
    if on {
        winit::window::WindowLevel::AlwaysOnTop
    } else {
        winit::window::WindowLevel::Normal
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fullscreen_kind_by_setting_and_os() {
        use bt_core::config::NonNativeFullscreen as N;
        assert_eq!(fullscreen_kind(N::False, true), FullscreenKind::Native);
        for k in [N::True, N::VisibleMenu, N::PaddedNotch] {
            assert_eq!(fullscreen_kind(k, true), FullscreenKind::Simple, "{k:?}");
            assert_eq!(
                fullscreen_kind(k, false),
                FullscreenKind::Native,
                "macOS가 아니면 {k:?}도 Native"
            );
        }
    }

    #[test]
    fn step_increments_only_when_on_and_cell_known() {
        assert_eq!(step_increments(true, 9, 18), Some((9, 18)));
        assert_eq!(step_increments(false, 9, 18), None);
        assert_eq!(step_increments(true, 0, 18), None);
        assert_eq!(step_increments(true, 9, 0), None);
    }

    #[test]
    fn mode_priority_and_quick() {
        assert_eq!(start_mode(false, false, false), StartMode::Normal);
        assert_eq!(start_mode(true, false, false), StartMode::Maximized);
        assert_eq!(start_mode(false, true, false), StartMode::Fullscreen);
        assert_eq!(
            start_mode(true, true, false),
            StartMode::Fullscreen,
            "전체 화면이 이긴다"
        );
        assert_eq!(
            start_mode(true, true, true),
            StartMode::Normal,
            "퀵 창에는 적용하지 않는다"
        );
    }

    #[test]
    fn float_level() {
        use winit::window::WindowLevel::*;
        assert_eq!(window_level(true), AlwaysOnTop);
        assert_eq!(window_level(false), Normal);
    }

    #[test]
    fn position_needs_both() {
        assert_eq!(configured_position(Some(10), Some(20)), Some((10, 20)));
        assert_eq!(configured_position(Some(10), None), None);
        assert_eq!(configured_position(None, Some(20)), None);
        assert_eq!(configured_position(None, None), None);
    }

    #[test]
    fn position_scales_from_origin() {
        assert_eq!(physical_position((100, 50), (0, 0), 2.0), (200, 100));
        assert_eq!(physical_position((100, 50), (1440, -30), 1.0), (1540, 20));
        assert_eq!(physical_position((-10, 0), (0, 0), 2.0), (-20, 0));
    }
}
