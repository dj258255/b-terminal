//! #775·#776: 8비트 C1 바이트(0x80~0x9F)를 Ghostty 1.4.0처럼 다룬다.
//!
//! #775: 바닥(Ground) 상태에서 UTF-8로 읽을 수 없는 C1 한 바이트는 U+FFFD 하나다(전에는 말없이 버렸다).
//! 기준: `src/terminal/stream.zig` 시험 `stream: ground state UTF-8-decoded C1 controls are ignored`(3764줄)와
//! `UTF8Decoder.zig`(거절하면 U+FFFD를 내고 그 바이트는 다시 읽는다). `C2 80`~`C2 9F`처럼 UTF-8로 풀린 C1은 버리고,
//! NBSP(`C2 A0`)와 `Ü`(`C3 9C`)는 찍는다.
//!
//! #776: APC·SOS·PM 문자열(`sos_pm_apc_string`)은 CAN·SUB·ESC 말고 C1로도 끝난다.
//! 기준: `src/terminal/parse_table.zig` 51~66줄의 "anywhere" 전이(0x80~0x8F·0x91~0x97·0x99·0x9A·0x9C는 Ground,
//! 0x9B는 CsiEntry, 0x90은 DcsEntry, 0x9D는 OscString, 0x98·0x9E·0x9F는 그 상태에 머묾), 0xA0~0xFF는 표에 없어 무시,
//! `stream.zig` 시험 `stream: apc bulk slice C1 ST`(5337줄)와 `stream_terminal.zig` 6781줄 시험.
//! 끝난 APC의 실행은 Ghostty 앱(`stream_handler.zig` `apcEnd`)대로 끝난 까닭(`terminated`)과 상관없이 한다.

use bt_core::Theme;
use bt_term::notify::{Scanner, Signal};
use bt_term::{Emulator, GridSize};

fn emu() -> Emulator {
    Emulator::new(GridSize::new(40, 6), Theme::default())
}

fn feed_all(b: &[u8]) -> Vec<Signal> {
    Scanner::default().feed(b)
}

/// 바이트를 먹이고 첫 줄 글과 커서 (열, 줄)을 돌려준다
fn run(chunks: &[&[u8]]) -> (String, (usize, usize)) {
    let mut e = emu();
    for c in chunks {
        e.feed(c);
    }
    let s = e.snapshot();
    (s.row_text(0), (s.cursor.col, s.cursor.row))
}

// ---- #775: 바닥 상태의 원시 C1 ----

#[test]
fn raw_c1_byte_in_ground_prints_one_replacement_char() {
    // 이슈의 실측: `A\x9bB`는 `AB`가 아니라 `A�B`
    assert_eq!(run(&[b"A\x9bB"]), ("A\u{FFFD}B".into(), (3, 0)));
    assert_eq!(
        run(&[b"A\x84B\x85C\x8dD\x88E"]),
        ("A\u{FFFD}B\u{FFFD}C\u{FFFD}D\u{FFFD}E".into(), (9, 0))
    );
}

#[test]
fn every_raw_c1_byte_prints_one_replacement_char() {
    for c in 0x80u8..=0x9f {
        // 한 번에 먹이기
        let got = run(&[&[b'A', b'B', c, b'C', b'D']]);
        assert_eq!(got, ("AB\u{FFFD}CD".into(), (5, 0)), "{c:#04x}");
        // 한 바이트씩 먹이기
        let bytes = [b'A', b'B', c, b'C', b'D'];
        let parts: Vec<&[u8]> = bytes.chunks(1).collect();
        assert_eq!(run(&parts), ("AB\u{FFFD}CD".into(), (5, 0)), "{c:#04x} 한 바이트씩");
        // 긴 글 한가운데(Ghostty 시험의 66바이트 꼴)
        let mut long = vec![b'A'; 66];
        long[32] = c;
        let mut e = Emulator::new(GridSize::new(80, 2), Theme::default());
        e.feed(&long);
        let s = e.snapshot();
        assert_eq!(
            s.row_text(0).chars().filter(|&ch| ch == '\u{FFFD}').count(),
            1,
            "{c:#04x}"
        );
        assert_eq!(s.row_text(0).chars().count(), 66, "{c:#04x}");
    }
}

