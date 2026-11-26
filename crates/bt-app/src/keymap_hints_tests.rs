//! `hints:<종류>[,<동작>]` 키 묶음(#865). kitty `hints` kitten(`--type`·`--program`)을 Ghostty식 `이름:값` 문법으로 옮긴 것이다

use super::*;
use bt_term::hints::{HintAction, HintKind};

fn tr(s: &str) -> Trigger {
    parse_trigger(s).unwrap()
}

fn act(s: &str) -> Action {
    parse_action(s).unwrap().unwrap()
}

#[test]
fn hints_action_parses_kind_and_optional_action() {
    assert_eq!(act("hints:url"), Action::Hints(HintKind::Url, HintAction::Open));
    assert_eq!(act("hints:url,copy"), Action::Hints(HintKind::Url, HintAction::Copy));
    assert_eq!(
        act("hints:path,paste"),
        Action::Hints(HintKind::Path, HintAction::Paste)
    );
    assert_eq!(act("hints:linenum"), Action::Hints(HintKind::Linenum, HintAction::Open));
    assert_eq!(
        act("hints:hyperlink,copy"),
        Action::Hints(HintKind::Hyperlink, HintAction::Copy)
    );
    // 열 수 없는 종류는 동작을 안 적으면 붙여넣기(kitty 기본 묶음의 `--program -`)
    assert_eq!(act("hints:word"), Action::Hints(HintKind::Word, HintAction::Paste));
    assert_eq!(act("hints:hash,copy"), Action::Hints(HintKind::Hash, HintAction::Copy));
    assert_eq!(
        act(" hints: line , paste "),
        Action::Hints(HintKind::Line, HintAction::Paste)
    );
}

#[test]
fn hints_action_rejects_unknown_kind_action_and_unopenable_open() {
    for bad in [
        "hints",
        "hints:",
        "hints:regex",
        "hints:ip",
        "hints:URL",
        "hints:url,run",
        "hints:url,open,copy",
        "hints:word,open",
        "hints:hash,open",
        "hints:line,open",
    ] {
        assert!(parse_action(bad).is_err(), "{bad}는 오류여야 한다");
    }
}

/// 기본 묶음: kitty의 `ctrl+shift+e`와 `ctrl+shift+p>…`를 macOS에 둔다. Linux는 Ghostty Linux 기본
/// (`ctrl+shift+e`=`new_split:down`, Config.zig v1.4.0 6888줄)과 겹쳐 두지 않는다
#[cfg(target_os = "macos")]
#[test]
fn kitty_hints_keys_are_default_on_macos() {
    let (km, w) = Keymap::build(&[]);
    assert!(w.is_empty(), "{w:?}");
    assert_eq!(
        km.lookup(&tr("ctrl+shift+e")),
        Some(&Action::Hints(HintKind::Url, HintAction::Open))
    );
    for (chain, want) in [
        ("ctrl+shift+p>f", Action::Hints(HintKind::Path, HintAction::Paste)),
        ("ctrl+shift+p>shift+f", Action::Hints(HintKind::Path, HintAction::Open)),
        ("ctrl+shift+p>l", Action::Hints(HintKind::Line, HintAction::Paste)),
        ("ctrl+shift+p>w", Action::Hints(HintKind::Word, HintAction::Paste)),
        ("ctrl+shift+p>h", Action::Hints(HintKind::Hash, HintAction::Paste)),
        ("ctrl+shift+p>n", Action::Hints(HintKind::Linenum, HintAction::Open)),
        ("ctrl+shift+p>y", Action::Hints(HintKind::Hyperlink, HintAction::Open)),
    ] {
        let keys: Vec<Trigger> = chain.split('>').map(tr).collect();
        assert_eq!(km.exact(&keys).map(|b| &b.action), Some(&want), "{chain}");
    }
}

/// Ghostty 1.4.0 macOS 기본에 `ctrl+shift+e`·`ctrl+shift+p`는 없다. 기본 묶음에 같은 트리거가 두 번 나오지 않는다
#[test]
fn hints_defaults_do_not_shadow_other_defaults() {
    let d = defaults();
    let mut seen = std::collections::HashSet::new();
    for (t, _) in &d {
        assert!(seen.insert(*t), "기본 묶음에 {t}가 두 번");
    }
    #[cfg(not(target_os = "macos"))]
    for (t, a) in &d {
        assert!(
            !a.starts_with("hints"),
            "Linux 기본에는 힌트 묶음을 두지 않는다: {t}={a}"
        );
    }
}

#[test]
fn hints_is_a_listed_action_name() {
    assert!(crate::cliinfo::action_names().contains(&"hints"));
}

/// 힌트는 키를 먹고 PTY에 보내지 않는 동작이고, 마우스 아래 링크 같은 조건이 필요 없다(performable이 아니다)
#[test]
fn hints_is_not_performable_by_default() {
    assert!(!default_performable(&act("hints:url")));
}

/// `+list-keybinds`도 키 순서 기본을 `a>b=동작` 꼴로 보여 준다
#[cfg(target_os = "macos")]
#[test]
fn list_keybinds_shows_the_hint_defaults() {
    let lines = crate::cliinfo::keybind_lines(&[], true);
    for want in [
        "keybind = ctrl+shift+e=hints:url",
        "keybind = ctrl+shift+p>f=hints:path,paste",
        "keybind = ctrl+shift+p>shift+f=hints:path,open",
    ] {
        assert!(lines.iter().any(|l| l == want), "{want}가 없다: {lines:?}");
    }
}
