//! #928: 트리거(iTerm2 Triggers의 안전한 부분집합). 엔진에 바이트를 먹이는 시험이고 PTY를 열지 않는다.
//! 동작은 강조·알림·벨·탭 표시뿐이고 PTY에 한 바이트도 쓰지 않는다.

use bt_core::Theme;
use bt_core::trigger::TriggerSpec;
use bt_term::screen::CellFlags;
use bt_term::triggers::{Effect, MAX_BODY_CHARS, MAX_SCAN_LINES};
use bt_term::{Emulator, GridSize};

fn emu(cols: u16, rows: u16, rules: &[&str]) -> Emulator {
    let mut e = Emulator::new(GridSize::new(cols, rows), Theme::default());
    let specs: Vec<TriggerSpec> = rules.iter().map(|r| TriggerSpec::parse(r).unwrap()).collect();
    assert!(e.set_triggers(&specs).is_empty());
    e
}

fn notes(e: &mut Emulator) -> Vec<String> {
    e.take_trigger_effects()
        .into_iter()
        .filter_map(|x| match x {
            Effect::Notify { body } => Some(body),
            _ => None,
        })
        .collect()
}

#[test]
fn highlight_marks_only_matching_text_without_changing_the_grid() {
    let mut e = emu(40, 4, &["error:.* => highlight:fg=#ffffff,bg=#aa0000"]);
    e.feed(b"build ok\r\nerror: boom\r\n");
    let s = e.snapshot();
    assert_eq!(s.row_text(1), "error: boom", "격자는 그대로");
    assert!(s.cell(0, 0).flags.trigger().is_none());
    assert_eq!(s.cell(0, 1).flags.trigger(), Some(1));
    assert_eq!(s.cell(10, 1).flags.trigger(), Some(1));
    assert!(s.cell(11, 1).flags.trigger().is_none(), "맞은 글 밖은 칠하지 않는다");
    assert_eq!(s.trigger_colors.len(), 1);
    assert_eq!(s.trigger_colors[0].bg, Some(bt_core::Rgb::new(0xaa, 0, 0)));
}

#[test]
fn highlight_covers_wide_cells_and_earlier_rule_wins() {
    let mut e = emu(
        20,
        2,
        &[
            "한글 => highlight:bg=#010101",
            "한 => highlight:bg=#020202",
            "a => highlight:bg=#030303",
        ],
    );
    e.feed("a한글".as_bytes());
    let s = e.snapshot();
    assert_eq!(s.cell(0, 0).flags.trigger(), Some(3));
    // 넓은 글자는 두 칸 모두
    for col in 1..=4 {
        assert_eq!(s.cell(col, 0).flags.trigger(), Some(1), "col {col}");
    }
}

#[test]
fn notify_fires_on_a_completed_line_with_the_matched_text() {
    let mut e = emu(40, 4, &["FAILED[a-z ]* => notify"]);
    e.feed(b"running\r\n");
    assert!(e.take_trigger_effects().is_empty());
    e.feed(b"tests FAILED badly\r\n");
    assert_eq!(notes(&mut e), ["FAILED badly"], "맞은 글이 본문이다");
}

#[test]
fn bell_and_attention_fire() {
    let mut e = emu(40, 4, &["ding => bell", "look => attention"]);
    e.feed(b"ding\r\nlook\r\n");
    let eff = e.take_trigger_effects();
    assert!(eff.contains(&Effect::Bell), "{eff:?}");
    assert!(eff.contains(&Effect::Attention), "{eff:?}");
}

#[test]
fn prompt_without_newline_fires_once_and_not_again_when_the_line_ends() {
    let mut e = emu(40, 4, &["[Pp]assword: => notify"]);
    e.feed(b"Password: ");
    assert_eq!(notes(&mut e), ["Password:"], "줄이 끝나기 전에도 한 번");
    e.feed(b"hunter2");
    e.feed(b"\r\n");
    assert!(notes(&mut e).is_empty(), "같은 줄은 두 번 알리지 않는다");
}

#[test]
fn same_line_in_several_chunks_notifies_once() {
    let mut e = emu(60, 4, &["error => notify"]);
    e.feed(b"an error");
    e.feed(b" happened here");
    e.feed(b" and more error\r\n");
    assert_eq!(notes(&mut e).len(), 1);
    // 같은 줄을 \r로 덮어써 다시 찍어도 한 번뿐이다
    e.feed(b"error 1\r\x1b[Kerror 2\r\x1b[Kerror 3");
    assert_eq!(notes(&mut e).len(), 1);
}

