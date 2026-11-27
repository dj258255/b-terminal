//! vi 복사 모드(#930)의 키 쪽: 동작 이름, 기본 키 테이블. 화면·PTY·창을 건드리지 않는 순수한 부분이다.
//! 커서 움직임과 고르기는 `bt_term::copy_mode`, 앱과의 연결은 `app/copy_cmd.rs`가 맡는다.
//!
//! 복사 모드는 이름이 `copy_mode`인 키 테이블이다(ADR-103). 기본 묶음은 WezTerm CopyMode를 따르고(Alacritty vi mode와 같은 키가 많다),
//! 사용자가 `keybind = copy_mode/h=…`로 덮거나 `copy_mode/h=unbind`로 지운다

use bt_term::{CopyOp, JumpKind};

/// 복사 모드 키 테이블 이름
pub const TABLE: &str = "copy_mode";

/// `copy_mode:<이름>` 동작 하나
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CopyAction {
    /// 커서·고르기 동작
    Op(CopyOp),
    /// 고른 글을 클립보드에 복사하고 나간다(고른 것이 없으면 복사 없이 나간다)
    Copy,
    /// 복사 없이 나간다
    Exit,
    /// 찾기 막대를 열어 글을 받는다(`/`면 앞으로, `?`면 뒤로, #932)
    SearchOpen { forward: bool },
    /// 마지막에 찾은 글을 다시 찾는다(`n`은 같은 방향, `N`은 반대 방향)
    SearchNext { reverse: bool },
    /// 다음에 치는 글자를 줄 안에서 찾는다(`f F t T`)
    Jump(JumpKind),
    /// 마지막 줄 안 글자 찾기를 되풀이한다(`;`는 같은 방향, `,`는 반대 방향)
    JumpAgain { reverse: bool },
}

const fn jump(forward: bool, till: bool) -> CopyAction {
    CopyAction::Jump(JumpKind { forward, till })
}

const NAMES: &[(&str, CopyAction)] = &[
    ("left", CopyAction::Op(CopyOp::Left)),
    ("right", CopyAction::Op(CopyOp::Right)),
    ("up", CopyAction::Op(CopyOp::Up)),
    ("down", CopyAction::Op(CopyOp::Down)),
    ("line_start", CopyAction::Op(CopyOp::LineStart)),
    ("line_end", CopyAction::Op(CopyOp::LineEnd)),
    ("top", CopyAction::Op(CopyOp::Top)),
    ("bottom", CopyAction::Op(CopyOp::Bottom)),
    ("half_page_up", CopyAction::Op(CopyOp::HalfPageUp)),
    ("half_page_down", CopyAction::Op(CopyOp::HalfPageDown)),
    ("page_up", CopyAction::Op(CopyOp::PageUp)),
    ("page_down", CopyAction::Op(CopyOp::PageDown)),
    ("word_forward", CopyAction::Op(CopyOp::WordForward)),
    ("word_backward", CopyAction::Op(CopyOp::WordBackward)),
    ("word_end", CopyAction::Op(CopyOp::WordEnd)),
    ("line_content_start", CopyAction::Op(CopyOp::LineContentStart)),
    ("viewport_top", CopyAction::Op(CopyOp::ViewportTop)),
    ("viewport_middle", CopyAction::Op(CopyOp::ViewportMiddle)),
    ("viewport_bottom", CopyAction::Op(CopyOp::ViewportBottom)),
    ("select", CopyAction::Op(CopyOp::SelectCell)),
    ("select_line", CopyAction::Op(CopyOp::SelectLine)),
    ("select_block", CopyAction::Op(CopyOp::SelectBlock)),
    ("next_line", CopyAction::Op(CopyOp::NextLineStart)),
    ("other_end", CopyAction::Op(CopyOp::OtherEnd)),
    ("other_end_horiz", CopyAction::Op(CopyOp::OtherEndHoriz)),
    ("search_forward", CopyAction::SearchOpen { forward: true }),
    ("search_backward", CopyAction::SearchOpen { forward: false }),
    ("search_next", CopyAction::SearchNext { reverse: false }),
    ("search_prev", CopyAction::SearchNext { reverse: true }),
    ("jump_forward", jump(true, false)),
    ("jump_backward", jump(false, false)),
    ("jump_till_forward", jump(true, true)),
    ("jump_till_backward", jump(false, true)),
    ("jump_again", CopyAction::JumpAgain { reverse: false }),
    ("jump_reverse", CopyAction::JumpAgain { reverse: true }),
    ("copy", CopyAction::Copy),
    ("exit", CopyAction::Exit),
];

