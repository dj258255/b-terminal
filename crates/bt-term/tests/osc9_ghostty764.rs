//! #764: OSC 9 하위 명령 분기를 Ghostty와 맞춘다.
//! 기준: Ghostty v1.4.0 `src/terminal/osc/parsers/osc9.zig`의 시험(같은 입력, 같은 기대), `report_pwd.zig`("OSC 7: report pwd empty"),
//! `kitty_desktop_notification.zig`의 `parse`(둘째 `;`가 없으면 무효), `stream.zig` `oscDispatch`(2825줄~: 앱이 어떤 명령을 쓰고 버리는지).
//! Ghostty가 `conemu_*`로 읽고 앱이 "unimplemented OSC callback"으로 버리는 명령은 여기서 신호가 없는 것(`Ignore`)이다.
//! 진행 값은 Ghostty의 `progress: ?u8`처럼 `Option<u8>`이다.
//! 시험 이름 뒤 괄호는 osc9.zig의 시험 이름이다.

use bt_term::notify::{PromptMark, Scanner, Signal};
use bt_term::osc133;

#[derive(Debug, PartialEq)]
enum Got {
    Ignore,
    Notify(String),
    Prompt,
    Progress(u8, Option<u8>),
    Cwd(String),
    CwdReset,
}

/// `ESC ] 입력 BEL`을 먹여 나온 신호를 Ghostty 쪽 분류로 바꾼다
fn osc(input: &str) -> Got {
    let sigs = Scanner::default().feed(format!("\x1b]{input}\x07").as_bytes());
    match sigs.as_slice() {
        [] => Got::Ignore,
        [Signal::Notify(n)] => {
            assert_eq!(n.title, "", "OSC 9 알림은 제목이 없다: {input}");
            Got::Notify(n.body.clone())
        }
        [Signal::Semantic(_), Signal::Prompt(PromptMark::PromptStart)] => Got::Prompt,
        [Signal::Progress { state, percent }] => Got::Progress(*state, *percent),
        [Signal::Cwd { path, .. }] => Got::Cwd(path.display().to_string()),
        [Signal::CwdReset] => Got::CwdReset,
        other => panic!("{input:?}: 뜻밖의 신호 {other:?}"),
    }
}

fn notify(s: &str) -> Got {
    Got::Notify(s.into())
}

#[test]
fn show_desktop_notification() {
    // "OSC 9: show desktop notification", "OSC 9: show single character desktop notification"
    assert_eq!(osc("9;Hello world"), notify("Hello world"));
    assert_eq!(osc("9;H"), notify("H"));
}

#[test]
fn sleep_9_1() {
    // "OSC 9;1: ConEmu sleep"(앱은 버린다), "...no value default", "...cannot exceed", "...invalid input"
    for i in ["9;1;420", "9;1;", "9;1;12345", "9;1;foo", "9;1;4_2"] {
        assert_eq!(osc(i), Got::Ignore, "{i}");
    }
    // "conemu sleep -> desktop notification 1·2"
    assert_eq!(osc("9;1"), notify("1"));
    assert_eq!(osc("9;1a"), notify("1a"));
}

#[test]
fn message_box_9_2() {
    // "OSC 9;2: ConEmu message box"(+ invalid input, empty message, spaces only, -> desktop notification 1·2)
    for i in ["9;2;hello world", "9;2;", "9;2;   "] {
        assert_eq!(osc(i), Got::Ignore, "{i}");
    }
    assert_eq!(osc("9;2"), notify("2"));
    assert_eq!(osc("9;2a"), notify("2a"));
}

#[test]
fn change_tab_title_9_3() {
    // "OSC 9;3: ConEmu change tab title"(+ reset, spaces only, -> desktop notification 1·2)
    for i in ["9;3;foo bar", "9;3;", "9;3;   "] {
        assert_eq!(osc(i), Got::Ignore, "{i}");
    }
    assert_eq!(osc("9;3"), notify("3"));
    assert_eq!(osc("9;3a"), notify("3a"));
}

