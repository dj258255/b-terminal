//! #603: 신호 스캐너(`Scanner::feed_at`)가 엔진(vte)보다 ENQ에 더 답하던 틈.
//! 기준: vendor/vte `src/lib.rs`의 `advance_esc`(340~391줄), `advance_esc_intermediate`(393줄), `advance_osc_string`(407줄), `anywhere`(175줄).
//! 시험마다 엔진 쪽 사실을 vte 파서에 같은 바이트를 먹여 먼저 확인하고(`oracle`), 스캐너 결과를 견준다.
//! 일부러 엔진과 다르게 두는 것(tmux 패스스루 안쪽 글에는 답하지 않는다, ADR-053)은 스캐너가 엔진보다 덜 내는 쪽으로만 다르다.

use alacritty_terminal::vte::{Params, Parser, Perform};
use bt_core::Theme;
use bt_term::notify::{Scanner, Signal};
use bt_term::{Emulator, GridSize};

/// 엔진이 한 일 가운데 신호와 견줄 것만 센다
#[derive(Default)]
struct Oracle {
    enq: usize,
    da1: usize,
    dcs_query: usize,
}

impl Perform for Oracle {
    fn execute(&mut self, b: u8) {
        if b == 0x05 {
            self.enq += 1;
        }
    }
    fn hook(&mut self, _: &Params, i: &[u8], _: bool, a: char) {
        if a == 'q' && (i == b"$" || i == b"+") {
            self.dcs_query += 1;
        }
    }
    fn csi_dispatch(&mut self, _: &Params, i: &[u8], _: bool, a: char) {
        if a == 'c' && i.is_empty() {
            self.da1 += 1;
        }
    }
}

fn oracle(bytes: &[u8]) -> Oracle {
    let mut o = Oracle::default();
    Parser::new().advance(&mut o, bytes);
    o
}

fn feed_all(b: &[u8]) -> Vec<Signal> {
    Scanner::default().feed(b)
}

fn enq(sigs: &[Signal]) -> usize {
    sigs.iter().filter(|s| **s == Signal::Enquiry).count()
}

fn da1(sigs: &[Signal]) -> usize {
    sigs.iter().filter(|s| **s == Signal::Da1).count()
}

fn dcs_queries(sigs: &[Signal]) -> usize {
    sigs.iter().filter(|s| matches!(s, Signal::DcsQuery(_))).count()
}

fn emu() -> Emulator {
    Emulator::new(GridSize::new(40, 6), Theme::default())
}

/// 엔진 쪽 사실을 먼저 단언하고 스캐너가 같은 수만큼 답하는지 본다
#[track_caller]
fn same_enq(bytes: &[u8], want: usize) {
    assert_eq!(oracle(bytes).enq, want, "엔진 {bytes:?}");
    assert_eq!(enq(&feed_all(bytes)), want, "스캐너 {bytes:?}");
}

// ---- 1. 겹친 ESC 뒤의 문자열 ----

#[test]
fn double_esc_then_string_intro_swallows_enq_like_the_engine() {
    for head in [&b"\x1bP$0q"[..], b"\x1bP"] {
        for intro in *b"X^_]P" {
            let bytes = [head, b"\x1b\x1b", &[intro], b"\x05"].concat();
            same_enq(&bytes, 0);
            assert!(feed_all(&bytes).is_empty(), "{bytes:?}");
        }
    }
}

#[test]
fn double_esc_string_ends_like_the_engine() {
    // 문자열 상태는 CAN·SUB로 Ground, ESC로 Escape로 간다. OSC는 BEL로도 끝난다
    for intro in *b"X^_]P" {
        for end in [0x18u8, 0x1a] {
            // P는 머리부터라 끝 바이트(`q`)를 줘 본문으로 보낸다
            let body: &[u8] = if intro == b'P' { b"q\x05" } else { b"\x05" };
            let bytes = [b"\x1bP\x1b\x1b".as_slice(), &[intro], body, &[end], b"\x05"].concat();
            same_enq(&bytes, 1);
        }
    }
    same_enq(b"\x1bP\x1b\x1b]a\x05\x07\x05", 1);
    // 엔진은 ESC 뒤 `\`로 Ground에 가서 ENQ를 실행한다
    for intro in *b"X^_]P" {
        let body: &[u8] = if intro == b'P' { b"q\x05" } else { b"\x05" };
        let bytes = [b"\x1bP\x1b\x1b".as_slice(), &[intro], body, b"\x1b\\\x05"].concat();
        same_enq(&bytes, 1);
    }
}

#[test]
fn double_esc_c0_stays_in_escape_like_the_engine() {
    // `ESC ESC ENQ X ENQ`: 엔진은 Escape에 머물러 둘째 ENQ를 `X` 문자열 안에서 버린다
    same_enq(b"\x1bP\x1b\x1b\x05X\x05", 1);
    same_enq(b"\x1bP\x1b\x1b\x01X\x05", 0);
    same_enq(b"\x1bP\x1b\x1b\x7fX\x05", 0);
    same_enq(b"\x1bP\x1b\x1b\x9cX\x05", 0);
}

