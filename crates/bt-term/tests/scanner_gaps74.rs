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

// ---- 3. DCS 안의 8비트 ST(0x9C) ----

#[test]
fn dcs_8bit_st_ends_the_dcs_like_the_engine() {
    // vte `advance_dcs_passthrough`: 0x9C에서 unhook하고 Ground
    let bytes = b"\x1bP1$r\x9c\x05";
    let o = oracle(bytes);
    assert_eq!(o.last().unwrap(), "exec 0x05", "{o:?}");
    assert!(o.contains(&"unhook".to_string()));
    // 질의가 아닌 DCS: 끝난 뒤의 ENQ를 스캐너도 읽는다
    assert_eq!(feed_all(b"\x1bP1$r\x9c\x05"), vec![Signal::Enquiry]);
    assert_eq!(feed_all(b"\x1bPq\x9c\x05"), vec![Signal::Enquiry]);
}

#[test]
fn dcs_8bit_st_answers_a_query_like_esc_backslash() {
    // 끝을 ST로 보니 `ESC \`와 같이 답한다(vte는 0x9C에서 unhook, Ghostty `Parser.zig` 280줄도 dcs_unhook)
    let via_esc = feed_all(b"\x1bP$qm\x1b\\");
    let via_c1 = feed_all(b"\x1bP$qm\x9c");
    assert_eq!(via_esc.len(), 1, "{via_esc:?}");
    assert_eq!(via_c1, via_esc);
    let via_esc = feed_all(b"\x1bP+q544e\x1b\\");
    let via_c1 = feed_all(b"\x1bP+q544e\x9c");
    assert_eq!(via_esc.len(), 1, "{via_esc:?}");
    assert_eq!(via_c1, via_esc);
}

#[test]
fn dcs_8bit_st_in_header_is_ignored_like_the_engine() {
    // 머리(끝 바이트 전)의 0x9C는 vte가 무시하고 DCS를 이어 간다
    let o = oracle(b"\x1bP$\x9cqm\x1b\\");
    assert!(o.contains(&"hook q".to_string()), "{o:?}");
    assert_eq!(feed_all(b"\x1bP$\x9cqm\x1b\\").len(), 1);
}

#[test]
fn dcs_8bit_st_scanner_and_emulator_agree() {
    let mut e = emu();
    let sigs = e.feed(b"\x1bP$qm\x9chello");
    assert_eq!(sigs.len(), 1, "{sigs:?}");
    assert_eq!(screen_text(&mut e), "hello");
    // 끝난 뒤 새 DCS도 읽는다
    let sigs = e.feed(b"\x1bP1$r\x9c\x1bP$qm\x9c");
    assert_eq!(sigs.len(), 1, "{sigs:?}");
}

// ---- 4. ESC X · ESC ^ (SOS·PM) ----

#[test]
fn sos_pm_ignore_enq_like_the_engine() {
    for intro in [&b"\x1bX"[..], b"\x1b^"] {
        let bytes = [intro, b"a\x05b\x1b\\"].concat();
        assert!(!oracle(&bytes).contains(&"exec 0x05".to_string()), "{intro:?}");
        assert_eq!(enq(&feed_all(&bytes)), 0, "{intro:?}");
    }
}

#[test]
fn sos_pm_end_with_st_can_sub_esc() {
    for intro in [&b"\x1bX"[..], b"\x1b^"] {
        // ST(ESC \) 뒤의 ENQ는 Ground라 실행된다
        let b = [intro, b"x\x1b\\\x05"].concat();
        assert_eq!(enq(&feed_all(&b)), 1, "ST {intro:?}");
        // CAN·SUB: 엔진은 실행하고 Ground
        for c in [0x18u8, 0x1a] {
            let b = [intro, b"x\x05", &[c], b"\x05"].concat();
            assert_eq!(enq(&feed_all(&b)), 1, "{intro:?} {c:#04x}");
            assert_eq!(oracle(&b).iter().filter(|e| *e == "exec 0x05").count(), 1);
        }
        // ESC 뒤 `[ c`: 문자열이 끝나고 새 CSI다
        let b = [intro, b"x\x1b[c"].concat();
        assert_eq!(feed_all(&b), vec![Signal::Da1], "{intro:?}");
        // ESC 뒤 `]`: 새 OSC
        let b = [intro, b"x\x1b]9;hi\x07"].concat();
        assert!(matches!(feed_all(&b).first(), Some(Signal::Notify(_))), "{intro:?}");
    }
}

#[test]
fn sos_pm_scanner_and_emulator_agree() {
    for intro in [&b"\x1bX"[..], b"\x1b^"] {
        // 안의 글은 엔진이 먹는다. 스캐너가 그 안의 DA1을 읽지 않는다
        let mut e = emu();
        let sigs = e.feed(&[intro, b"abc\x05\x1b\\world"].concat());
        assert!(sigs.is_empty(), "{sigs:?}");
        assert_eq!(screen_text(&mut e), "world");
        // 안의 ESC c는 문자열을 끝내고 RIS다
        let mut e = emu();
        e.feed(b"hello");
        let sigs = e.feed(&[intro, b"abc\x1bc"].concat());
        assert_eq!(
            sigs,
            vec![Signal::Erase {
                visible: true,
                scrollback: true
            }]
        );
        assert_eq!(screen_text(&mut e), "");
    }
}

#[test]
fn apc_is_still_its_own_path() {
    assert_eq!(
        feed_all(b"\x1b_Ga=q,i=1\x1b\\"),
        vec![Signal::Graphics(b"a=q,i=1".to_vec())]
    );
}

// ---- 5. 상한을 넘긴 OSC(Skip)는 일부러 엔진과 다르다 ----

#[test]
fn oversize_osc_is_never_emitted_even_after_can_or_esc() {
    // 상한(8KiB)을 넘긴 문자열은 내보내지 않는 보안 결정이다. 엔진은 잘린 OSC를 내보낼 수 있다
    for end in [&b"\x07"[..], b"\x18", b"\x1a", b"\x1b\\", b"\x1b[c"] {
        let mut v = b"\x1b]9;".to_vec();
        v.extend(vec![b'a'; 9000]);
        v.extend(end);
        let got = feed_all(&v);
        assert!(!got.iter().any(|s| matches!(s, Signal::Notify(_))), "{end:?}: {got:?}");
    }
    // 끝난 뒤의 질의는 읽는다
    let mut v = b"\x1b]9;".to_vec();
    v.extend(vec![b'a'; 9000]);
    v.extend(b"\x18\x1b[c");
    assert_eq!(feed_all(&v), vec![Signal::Da1]);
}

// ---- 6. tmux 패스스루 겹친 ESC 뒤는 일부러 엔진과 다르다 ----

#[test]
fn tmux_double_esc_inner_query_is_not_ours() {
    assert!(feed_all(b"\x1bPtmux;\x1b\x1bP$qm\x1b\x1b\\\x1b\\").is_empty());
    assert!(feed_all(b"\x1bPtmux;\x1b\x1b[c\x1b\\").is_empty());
}