#[test]
fn progress_9_4_set() {
    // "OSC 9;4: ConEmu progress set"(100), "set overflow"(900 -> 100), "single digit", "double digit", "extra semicolon ignored"
    assert_eq!(osc("9;4;1;100"), Got::Progress(1, Some(100)));
    assert_eq!(osc("9;4;1;900"), Got::Progress(1, Some(100)));
    assert_eq!(osc("9;4;1;9"), Got::Progress(1, Some(9)));
    assert_eq!(osc("9;4;1;94"), Got::Progress(1, Some(94)));
    // 값 자리가 없으면 set은 0이다(Ghostty `.set => .progress = 0`)
    assert_eq!(osc("9;4;1"), Got::Progress(1, Some(0)));
    // 읽을 수 없는 값은 없음이다: "numeric fields reject digit separators"(`9;4;1;4_2`)
    assert_eq!(osc("9;4;1;4_2"), Got::Progress(1, None));
    assert_eq!(osc("9;4;1;"), Got::Progress(1, None));
    assert_eq!(osc("9;4;1;abc"), Got::Progress(1, None));
    assert_eq!(
        osc("9;4;1;99999999999999999999"),
        Got::Progress(1, None),
        "usize를 넘으면 오류라 없음"
    );
    // `9;4;1x`는 둘째 `;`가 아니라 값 자리를 읽지 않는다
    assert_eq!(osc("9;4;1x"), Got::Progress(1, Some(0)));
}

#[test]
fn progress_9_4_remove_error_pause_indeterminate() {
    // "remove with no progress", "remove with double semicolon", "remove ignores progress", "remove extra semicolon"
    for i in ["9;4;0", "9;4;0;", "9;4;0;;", "9;4;0;100", "9;4;0;100;"] {
        assert_eq!(osc(i), Got::Progress(0, None), "{i}");
    }
    // "progress error", "error with progress"
    assert_eq!(osc("9;4;2"), Got::Progress(2, None));
    assert_eq!(osc("9;4;2;100"), Got::Progress(2, Some(100)));
    // "progress pause", "pause with progress"
    assert_eq!(osc("9;4;4"), Got::Progress(4, None));
    assert_eq!(osc("9;4;4;100"), Got::Progress(4, Some(100)));
    // 끝 모름(3)은 값을 읽지 않는다
    assert_eq!(osc("9;4;3"), Got::Progress(3, None));
    assert_eq!(osc("9;4;3;50"), Got::Progress(3, None));
}

#[test]
fn progress_9_4_falls_back_to_notification() {
    // "progress -> desktop notification 1~4"
    assert_eq!(osc("9;4"), notify("4"));
    assert_eq!(osc("9;4;"), notify("4;"));
    assert_eq!(osc("9;4;5"), notify("4;5"));
    assert_eq!(osc("9;4;5a"), notify("4;5a"));
}

#[test]
fn wait_input_9_5() {
    // "OSC 9;5: ConEmu wait input", "wait ignores trailing characters"(`9;5;foo`). 첫 글자가 5면 뒤는 보지 않는다
    for i in ["9;5", "9;5;foo", "9;5a", "9;50"] {
        assert_eq!(osc(i), Got::Ignore, "{i}");
    }
}

#[test]
fn guimacro_run_process_env_6_7_8() {
    // "OSC 9;6 guimacro 1·2·3 incomplete", "9;7 run process 1·2·incomplete", "9;8 output environment variable 1·2·incomplete"
    for n in ["6", "7", "8"] {
        for tail in [";a", ";ab", ";"] {
            assert_eq!(osc(&format!("9;{n}{tail}")), Got::Ignore, "9;{n}{tail}");
        }
        assert_eq!(osc(&format!("9;{n}")), notify(n));
        assert_eq!(osc(&format!("9;{n}a")), notify(&format!("{n}a")));
    }
}

#[test]
fn cwd_9_9() {
    // "OSC 9;9: ConEmu set current working directory"는 `report_pwd`다(앱이 OSC 7과 같은 길로 읽는다). `ab`는 URL이 아니라 OSC 7처럼 버린다
    assert_eq!(osc("9;9;ab"), Got::Ignore);
    assert_eq!(osc("9;9;file://localhost/tmp/x"), Got::Cwd("/tmp/x".into()));
    // 빈 값은 알려진 작업 폴더를 지운다(`reportPwd`의 빈 URL)
    assert_eq!(osc("9;9;"), Got::CwdReset);
    // "incomplete -> desktop notification"
    assert_eq!(osc("9;9"), notify("9"));
    assert_eq!(osc("9;9a"), notify("9a"));
}

