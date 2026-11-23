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
    // BEL은 OSC를 끝내고 내보낸다. ESC 뒤 `\`도 같다
    for end in [&b"\x07"[..], b"\x1b\\"] {
        let b = [b"\x1b]133;A".as_slice(), end].concat();
        assert!(
            feed_all(&b).contains(&Signal::Prompt(PromptMark::PromptStart)),
            "{end:?}"
        );
    }
    // CAN·SUB는 OSC를 끝내되 버린다(#763, Ghostty 1.4.0 `osc.zig` `Parser.end`)
    for end in [&b"\x18"[..], b"\x1a"] {
        let b = [b"\x1b]133;A".as_slice(), end].concat();
        assert!(feed_all(&b).is_empty(), "{end:?}");
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
// #792: 통과 상태에서 0x80~0xFF는 모두 자료다. 0x9C도 DCS를 끝내지 않는다(Ghostty 1.4.0 `parse_table.zig` 412줄 시험).
// 전에는 이 시험들이 "0x9C에서 끝난다"를 확인했다

#[test]
fn dcs_8bit_st_does_not_end_the_dcs_like_the_engine() {
    // vte `advance_dcs_passthrough`: 0x9C는 자료(`put`)이고 unhook하지 않는다
    let bytes = b"\x1bP1$r\x9c\x05";
    let o = oracle(bytes);
    assert_eq!(o.last().unwrap(), "put 0x05", "{o:?}");
    assert!(o.contains(&"put 0x9c".to_string()), "{o:?}");
    assert!(!o.contains(&"unhook".to_string()), "{o:?}");
    // 스캐너도 ENQ를 자료로 본다. `ESC \`로 끝난 뒤에야 읽는다
    assert_eq!(feed_all(b"\x1bP1$r\x9c\x05"), vec![]);
    assert_eq!(feed_all(b"\x1bPq\x9c\x05"), vec![]);
    assert_eq!(feed_all(b"\x1bPq\x9c\x1b\\\x05"), vec![Signal::Enquiry]);
}

#[test]
fn dcs_8bit_st_does_not_answer_a_query() {
    // 질의는 `ESC \`로 끝나야 답한다. 0x9C는 본문(자료)이다
    assert!(feed_all(b"\x1bP$qm\x9c").is_empty());
    assert!(feed_all(b"\x1bP+q544e\x9c").is_empty());
    assert_eq!(feed_all(b"\x1bP$qm\x1b\\").len(), 1);
    assert_eq!(feed_all(b"\x1bP+q544e\x1b\\").len(), 1);
}

#[test]
fn dcs_8bit_st_in_header_ends_the_dcs_like_the_engine() {
    // #793: 머리(끝 바이트 전)의 0x9C는 Ghostty 표의 "어디서나" 전이로 Ground로 간다(전에는 vte가 무시했다).
    // 이어지는 `qm`은 글자이고 질의가 아니다
    let o = oracle(b"\x1bP$\x9cqm\x1b\\");
    assert!(!o.iter().any(|s| s.starts_with("hook")), "{o:?}");
    assert_eq!(
        o,
        vec!["print q".to_string(), "print m".to_string(), "esc \\".to_string()]
    );
    assert!(feed_all(b"\x1bP$\x9cqm\x1b\\").is_empty());
}

#[test]
fn dcs_8bit_st_scanner_and_emulator_agree() {
    let mut e = emu();
    let sigs = e.feed(b"\x1bP$qm\x9chello");
    assert!(sigs.is_empty(), "{sigs:?}");
    assert_eq!(screen_text(&mut e), "");
    // `ESC \`로 끝난 뒤에는 화면에 찍힌다
    e.feed(b"\x1b\\shown");
    assert_eq!(screen_text(&mut e), "shown");
    // 끝난 뒤 새 DCS는 읽는다
    let sigs = e.feed(b"\x1bP1$r\x1b\\\x1bP$qm\x1b\\");
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

// ---- 7. DCS 머리가 엔진에서 무시(DcsIgnore)로 가는 길 ----
// vte `advance_dcs_intermediate`(291줄): 중간 바이트 뒤의 0x30~0x3F, `advance_dcs_param`(309줄): 첫 자리가 아닌 0x3C~0x3F.
// 무시 상태는 `anywhere`(175줄)만 보므로 0x9C로 끝나지 않고 ESC·CAN·SUB로만 끝난다

/// 엔진이 hook한 DCS의 중간 바이트(맨 앞 사설 표시 포함)
fn hook_intermediates(bytes: &[u8]) -> Option<Vec<u8>> {
    #[derive(Default)]
    struct H(Option<Vec<u8>>);
    impl Perform for H {
        fn hook(&mut self, _: &Params, i: &[u8], _: bool, _: char) {
            self.0 = Some(i.to_vec());
        }
    }
    let mut h = H::default();
    Parser::new().advance(&mut h, bytes);
    h.0
}

#[test]
fn dcs_param_after_intermediate_is_ignored_like_the_engine() {
    for bytes in [
        b"\x1bP$0qm\x1b\\".as_slice(),
        b"\x1bP$;qm\x1b\\",
        b"\x1bP+1q544e\x1b\\",
        b"\x1bP$?qm\x1b\\",
    ] {
        let o = oracle(bytes);
        assert!(!o.iter().any(|s| s.starts_with("hook")), "{bytes:?}: {o:?}");
        assert!(feed_all(bytes).is_empty(), "{bytes:?}: {:?}", feed_all(bytes));
    }
}

#[test]
fn ignored_dcs_does_not_end_at_8bit_st() {
    // 엔진은 무시 상태에서 0x9C도 ENQ도 버린다. 스캐너가 0x9C에서 끝내면 엔진이 버린 ENQ에 답한다
    let bytes = b"\x1bP$0qm\x9c\x05";
    let o = oracle(bytes);
    assert!(o.is_empty(), "{o:?}");
    assert_eq!(feed_all(bytes), vec![]);
    // ESC·CAN·SUB로는 끝나고 뒤의 ENQ를 읽는다
    for end in [b"\x1b\\".as_slice(), b"\x18", b"\x1a"] {
        let b = [b"\x1bP$0qm\x9c".as_slice(), end, b"\x05"].concat();
        assert_eq!(oracle(&b).last().unwrap(), "exec 0x05", "{end:?}");
        assert_eq!(feed_all(&b), vec![Signal::Enquiry], "{end:?}");
    }
    // 끝 바이트 전에 무시로 가도 같다
    assert_eq!(feed_all(b"\x1bP$0\x9c\x05"), vec![]);
    assert!(oracle(b"\x1bP$0\x9c\x05").is_empty());
}

#[test]
fn private_marker_after_params_is_ignored_like_the_engine() {
    let bytes = b"\x1bP1?$qm\x9c\x05";
    assert!(oracle(bytes).is_empty(), "{:?}", oracle(bytes));
    assert_eq!(feed_all(bytes), vec![]);
}

#[test]
fn leading_private_marker_makes_two_intermediates() {
    // 엔진은 맨 앞 `?`를 중간 바이트로 모은다. Ghostty `dcs.zig`의 DECRQSS·XTGETTCAP은 중간 바이트가 하나일 때만이라 답하지 않는다
    assert_eq!(hook_intermediates(b"\x1bP?$qm\x1b\\"), Some(b"?$".to_vec()));
    assert!(feed_all(b"\x1bP?$qm\x1b\\").is_empty());
    assert!(feed_all(b"\x1bP>+q544e\x1b\\").is_empty());
}

#[test]
fn numeric_params_before_intermediate_still_answer() {
    // 숫자 매개변수는 Ghostty처럼 보지 않는다
    assert_eq!(hook_intermediates(b"\x1bP1;2$qm\x1b\\"), Some(b"$".to_vec()));
    assert_eq!(feed_all(b"\x1bP1;2$qm\x1b\\").len(), 1);
    assert_eq!(feed_all(b"\x1bP0+q544e\x1b\\").len(), 1);
}

#[test]
fn ignore_is_seen_past_the_header_cap() {
    // 머리 상한(16바이트)을 넘겨 모으지 않아도 중간 바이트 뒤의 매개변수는 무시로 간다
    let mut b = b"\x1bP".to_vec();
    b.extend(vec![b'1'; 40]);
    b.extend(b"$0qm\x9c\x05");
    assert!(oracle(&b).is_empty(), "{:?}", oracle(&b));
    assert_eq!(feed_all(&b), vec![]);
}

#[test]
fn ignored_dcs_scanner_and_emulator_agree() {
    let mut e = emu();
    let sigs = e.feed(b"\x1bP$0qm\x9c\x05hidden");
    assert!(sigs.is_empty(), "{sigs:?}");
    assert_eq!(screen_text(&mut e), "");
    let sigs = e.feed(b"\x1b\\shown\x1bP$qm\x1b\\");
    assert_eq!(sigs.len(), 1, "{sigs:?}");
    assert_eq!(screen_text(&mut e), "shown");
}