#[test]
fn inner_string_of_tmux_passthrough_gives_no_signal() {
    // 안쪽 DCS·OSC는 질의·알림 신호를 내면 안 된다(엔진은 `ESC ESC P`의 둘째 ESC를 새 이스케이프로 읽는다)
    assert!(feed_all(b"\x1bPtmux;\x1b\x1bP$qm\x1b\x1b\\\x1b\\").is_empty());
    assert!(feed_all(b"\x1bPtmux;\x1b\x1bP$qm\x1b\\").is_empty());
    assert!(feed_all(b"\x1bPtmux;\x1b\x1bP+q544e\x9c").is_empty());
    assert!(feed_all(b"\x1bPtmux;\x1b\x1b]9;hi\x07\x1b\\").is_empty());
    assert!(feed_all(b"\x1bPtmux;\x1b\x1b]9;hi\x1b\x1b\\\x1b\\").is_empty());
    assert!(feed_all(b"\x1bPtmux;\x1b\x1b]133;A\x07\x1b\\").is_empty());
    assert!(feed_all(b"\x1bPtmux;\x1b\x1b_Ga=q,i=1\x1b\x1b\\\x1b\\").is_empty());
    // 안쪽 DCS 머리가 무시로 가는 경우도 같다
    assert!(feed_all(b"\x1bPtmux;\x1b\x1bP$0qm\x1b\\").is_empty());
    // 겹친 ESC 뒤의 CSI는 지금처럼 읽지 않는다(엔진보다 덜 낸다)
    assert!(feed_all(b"\x1bPtmux;\x1b\x1b[c\x1b\\").is_empty());
    assert_eq!(oracle(b"\x1bPtmux;\x1b\x1b[c\x1b\\").da1, 1);
}

#[test]
fn quiet_string_then_esc_does_not_read_new_queries() {
    // 안쪽 문자열이 ESC로 끝나도 그 뒤 `[ c`는 안쪽 글이라 답하지 않는다(엔진은 DA1이다)
    let bytes = b"\x1bP\x1b\x1bX\x1b[c";
    assert_eq!(oracle(bytes).da1, 1);
    assert!(feed_all(bytes).is_empty());
    let bytes = b"\x1bP\x1b\x1b]9;a\x1b[c";
    assert_eq!(oracle(bytes).da1, 1);
    assert!(feed_all(bytes).is_empty());
    // 그래도 ENQ는 엔진처럼 답한다(Escape에서 실행)
    same_enq(b"\x1bP\x1b\x1bX\x1b\x05", 1);
}

#[test]
fn a_normal_dcs_and_a_new_dcs_after_the_quiet_one_still_answer() {
    // 겹친 ESC가 없는 DCS는 그대로 답한다
    assert_eq!(dcs_queries(&feed_all(b"\x1bP$qm\x1b\\")), 1);
    // 안쪽 DCS가 끝난 뒤(Ground) 새로 연 DCS도 답한다
    let got = feed_all(b"\x1bPtmux;\x1b\x1bP$qm\x1b\x1b\\\x1b\\\x1bP$qm\x1b\\");
    assert_eq!(dcs_queries(&got), 1, "{got:?}");
}

// ---- 2. ESC 뒤의 0x7F·0x80 이상 ----

#[test]
fn del_and_high_bytes_after_esc_stay_in_escape_like_the_engine() {
    for c in std::iter::once(0x7fu8).chain(0x80..=0xff) {
        // `ESC c ENQ X ENQ`: 엔진은 Escape에 머물러 첫 ENQ만 실행한다
        same_enq(&[b"\x1b".as_slice(), &[c], b"\x05X\x05"].concat(), 1);
    }
    // 머문 뒤 `[ c`는 DA1이다
    let bytes = b"\x1b\x9c[c";
    assert_eq!(oracle(bytes).da1, 1);
    assert_eq!(da1(&feed_all(bytes)), 1);
    let bytes = b"\x1b\x7f[c";
    assert_eq!(oracle(bytes).da1, 1);
    assert_eq!(da1(&feed_all(bytes)), 1);
}

// ---- 3. ESC 중간 바이트 뒤의 C0 ----

#[test]
fn c0_after_esc_intermediate_matches_the_engine() {
    for inter in 0x20u8..=0x2f {
        for tail in [
            &b"\x05X\x05"[..],
            b"\x7f\x05X\x05",
            b"\x9c\x05X\x05",
            b"\x01\x05",
            b"\x18\x05",
            b"\x1a\x05",
            b"\x1b\x05",
            b"\x05\x05",
        ] {
            let bytes = [b"\x1b".as_slice(), &[inter], tail].concat();
            let want = oracle(&bytes).enq;
            assert_eq!(enq(&feed_all(&bytes)), want, "{bytes:?}");
        }
    }
}

// ---- 4. Emulator::feed ----

