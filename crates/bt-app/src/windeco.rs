//! 창 모양 고르기(#192): `window-decoration`·`macos-titlebar-style`·`macos-window-shadow`·`window-theme`.
//! 창을 띄우지 않고 테스트하려고 설정에서 창 속성을 정하는 부분을 순수 함수로 뺐다

use bt_core::Rgb;
use bt_core::config::{MacTitlebarStyle, WindowDecoration, WindowTheme};
use winit::window::Theme;

/// macOS 기본 제목 막대의 높이(논리 점). 제목 막대 밑으로 콘텐츠를 넣는 모양에서 탭 막대·격자가 신호등 단추 밑에서 시작하게 한다.
/// 이 값은 재지 않은 값이다(macOS 26에서 다를 수 있다)
pub const TITLEBAR_POINTS: f32 = 28.0;

/// 새 창이 가질 모양
#[cfg_attr(not(target_os = "macos"), allow(dead_code))]
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct WindowLook {
    /// 제목 막대·테두리가 있는가
    pub decorations: bool,
    /// 제목 막대를 투명하게(창 바탕색이 비친다)
    pub titlebar_transparent: bool,
    /// 콘텐츠가 제목 막대 밑까지 차지한다
    pub fullsize_content: bool,
    /// 신호등 단추를 숨긴다
    pub buttons_hidden: bool,
    /// 제목 막대의 창 제목 글을 숨긴다(hidden 모양에서 제목이 콘텐츠 위에 겹치지 않게)
    pub title_hidden: bool,
    pub shadow: bool,
    /// 콘텐츠가 제목 막대에 가리지 않으려고 비워 둘 위쪽 높이(논리 점). 창을 만든 때 정해진다
    pub inset_points: f32,
}

/// 설정에서 새 창의 모양을 정한다. `quick`이면 퀵 터미널 창(처음부터 테두리 없음).
/// `window-decoration = none`만 어느 OS에서나 듣고, 제목 막대 모양은 macOS에서만 뜻이 있다.
/// `tabs`는 b-terminal이 탭 막대를 직접 그리므로 `transparent`와 같다
pub fn window_look(
    quick: bool,
    macos: bool,
    decoration: WindowDecoration,
    style: MacTitlebarStyle,
    buttons_hidden: bool,
    shadow: bool,
) -> WindowLook {
    let decorations = !quick && decoration != WindowDecoration::None;
    let mut look = WindowLook {
        decorations,
        titlebar_transparent: false,
        fullsize_content: false,
        buttons_hidden: false,
        title_hidden: false,
        shadow,
        inset_points: 0.0,
    };
    if !decorations || !macos {
        return look;
    }
    look.buttons_hidden = buttons_hidden;
    match style {
        MacTitlebarStyle::Native => {}
        // 제목 막대를 투명하게 하고 콘텐츠를 그 밑까지 늘린다. 제목 막대 색은 우리가 그린 바탕색이 비친 것이다
        MacTitlebarStyle::Transparent | MacTitlebarStyle::Tabs => {
            look.titlebar_transparent = true;
            look.fullsize_content = true;
            look.inset_points = TITLEBAR_POINTS;
        }
        // 틀(둥근 모서리)은 두고 제목 막대와 단추만 감춘다. 콘텐츠가 창 전체를 쓴다
        MacTitlebarStyle::Hidden => {
            look.titlebar_transparent = true;
            look.fullsize_content = true;
            look.buttons_hidden = true;
            look.title_hidden = true;
        }
    }
    look
}

/// 지금 콘텐츠가 비워 둘 위쪽 높이(픽셀). 전체 화면이거나 꾸밈을 껐으면 제목 막대가 없으니 0
pub fn top_inset_px(inset_points: f32, scale: f64, decorations_now: bool, fullscreen: bool) -> u32 {
    if !decorations_now || fullscreen || inset_points <= 0.0 {
        return 0;
    }
    (inset_points as f64 * scale).round() as u32
}

/// 바탕색이 밝은가(지각 밝기 0.299R+0.587G+0.114B가 0.5를 넘는가)
pub fn is_light(bg: Rgb) -> bool {
    let l = 0.299 * bg.r as f32 + 0.587 * bg.g as f32 + 0.114 * bg.b as f32;
    l / 255.0 > 0.5
}