#[test]
fn utf8_decoded_c1_is_still_dropped() {
    for c in 0x80u8..=0x9f {
        assert_eq!(run(&[&[b'A', 0xc2, c, b'B']]), ("AB".into(), (2, 0)), "C2 {c:#04x}");
        // 두 바이트 사이에서 조각이 나뉘어도 같다(부분 UTF-8 길)
        assert_eq!(
            run(&[&[b'A', 0xc2], &[c, b'B']]),
            ("AB".into(), (2, 0)),
            "C2 | {c:#04x}"
        );
    }
}

#[test]
fn nbsp_and_u_umlaut_still_print() {
    assert_eq!(run(&[b"A\xc2\xa0B"]), ("A\u{a0}B".into(), (3, 0)));
    assert_eq!(run(&[b"A\xc3\x9cB"]), ("A\u{dc}B".into(), (3, 0)));
    assert_eq!(run(&[b"A\xc3", b"\x9cB"]), ("A\u{dc}B".into(), (3, 0)));
}

#[test]
fn c1_inside_a_utf8_sequence_follows_the_ghostty_decoder() {
    // 이어지는 바이트로 맞으면 한 글자다: E2 82 9B는 U+209B
    assert_eq!(run(&[b"A\xe2\x82\x9bB"]), ("A\u{209b}B".into(), (3, 0)));
    // 시작 바이트 뒤에 올 수 없는 C1이면 시작 바이트는 U+FFFD 하나, 그 C1도 U+FFFD 하나(E0 뒤는 A0~BF만)
    assert_eq!(run(&[b"A\xe0\x9bB"]), ("A\u{FFFD}\u{FFFD}B".into(), (4, 0)));
    assert_eq!(run(&[b"A\xe0\x80\x9bB"]), ("A\u{FFFD}\u{FFFD}\u{FFFD}B".into(), (5, 0)));
    // 끊긴 시퀀스 뒤의 보통 글자는 그대로 찍는다
    assert_eq!(run(&[b"A\xe2\x82B"]), ("A\u{FFFD}B".into(), (3, 0)));
    assert_eq!(run(&[b"A\xf0\x9f\x9bB"]), ("A\u{FFFD}B".into(), (3, 0)));
    // 조각이 나뉜 곳에서도 같다
    assert_eq!(run(&[b"A\xe0", b"\x9bB"]), ("A\u{FFFD}\u{FFFD}B".into(), (4, 0)));
    assert_eq!(run(&[b"A\xe2\x82", b"\x9bB"]), ("A\u{209b}B".into(), (3, 0)));
}

// ---- #776: APC·SOS·PM을 끝내는 C1 ----

/// 문자열 시작 부호: APC·SOS·PM이 같은 상태다
const STARTS: [&[u8]; 3] = [b"\x1b_G", b"\x1bXs", b"\x1b^p"];

#[test]
fn raw_9c_ends_apc_sos_pm_in_the_engine() {
    // Ghostty `stream: apc bulk slice C1 ST`: `\x1b_Gpayload\x9c` 뒤는 문자열이 아니다
    for start in STARTS {
        let bytes = [start, b"payload\x9cAB"].concat();
        assert_eq!(run(&[&bytes]), ("AB".into(), (2, 0)), "{start:?}");
        // 조각이 나뉘어도 같다
        let (a, b) = bytes.split_at(start.len() + 3);
        assert_eq!(run(&[a, b]), ("AB".into(), (2, 0)), "{start:?} 나눔");
    }
}

