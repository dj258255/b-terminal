//! #598: 신호 스캐너(`Scanner::feed_at`)가 엔진(vte)과 같은 바이트를 다르게 읽던 틈.
//! 기준: vendor/vte `src/lib.rs`의 상태표(OSC 407~413줄, Escape 340~391줄, DCS 통과 317~337줄, SOS·PM 359·377줄)와
//! Ghostty `Parser.zig`(main 7fa26e8b) 276~281줄의 상태 나갈 때 동작.
//! 시험마다 엔진 쪽 사실을 vte 파서에 같은 바이트를 먹여 확인하고(`oracle`),
//! `Emulator::feed`에 같이 넣어 신호와 화면이 맞는지도 본다.

use alacritty_terminal::vte::{Params, Parser, Perform};
use bt_core::Theme;
use bt_term::notify::{PromptMark, Scanner, Signal};
use bt_term::{Emulator, GridSize};

/// 엔진이 본 일을 글로 적는다
#[derive(Default)]
struct Oracle(Vec<String>);

impl Perform for Oracle {
    fn print(&mut self, c: char) {
        self.0.push(format!("print {c}"));
    }
    fn execute(&mut self, b: u8) {
        self.0.push(format!("exec {b:#04x}"));
    }
    fn hook(&mut self, _: &Params, _: &[u8], _: bool, a: char) {
        self.0.push(format!("hook {a}"));
    }
    fn put(&mut self, b: u8) {
        self.0.push(format!("put {b:#04x}"));
    }
    fn unhook(&mut self) {
        self.0.push("unhook".into());
    }
    fn osc_dispatch(&mut self, p: &[&[u8]], _: bool) {
        let v: Vec<String> = p.iter().map(|x| String::from_utf8_lossy(x).into_owned()).collect();
        self.0.push(format!("osc {v:?}"));
    }
    fn csi_dispatch(&mut self, _: &Params, _: &[u8], _: bool, a: char) {
        self.0.push(format!("csi {a}"));
    }
    fn esc_dispatch(&mut self, _: &[u8], _: bool, b: u8) {
        self.0.push(format!("esc {}", b as char));
    }
}

fn oracle(bytes: &[u8]) -> Vec<String> {
    let mut o = Oracle::default();
    Parser::new().advance(&mut o, bytes);
    o.0
}

fn feed_all(b: &[u8]) -> Vec<Signal> {
    Scanner::default().feed(b)
}

fn enq(sigs: &[Signal]) -> usize {
    sigs.iter().filter(|s| **s == Signal::Enquiry).count()
}

fn emu() -> Emulator {
    Emulator::new(GridSize::new(40, 6), Theme::default())
}

fn screen_text(e: &mut Emulator) -> String {
    e.snapshot().text().replace('\n', "").trim().to_string()
}

// ---- 1. OSC 안의 C0는 엔진이 버린다 ----

