//! #605: 신호 스캐너(`Scanner::feed_at`)가 엔진(vte)이 dispatch하지 않는 CSI에서도 신호를 내던 틈.
//! 화면 지우기(ED, `J`) 판정이 `csi_taint`·`csi_inter` 검사 바깥에 있어서, 엔진이 CsiIgnore로 보내거나 중간 바이트·표시(`< = >`)를
//! 붙여 다른 명령으로 dispatch하는 바이트열에서도 `Signal::Erase`를 냈다. 매개변수가 32개를 넘어 엔진이 `ignore`로 dispatch하는 CSI도 같다.
//! 기준: vendor/vte `src/lib.rs`의 `advance_csi_entry`·`advance_csi_param`·`advance_csi_intermediate`·`advance_csi_ignore`(188~260줄),
//! `action_csi_dispatch`(453줄), `MAX_INTERMEDIATES`(44줄), `params::MAX_PARAMS`(32), `vendor/vte/src/ansi.rs`의 `csi_dispatch`(1647줄).
//! 시험마다 엔진 쪽 사실을 vte 파서에 같은 바이트를 먹여 먼저 확인하고(`oracle`) 스캐너 결과를 견준다.
//! 스캐너가 엔진보다 덜 내는 것(`CSI ? 2 J`, `CSI 2 : J` 등)은 이 시험의 대상이 아니다.

use std::collections::HashMap;

use alacritty_terminal::vte::{Params, Parser, Perform};
use bt_core::Theme;
use bt_term::notify::{Scanner, Signal};
use bt_term::{Emulator, GridSize};

/// 엔진이 한 일 가운데 신호와 견줄 것만 센다
#[derive(Default)]
struct Oracle {
    /// `ignore` 없이 dispatch된 CSI: (끝 바이트, 중간 바이트·표시) -> 수
    csi: HashMap<(char, String), usize>,
    /// `ignore`로 dispatch된 CSI의 수(엔진은 이것을 처리하지 않는다)
    ignored: usize,
    /// 엔진이 실제로 화면 지우기 2·3으로 처리하는 ED(`CSI 2 J`·`CSI 3 J`, 첫 매개변수 기준, 비공개 표시 없음)
    ed: usize,
    /// `ESC c`(RIS)도 스캐너는 Erase로 낸다
    ris: usize,
}

impl Oracle {
    fn count(&self, fin: char, inter: &str) -> usize {
        self.csi.get(&(fin, inter.to_string())).copied().unwrap_or(0)
    }
}