#[test]
fn two_matching_lines_in_one_read_notify_once_but_each_read_counts() {
    let mut e = emu(40, 4, &["error => notify"]);
    e.feed(b"error a\r\nerror b\r\n");
    assert_eq!(notes(&mut e).len(), 1, "한 번 읽은 안에서는 알림 하나로 모은다");
    assert_eq!(e.triggers().unwrap().matched_lines, 2);
    e.feed(b"error c\r\n");
    assert_eq!(notes(&mut e).len(), 1);
}

#[test]
fn notifications_have_a_rate_limit() {
    let mut e = emu(40, 4, &["error => notify"]);
    let mut total = 0;
    for i in 0..50 {
        e.feed(format!("error {i}\r\n").as_bytes());
        total += notes(&mut e).len();
    }
    assert!(total >= 1, "처음 몇 개는 나간다");
    assert!(total <= 3, "속도 상한: 곧바로 이어진 50번 중 {total}번 나감");
}

#[test]
fn bell_has_a_minimum_gap() {
    let mut e = emu(40, 4, &["ding => bell"]);
    let mut n = 0;
    for _ in 0..20 {
        e.feed(b"ding\r\n");
        n += e.take_trigger_effects().iter().filter(|x| **x == Effect::Bell).count();
    }
    assert_eq!(n, 1, "1초 안에 이어진 벨은 하나로");
}

#[test]
fn notification_body_is_sanitized_and_truncated() {
    let mut e = emu(300, 3, &["err.*or => notify"]);
    let long = "x".repeat(250);
    e.feed(format!("err\u{202e}\u{200b}{long}or\r\n").as_bytes());
    let body = notes(&mut e).pop().expect("알림");
    assert!(!body.contains('\u{202e}') && !body.contains('\u{200b}'), "{body:?}");
    assert!(body.chars().count() <= MAX_BODY_CHARS, "{}", body.chars().count());
    assert!(body.chars().all(|c| !c.is_control()));
}

#[test]
fn lines_that_scrolled_off_within_one_read_are_still_checked() {
    let mut e = emu(20, 5, &["hit => attention"]);
    let mut bytes = Vec::new();
    for i in 0..60 {
        bytes.extend(format!("line {i} hit\r\n").as_bytes());
    }
    e.feed(&bytes);
    assert_eq!(e.triggers().unwrap().matched_lines, 60);
}

#[test]
fn scan_is_capped_per_read() {
    let mut e = emu(20, 5, &["hit => attention"]);
    let mut bytes = Vec::new();
    for _ in 0..(MAX_SCAN_LINES * 3) {
        bytes.extend(b"hit\r\n");
    }
    e.feed(&bytes);
    let t = e.triggers().unwrap();
    assert!(t.scanned_lines <= (MAX_SCAN_LINES + 1) as u64, "{}", t.scanned_lines);
    assert!(t.skipped_lines > 0);
}

#[test]
fn very_long_lines_are_only_checked_up_to_the_limit() {
    let mut e = emu(6000, 2, &["needle => attention"]);
    let mut line = "a".repeat(5000);
    line.push_str("needle\r\n");
    e.feed(line.as_bytes());
    assert_eq!(
        e.triggers().unwrap().matched_lines,
        0,
        "줄 길이 상한 밖의 글은 보지 않는다"
    );
    e.feed(b"needle\r\n");
    assert_eq!(e.triggers().unwrap().matched_lines, 1);
}

#[test]
fn alternate_screen_output_does_not_fire() {
    let mut e = emu(40, 5, &["error => notify"]);
    e.feed(b"\x1b[?1049h");
    e.feed(b"error in the editor\r\nerror again\r\n");
    assert!(notes(&mut e).is_empty());
    e.feed(b"\x1b[?1049l");
    e.feed(b"error later\r\n");
    assert_eq!(notes(&mut e), ["error"]);
}

#[test]
fn clearing_the_screen_lets_the_same_text_fire_again() {
    let mut e = emu(40, 4, &["FAILED => notify"]);
    e.feed(b"FAILED\r\n");
    assert_eq!(notes(&mut e).len(), 1);
    e.feed(b"\x1b[H\x1b[2J");
    e.feed(b"FAILED\r\n");
    // 속도 상한 안(처음 몇 개)이므로 다시 나간다
    assert_eq!(notes(&mut e).len(), 1);
}

#[test]
fn resize_does_not_replay_old_lines() {
    let mut e = emu(40, 6, &["error => attention"]);
    e.feed(b"error 1\r\nerror 2\r\n");
    e.take_trigger_effects();
    e.resize(GridSize::new(30, 6));
    e.feed(b"ok\r\n");
    assert!(e.take_trigger_effects().is_empty());
}