#[test]
fn c0_inside_osc_is_dropped_like_the_engine() {
    // 엔진: 0x01·0x08·0x19·0x1f는 버리고 `133;A`로 읽는다
    let bytes = b"\x1b]133;\x01A\x08\x19\x1f\x07";
    assert_eq!(oracle(bytes), vec![r#"osc ["133", "A"]"#.to_string()]);
    let got = feed_all(bytes);
    assert!(got.contains(&Signal::Prompt(PromptMark::PromptStart)), "{got:?}");
    // 0x00..=0x06, 0x08..=0x17, 0x19, 0x1C..=0x1F 전부(BEL·CAN·ESC·SUB 제외)
    for c in (0x00u8..=0x06).chain(0x08..=0x17).chain([0x19]).chain(0x1c..=0x1f) {
        let b = [b"\x1b]133;".as_slice(), &[c], b"A\x07"].concat();
        let got = feed_all(&b);
        assert!(
            got.contains(&Signal::Prompt(PromptMark::PromptStart)),
            "C0 {c:#04x}: {got:?}"
        );
    }
}

#[test]
fn enq_inside_osc_is_not_executed() {
    let bytes = b"\x1b]9;a\x05b\x07";
    assert_eq!(oracle(bytes), vec![r#"osc ["9", "ab"]"#.to_string()]);
    assert_eq!(enq(&feed_all(bytes)), 0);
}

#[test]
fn osc_c0_scanner_and_emulator_agree() {
    let mut e = emu();
    let sigs = e.feed(b"\x1b]133;\x01A\x07ok");
    assert!(sigs.contains(&Signal::Prompt(PromptMark::PromptStart)), "{sigs:?}");
    assert!(screen_text(&mut e).ends_with("ok"));
}

#[test]
fn bel_can_sub_esc_in_osc_keep_the_073_rule() {
    // BEL·CAN·SUB는 OSC를 끝내고 내보낸다. ESC 뒤 `\`도 같다
    for end in [&b"\x07"[..], b"\x18", b"\x1a", b"\x1b\\"] {
        let b = [b"\x1b]133;A".as_slice(), end].concat();
        assert!(
            feed_all(&b).contains(&Signal::Prompt(PromptMark::PromptStart)),
            "{end:?}"
        );
    }
}

// ---- 2. ESC 바로 뒤의 C0: 엔진은 C0를 실행하고 Escape에 머문다 ----

#[test]
fn c0_after_esc_stays_in_escape_like_the_engine() {
    // `ESC LF [ c`: 엔진은 LF를 실행하고 `[ c`를 CSI로 읽어 DA1이다
    let bytes = b"\x1b\n[c";
    assert_eq!(oracle(bytes), vec!["exec 0x0a".to_string(), "csi c".to_string()]);
    assert_eq!(feed_all(bytes), vec![Signal::Da1]);
    for c in (0x00u8..=0x17).chain([0x19]).chain(0x1c..=0x1f) {
        let b = [b"\x1b".as_slice(), &[c], b"[c"].concat();
        let want = vec![Signal::Da1];
        let got: Vec<Signal> = feed_all(&b).into_iter().filter(|s| *s != Signal::Enquiry).collect();
        assert_eq!(got, want, "C0 {c:#04x}");
    }
}

#[test]
fn c0_after_esc_then_notification_osc_is_seen() {
    let got = feed_all(b"\x1b\n]9;hi\x07");
    assert!(
        matches!(got.first(), Some(Signal::Notify(n)) if n.body == "hi"),
        "{got:?}"
    );
}

#[test]
fn enq_after_esc_still_answers_and_stays() {
    let got = feed_all(b"\x1b\x05[c");
    assert_eq!(got, vec![Signal::Enquiry, Signal::Da1]);
    assert_eq!(
        oracle(b"\x1b\x05[c"),
        vec!["exec 0x05".to_string(), "csi c".to_string()]
    );
}

#[test]
fn can_sub_after_esc_go_to_ground() {
    // 엔진: CAN을 실행하고 Ground. 뒤의 `[c`는 글자다
    for c in [0x18u8, 0x1a] {
        let b = [b"\x1b".as_slice(), &[c], b"[c"].concat();
        assert_eq!(
            oracle(&b),
            vec![format!("exec {c:#04x}"), "print [".into(), "print c".into()]
        );
        assert!(feed_all(&b).is_empty(), "{c:#04x}");
    }
}

#[test]
fn esc_lf_c_resets_both_engine_and_scanner() {
    // `ESC LF c`: 엔진은 RIS로 화면을 지우고, 스캐너는 같은 자리에서 Erase를 낸다
    let mut e = emu();
    e.feed(b"hello");
    let sigs = e.feed(b"\x1b\nc");
    assert_eq!(
        sigs,
        vec![Signal::Erase {
            visible: true,
            scrollback: true
        }]
    );
    assert_eq!(screen_text(&mut e), "");
}
