//! #793: 8비트 C1(0x80~0x9F)을 Escape·CSI·DCS 진입 상태에서도 Ghostty 1.4.0 `parse_table.zig` 51~66줄의
//! "anywhere" 전이대로 읽는다(#775는 바닥 상태, #776은 APC·SOS·PM만 고쳤다).
//!
//! 표(`parse_table.zig`): 모든 상태에 0x80~0x8F·0x91~0x97·0x99·0x9A는 Ground+실행, 0x9C는 Ground, 0x98·0x9E·0x9F는 SOS·PM·APC 문자열,
//! 0x9B는 CsiEntry, 0x90은 DcsEntry, 0x9D는 OscString이 깔린다. 그 위에 상태마다 덮는 것 가운데 0x80 이상을 건드리는 것은
//! `dcs_passthrough`(0x80~0xFF 자료, 265줄), `dcs_ignore`(0x80~0xFF 무시, 215줄), `osc_string`(0x20~0xFF 자료, 358줄) 셋뿐이다.
//! 그래서 Escape·EscapeIntermediate·CsiEntry·CsiParam·CsiIntermediate·CsiIgnore·DcsEntry·DcsParam·DcsIntermediate(과 APC)에는
//! 그대로 적용되고, OSC 문자열과 DCS 통과·무시는 자료다. 0xA0~0xFF는 표에 없어 어느 상태에서나 머문다(바닥 상태만 UTF-8 디코더).
//! 실행형 C1은 Ghostty `execute`가 `ESC (c-0x40)`로 보낸다.

use alacritty_terminal::vte::{Params, Parser, Perform};
use bt_core::Theme;
use bt_term::notify::{Scanner, Signal};
use bt_term::{CellFlags, Emulator, GridSize};

fn emu() -> Emulator {
    Emulator::new(GridSize::new(40, 6), Theme::default())
}

fn screen_of(bytes: &[u8]) -> Emulator {
    let mut e = emu();
    e.feed(bytes);
    e
}

fn text(bytes: &[u8]) -> String {
    screen_of(bytes).snapshot().text()
}

fn feed_all(b: &[u8]) -> Vec<Signal> {
    Scanner::default().feed(b)
}

/// 엔진이 본 일(vte 파서에 직접)
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

/// 이름과 그 상태로 들어가는 앞부분
const STATES: [(&str, &[u8]); 9] = [
    ("escape", b"\x1b"),
    ("escape_intermediate", b"\x1b("),
    ("csi_entry", b"\x1b["),
    ("csi_param", b"\x1b[1"),
    ("csi_intermediate", b"\x1b[ "),
    ("csi_ignore", b"\x1b[1<"),
    ("dcs_entry", b"\x1bP"),
    ("dcs_param", b"\x1bP1"),
    ("dcs_intermediate", b"\x1bP$"),
];

fn cat(parts: &[&[u8]]) -> Vec<u8> {
    parts.concat()
}

#[test]
fn issue_example_9b_restarts_the_csi() {
    // 이슈 재현: `ESC [ 1 0x9B 2 m`. 0x9B가 CSI를 새로 시작해 `2m`만 읽는다
    // 전에는 0x9B를 무시해 `ESC [ 1 2 m`(12m)으로 읽었다. 밑줄(4)로 보면 뚜렷하다
    let mut e = screen_of(b"\x1b[1\x9b4m\xea\xb8\x80");
    let s = e.snapshot();
    let f = s.cell(0, 0).flags;
    assert!(f.contains(CellFlags::UNDERLINE), "{f:?}");
    assert!(!f.contains(CellFlags::BOLD), "{f:?}");
    // 커서 위치로도: `ESC [ 1 0x9B 3;4 H`는 3행 4열
    let mut e = screen_of(b"\x1b[1\x9b3;4HX");
    assert_eq!(e.snapshot().row_text(2), "   X");
}

#[test]
fn st_ends_every_state_and_text_prints() {
    for (name, pre) in STATES {
        assert_eq!(text(&cat(&[pre, b"\x9cAB"])), "AB", "{name}");
    }
}

#[test]
fn execute_type_c1_goes_to_ground_and_runs_as_esc_final() {
    for (name, pre) in STATES {
        for c in (0x80u8..=0x8f).chain(0x91..=0x97).chain([0x99, 0x9a]) {
            let t = text(&cat(&[pre, &[c], b"AB"]));
            assert!(t.contains("AB"), "{name} {c:#04x}: {t:?}");
        }
        // 0x85는 NEL(줄바꿈), 0x84는 IND
        let mut e = screen_of(&cat(&[pre, b"\x85AB"]));
        assert_eq!(e.snapshot().row_text(1), "AB", "{name} NEL");
    }
}

#[test]
fn csi_dcs_osc_introducers_restart_from_every_state() {
    for (name, pre) in STATES {
        // 0x9B: CsiEntry. `3;4H`는 3행 4열로 가는 CUP
        let mut e = screen_of(&cat(&[pre, b"\x9b3;4HX"]));
        assert_eq!(e.snapshot().row_text(2), "   X", "{name} 0x9B");
        // 0x90: DcsEntry. `qabc`는 DCS 통과라 찍히지 않고 `ESC \` 뒤가 찍힌다
        assert_eq!(text(&cat(&[pre, b"\x90qabc\x1b\\AB"])), "AB", "{name} 0x90");
        // 0x9D: OscString. BEL까지가 OSC
        assert_eq!(text(&cat(&[pre, b"\x9d0;t\x07AB"])), "AB", "{name} 0x9D");
    }
}