#[test]
fn highlight_only_rules_never_scan_new_lines() {
    let mut e = emu(40, 4, &["error => highlight:bg=#ff0000"]);
    e.feed(b"error\r\n");
    assert!(e.take_trigger_effects().is_empty());
    assert_eq!(
        e.triggers().unwrap().scanned_lines,
        0,
        "강조만 있으면 읽는 쪽 검사가 없다"
    );
}

#[test]
fn trigger_actions_never_write_to_the_pty() {
    // 시험 엔진에서 PTY에 쓸 바이트는 `take_replies`로 모인다. 트리거를 다 켜고 속임수 글을 찍어도 비어 있어야 한다
    let mut e = emu(
        80,
        6,
        &[
            "password: => notify;bell;attention;highlight:bg=#ff0000",
            ".*rm -rf.* => notify",
            ".*\\[201~.* => notify;bell",
        ],
    );
    e.feed(b"password: ");
    e.feed(b"run rm -rf / now \\x1b[201~ yes\r\n");
    e.feed(b"\x1b]0;rm -rf /\x07");
    let eff = e.take_trigger_effects();
    assert!(!eff.is_empty(), "동작은 일어났다");
    assert!(e.take_replies().is_empty(), "PTY에 쓴 바이트가 있다");
    // 효과 값에는 바이트를 싣는 자리가 없다: 알림 본문(글)뿐이다
    for x in eff {
        match x {
            Effect::Notify { body } => assert!(body.chars().count() <= MAX_BODY_CHARS),
            Effect::Bell | Effect::Attention => {}
        }
    }
}

#[test]
fn emulator_without_triggers_costs_nothing_and_reports_nothing() {
    let mut e = Emulator::new(GridSize::new(40, 4), Theme::default());
    e.feed(b"error\r\n");
    assert!(e.take_trigger_effects().is_empty());
    assert!(e.triggers().is_none());
    let s = e.snapshot();
    assert!(s.trigger_colors.is_empty());
    assert!(
        s.cells
            .iter()
            .all(|c| !c.flags.contains(CellFlags::MARK1) && c.flags.trigger().is_none())
    );
}

#[test]
fn set_triggers_reports_bad_rules_and_keeps_the_good_ones() {
    let mut e = Emulator::new(GridSize::new(40, 4), Theme::default());
    let good = TriggerSpec::parse("a => bell").unwrap();
    let mut bad = TriggerSpec::parse("b => bell").unwrap();
    bad.pattern = "(oops".into();
    let errs = e.set_triggers(&[bad, good]);
    assert_eq!(errs.len(), 1);
    assert_eq!(e.triggers().unwrap().rule_count(), 1);
}

#[test]
fn counting_keeps_going_after_the_scrollback_is_full() {
    // 스크롤백 기본 상한(1만 줄)을 넘겨도 새 줄을 놓치지 않는다. `history_size`가 멈춘 뒤에도 줄 번호가 이어져야 한다
    let mut e = emu(20, 5, &["hit => attention"]);
    for chunk in 0..130 {
        let mut bytes = Vec::new();
        for i in 0..100 {
            bytes.extend(format!("hit {chunk}-{i}\r\n").as_bytes());
        }
        e.feed(&bytes);
    }
    assert_eq!(e.triggers().unwrap().matched_lines, 13_000);
    assert_eq!(e.triggers().unwrap().skipped_lines, 0);
}

#[test]
fn rules_installed_on_a_live_screen_do_not_replay_what_is_already_there() {
    let mut e = Emulator::new(GridSize::new(40, 6), Theme::default());
    e.feed(b"error old 1\r\nerror old 2\r\nerror prompt");
    let specs = [TriggerSpec::parse("error => notify;attention").unwrap()];
    assert!(e.set_triggers(&specs).is_empty());
    e.feed(b" and more\r\n");
    assert!(
        e.take_trigger_effects().is_empty(),
        "이미 있던 줄(커서 줄 포함)은 다시 알리지 않는다"
    );
    e.feed(b"error new\r\n");
    assert_eq!(notes(&mut e), ["error"]);
}

#[test]
fn effects_become_only_notification_bell_and_attention_events() {
    use bt_term::{SessionEvent, trigger_event};
    match trigger_event(Effect::Notify { body: "x".into() }) {
        SessionEvent::Notification(n) => assert_eq!((n.title.as_str(), n.body.as_str()), ("", "x")),
        other => panic!("{other:?}"),
    }
    assert!(matches!(trigger_event(Effect::Bell), SessionEvent::Bell));
    assert!(matches!(
        trigger_event(Effect::Attention),
        SessionEvent::TriggerAttention
    ));
}