#[test]
fn execute_type_c1_ends_the_string_and_runs_as_esc_final() {
    // 0x80~0x8F·0x91~0x97·0x99·0x9A는 Ground로 가며 `ESC (c-0x40)`처럼 실행된다(Ghostty `execute`의 C1 길).
    // 뒤의 `AB`는 찍힌다. 0x85(NEL)는 줄바꿈, 0x84(IND)는 아래 줄, 0x8d(RI)는 위로
    for c in (0x80u8..=0x8f).chain(0x91..=0x97).chain([0x99, 0x9a]) {
        let mut e = emu();
        e.feed(b"\x1b[3;1H");
        e.feed(&[b"\x1b_Gx".as_slice(), &[c], b"AB"].concat());
        let s = e.snapshot();
        assert!(s.text().contains("AB"), "{c:#04x}: {:?}", s.text());
    }
    // 실행 결과: NEL은 다음 줄 맨 앞, IND는 같은 열 아래 줄, HTS는 현재 열에 탭 정지
    let mut e = emu();
    e.feed(b"\x1b_Gx\x85AB");
    assert_eq!(e.snapshot().row_text(1), "AB");
    let mut e = emu();
    e.feed(b"ab\x1b_Gx\x84AB");
    let s = e.snapshot();
    assert_eq!((s.row_text(0), s.row_text(1)), ("ab".into(), "  AB".into()));
}

#[test]
fn c1_csi_dcs_osc_introducers_end_the_string_and_start_theirs() {
    // 0x9B: CsiEntry. `3;4H`가 커서 이동이고 `X`가 그 자리에 찍힌다
    let mut e = emu();
    e.feed(b"\x1b_Gx\x9b3;4HX");
    let s = e.snapshot();
    assert_eq!(s.row_text(2), "   X");
    // 0x90: DcsEntry. `qabc`는 DCS 통과라 화면에 안 찍히고 ST 뒤가 찍힌다
    assert_eq!(run(&[b"\x1b_Gx\x90qabc\x1b\\AB"]), ("AB".into(), (2, 0)));
    // 0x9D: OscString. BEL까지가 OSC라 찍히지 않고 그 뒤가 찍힌다
    assert_eq!(run(&[b"\x1b_Gx\x9d0;title\x07AB"]), ("AB".into(), (2, 0)));
}

#[test]
fn c1_that_stay_in_the_string_do_not_end_it() {
    // 0x98·0x9E·0x9F(문자열 시작 C1)와 0xA0~0xFF는 표에서 머묾이거나 무시다. 문자열이 이어져 `AB`는 찍히지 않는다
    for c in [0x98u8, 0x9e, 0x9f].into_iter().chain(0xa0..=0xff) {
        let (text, cur) = run(&[&[b"\x1b_Gx".as_slice(), &[c], b"AB"].concat()]);
        assert_eq!((text.as_str(), cur), ("", (0, 0)), "{c:#04x}");
        // 그 뒤 `ESC \`로 끝나면 이어서 찍는다
        let (text, _) = run(&[&[b"\x1b_Gx".as_slice(), &[c], b"AB\x1b\\CD"].concat()]);
        assert_eq!(text, "CD", "{c:#04x}");
    }
}

#[test]
fn bel_is_still_payload() {
    // Ghostty `stream: apc bulk slice keeps C0 bytes as data`: BEL은 자료다
    assert_eq!(run(&[b"\x1b_Gx\x07y\x1b\\AB"]), ("AB".into(), (2, 0)));
    assert_eq!(run(&[b"\x1b_Gx\x07yAB"]), ("".into(), (0, 0)));
}

// ---- #776: 신호 스캐너 ----

#[test]
fn scanner_emits_the_graphics_command_when_9c_ends_it() {
    // Ghostty `stream: apc bulk slice C1 ST`: 자료는 `Gpayload`(앞 `G`는 kitty 표시)
    assert_eq!(
        feed_all(b"\x1b_Gpayload\x9c"),
        vec![Signal::Graphics(b"payload".to_vec())]
    );
    assert_eq!(
        feed_all(b"\x1b_Ga=q,i=1\x9c\x1b[c"),
        vec![Signal::Graphics(b"a=q,i=1".to_vec()), Signal::Da1]
    );
}