/// 복사 모드 중에 들어온 확정 글(IME 확정·붙여넣기)이 갈 곳. 복사 모드 중에는 PTY로 가지 않는다(ADR-103).
/// 찾기 막대가 열려 있으면 막대로(조합 중인 글은 오지 않고 확정한 글만 온다), 글자를 기다리는 `f F t T`면 그 글자로, 아니면 버린다
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TextRoute {
    Search,
    Jump,
    Drop,
}

pub fn route_text(search_open: bool, jump_waiting: bool) -> TextRoute {
    if search_open {
        TextRoute::Search
    } else if jump_waiting {
        TextRoute::Jump
    } else {
        TextRoute::Drop
    }
}

/// `copy_mode:<이름>`의 값을 읽는다
pub fn parse(arg: Option<&str>) -> Result<CopyAction, String> {
    let arg = arg.map(str::trim).unwrap_or("");
    NAMES.iter().find(|(n, _)| *n == arg).map(|(_, a)| *a).ok_or_else(|| {
        let names: Vec<&str> = NAMES.iter().map(|(n, _)| *n).collect();
        format!("copy_mode:{}", names.join("·"))
    })
}

/// 기본 키 테이블(트리거, 동작). WezTerm CopyMode의 기본 묶음 가운데 이번 판까지의 동작만 옮겼다.
/// 낱말(w b e, Tab·Shift+Tab, Alt+f·Alt+b·Alt+←→)·줄의 첫 글자(^, Alt+m)·화면 위·중간·아래(H M L)는 #931이다.
/// WezTerm에 없는 W B E { }는 넣지 않았다. 줄 안 글자 찾기(f F t T ; ,)·o O·Enter는 WezTerm 기본 묶음 그대로(#932).
/// 찾기 `/ ? n N`은 WezTerm 기본 복사 모드 묶음에는 없고 vi를 따랐다(#932)
/// 글자 키는 `super`·`ctrl`이 없어도 라틴 글자로 맞춘다(한글 입력 소스에서도 같은 키, `app/copy_cmd.rs`).
/// 마지막의 `catch_all=ignore`가 묶이지 않은 키를 먹어 프로그램에 가지 않게 한다
pub(crate) fn default_table() -> Vec<(&'static str, &'static str)> {
    let mut v = vec![
        ("h", "copy_mode:left"),
        ("arrow_left", "copy_mode:left"),
        ("l", "copy_mode:right"),
        ("arrow_right", "copy_mode:right"),
        ("k", "copy_mode:up"),
        ("arrow_up", "copy_mode:up"),
        ("j", "copy_mode:down"),
        ("arrow_down", "copy_mode:down"),
        ("0", "copy_mode:line_start"),
        ("home", "copy_mode:line_start"),
        ("shift+4", "copy_mode:line_end"),
        ("end", "copy_mode:line_end"),
        ("w", "copy_mode:word_forward"),
        ("tab", "copy_mode:word_forward"),
        ("alt+f", "copy_mode:word_forward"),
        ("alt+arrow_right", "copy_mode:word_forward"),
        ("b", "copy_mode:word_backward"),
        ("shift+tab", "copy_mode:word_backward"),
        ("alt+b", "copy_mode:word_backward"),
        ("alt+arrow_left", "copy_mode:word_backward"),
        ("e", "copy_mode:word_end"),
        ("shift+6", "copy_mode:line_content_start"),
        ("alt+m", "copy_mode:line_content_start"),
        ("shift+h", "copy_mode:viewport_top"),
        ("shift+m", "copy_mode:viewport_middle"),
        ("shift+l", "copy_mode:viewport_bottom"),
        ("g", "copy_mode:top"),
        ("shift+g", "copy_mode:bottom"),
        ("ctrl+u", "copy_mode:half_page_up"),
        ("ctrl+d", "copy_mode:half_page_down"),
        ("ctrl+b", "copy_mode:page_up"),
        ("page_up", "copy_mode:page_up"),
        ("ctrl+f", "copy_mode:page_down"),
        ("page_down", "copy_mode:page_down"),
        ("enter", "copy_mode:next_line"),
        ("o", "copy_mode:other_end"),
        ("shift+o", "copy_mode:other_end_horiz"),
        ("f", "copy_mode:jump_forward"),
        ("shift+f", "copy_mode:jump_backward"),
        ("t", "copy_mode:jump_till_forward"),
        ("shift+t", "copy_mode:jump_till_backward"),
        ("semicolon", "copy_mode:jump_again"),
        ("comma", "copy_mode:jump_reverse"),
        ("slash", "copy_mode:search_forward"),
        ("shift+slash", "copy_mode:search_backward"),
        ("n", "copy_mode:search_next"),
        ("shift+n", "copy_mode:search_prev"),
        ("v", "copy_mode:select"),
        ("shift+v", "copy_mode:select_line"),
        ("ctrl+v", "copy_mode:select_block"),
        ("y", "copy_mode:copy"),
        ("escape", "copy_mode:exit"),
        ("q", "copy_mode:exit"),
        ("ctrl+c", "copy_mode:exit"),
        ("ctrl+g", "copy_mode:exit"),
    ];
    // 시스템 복사 키도 y와 같다(고른 것이 없으면 아무 일도 하지 않고 모드는 그대로 두지 않고 나간다)
    v.push((
        if cfg!(target_os = "macos") {
            "super+c"
        } else {
            "ctrl+shift+c"
        },
        "copy_mode:copy",
    ));
    v.push(("catch_all", "ignore"));
    v
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::keymap::{Action, Keymap, Sequencer, parse_trigger};

    fn entered() -> (Keymap, Sequencer) {
        let (km, warnings) = Keymap::build(&[]);
        assert!(warnings.is_empty(), "{warnings:?}");
        let mut sq = Sequencer::default();
        assert!(
            sq.apply_table_action(&km, &Action::ActivateKeyTable(TABLE.into())).0,
            "기본 묶음에 copy_mode 테이블이 있다"
        );
        (km, sq)
    }

    fn press(km: &Keymap, sq: &mut Sequencer, key: &str) -> crate::keymap::KeyOutcome {
        // 눌러 PTY에 갈 바이트가 있는 것처럼 비어 있지 않은 바이트를 준다
        sq.feed(km, Some(parse_trigger(key).unwrap()), key.as_bytes().to_vec())
    }

    #[test]
    fn names_round_trip_and_unknown_names_are_errors() {
        for (n, a) in NAMES {
            assert_eq!(parse(Some(n)), Ok(*a));
        }
        assert!(parse(None).is_err());
        assert!(parse(Some("nope")).is_err());
        assert_eq!(parse(Some(" left ")), Ok(CopyAction::Op(CopyOp::Left)));
    }

    #[test]
    fn every_default_key_does_what_the_table_says() {
        let (km, mut sq) = entered();
        for (key, act) in default_table() {
            if key == "catch_all" {
                continue;
            }
            let out = press(&km, &mut sq, key);
            let want = parse_action_of(act);
            assert_eq!(out.action, Some(want), "{key}");
            assert!(out.send.is_empty(), "{key}: 키가 프로그램으로 새면 안 된다");
        }
    }

    fn parse_action_of(s: &str) -> Action {
        crate::keymap::parse_action(s).unwrap().unwrap()
    }

    /// 복사 모드 중에는 어떤 키도 PTY에 바이트를 보내지 않는다(묶인 키도, 묶이지 않은 키도, 수식 키 조합도)
    #[test]
    fn no_key_reaches_the_pty_while_the_mode_is_on() {
        let (km, mut sq) = entered();
        let keys = [
            "a",
            "z",
            "x",
            "w",
            "b",
            "e",
            "p",
            "i",
            "o",
            "d",
            "u",
            "f",
            "n",
            "r",
            "s",
            "t",
            "m",
            "c",
            "1",
            "2",
            "9",
            "space",
            "enter",
            "tab",
            "backspace",
            "delete",
            "f1",
            "f5",
            "insert",
            "shift+a",
            "shift+z",
            "ctrl+a",
            "ctrl+z",
            "ctrl+x",
            "ctrl+r",
            "alt+x",
            "alt+f",
            "ctrl+alt+a",
            "shift+enter",
            "ctrl+shift+a",
            "slash",
            "shift+slash",
            "period",
            "comma",
            "bracket_left",
            "ctrl+bracket_left",
        ];
        for k in keys {
            let out = press(&km, &mut sq, k);
            assert!(out.send.is_empty(), "{k}: PTY로 {:?}", out.send);
        }
        // 묶이지 않은 키는 아무 동작도 하지 않고 먹힌다
        for k in ["a", "space", "ctrl+a", "f1", "alt+x"] {
            let out = press(&km, &mut sq, k);
            assert_eq!(out.action, Some(Action::Ignore), "{k}");
        }
    }

    #[test]
    fn word_and_viewport_keys_are_bound_like_wezterm_and_the_ones_it_lacks_are_not() {
        let (km, mut sq) = entered();
        for (key, name) in [
            ("w", "word_forward"),
            ("b", "word_backward"),
            ("e", "word_end"),
            ("tab", "word_forward"),
            ("shift+tab", "word_backward"),
            ("shift+6", "line_content_start"),
            ("shift+h", "viewport_top"),
            ("shift+m", "viewport_middle"),
            ("shift+l", "viewport_bottom"),
        ] {
            let out = press(&km, &mut sq, key);
            assert_eq!(out.action, Some(parse_action_of(&format!("copy_mode:{name}"))), "{key}");
        }
        // WezTerm 기본 묶음에 없는 W B E { }는 묶지 않는다(먹기만 한다)
        for key in [
            "shift+w",
            "shift+b",
            "shift+e",
            "shift+bracket_left",
            "shift+bracket_right",
        ] {
            assert_eq!(press(&km, &mut sq, key).action, Some(Action::Ignore), "{key}");
        }
    }

    #[test]
    fn keys_pass_to_the_pty_again_after_the_table_is_off() {
        let (km, mut sq) = entered();
        assert!(sq.apply_table_action(&km, &Action::DeactivateKeyTable).0);
        let out = press(&km, &mut sq, "a");
        assert_eq!(out.send, b"a".to_vec());
        assert_eq!(out.action, None);
    }

    #[test]
    fn the_user_can_override_and_unbind_copy_mode_keys() {
        let (km, warnings) = Keymap::build(&[
            "copy_mode/h=copy_mode:right".into(),
            "copy_mode/q=unbind".into(),
            "copy_mode/n=copy_mode:down".into(),
        ]);
        assert!(warnings.is_empty(), "{warnings:?}");
        let mut sq = Sequencer::default();
        sq.apply_table_action(&km, &Action::ActivateKeyTable(TABLE.into()));
        assert_eq!(
            press(&km, &mut sq, "h").action,
            Some(parse_action_of("copy_mode:right"))
        );
        assert_eq!(press(&km, &mut sq, "n").action, Some(parse_action_of("copy_mode:down")));
        // 지운 키는 catch_all이 먹는다(프로그램에 가지 않는다)
        let out = press(&km, &mut sq, "q");
        assert!(out.send.is_empty());
        assert_eq!(out.action, Some(Action::Ignore));
    }

    #[test]
    fn the_table_is_gone_after_keybind_clear_and_the_entry_does_nothing() {
        let (km, _) = Keymap::build(&["clear".into()]);
        let mut sq = Sequencer::default();
        assert!(!sq.apply_table_action(&km, &Action::ActivateKeyTable(TABLE.into())).0);
    }

    #[test]
    fn the_entry_key_is_ctrl_shift_x_and_shadows_no_default() {
        let (km, _) = Keymap::build(&[]);
        let mut sq = Sequencer::default();
        let out = sq.feed(&km, Some(parse_trigger("ctrl+shift+x").unwrap()), vec![0x18]);
        assert_eq!(out.action, Some(Action::ActivateCopyMode));
        assert!(out.send.is_empty());
        // 다른 기본 묶음이 같은 키를 쓰지 않는다
        let uses: Vec<_> = crate::keymap::defaults()
            .into_iter()
            .filter(|(t, a)| (*t == "ctrl+shift+x" || *t == "super+x") && *a != "activate_copy_mode")
            .collect();
        assert!(uses.is_empty(), "{uses:?}");
    }

    #[test]
    fn search_jump_and_other_end_keys_are_bound_like_the_docs() {
        let (km, mut sq) = entered();
        for (key, name) in [
            ("slash", "search_forward"),
            ("shift+slash", "search_backward"),
            ("n", "search_next"),
            ("shift+n", "search_prev"),
            ("f", "jump_forward"),
            ("shift+f", "jump_backward"),
            ("t", "jump_till_forward"),
            ("shift+t", "jump_till_backward"),
            ("semicolon", "jump_again"),
            ("comma", "jump_reverse"),
            ("o", "other_end"),
            ("shift+o", "other_end_horiz"),
            ("enter", "next_line"),
        ] {
            let out = press(&km, &mut sq, key);
            assert_eq!(out.action, Some(parse_action_of(&format!("copy_mode:{name}"))), "{key}");
            assert!(out.send.is_empty(), "{key}");
        }
        assert_eq!(parse(Some("search_prev")), Ok(CopyAction::SearchNext { reverse: true }));
        assert_eq!(
            parse(Some("jump_till_backward")),
            Ok(CopyAction::Jump(JumpKind {
                forward: false,
                till: true
            }))
        );
    }

    /// 찾기 입력 중에 들어온 확정 글은 막대로만 간다. PTY(Drop이 아닌 곳)로 가는 길이 없다
    #[test]
    fn confirmed_text_in_copy_mode_goes_to_the_bar_or_the_jump_never_the_pty() {
        assert_eq!(route_text(true, false), TextRoute::Search);
        assert_eq!(route_text(true, true), TextRoute::Search);
        assert_eq!(route_text(false, true), TextRoute::Jump);
        assert_eq!(route_text(false, false), TextRoute::Drop);
    }
}
