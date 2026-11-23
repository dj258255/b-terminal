//! Ghostty v1.4.0 `src/input/Binding.zig`의 `chain=<동작>` 시험을 우리 해석기(`Keymap::build`)에 옮긴 것(#766):
//! `parse: chain`(3469줄), `set: parseAndPut chain`(4184줄), `... without parent is error`(4211줄), `... multiple times`(4222줄),
//! `... preserves flags`(4245줄), `... after unbind is error`(4265줄), `... on sequence`(4279줄), `... with unbind is error`(4303줄),
//! `set: appendChain removes reverse mapping`(4748줄), `... restores next valid reverse mapping`(4796줄),
//! `set: multiple leaves under leader updates chain_parent`(3721줄), `set: invalid format preserves chain_parent`(3805줄),
//! `set: direct remove clears chain_parent`(3789줄), Config.zig의 `parseCLI table chain syntax is invalid`(7977줄)·
//! `parseCLI chain applies to most recent table binding`(7992줄). 같은 입력과 같은 기대다. Ghostty의 오류(`InvalidFormat`)는 우리의 설정 경고다.
//! 앱이 하는 일(누르면 동작이 차례로 나오는가)은 `Sequencer`로 본다.

use super::*;

fn tr(s: &str) -> Trigger {
    parse_trigger(s).unwrap()
}

/// 기본 묶음을 지우고 `lines`를 차례로 얹는다
fn build(lines: &[&str]) -> (Keymap, Vec<String>) {
    let mut v = vec!["clear".to_string()];
    v.extend(lines.iter().map(|s| s.to_string()));
    Keymap::build(&v)
}

/// 키 하나를 눌렀을 때 차례로 나오는 동작(첫 동작, 이어 붙은 동작)
fn press(km: &Keymap, key: &str) -> Vec<Action> {
    let mut sq = Sequencer::default();
    let out = sq.feed(km, Some(tr(key)), key.as_bytes().to_vec());
    out.action.into_iter().chain(out.chained).collect()
}

fn root(km: &Keymap, key: &str) -> Vec<Action> {
    km.exact(&[tr(key)])
        .map(|b| std::iter::once(b.action.clone()).chain(b.more.clone()).collect())
        .unwrap_or_default()
}

/// 시험 20 `parse: chain`: 앞 묶음이 있으면 `chain=new_tab`은 된다. 접두사가 붙은 chain과 순서 안의 chain은 오류(경고)
#[test]
fn ghostty_parse_chain() {
    let (km, w) = build(&["a=new_window", "chain=new_tab"]);
    assert!(w.is_empty(), "{w:?}");
    assert_eq!(root(&km, "a"), vec![Action::NewWindow, Action::NewTab]);
    // Chain can't have flags
    let (km, w) = build(&["a=new_window", "global:chain=ignore"]);
    assert_eq!(w.len(), 1, "{w:?}");
    assert_eq!(root(&km, "a"), vec![Action::NewWindow]);
    assert!(km.global.is_empty());
    for line in [
        "all:chain=ignore",
        "unconsumed:chain=ignore",
        "performable:chain=ignore",
    ] {
        let (km, w) = build(&["a=new_window", line]);
        assert_eq!(w.len(), 1, "{line}: {w:?}");
        assert_eq!(root(&km, "a"), vec![Action::NewWindow], "{line}");
    }
    // Chain can't be part of a sequence
    let (km, w) = build(&["a=new_window", "a>chain=ignore"]);
    assert_eq!(w.len(), 1, "{w:?}");
    assert_eq!(root(&km, "a"), vec![Action::NewWindow]);
    assert!(km.seqs.is_empty());
}

/// 시험 49 `set: parseAndPut chain`: 앞 묶음에 덧붙고, 덧붙은 묶음은 되짚기에서 빠진다
#[test]
fn ghostty_set_parse_and_put_chain() {
    let (km, w) = build(&["a=new_window", "chain=new_tab"]);
    assert!(w.is_empty(), "{w:?}");
    assert_eq!(root(&km, "a"), vec![Action::NewWindow, Action::NewTab]);
    assert!(km.triggers_for(&Action::NewWindow).is_empty());
    assert!(km.triggers_for(&Action::NewTab).is_empty());
}