#[test]
fn xterm_emulation_9_10() {
    // "OSC 9;10: ConEmu xterm keyboard and output emulation 1~5"
    for i in ["9;10", "9;10;0", "9;10;1", "9;10;2", "9;10;3"] {
        assert_eq!(osc(i), Got::Ignore, "{i}");
    }
    // "... 6·7·8"
    assert_eq!(osc("9;10;4"), notify("10;4"));
    assert_eq!(osc("9;10;"), notify("10;"));
    assert_eq!(osc("9;10;abc"), notify("10;abc"));
    assert_eq!(osc("9;100"), notify("100"));
}

#[test]
fn comment_9_11() {
    // "OSC 9;11: ConEmu comment"
    assert_eq!(osc("9;11;ab"), Got::Ignore);
    // "comment incomplete -> desktop notification"
    assert_eq!(osc("9;11"), notify("11"));
    assert_eq!(osc("9;11a"), notify("11a"));
}

#[test]
fn prompt_start_9_12() {
    // "OSC 9;12: ConEmu mark prompt start 1·2"(`9;12`, `9;12;abc`). 둘째 `;` 뒤는 보지 않는다(osc9.zig는 `'2'` 가지에서 바로 돌려준다)
    // `9;123456;x`도 프롬프트다: osc9.zig는 `1` 다음 `2`면 뒤를 보지 않고 돌려준다(#764 본문 표는 알림이라 적었지만 소스와 다르다)
    for i in ["9;12", "9;12;abc", "9;12abc", "9;123456;x"] {
        assert_eq!(osc(i), Got::Prompt, "{i}");
    }
    // `9;12`는 `133;A`와 같은 신호(엔진의 의미 있는 프롬프트 + 알림 쪽 `Prompt`)를 낸다
    let a = Scanner::default().feed(b"\x1b]133;A\x07");
    let b = Scanner::default().feed(b"\x1b]9;12\x07");
    assert_eq!(a, b);
    assert_eq!(a[0], Signal::Semantic(osc133::parse("A").unwrap()));
}

#[test]
fn other_numbers_are_notifications() {
    // ConEmu 하위 명령 번호가 아니거나 형식이 맞지 않으면 알림이다(iTerm2 알림). 번호 뒤 `;`가 있어도 같다
    assert_eq!(osc("9;42;hi"), notify("42;hi"));
    assert_eq!(osc("9;2025-11-23;배포"), notify("2025-11-23;배포"));
    assert_eq!(osc("9;13"), notify("13"));
    assert_eq!(osc("9;0"), notify("0"));
    assert_eq!(osc("9;0;x"), notify("0;x"));
    assert_eq!(osc("9;1x;y"), notify("1x;y"));
}

#[test]
fn number_only_osc_7_has_no_command() {
    // Ghostty: `ESC ] 7 ST`는 `;`가 없어 명령이 없다(작업 폴더를 지우지 않는다). 지우는 것은 `7;`(report_pwd.zig "report pwd empty")뿐
    assert!(Scanner::default().feed(b"\x1b]7\x07").is_empty());
    assert!(Scanner::default().feed(b"\x1b]7\x1b\\").is_empty());
    assert_eq!(Scanner::default().feed(b"\x1b]7;\x07"), vec![Signal::CwdReset]);
}

#[test]
fn osc_99_needs_the_second_semicolon() {
    // kitty_desktop_notification.zig `parse`: 메타데이터와 페이로드 사이 `;`가 없으면 "missing semicolon before payload"로 무효
    for s in [&b"\x1b]99;foobar\x07"[..], b"\x1b]99;\x07", b"\x1b]99;i=1:d=0\x07"] {
        assert!(
            Scanner::default().feed(s).is_empty(),
            "{:?}",
            String::from_utf8_lossy(s)
        );
    }
    // 메타데이터가 비어도 `;`가 있으면 제목 알림이다
    let got = Scanner::default().feed(b"\x1b]99;;foobar\x07");
    assert!(
        matches!(got.as_slice(), [Signal::Notify(n)] if n.body.is_empty() && n.title == "foobar"),
        "{got:?}"
    );
}