impl Perform for Oracle {
    fn csi_dispatch(&mut self, p: &Params, i: &[u8], ignore: bool, a: char) {
        if ignore {
            self.ignored += 1;
            return;
        }
        let inter = String::from_utf8_lossy(i).into_owned();
        if a == 'J' && i.is_empty() && matches!(p.iter().next().map(|g| g[0]), Some(2 | 3)) {
            self.ed += 1;
        }
        *self.csi.entry((a, inter)).or_default() += 1;
    }
    fn esc_dispatch(&mut self, i: &[u8], ignore: bool, b: u8) {
        if b == b'c' && i.is_empty() && !ignore {
            self.ris += 1;
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

fn erases(sigs: &[Signal]) -> usize {
    sigs.iter().filter(|s| matches!(s, Signal::Erase { .. })).count()
}

fn emu() -> Emulator {
    Emulator::new(GridSize::new(40, 6), Theme::default())
}

// ---- 1. ED: 엔진이 dispatch하지 않거나 다른 명령으로 dispatch하는 CSI ----

/// 엔진은 ED로 처리하지 않는다(CsiIgnore이거나 중간 바이트·표시가 붙은 다른 명령). 스캐너도 Erase를 내지 않아야 한다
#[track_caller]
fn no_erase(bytes: &[u8]) {
    let o = oracle(bytes);
    assert_eq!(o.ed, 0, "엔진 {bytes:?}");
    assert_eq!(erases(&feed_all(bytes)), 0, "스캐너 {bytes:?}");
    let mut e = emu();
    assert_eq!(erases(&e.feed(bytes)), 0, "Emulator {bytes:?}");
}

/// 엔진이 ED로 처리한다. 스캐너도 Erase를 하나 낸다
#[track_caller]
fn one_erase(bytes: &[u8]) {
    assert_eq!(oracle(bytes).ed, 1, "엔진 {bytes:?}");
    assert_eq!(erases(&feed_all(bytes)), 1, "스캐너 {bytes:?}");
    let mut e = emu();
    assert_eq!(erases(&e.feed(bytes)), 1, "Emulator {bytes:?}");
}

#[test]
fn ed_after_an_intermediate_is_not_an_erase() {
    // 중간 바이트 뒤의 매개변수는 엔진이 CsiIgnore로 보낸다(`advance_csi_intermediate`의 0x30..=0x3F)
    no_erase(b"\x1b[(2J");
    no_erase(b"\x1b[ 3J");
    no_erase(b"\x1b[!2J");
    // 0x80~0x9F는 CSI를 끝낸다(#793, Ghostty 표의 "어디서나" 전이): 0x80은 실행하고 Ground로 가서 뒤는 그냥 글자다
    no_erase(b"\x1b[\x80$3J");
    no_erase(b"\x1b[\x9d(2J");
    no_erase(b"\x1b[\x9c(2J");
    // 중간 바이트가 매개변수 다음에 오면 엔진은 `J`를 중간 바이트가 붙은 명령으로 dispatch한다(ED가 아니다)
    no_erase(b"\x1b[2$J");
    no_erase(b"\x1b[3 J");
    // 중간 바이트가 셋 이상이면 엔진은 ignore로 dispatch한다
    no_erase(b"\x1b[$$$2J");
    no_erase(b"\x1b[2$$$J");
}

#[test]
fn ed_with_a_marker_is_not_an_erase() {
    // `< = >`가 맨 앞에 오면 엔진은 표시로 모아 `J`를 비공개 명령으로 dispatch한다(`ansi.rs`의 `('J', [])`가 아니다)
    for m in *b"<=>?" {
        no_erase(&[b"\x1b[".as_slice(), &[m], b"2J"].concat());
        no_erase(&[b"\x1b[".as_slice(), &[m], b"3J"].concat());
    }
    // 매개변수 뒤의 표시는 엔진이 CsiIgnore로 보낸다
    for m in *b"<=>?" {
        no_erase(&[b"\x1b[2".as_slice(), &[m], b"J"].concat());
    }
    // 표시와 중간 바이트가 같이 와도 마찬가지다
    no_erase(b"\x1b[>$2J");
    no_erase(b"\x1b[?$3J");
}

#[test]
fn ed_with_a_leading_colon_is_not_an_erase() {
    // `: 2`는 엔진이 첫 매개변수 묶음 [0, 2]로 읽어 ED 0(아래로 지우기)이다. 화면 전체·기록 지우기가 아니다
    no_erase(b"\x1b[:2J");
    no_erase(b"\x1b[::3J");
}

#[test]
fn plain_ed_and_the_bytes_the_engine_ignores_still_erase() {
    one_erase(b"\x1b[2J");
    one_erase(b"\x1b[3J");
    // DEL과 0xA0 이상은 CSI 안 어디서나 무시된다(표에 없다). 매개변수 앞뒤에 끼어도 ED는 그대로다.
    // 0x80~0x9F는 CSI를 끝내거나 새로 연다(#793): `ESC [ 0x80 …`은 Ground, `ESC [ 1 0x9B 2 J`는 `2J`
    one_erase(b"\x1b[\x7f3J");
    one_erase(b"\x1b[2\x7fJ");
    one_erase(b"\x1b[\xa02\xffJ");
    one_erase(b"\x1b[1\x9b2J");
    no_erase(b"\x1b[\x80\x9d2\x9cJ");
    // C0는 실행하고 CSI에 머문다. CAN·SUB는 끊고 ESC는 새 이스케이프를 연다
    one_erase(b"\x1b[\x012\x0aJ");
    one_erase(b"\x1b[2\x00\x1fJ");
    no_erase(b"\x1b[2\x18J");
    no_erase(b"\x1b[2\x1aJ");
    no_erase(b"\x1b[2\x1bJ");
    // 끊긴 뒤 새로 시작한 ED는 읽는다
    one_erase(b"\x1b[(2\x18\x1b[2J");
}

#[test]
fn ed_after_a_failed_sequence_is_read_again() {
    // 앞의 시퀀스가 남긴 중간 바이트·표시 상태가 뒤의 ED로 넘어가지 않는다
    one_erase(b"\x1b[(2J\x1b[3J");
    one_erase(b"\x1b[>c\x1b[2J");
    // 같은 바이트를 한 번에 먹이든 둘로 나누든 같다
    let mut s = Scanner::default();
    let mut got = s.feed(b"\x1b[(");
    got.extend(s.feed(b"2J\x1b[2"));
    got.extend(s.feed(b"J"));
    assert_eq!(erases(&got), 1, "{got:?}");
}

// ---- 2. 매개변수 상한: 엔진은 32개를 넘으면 ignore로 dispatch한다 ----

fn with_semis(head: &[u8], n: usize, tail: &[u8]) -> Vec<u8> {
    [head, ";".repeat(n).as_bytes(), tail].concat()
}

#[test]
fn csi_with_too_many_parameters_gives_no_signal() {
    // 세미콜론 32개(매개변수 33개)부터 엔진은 `ignore`로 dispatch하고 `ansi.rs`는 버린다. 31개까지는 처리한다
    for (head, tail, fin, inter) in [
        (&b"\x1b[2"[..], &b"h"[..], 'h', ""),
        (b"\x1b[2", b"l", 'l', ""),
        (b"\x1b[?2031", b"h", 'h', "?"),
        (b"\x1b[?2048", b"l", 'l', "?"),
        (b"\x1b[?996", b"n", 'n', "?"),
        (b"\x1b[>", b"c", 'c', ">"),
        (b"\x1b[>", b"q", 'q', ">"),
        (b"\x1b[", b"c", 'c', ""),
    ] {
        let ok = with_semis(head, 31, tail);
        assert_eq!(oracle(&ok).count(fin, inter), 1, "엔진 {ok:?}");
        let bad = with_semis(head, 32, tail);
        let o = oracle(&bad);
        assert_eq!((o.count(fin, inter), o.ignored), (0, 1), "엔진 {bad:?}");
        assert!(feed_all(&bad).is_empty(), "스캐너 {bad:?}: {:?}", feed_all(&bad));
        let mut e = emu();
        assert!(e.feed(&bad).is_empty(), "Emulator {bad:?}");
    }
    // 31개까지는 지금처럼 읽는다
    assert_eq!(feed_all(&with_semis(b"\x1b[2", 31, b"h")), vec![Signal::Kam(true)]);
    assert_eq!(feed_all(&with_semis(b"\x1b[>", 31, b"c")), vec![Signal::Da2]);
    assert_eq!(feed_all(&with_semis(b"\x1b[", 31, b"c")), vec![Signal::Da1]);
}

// ---- 3. 그 밖의 CSI 신호: 엔진이 같은 꼴로 dispatch한 수를 넘지 않는다 ----

/// 스캐너 신호를 엔진이 dispatch한 (끝 바이트, 중간 바이트·표시) 꼴로 바꾼다. CSI에서 나오지 않는 신호는 `None`
fn key(s: &Signal) -> Option<(char, &'static str, usize)> {
    // 셋째는 한 번의 dispatch에서 나올 수 있는 신호의 최대 수(`CSI ? 2031 ; 2033 ; 2048 ; 5522 h`는 넷, XTSAVE·XTRESTORE는 적힌 모드마다 하나)
    let hl = |b: &bool| if *b { 'h' } else { 'l' };
    Some(match s {
        Signal::ColorSchemeQuery | Signal::DecDsr(_) => ('n', "?", 1),
        Signal::ColorSchemeUpdates(b)
        | Signal::InBandSizeReports(b)
        | Signal::VisibilityReports(b)
        | Signal::KittyPasteEvents(b) => (hl(b), "?", 4),
        Signal::Kam(b) => (hl(b), "", 1),
        Signal::TitleReport | Signal::WindowResize { .. } | Signal::SizeReport(_) => ('t', "", 1),
        Signal::Da1 => ('c', "", 1),
        Signal::Da2 => ('c', ">", 1),
        Signal::XtVersion => ('q', ">", 1),
        Signal::ModeSave(_) => ('s', "?", 40),
        Signal::ModeRestore(_) => ('r', "?", 40),
        Signal::XtChecksumReport(_) => ('y', "*", 1),
        Signal::XtChecksumExtension(_) => ('y', "#", 1),
        Signal::SoftReset => ('p', "!", 1),
        Signal::InBandSizeQuery | Signal::ColorSchemeModeQuery | Signal::VisibilityQuery | Signal::KittyPasteQuery => {
            ('p', "?$", 1)
        }
        _ => return None,
    })
}

/// 스캐너가 낸 신호 중 엔진이 같은 꼴로 `ignore` 없이 dispatch한 수를 넘는 것이 있으면 설명을 돌려준다
fn excess(bytes: &[u8], got: &[Signal]) -> Option<String> {
    let o = oracle(bytes);
    let mut per: HashMap<(char, &str), (usize, usize)> = HashMap::new();
    let mut erase = 0;
    for s in got {
        match s {
            Signal::Erase { .. } => erase += 1,
            // ENQ는 CSI의 신호가 아니다(#603에서 따로 센다)
            Signal::Enquiry => {}
            s => {
                let Some((f, i, max)) = key(s) else {
                    return Some(format!("CSI에서 나올 수 없는 신호 {s:?}"));
                };
                per.entry((f, i)).or_insert((0, max)).0 += 1;
            }
        }
    }
    if erase > o.ed + o.ris {
        return Some(format!("Erase {erase} > 엔진 ED {} + RIS {}", o.ed, o.ris));
    }
    for ((f, i), (n, max)) in per {
        let want = o.count(f, i);
        if n > want * max {
            return Some(format!("{f}{i:?} 신호 {n} > 엔진 dispatch {want} x {max}"));
        }
    }
    None
}

#[test]
fn csi_signals_follow_the_engine_dispatch() {
    // 엔진이 CsiIgnore로 보내거나 다른 꼴로 dispatch하는 바이트열에서는 하나도 나오지 않는다
    for bytes in [
        &b"\x1b[(2J"[..],
        b"\x1b[ 3J",
        b"\x1b[>2J",
        b"\x1b[2$J",
        b"\x1b[\x80$3J",
        b"\x1b[\x9d(2J",
        b"\x1b[?(2031h",
        b"\x1b[2$h",
        b"\x1b[>$c",
        b"\x1b[1>c",
        b"\x1b[ c",
        b"\x1b[(21t",
        b"\x1b[(2h",
    ] {
        assert_eq!(excess(bytes, &feed_all(bytes)), None, "{bytes:?}");
    }
    // 같은 꼴로 dispatch되는 것은 그대로 읽는다
    for (bytes, want) in [
        (&b"\x1b[c"[..], vec![Signal::Da1]),
        (b"\x1b[>c", vec![Signal::Da2]),
        (b"\x1b[>q", vec![Signal::XtVersion]),
        (b"\x1b[21t", vec![Signal::TitleReport]),
        (b"\x1b[2h", vec![Signal::Kam(true)]),
        (b"\x1b[?2031h", vec![Signal::ColorSchemeUpdates(true)]),
        (b"\x1b[!p", vec![Signal::SoftReset]),
        (b"\x1b[?2048$p", vec![Signal::InBandSizeQuery]),
        (b"\x1b[\x7fc", vec![Signal::Da1]),
        (b"\x1b[\x01c", vec![Signal::Da1]),
    ] {
        assert_eq!(feed_all(bytes), want, "{bytes:?}");
        assert_eq!(excess(bytes, &want), None, "{bytes:?}");
    }
}

// ---- 4. 차분 퍼징 ----

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

/// 조각: 흔한 매개변수와 표시, 중간 바이트, 엔진이 무시하거나 실행하는 바이트
const PIECES: &[&[u8]] = &[
    b"0", b"1", b"2", b"3", b"21", b"16", b"19", b"8", b"996", b"2031", b"2033", b"2048", b"5522", b"63", b"15", b";",
    b";", b":", b"<", b"=", b">", b">", b"?", b"?", b" ", b"$", b"!", b"*", b"#", b"(", b"\x7f", b"\x80", b"\x9c",
    b"\x9b", b"\x00", b"\x01", b"\x05", b"\x0a", b"\x18", b"\x1a", b"\x1b",
];

const FINALS: &[u8] = b"JcqtypnhlsrKm@\x7f";

fn make_case(rng: &mut Rng) -> Vec<u8> {
    let mut v = Vec::new();
    for _ in 0..1 + rng.below(3) {
        if rng.below(8) != 0 {
            v.extend_from_slice(b"\x1b[");
        }
        for _ in 0..rng.below(8) {
            v.extend_from_slice(PIECES[rng.below(PIECES.len())]);
        }
        // 매개변수 상한 근처를 자주 건드린다
        if rng.below(10) == 0 {
            v.extend(std::iter::repeat_n(b';', 28 + rng.below(8)));
        }
        v.push(FINALS[rng.below(FINALS.len())]);
        if rng.below(6) == 0 {
            v.extend_from_slice(b"\x1bc");
        }
    }
    v
}

fn fuzz(seed: u64, cases: usize) {
    let mut rng = Rng(seed);
    let mut signals = 0usize;
    for n in 0..cases {
        let bytes = make_case(&mut rng);
        let got = feed_all(&bytes);
        signals += got.len();
        if let Some(why) = excess(&bytes, &got) {
            panic!("case {n}: {why} {bytes:?} -> {got:?}");
        }
        // 두 조각으로 나눠 먹여도 같다
        let cut = rng.below(bytes.len() + 1);
        let mut s = Scanner::default();
        let mut split = s.feed(&bytes[..cut]);
        split.extend(s.feed(&bytes[cut..]));
        assert_eq!(split, got, "case {n}: 조각 {cut} {bytes:?}");
    }
    // 신호가 하나도 안 나오는 퍼저는 아무것도 확인하지 않는다
    assert!(signals > cases / 20, "신호가 너무 적다: {signals}");
}

#[test]
fn scanner_csi_signal_shapes_never_exceed_the_engine() {
    fuzz(0x0076_5EED, 200_000);
}

#[test]
#[ignore = "큰 판: cargo test -p bt-term --test scanner_csi76 --release -- --ignored"]
fn scanner_csi_signal_shapes_never_exceed_the_engine_large() {
    for seed in 1..=10u64 {
        fuzz(0x0076_0000 + seed, 1_000_000);
    }
}