/// 시험 50 `set: parseAndPut chain without parent is error`
#[test]
fn ghostty_chain_without_parent_is_error() {
    let (km, w) = build(&["chain=new_tab"]);
    assert_eq!(w.len(), 1, "{w:?}");
    assert!(km.map.is_empty() && km.seqs.is_empty());
    // 기본 묶음만 있고 설정에 앞 묶음이 없어도 같다(기본 묶음에는 덧붙지 않는다)
    let (km, w) = Keymap::build(&["chain=new_tab".to_string()]);
    assert_eq!(w.len(), 1, "{w:?}");
    // 기본 새 탭 키(macOS는 super+t, 그 밖은 ctrl+shift+t)가 덧붙음 없이 그대로다
    let keys = km.triggers_for(&Action::NewTab);
    assert!(!keys.is_empty());
    for t in keys {
        assert_eq!(km.lookup(t), Some(&Action::NewTab));
    }
}

/// 시험 51 `set: parseAndPut chain multiple times`
#[test]
fn ghostty_chain_multiple_times() {
    let (km, w) = build(&["a=new_window", "chain=new_tab", "chain=close_surface"]);
    assert!(w.is_empty(), "{w:?}");
    assert_eq!(
        root(&km, "a"),
        vec![Action::NewWindow, Action::NewTab, Action::CloseSurface]
    );
}

/// 시험 52 `set: parseAndPut chain preserves flags`
#[test]
fn ghostty_chain_preserves_flags() {
    let (km, w) = build(&["unconsumed:a=new_window", "chain=new_tab"]);
    assert!(w.is_empty(), "{w:?}");
    assert!(km.flags(&[tr("a")]).unwrap().unconsumed);
    assert_eq!(root(&km, "a").len(), 2);
}

/// 시험 53 `set: parseAndPut chain after unbind is error`
#[test]
fn ghostty_chain_after_unbind_is_error() {
    let (km, w) = build(&["a=new_window", "a=unbind", "chain=new_tab"]);
    assert_eq!(w.len(), 1, "{w:?}");
    assert!(km.map.is_empty());
}

/// 시험 54 `set: parseAndPut chain on sequence`
#[test]
fn ghostty_chain_on_sequence() {
    let (km, w) = build(&["a>b=new_window", "chain=new_tab"]);
    assert!(w.is_empty(), "{w:?}");
    let b = km.exact(&[tr("a"), tr("b")]).unwrap();
    assert_eq!(b.action, Action::NewWindow);
    assert_eq!(b.more, vec![Action::NewTab]);
}

/// 시험 55 `set: parseAndPut chain with unbind is error`: 오류여도 원래 묶음은 그대로
#[test]
fn ghostty_chain_with_unbind_is_error() {
    let (km, w) = build(&["a=new_window", "chain=unbind"]);
    assert_eq!(w.len(), 1, "{w:?}");
    assert_eq!(root(&km, "a"), vec![Action::NewWindow]);
}

/// 시험 74~80 `appendChain`의 되짚기: 덧붙으면 그 키는 되짚기에서 빠지고 같은 동작의 앞 키가 돌아온다.
/// performable 묶음에 덧붙여도 다른 되짚기에 영향이 없다
#[test]
fn ghostty_append_chain_reverse_mapping() {
    let (km, _) = build(&["a=new_window", "b=new_window"]);
    assert_eq!(km.triggers_for(&Action::NewWindow).first(), Some(&&tr("b")));
    let (km, w) = build(&["a=new_window", "b=new_window", "chain=new_tab"]);
    assert!(w.is_empty(), "{w:?}");
    assert_eq!(km.triggers_for(&Action::NewWindow), vec![&tr("a")]);
    let (km, _) = build(&["b=new_window", "performable:a=close_surface", "chain=new_tab"]);
    assert_eq!(km.triggers_for(&Action::NewWindow), vec![&tr("b")]);
    assert!(km.triggers_for(&Action::CloseSurface).is_empty());
}

/// 시험 `multiple leaves under leader`: 앞 묶음은 가장 최근에 정한 키 하나다
#[test]
fn ghostty_chain_target_is_most_recent_binding() {
    let (km, w) = build(&["a>b=new_window", "a>c=new_tab", "chain=close_surface"]);
    assert!(w.is_empty(), "{w:?}");
    assert!(km.exact(&[tr("a"), tr("b")]).unwrap().more.is_empty());
    assert_eq!(km.exact(&[tr("a"), tr("c")]).unwrap().more, vec![Action::CloseSurface]);
}

