//! #928: 트리거 설정 문법(`trigger = 정규식 => 동작;동작`). 동작은 highlight·notify·bell·attention뿐이고,
//! 글 보내기·명령 실행 같은 "화면 글이 입력이 되는" 이름은 이유와 함께 거절한다.

use bt_core::trigger::{Highlight, MAX_TRIGGERS, TriggerSpec};
use bt_core::{Config, Rgb};

#[test]
fn parses_each_action() {
    let t = TriggerSpec::parse("error: .* => notify").unwrap();
    assert_eq!(t.pattern, "error: .*");
    assert!(t.notify && !t.bell && !t.attention && t.highlight.is_none());
    let t = TriggerSpec::parse("BEL => bell;attention").unwrap();
    assert!(t.bell && t.attention && !t.notify);
    let t = TriggerSpec::parse("FAIL => highlight:fg=#ffffff,bg=#aa0000").unwrap();
    assert_eq!(
        t.highlight,
        Some(Highlight {
            fg: Some(Rgb::new(255, 255, 255)),
            bg: Some(Rgb::new(0xaa, 0, 0))
        })
    );
    // 색 하나만, X11 이름도, 동작 여러 개
    let t = TriggerSpec::parse("x => highlight:bg=red ; notify ; bell").unwrap();
    assert_eq!(t.highlight.unwrap().fg, None);
    assert!(t.highlight.unwrap().bg.is_some() && t.notify && t.bell);
}

#[test]
fn splits_at_the_last_arrow_and_trims_the_pattern() {
    let t = TriggerSpec::parse("a=>b => notify").unwrap();
    assert_eq!(t.pattern, "a=>b", "정규식 안의 =>는 그대로");
    let t = TriggerSpec::parse("  spaced  =>  bell ").unwrap();
    assert_eq!(t.pattern, "spaced");
}

#[test]
fn rejects_bad_shapes_with_a_reason() {
    for bad in [
        "no arrow",
        " => bell",                    // 빈 정규식
        "x =>",                        // 동작 없음
        "x => ;;",                     // 동작 없음
        "(unclosed => bell",           // 정규식 오류
        r"(a)\1 => bell",              // 역참조
        "x => highlight",              // 색 없음
        "x => highlight:",             // 색 없음
        "x => highlight:fg=notacolor", // 색 오류
        "x => highlight:fg=#fff,fg=#000",
        "x => highlight:underline=1",
        "x => notify:custom", // 값을 받지 않음
        "x => bell;bell",     // 중복
        "x => frobnicate",    // 모르는 동작
    ] {
        assert!(TriggerSpec::parse(bad).is_err(), "받으면 안 됨: {bad}");
    }
}

#[test]
fn input_producing_actions_are_refused_by_name() {
    for name in [
        "send",
        "send_text",
        "run",
        "run-command",
        "inject",
        "open",
        "profile",
        "coprocess",
        "script",
    ] {
        let e = TriggerSpec::parse(&format!("password: => {name}:hunter2")).unwrap_err();
        assert!(e.contains("지원하지 않습니다"), "{name}: {e}");
        assert!(e.contains("입력이나 명령"), "{name}: {e}");
    }
}

#[test]
fn round_trips_through_config_value() {
    for s in [
        "error: .* => highlight:fg=#ffffff,bg=#aa0000;notify;bell;attention",
        "x => bell",
        "x => highlight:bg=#112233",
    ] {
        let t = TriggerSpec::parse(s).unwrap();
        assert_eq!(TriggerSpec::parse(&t.to_config_value()).unwrap(), t, "{s}");
    }
}

#[test]
fn config_accumulates_clears_and_caps() {
    assert!(Config::default().triggers.is_empty());
    let c = Config::from_text(
        "trigger = a => bell\ntrigger = (bad => bell\ntrigger = b => run:rm\ntrigger = c => notify\n",
        true,
    );
    let pats: Vec<_> = c.triggers.iter().map(|t| t.pattern.as_str()).collect();
    assert_eq!(pats, ["a", "c"], "못 읽는 줄은 빼고 나머지는 받는다");
    assert_eq!(c.warnings.len(), 2, "{:?}", c.warnings);
    // 빈 값은 지금까지 쌓은 것을 비운다
    let c = Config::from_text("trigger = a => bell\ntrigger =\ntrigger = z => bell\n", true);
    assert_eq!(c.triggers.len(), 1);
    assert_eq!(c.triggers[0].pattern, "z");
    // 상한
    let mut text = String::new();
    for i in 0..MAX_TRIGGERS + 3 {
        text.push_str(&format!("trigger = p{i} => bell\n"));
    }
    let c = Config::from_text(&text, true);
    assert_eq!(c.triggers.len(), MAX_TRIGGERS);
    assert_eq!(c.warnings.len(), 3);
}

#[test]
fn config_dump_keeps_triggers() {
    let c = Config::from_text(
        "trigger = a => highlight:bg=#010203;notify\ntrigger = b => bell\n",
        true,
    );
    let dump = c.render();
    assert!(dump.contains("trigger = a => highlight:bg=#010203;notify"), "{dump}");
    let again = Config::from_text(&dump, true);
    assert_eq!(again.triggers, c.triggers);
}