#[test]
fn string_introducers_swallow_until_st() {
    for (name, pre) in STATES {
        for c in [0x98u8, 0x9e, 0x9f] {
            assert_eq!(text(&cat(&[pre, &[c], b"AB"])), "", "{name} {c:#04x}");
            assert_eq!(text(&cat(&[pre, &[c], b"AB\x1b\\CD"])), "CD", "{name} {c:#04x} ST");
        }
    }
}

#[test]
fn high_bytes_not_in_the_table_stay_in_the_state() {
    // 0xA0~0xFF는 표에 없어 상태에 머문다: 그 바이트를 뺀 입력과 같게 보인다
    for (name, pre) in STATES {
        for c in 0xa0u8..=0xff {
            for tail in [&b"3;4HX"[..], b"\x1b\\AB", b"AB"] {
                let with = text(&cat(&[pre, &[c], tail]));
                let without = text(&cat(&[pre, tail]));
                assert_eq!(with, without, "{name} {c:#04x} {tail:?}");
            }
        }
    }
}

#[test]
fn osc_string_and_dcs_passthrough_and_ignore_keep_high_bytes_as_data() {
    // Ghostty 표가 덮는 세 상태. C1 전부가 자료(또는 무시)이고 상태를 바꾸지 않는다
    for c in 0x80u8..=0xff {
        let o = oracle(&cat(&[b"\x1b]0;a", &[c], b"b\x07"]));
        assert_eq!(o.len(), 1, "OSC {c:#04x}: {o:?}");
        assert!(o[0].starts_with("osc"), "OSC {c:#04x}: {o:?}");
        let o = oracle(&cat(&[b"\x1bPqa", &[c], b"b\x1b\\"]));
        assert_eq!(o.first().map(String::as_str), Some("hook q"), "{o:?}");
        assert!(o.contains(&format!("put {c:#04x}")), "DCS 통과 {c:#04x}: {o:?}");
        assert!(
            !o.iter().any(|s| s.starts_with("print") || s.starts_with("csi")),
            "{c:#04x}: {o:?}"
        );
        let o = oracle(&cat(&[b"\x1bP$0a", &[c], b"b\x1b\\"]));
        assert!(
            !o.iter().any(|s| s.starts_with("hook") || s.starts_with("print")),
            "무시 {c:#04x}: {o:?}"
        );
    }
}

#[test]
fn scanner_follows_the_engine_after_a_c1_in_each_state() {
    for (name, pre) in STATES {
        // 0x9B 뒤 `c`는 DA1(CSI c)이다
        assert_eq!(feed_all(&cat(&[pre, b"\x9bc"])), vec![Signal::Da1], "{name} 0x9B");
        // 0x9C·실행형 C1 뒤의 `c`는 그냥 글자(질의 아님)
        for c in [0x9cu8, 0x85, 0x80] {
            assert_eq!(
                feed_all(&cat(&[pre, &[c], b"\x1b[c"])),
                vec![Signal::Da1],
                "{name} {c:#04x}"
            );
            assert_eq!(feed_all(&cat(&[pre, &[c], b"c"])), vec![], "{name} {c:#04x} c");
        }
        // 0x9D 뒤는 OSC라 `ESC [ c`가 질의가 아니다(ESC는 OSC를 끝내고 새 이스케이프를 연다: 질의가 된다)
        assert_eq!(
            feed_all(&cat(&[pre, b"\x9d0;t\x07\x1b[c"])),
            vec![Signal::Da1],
            "{name} 0x9D"
        );
        assert_eq!(feed_all(&cat(&[pre, b"\x9dc\x07"])), vec![], "{name} 0x9D 안");
        // 0x90 뒤는 DCS: `$qm`에 ST로 답한다
        let got = feed_all(&cat(&[pre, b"\x90$qm\x1b\\"]));
        assert_eq!(got.len(), 1, "{name} 0x90: {got:?}");
        // 문자열 시작 C1은 ST까지 버린다
        assert_eq!(
            feed_all(&cat(&[pre, b"\x9fa\x1b\\\x05"])),
            vec![Signal::Enquiry],
            "{name} 0x9F"
        );
        assert_eq!(feed_all(&cat(&[pre, b"\x9fa\x05"])), vec![], "{name} 0x9F ENQ는 자료");
        // 0xA0 이상은 머문다: 뒤의 ESC [ c는 그대로 읽는다
        assert_eq!(feed_all(&cat(&[pre, b"\xa0\x1b[c"])), vec![Signal::Da1], "{name} 0xA0");
    }
}

#[test]
fn scanner_and_engine_agree_on_a_full_sweep() {
    // 상태 × 모든 높은 바이트 × `0x9B c`: 엔진 파서가 CSI `c`를 dispatch하는지와 스캐너가 DA1을 내는지가 같다
    for (name, pre) in STATES {
        for c in 0x80u8..=0xff {
            let b = cat(&[pre, &[c], b"\x9bc"]);
            let dispatched = oracle(&b).contains(&"csi c".to_string());
            let signalled = feed_all(&b).contains(&Signal::Da1);
            assert_eq!(dispatched, signalled, "{name} {c:#04x}");
        }
    }
}

#[test]
fn emulator_still_reads_the_text_after_a_c1_restart() {
    // 엔진과 스캐너를 함께 도는 Emulator에서도 같다
    let mut e = emu();
    let sigs = e.feed(b"\x1b[1\x9bc");
    assert_eq!(sigs, vec![Signal::Da1]);
}