/// 시험 `invalid format preserves chain_parent`: 읽지 못한 줄은 앞 묶음을 바꾸지 않는다
#[test]
fn ghostty_invalid_line_preserves_chain_target() {
    let (km, w) = build(&[
        "a=new_window",
        "a=invalid_action_xyz",
        "x+y+=new_tab",
        "chain=close_surface",
    ]);
    assert_eq!(w.len(), 2, "{w:?}");
    assert_eq!(root(&km, "a"), vec![Action::NewWindow, Action::CloseSurface]);
    // chain 줄 자신의 오류도 앞 묶음을 바꾸지 않는다
    let (km, w) = build(&["a=new_window", "chain=nope", "chain=unbind", "chain=new_tab"]);
    assert_eq!(w.len(), 2, "{w:?}");
    assert_eq!(root(&km, "a"), vec![Action::NewWindow, Action::NewTab]);
}

/// 시험 `sequence unbind clears chain_parent`·`direct remove clears chain_parent`·`clear`: 지우면 앞 묶음이 없다
#[test]
fn ghostty_unbind_and_clear_reset_chain_target() {
    let (_, w) = build(&["a>b=new_window", "a>b=unbind", "chain=new_tab"]);
    assert_eq!(w.len(), 1, "{w:?}");
    let (km, w) = build(&["a>b=new_window", "a>c=new_tab", "a>b=unbind", "chain=new_tab"]);
    assert_eq!(w.len(), 1, "{w:?}");
    assert_eq!(km.exact(&[tr("a"), tr("c")]).unwrap().more, Vec::<Action>::new());
    let (_, w) = build(&["a=new_window", "clear", "chain=new_tab"]);
    assert_eq!(w.len(), 1, "{w:?}");
    // 없는 키를 unbind해도 앞 묶음은 사라진다(Ghostty: 지우기는 늘 되짚기를 끊는다)
    let (_, w) = build(&["a=new_window", "z=unbind", "chain=new_tab"]);
    assert_eq!(w.len(), 1, "{w:?}");
}

/// Config.zig `parseCLI table chain syntax is invalid`: `foo/chain=…`은 오류. 오류는 앞 묶음을 바꾸지 않는다
#[test]
fn ghostty_table_chain_syntax_is_invalid() {
    let (km, w) = build(&["foo/a=text:hello", "foo/chain=deactivate_key_table"]);
    assert_eq!(w.len(), 1, "{w:?}");
    assert_eq!(km.tables["foo"].exact(&[tr("a")]).unwrap().more, Vec::<Action>::new());
    // 앞 묶음은 그대로라 그 뒤의 chain은 foo/a에 붙는다
    let (km, w) = build(&[
        "foo/a=text:hello",
        "foo/chain=deactivate_key_table",
        "chain=deactivate_key_table",
    ]);
    assert_eq!(w.len(), 1, "{w:?}");
    assert_eq!(
        km.tables["foo"].exact(&[tr("a")]).unwrap().more,
        vec![Action::DeactivateKeyTable]
    );
}

/// Config.zig `parseCLI chain applies to most recent table binding`: chain은 접두사 없이 가장 최근 묶음(어느 테이블이든)에 붙는다
#[test]
fn ghostty_chain_applies_to_most_recent_table_binding() {
    let (km, w) = build(&[
        "ctrl+n=activate_key_table:foo",
        "foo/a=text:hello",
        "chain=deactivate_key_table",
    ]);
    assert!(w.is_empty(), "{w:?}");
    assert_eq!(root(&km, "ctrl+n"), vec![Action::ActivateKeyTable("foo".into())]);
    let foo = km.tables["foo"].exact(&[tr("a")]).unwrap();
    assert_eq!(foo.action, Action::Text("hello".into()));
    assert_eq!(foo.more, vec![Action::DeactivateKeyTable]);
    // 테이블을 비우면(`foo/`) 앞 묶음은 없다
    let (_, w) = build(&["foo/a=text:hello", "foo/", "chain=new_tab"]);
    assert_eq!(w.len(), 1, "{w:?}");
}