#[test]
fn scanner_ends_apc_on_every_ghostty_c1_end_and_runs_it_like_the_app() {
    // 앱(`stream_handler.zig` `apcEnd`)은 `terminated`와 상관없이 명령을 실행한다. CAN·SUB가 이미 그렇다(#595)
    for c in (0x80u8..=0x8f).chain(0x90..=0x97).chain([0x99, 0x9a, 0x9b, 0x9c, 0x9d]) {
        let got = feed_all(&[b"\x1b_Gab".as_slice(), &[c]].concat());
        assert_eq!(got, vec![Signal::Graphics(b"ab".to_vec())], "{c:#04x}");
    }
}

#[test]
fn scanner_keeps_reading_after_the_c1_like_the_engine() {
    // 0x9B 뒤는 CsiEntry: 엔진처럼 `c`가 DA1이다. 0x9C 뒤 `ESC [ c`도 읽는다
    assert_eq!(
        feed_all(b"\x1b_Ga\x9bc"),
        vec![Signal::Graphics(b"a".to_vec()), Signal::Da1]
    );
    // 0x9D 뒤는 OscString: 끝까지 OSC라 `c`는 질의가 아니다
    assert_eq!(feed_all(b"\x1b_Ga\x9d0;t\x07"), vec![Signal::Graphics(b"a".to_vec())]);
    // 0x90 뒤는 DcsEntry: `$qm`은 DECRQSS다
    // (#792: DCS는 0x9C가 아니라 `ESC \`로 끝난다)
    let got = feed_all(b"\x1b_Ga\x90$qm\x1b\\");
    assert_eq!(got.first(), Some(&Signal::Graphics(b"a".to_vec())), "{got:?}");
    assert_eq!(got.len(), 2, "{got:?}");
}

#[test]
fn scanner_does_not_end_on_staying_c1_and_ignores_them_as_data() {
    // 0x98·0x9E·0x9F·0xA0~0xFF는 문자열에 머물고 자료도 아니다(Ghostty `consumeApcString`이 ≥0x80을 표로 넘기고 표가 무시)
    for c in [0x98u8, 0x9e, 0x9f].into_iter().chain(0xa0..=0xff) {
        let got = feed_all(&[b"\x1b_Ga".as_slice(), &[c], b"b\x1b\\"].concat());
        assert_eq!(got, vec![Signal::Graphics(b"ab".to_vec())], "{c:#04x}");
    }
}

#[test]
fn scanner_sos_pm_end_on_c1_like_the_engine() {
    // SOS·PM은 엔진에서 APC와 같은 상태다. 끝난 뒤의 질의를 읽는다
    for start in [&b"\x1bX"[..], b"\x1b^"] {
        for end in [&b"\x9c"[..], b"\x85", b"\x9b"] {
            let mut v = start.to_vec();
            v.extend(b"junk");
            v.extend(end);
            v.extend(if end == b"\x9b" { &b"c"[..] } else { b"\x1b[c" });
            assert_eq!(feed_all(&v), vec![Signal::Da1], "{start:?} {end:?}");
        }
    }
}

#[test]
fn emulator_runs_a_kitty_query_ended_by_9c() {
    // 엔진과 스캐너가 같이 움직인다: `a=q` 질의가 0x9C로 끝나도 답이 온다
    let mut e = emu();
    let sigs = e.feed(b"\x1b_Gi=31,s=1,v=1,a=q,t=d,f=24;AAAA\x9c");
    assert!(sigs.iter().any(|s| matches!(s, Signal::Graphics(_))), "{sigs:?}");
    let replies = e.take_replies();
    assert!(String::from_utf8_lossy(&replies).contains("i=31"), "{replies:?}");
}
