//! 시스템 밝음·어두움이 바뀔 때 테마를 다시 고를지 정한다(#221). 창을 띄우지 않고 테스트하려고 판단을 순수 함수로 뺐다

use bt_core::config::WindowTheme;

/// 창 테마가 밝음·어두움으로 고정돼 있는가(`window-theme = light|dark`, 또는 `set_window_theme`으로 고름).
/// 고정된 창은 시스템 모양을 따르지 않으므로 winit도 사건을 보내지 않는다(macOS는 창의 `appearance`가 있으면 건너뛴다)
pub fn window_theme_forced(mode: WindowTheme) -> bool {
    matches!(mode, WindowTheme::Light | WindowTheme::Dark)
}

/// 시스템 모양 사건(`now_dark`)을 받았을 때 색 테마를 다시 골라야 하는가.
/// `applied_dark`는 지금 깔린 테마를 고른 모양. 테마가 하나뿐이면(`split` 거짓) 아무 일도 하지 않고,
/// 창 테마가 고정이면 그 창은 시스템을 따르지 않으니 건너뛰며, 같은 모양이 창마다 두 번 와도 한 번만 처리한다
pub fn should_repick(split: bool, forced: bool, applied_dark: bool, now_dark: bool) -> bool {
    split && !forced && applied_dark != now_dark
}

/// 시스템 모양 사건(`now_dark`)을 모드 2031이 켜진 칸에 알려야 하는가(#232). Ghostty처럼 테마가 하나뿐이어도 알리고,
/// 창 테마가 고정이면 그 창은 시스템을 따르지 않으니 건너뛴다. `reported_dark`는 마지막으로 알린 모양
pub fn should_report(forced: bool, reported_dark: bool, now_dark: bool) -> bool {
    !forced && reported_dark != now_dark
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn report_ignores_theme_split_but_not_forced_or_same_look() {
        assert!(should_report(false, true, false));
        assert!(should_report(false, false, true));
        assert!(!should_report(false, true, true), "같은 모양");
        assert!(!should_report(true, true, false), "창 테마 고정");
    }

    #[test]
    fn repicks_only_when_split_and_the_look_changed() {
        assert!(should_repick(true, false, true, false));
        assert!(should_repick(true, false, false, true));
        assert!(!should_repick(false, false, true, false), "테마가 하나뿐");
        assert!(!should_repick(true, false, true, true), "같은 모양");
    }

    #[test]
    fn forced_window_theme_is_ignored() {
        assert!(!should_repick(true, true, true, false));
        assert!(window_theme_forced(WindowTheme::Light));
        assert!(window_theme_forced(WindowTheme::Dark));
        assert!(!window_theme_forced(WindowTheme::System));
        assert!(!window_theme_forced(WindowTheme::Auto));
        assert!(!window_theme_forced(WindowTheme::Ghostty));
    }

    #[test]
    fn same_look_twice_is_handled_once() {
        // 창이 둘이면 같은 사건이 두 번 온다: 처음에 고른 뒤 applied가 now가 되므로 두 번째는 건너뛴다
        let mut applied = true;
        let mut handled = 0;
        for now in [false, false] {
            if should_repick(true, false, applied, now) {
                handled += 1;
                applied = now;
            }
        }
        assert_eq!(handled, 1);
    }
}