/// 순서 묶음·전역 묶음·플래그 붙은 묶음에도 덧붙는다. 전역은 첫 동작 뒤에 차례로 둔다
#[test]
fn chain_on_global_binding() {
    let (km, w) = build(&["global:ctrl+a=new_window", "chain=new_tab"]);
    assert!(w.is_empty(), "{w:?}");
    assert_eq!(km.global, vec![(tr("ctrl+a"), Action::NewWindow)]);
    assert_eq!(km.global_more[&tr("ctrl+a")], vec![Action::NewTab]);
    // 같은 전역 키를 다시 정하면 덧붙인 것은 사라진다
    let (km, w) = build(&[
        "global:ctrl+a=new_window",
        "chain=new_tab",
        "global:ctrl+a=close_surface",
    ]);
    assert!(w.is_empty(), "{w:?}");
    assert!(km.global_more.get(&tr("ctrl+a")).is_none_or(Vec::is_empty));
    // 전역 키를 지우면 덧붙인 것도 사라지고 앞 묶음도 없다
    let (km, w) = build(&[
        "global:ctrl+a=new_window",
        "chain=new_tab",
        "global:ctrl+a=unbind",
        "chain=new_tab",
    ]);
    assert_eq!(w.len(), 1, "{w:?}");
    assert!(km.global.is_empty() && km.global_more.is_empty());
}

/// 누르면 동작이 차례로 나온다(순서 묶음 포함), 되짚기·catch_all=ignore도 덧붙은 동작을 본다
#[test]
fn press_runs_chained_actions_in_order() {
    let (km, w) = build(&["a=new_window", "chain=new_tab", "chain=close_surface"]);
    assert!(w.is_empty(), "{w:?}");
    assert_eq!(
        press(&km, "a"),
        vec![Action::NewWindow, Action::NewTab, Action::CloseSurface]
    );
    // 키 순서: 마지막 키에서 차례로
    let (km, _) = build(&["ctrl+a>n=new_window", "chain=new_tab"]);
    let mut sq = Sequencer::default();
    assert_eq!(sq.feed(&km, Some(tr("ctrl+a")), vec![1]), KeyOutcome::default());
    let out = sq.feed(&km, Some(tr("n")), b"n".to_vec());
    assert_eq!(out.action, Some(Action::NewWindow));
    assert_eq!(out.chained, vec![Action::NewTab]);
    // catch_all 묶음에 ignore가 덧붙어 있어도 순서를 버린다(Ghostty: 덧붙은 동작 가운데 하나라도 ignore면)
    let (km, _) = build(&["ctrl+a>n=new_window", "catch_all=new_tab", "chain=ignore"]);
    let mut sq = Sequencer::default();
    sq.feed(&km, Some(tr("ctrl+a")), vec![1]);
    assert_eq!(sq.feed(&km, Some(tr("x")), b"x".to_vec()), KeyOutcome::default());
    assert!(!sq.waiting());
}

/// performable 묶음은 덧붙은 동작 가운데 하나라도 할 수 있으면 키를 먹는다(Ghostty: 하나라도 했으면 performed)
#[test]
fn performable_chain_is_performed_if_any_action_can() {
    let (km, _) = build(&["performable:a=copy_to_clipboard", "chain=new_tab"]);
    // 복사는 못 하지만 new_tab은 할 수 있다: 키를 먹고 두 동작을 낸다
    let mut sq = Sequencer::default();
    let out = sq.feed_with(&km, Some(tr("a")), b"a".to_vec(), &mut |a| {
        !matches!(a, Action::Copy(_))
    });
    assert!(out.send.is_empty());
    assert_eq!(out.action.iter().chain(&out.chained).count(), 2);
    // 둘 다 못 하면 묶이지 않은 키처럼 프로그램에 보낸다
    let (km, _) = build(&["performable:a=copy_to_clipboard", "chain=paste_from_clipboard"]);
    let mut sq = Sequencer::default();
    let out = sq.feed_with(&km, Some(tr("a")), b"a".to_vec(), &mut |_| false);
    assert_eq!(out.send, b"a".to_vec());
    assert!(out.action.is_none() && out.chained.is_empty());
}

/// 키 테이블 동작도 덧붙일 수 있고 차례로 한다: 글을 보내고 테이블을 끈다
#[test]
fn chained_key_table_action_is_applied_in_order() {
    let (km, w) = build(&[
        "ctrl+n=activate_key_table:foo",
        "foo/a=text:hello",
        "chain=deactivate_key_table",
    ]);
    assert!(w.is_empty(), "{w:?}");
    let mut sq = Sequencer::default();
    let out = sq.feed(&km, Some(tr("ctrl+n")), vec![14]);
    assert!(out.action.is_none() && out.chained.is_empty());
    assert_eq!(sq.stack_names(), vec!["foo"]);
    let out = sq.feed(&km, Some(tr("a")), b"a".to_vec());
    assert_eq!(out.action, Some(Action::Text("hello".into())));
    assert!(out.chained.is_empty());
    assert!(sq.stack_names().is_empty());
}