/// `window-theme`으로 창에 강제할 모양. `None`이면 시스템 모양을 따른다.
/// `auto`는 macOS의 transparent·tabs 제목 막대에서만(macOS 밖에서는 늘) 바탕색 밝기로 정하고,
/// 테마가 밝은·어두운 갈래로 나뉘면 system과 같다. `ghostty`는 Linux에서만 뜻이 있어 다른 곳에서는 `auto`처럼 본다
pub fn window_theme(mode: WindowTheme, style: MacTitlebarStyle, split: bool, bg: Rgb, macos: bool) -> Option<Theme> {
    match mode {
        WindowTheme::System => None,
        WindowTheme::Light => Some(Theme::Light),
        WindowTheme::Dark => Some(Theme::Dark),
        WindowTheme::Auto | WindowTheme::Ghostty => {
            let by_background = !macos || matches!(style, MacTitlebarStyle::Transparent | MacTitlebarStyle::Tabs);
            if split || !by_background {
                None
            } else {
                Some(if is_light(bg) { Theme::Light } else { Theme::Dark })
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const BLACK: Rgb = Rgb { r: 0, g: 0, b: 0 };
    const WHITE: Rgb = Rgb { r: 255, g: 255, b: 255 };

    fn look(decoration: WindowDecoration, style: MacTitlebarStyle) -> WindowLook {
        window_look(false, true, decoration, style, false, true)
    }

    #[test]
    fn default_look_is_transparent_titlebar_under_content() {
        let l = look(WindowDecoration::Auto, MacTitlebarStyle::Transparent);
        assert!(l.decorations && l.titlebar_transparent && l.fullsize_content && l.shadow);
        assert!(!l.buttons_hidden);
        assert!(!l.title_hidden, "transparent는 제목을 보인다(Ghostty와 같다)");
        assert_eq!(l.inset_points, TITLEBAR_POINTS);
    }

    #[test]
    fn tabs_is_the_same_as_transparent() {
        assert_eq!(
            look(WindowDecoration::Auto, MacTitlebarStyle::Tabs),
            look(WindowDecoration::Auto, MacTitlebarStyle::Transparent)
        );
    }

    #[test]
    fn native_has_no_inset_and_no_tricks() {
        let l = look(WindowDecoration::Auto, MacTitlebarStyle::Native);
        assert!(l.decorations && !l.titlebar_transparent && !l.fullsize_content && !l.buttons_hidden);
        assert_eq!(l.inset_points, 0.0);
    }

    #[test]
    fn hidden_keeps_frame_but_hides_bar_and_buttons() {
        let l = look(WindowDecoration::Auto, MacTitlebarStyle::Hidden);
        assert!(l.decorations && l.titlebar_transparent && l.fullsize_content && l.buttons_hidden);
        assert!(l.title_hidden, "제목 글이 콘텐츠 첫 줄 위에 겹치지 않게 숨긴다");
        // 제목 막대가 안 보이니 콘텐츠를 비우지 않는다
        assert_eq!(l.inset_points, 0.0);
    }

    #[test]
    fn decoration_none_wins_over_style() {
        for style in [
            MacTitlebarStyle::Native,
            MacTitlebarStyle::Transparent,
            MacTitlebarStyle::Tabs,
            MacTitlebarStyle::Hidden,
        ] {
            let l = look(WindowDecoration::None, style);
            assert!(
                !l.decorations && !l.titlebar_transparent && !l.fullsize_content,
                "{style:?}"
            );
            assert_eq!(l.inset_points, 0.0, "{style:?}");
        }
    }

    #[test]
    fn client_and_server_are_default_decorations_on_macos() {
        for d in [
            WindowDecoration::Auto,
            WindowDecoration::Client,
            WindowDecoration::Server,
        ] {
            assert!(look(d, MacTitlebarStyle::Native).decorations, "{d:?}");
        }
    }

    #[test]
    fn window_buttons_setting_hides_buttons_on_any_style() {
        let l = window_look(
            false,
            true,
            WindowDecoration::Auto,
            MacTitlebarStyle::Native,
            true,
            true,
        );
        assert!(l.buttons_hidden);
    }

    #[test]
    fn shadow_follows_setting() {
        let l = window_look(
            false,
            true,
            WindowDecoration::Auto,
            MacTitlebarStyle::Native,
            false,
            false,
        );
        assert!(!l.shadow);
    }

    #[test]
    fn quick_window_is_borderless_without_inset() {
        let l = window_look(
            true,
            true,
            WindowDecoration::Auto,
            MacTitlebarStyle::Transparent,
            false,
            true,
        );
        assert!(!l.decorations && !l.fullsize_content && !l.titlebar_transparent);
        assert_eq!(l.inset_points, 0.0);
    }

    #[test]
    fn titlebar_style_does_nothing_off_macos() {
        let l = window_look(
            false,
            false,
            WindowDecoration::Auto,
            MacTitlebarStyle::Transparent,
            true,
            true,
        );
        assert!(l.decorations && !l.titlebar_transparent && !l.fullsize_content && !l.buttons_hidden);
        assert_eq!(l.inset_points, 0.0);
        // window-decoration = none은 어느 OS에서나 듣는다
        assert!(
            !window_look(
                false,
                false,
                WindowDecoration::None,
                MacTitlebarStyle::Native,
                false,
                true
            )
            .decorations
        );
    }

    #[test]
    fn inset_scales_and_vanishes_without_titlebar() {
        assert_eq!(top_inset_px(28.0, 2.0, true, false), 56);
        assert_eq!(top_inset_px(28.0, 1.0, true, false), 28);
        assert_eq!(top_inset_px(28.0, 1.5, true, false), 42);
        assert_eq!(top_inset_px(28.0, 2.0, false, false), 0);
        assert_eq!(top_inset_px(28.0, 2.0, true, true), 0);
        assert_eq!(top_inset_px(0.0, 2.0, true, false), 0);
    }

    #[test]
    fn light_and_dark_backgrounds() {
        assert!(is_light(WHITE));
        assert!(!is_light(BLACK));
        // 어두운 회색(Ghostty 기본 바탕)은 어둡다
        assert!(!is_light(Rgb {
            r: 0x28,
            g: 0x2c,
            b: 0x34
        }));
        assert!(is_light(Rgb {
            r: 0xfa,
            g: 0xfa,
            b: 0xfa
        }));
        // 초록은 파랑보다 밝게 보인다: 같은 값이어도 지각 밝기가 다르다
        assert!(is_light(Rgb { r: 0, g: 230, b: 0 }));
        assert!(!is_light(Rgb { r: 0, g: 0, b: 230 }));
    }

    #[test]
    fn theme_light_dark_are_forced() {
        for style in [MacTitlebarStyle::Native, MacTitlebarStyle::Transparent] {
            assert_eq!(
                window_theme(WindowTheme::Light, style, false, BLACK, true),
                Some(Theme::Light)
            );
            assert_eq!(
                window_theme(WindowTheme::Dark, style, false, WHITE, true),
                Some(Theme::Dark)
            );
        }
    }

    #[test]
    fn theme_system_follows_the_system() {
        assert_eq!(
            window_theme(WindowTheme::System, MacTitlebarStyle::Transparent, false, WHITE, true),
            None
        );
    }

    #[test]
    fn theme_auto_uses_background_on_transparent_and_tabs() {
        for style in [MacTitlebarStyle::Transparent, MacTitlebarStyle::Tabs] {
            assert_eq!(
                window_theme(WindowTheme::Auto, style, false, WHITE, true),
                Some(Theme::Light)
            );
            assert_eq!(
                window_theme(WindowTheme::Auto, style, false, BLACK, true),
                Some(Theme::Dark)
            );
        }
    }

    #[test]
    fn theme_auto_is_system_for_native_hidden_and_split_themes() {
        for style in [MacTitlebarStyle::Native, MacTitlebarStyle::Hidden] {
            assert_eq!(window_theme(WindowTheme::Auto, style, false, WHITE, true), None);
        }
        assert_eq!(
            window_theme(WindowTheme::Auto, MacTitlebarStyle::Transparent, true, WHITE, true),
            None
        );
        // macOS가 아니면 제목 막대 모양과 상관없이 바탕색을 따른다
        assert_eq!(
            window_theme(WindowTheme::Auto, MacTitlebarStyle::Native, false, WHITE, false),
            Some(Theme::Light)
        );
        assert_eq!(
            window_theme(WindowTheme::Auto, MacTitlebarStyle::Native, true, WHITE, false),
            None
        );
    }

    #[test]
    fn theme_ghostty_acts_like_auto() {
        assert_eq!(
            window_theme(WindowTheme::Ghostty, MacTitlebarStyle::Transparent, false, WHITE, true),
            Some(Theme::Light)
        );
        assert_eq!(
            window_theme(WindowTheme::Ghostty, MacTitlebarStyle::Transparent, true, WHITE, true),
            None
        );
    }
}
