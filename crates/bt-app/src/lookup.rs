//! 낱말 사전 조회(macOS force click, Ghostty `SurfaceView.quickLook`, #853)
//!
//! 사전 말풍선에 넘길 낱말을 정한다. Ghostty는 고른 글과 상관없이 마우스 아래 낱말(`selectWord`)을 쓴다.
//! 암호 입력 중(보안 입력)에는 화면 글을 시스템 사전으로 넘기지 않는다.

/// 마우스 아래 낱말 `word`에서 사전에 넘길 글을 정한다. 보안 입력 중이거나 낱말이 없거나 비어 있으면 `None`
#[cfg_attr(not(target_os = "macos"), allow(dead_code))]
pub fn target(word: Option<&str>, secure: bool) -> Option<String> {
    if secure {
        return None;
    }
    word.filter(|w| !w.trim().is_empty()).map(str::to_owned)
}

/// 시스템 설정 "힘주어 클릭하고 햅틱 반응"이 켜져 있는가(Ghostty `com.apple.trackpad.forceClick`). 없으면 꺼진 것으로 본다
#[cfg(target_os = "macos")]
pub fn force_click_enabled() -> bool {
    use objc2_foundation::{NSString, NSUserDefaults};
    NSUserDefaults::standardUserDefaults().boolForKey(&NSString::from_str("com.apple.trackpad.forceClick"))
}

/// 지금 앞에 있는 창에 사전 말풍선을 띄운다. `x`·`y`는 창 안쪽 위 왼쪽에서 잰 점(논리 단위)이고 `y`는 글자 기준선이다.
/// AppKit 좌표(왼쪽 아래 기준)로 바꿔 `showDefinitionForAttributedString:atPoint:`에 넘긴다. 띄웠으면 참
#[cfg(target_os = "macos")]
pub fn show_definition(text: &str, x: f64, y: f64) -> bool {
    use objc2_foundation::{NSAttributedString, NSPoint, NSString};
    let Some(mtm) = objc2::MainThreadMarker::new() else {
        return false;
    };
    let app = objc2_app_kit::NSApplication::sharedApplication(mtm);
    let Some(view) = app.keyWindow().and_then(|w| w.contentView()) else {
        return false;
    };
    let height = view.frame().size.height;
    let string = NSAttributedString::from_nsstring(&NSString::from_str(text));
    view.showDefinitionForAttributedString_atPoint(Some(&string), NSPoint::new(x, height - y));
    true
}

#[cfg(test)]
mod tests {
    use super::*;
    use bt_core::Theme;
    use bt_term::{GridSize, Session, SessionEvent, SpawnOptions};
    use std::sync::Arc;

    fn session(input: &str) -> Session {
        let opts = SpawnOptions {
            program: Some("/bin/sh".into()),
            args: vec!["-c".into(), "sleep 120".into()],
            cwd: Some(std::env::temp_dir()),
            scrollback: 100,
            word_chars: Some(" ".to_string()),
            ..Default::default()
        };
        let sink = Arc::new(|_e: SessionEvent| {});
        let s = Session::spawn(opts, GridSize::new(20, 4), Theme::default(), sink).expect("spawn");
        s.feed_local(input.replace('\n', "\r\n").as_bytes());
        s
    }

    #[test]
    fn word_under_the_mouse_is_the_target() {
        assert_eq!(target(Some("alpha"), false).as_deref(), Some("alpha"));
    }

    #[test]
    fn wide_characters_are_passed_whole() {
        assert_eq!(target(Some("한글"), false).as_deref(), Some("한글"));
    }

    #[test]
    fn blank_or_missing_word_is_not_looked_up() {
        assert_eq!(target(None, false), None);
        assert_eq!(target(Some(""), false), None);
        assert_eq!(target(Some("   "), false), None);
        assert_eq!(target(Some("\t\n"), false), None);
    }

    #[test]
    fn secure_input_never_looks_up() {
        assert_eq!(target(Some("hunter2"), true), None);
    }

    #[test]
    fn word_at_returns_word_and_its_start_cell() {
        let s = session("alpha beta");
        let hit = s.word_at(8, 0).expect("word");
        assert_eq!((hit.text.as_str(), hit.col, hit.row), ("beta", 6, 0));
        let hit = s.word_at(2, 0).expect("word");
        assert_eq!((hit.text.as_str(), hit.col, hit.row), ("alpha", 0, 0));
    }

    #[test]
    fn word_at_starts_at_the_wide_character_cell() {
        let s = session("a 한글 b");
        // "한글"은 칸 2~5. 뒷칸(3)을 짚어도 낱말 시작은 2
        let hit = s.word_at(3, 0).expect("word");
        assert_eq!((hit.text.as_str(), hit.col), ("한글", 2));
    }

    #[test]
    fn word_at_empty_cell_is_none() {
        let s = session("alpha");
        assert!(s.word_at(15, 0).is_none());
    }
}