#[test]
fn emulator_agrees_on_the_enquiry_count() {
    for bytes in [
        &b"\x1bP$0q\x1b\x1bX\x05"[..],
        b"\x1bP\x1b\x1bX\x05",
        b"\x1bP\x1b\x1b]a\x05\x07",
        b"\x1b\x9c\x05X\x05",
        b"\x1b\x7f\x05X\x05",
        b"\x1b \x05X\x05",
    ] {
        let want = oracle(bytes).enq;
        let mut e = emu();
        assert_eq!(enq(&e.feed(bytes)), want, "{bytes:?}");
    }
}

// ---- 5. 차분 퍼징: 스캐너가 엔진보다 더 답하는 길이 없다 ----
// 일부러 엔진과 다른 곳(겹친 ESC 뒤의 `[`·`c` 같은 CSI는 읽지 않는다, 상한을 넘긴 OSC는 내보내지 않는다)은 스캐너가 덜 내는 쪽이라
// "스캐너 수 <= 엔진 수"에 걸리지 않는다. 그래서 걸러 내는 경우가 없다

struct Rng(u64);
impl Rng {
    fn next(&mut self) -> u64 {
        let mut x = self.0;
        x ^= x >> 12;
        x ^= x << 25;
        x ^= x >> 27;
        self.0 = x;
        x.wrapping_mul(0x2545F4914F6CDD1D)
    }
    fn below(&mut self, n: usize) -> usize {
        (self.next() % n as u64) as usize
    }
}

fn alphabet() -> Vec<u8> {
    let mut v = vec![
        0x1b, 0x7f, 0x80, 0x9c, 0x05, 0x18, 0x1a, 0x07, 0x00, 0x01, 0x0a, 0x17, 0x19, 0x1c, 0x1f,
    ];
    v.extend(*b"X^_]P\\[c$q+ (;01?>");
    // ESC가 많아야 겹친 ESC와 문자열 경계가 자주 나온다
    v.extend([0x1b; 8]);
    v
}

fn fuzz(seed: u64, cases: usize) {
    let al = alphabet();
    let mut rng = Rng(seed);
    for n in 0..cases {
        let len = 1 + rng.below(24);
        let bytes: Vec<u8> = (0..len).map(|_| al[rng.below(al.len())]).collect();
        let o = oracle(&bytes);
        let got = feed_all(&bytes);
        assert!(
            enq(&got) <= o.enq,
            "case {n}: Enquiry {} > {} {bytes:?}",
            enq(&got),
            o.enq
        );
        assert!(da1(&got) <= o.da1, "case {n}: Da1 {} > {} {bytes:?}", da1(&got), o.da1);
        assert!(
            dcs_queries(&got) <= o.dcs_query,
            "case {n}: DcsQuery {} > {} {bytes:?}",
            dcs_queries(&got),
            o.dcs_query
        );
        // 두 조각으로 나눠 먹여도 같다(조각 경계가 상태를 바꾸지 않는다)
        let cut = rng.below(len + 1);
        let mut s = Scanner::default();
        let mut split = s.feed(&bytes[..cut]);
        split.extend(s.feed(&bytes[cut..]));
        assert_eq!(split, got, "case {n}: 조각 {cut} {bytes:?}");
    }
}

#[test]
fn scanner_never_answers_more_than_the_engine() {
    fuzz(0x0075_5EED, 300_000);
}

#[test]
#[ignore = "큰 판: cargo test -p bt-term --test scanner_enq75 --release -- --ignored"]
fn scanner_never_answers_more_than_the_engine_large() {
    for seed in 1..=10u64 {
        fuzz(0x0075_0000 + seed, 1_000_000);
    }
}

// ---- 지금의 한계를 고정: 조용히 따라가는 것은 겹친 ESC 바로 뒤의 첫 안쪽 시퀀스 하나다 ----

#[test]
fn only_the_first_inner_sequence_after_double_esc_is_quiet() {
    // 첫 안쪽 시퀀스(`[ c`)는 엔진이 DA1로 읽어도 스캐너는 읽지 않는다(tmux 결정)
    let one = b"\x1bPtmux;\x1b\x1b[c\x1b\\";
    assert_eq!(oracle(one).da1, 1);
    assert_eq!(da1(&feed_all(one)), 0);
    // 그 시퀀스가 끝나면 Ground로 돌아가므로 둘째 안쪽 `ESC ESC [ c`는 보통처럼 읽어 답한다(0.73부터 같았다).
    // 엔진도 같은 DA1을 처리하므로 스캐너가 엔진보다 더 내는 것은 아니다
    for two in [
        b"\x1bPtmux;\x1b\x1b]7;file://h/t\x07\x1b\x1b[c\x1b\\".as_slice(),
        b"\x1bPtmux;\x1b\x1bc\x1b\x1b[c\x1b\\",
    ] {
        assert_eq!(oracle(two).da1, 1, "{two:?}");
        assert_eq!(da1(&feed_all(two)), 1, "{two:?}");
    }
}
