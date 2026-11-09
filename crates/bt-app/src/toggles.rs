//! 토글·설정 동작(#209): `toggle_background_opacity`, `toggle_mouse_reporting`, `set_window_theme`, `open_config`.
//! 창을 띄우지 않고 테스트하려고 판단을 순수 함수로 뺐다

use std::ffi::OsString;
use std::path::Path;

use bt_core::config::WindowTheme;

/// 지금 창이 쓸 바탕 불투명도. `opaque`는 이 창이 `toggle_background_opacity`로 불투명해졌는가
pub fn effective_opacity(config: f32, opaque: bool) -> f32 {
    if opaque { 1.0 } else { config }
}

/// `toggle_background_opacity`를 할 수 있는가. `background-opacity`가 1 이상이면 창이 처음부터
/// 불투명하게 만들어지므로 하지 않는다(Ghostty 주석)
pub fn can_toggle_opacity(config: f32) -> bool {
    config < 1.0
}

/// 창이 따를 `window-theme`: 이번 실행 동안 `set_window_theme`으로 고른 것이 있으면 그것, 없으면 설정
/// (`config`를 고르면 `over`가 `None`이 되어 설정으로 돌아간다)
pub fn window_theme_mode(config: WindowTheme, over: Option<WindowTheme>) -> WindowTheme {
    over.unwrap_or(config)
}

/// 이 칸이 프로그램에 마우스를 보고하는가. `over`는 `toggle_mouse_reporting`으로 칸에 정한 값(없으면 설정을 따른다)
pub fn mouse_reporting_on(config: bool, over: Option<bool>) -> bool {
    over.unwrap_or(config)
}

/// `toggle_mouse_reporting`을 한 뒤 칸에 둘 값
pub fn toggled_mouse_reporting(config: bool, over: Option<bool>) -> Option<bool> {
    Some(!mouse_reporting_on(config, over))
}

/// 설정 파일을 기본 편집기로 여는 명령(프로그램, 인자). 셸을 거치지 않고 인자 배열로 넘긴다.
/// macOS는 `open -t`(기본 텍스트 편집기), 그 밖에는 `xdg-open`
pub fn config_open_argv(path: &Path, macos: bool) -> Vec<OsString> {
    if macos {
        vec!["open".into(), "-t".into(), path.as_os_str().to_owned()]
    } else {
        vec!["xdg-open".into(), path.as_os_str().to_owned()]
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::keymap::{Action, parse_action};

    #[test]
    fn opacity_toggle_switches_between_config_and_opaque() {
        assert_eq!(effective_opacity(0.8, false), 0.8);
        assert_eq!(effective_opacity(0.8, true), 1.0);
        assert_eq!(effective_opacity(0.0, true), 1.0);
    }

    #[test]
    fn opacity_toggle_does_nothing_when_config_is_opaque() {
        assert!(can_toggle_opacity(0.99));
        assert!(!can_toggle_opacity(1.0));
        assert!(!can_toggle_opacity(1.5));
    }

    #[test]
    fn window_theme_override_survives_and_config_returns() {
        // 고른 것이 없으면 설정을 따른다(설정을 다시 읽어 window-theme이 바뀌어도 따라간다)
        assert_eq!(window_theme_mode(WindowTheme::Dark, None), WindowTheme::Dark);
        // 고른 것이 있으면 설정이 무엇이든 그것이 이긴다
        assert_eq!(
            window_theme_mode(WindowTheme::Dark, Some(WindowTheme::Light)),
            WindowTheme::Light
        );
        assert_eq!(
            window_theme_mode(WindowTheme::Auto, Some(WindowTheme::System)),
            WindowTheme::System
        );
    }

    #[test]
    fn mouse_reporting_toggle_flips_the_effective_value() {
        assert!(mouse_reporting_on(true, None));
        assert!(!mouse_reporting_on(true, Some(false)));
        assert!(mouse_reporting_on(false, Some(true)));
        // 설정이 켬이면 첫 토글은 끔, 한 번 더 하면 켬
        let once = toggled_mouse_reporting(true, None);
        assert_eq!(once, Some(false));
        assert_eq!(toggled_mouse_reporting(true, once), Some(true));
        // 설정이 끔이면 첫 토글은 켬
        assert_eq!(toggled_mouse_reporting(false, None), Some(true));
    }

    #[test]
    fn config_is_opened_with_an_argument_array() {
        let p = Path::new("/tmp/b term; rm -rf x/config");
        let mac = config_open_argv(p, true);
        assert_eq!(mac, vec![OsString::from("open"), "-t".into(), p.as_os_str().to_owned()]);
        let other = config_open_argv(p, false);
        assert_eq!(other, vec![OsString::from("xdg-open"), p.as_os_str().to_owned()]);
    }

    #[test]
    fn parses_the_toggle_actions() {
        assert_eq!(
            parse_action("toggle_background_opacity"),
            Ok(Some(Action::ToggleBackgroundOpacity))
        );
        assert_eq!(
            parse_action("toggle_mouse_reporting"),
            Ok(Some(Action::ToggleMouseReporting))
        );
        assert_eq!(parse_action("toggle_readonly"), Ok(Some(Action::ToggleReadonly)));
        assert_eq!(parse_action("open_config"), Ok(Some(Action::OpenConfig)));
        // 기본값 os_open과 Ghostty에 있는 값 new_window(GTK 전용)를 받는다
        assert_eq!(parse_action("open_config:os_open"), Ok(Some(Action::OpenConfig)));
        assert_eq!(parse_action("open_config:new_window"), Ok(Some(Action::OpenConfig)));
        assert!(parse_action("open_config:vim").is_err());
    }

    #[test]
    fn parses_set_window_theme() {
        assert_eq!(
            parse_action("set_window_theme:config"),
            Ok(Some(Action::SetWindowTheme(None)))
        );
        assert_eq!(
            parse_action("set_window_theme:system"),
            Ok(Some(Action::SetWindowTheme(Some(WindowTheme::System))))
        );
        assert_eq!(
            parse_action("set_window_theme:light"),
            Ok(Some(Action::SetWindowTheme(Some(WindowTheme::Light))))
        );
        assert_eq!(
            parse_action("set_window_theme:dark"),
            Ok(Some(Action::SetWindowTheme(Some(WindowTheme::Dark))))
        );
        // 인자가 없거나 모르는 값, 설정의 window-theme 값(auto·ghostty)은 동작의 값이 아니다
        assert!(parse_action("set_window_theme").is_err());
        assert!(parse_action("set_window_theme:auto").is_err());
        assert!(parse_action("set_window_theme:ghostty").is_err());
    }
}
