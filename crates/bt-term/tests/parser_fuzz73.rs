use std::sync::Arc;
use std::time::{Duration, Instant};

use alacritty_terminal::event::EventListener;
use alacritty_terminal::grid::Dimensions;
use alacritty_terminal::term::{Config, Term};
use bt_core::Theme;
use bt_term::graphics::{Cursor, Graphics};
use bt_term::notify::{Scanner, Signal};
use bt_term::{Emulator, GridSize};

/// 일반 판은 `default`, 3만 회 판(`#[ignore]`)은 `scale`로 덮는다
fn iters(default: usize, scale: usize) -> usize {
    if scale == 0 { default } else { scale }
}
/// 시드는 고정이다(결정적)
fn seed() -> u64 {
    0x005E_ED73
}

struct Rng(std::cell::Cell<u64>);
impl Rng {
    fn next(&self) -> u64 {
        let mut x = self.0.get();
        x ^= x >> 12;
        x ^= x << 25;
        x ^= x >> 27;
        self.0.set(x);
        x.wrapping_mul(0x2545F4914F6CDD1D)
    }
    fn below(&self, n: usize) -> usize {
        if n == 0 { 0 } else { (self.next() % n as u64) as usize }
    }
    fn chance(&self, n: usize) -> bool {
        self.below(n) == 0
    }
    fn pick<'a, T>(&self, v: &'a [T]) -> &'a T {
        &v[self.below(v.len())]
    }
}

const NUMS: &[&str] = &[
    "",
    "0",
    "1",
    "2",
    "3",
    "7",
    "10",
    "99",
    "255",
    "256",
    "1000",
    "4096",
    "65535",
    "65536",
    "99999",
    "2147483647",
    "2147483648",
    "4294967295",
    "4294967296",
    "18446744073709551615",
    "18446744073709551616",
    "-1",
    "-2147483648",
    "-99999999999",
    "99999999999999999999999999",
    "1e9",
    "0x10",
    "+5",
    " 5",
    "٣",
];

fn num(r: &Rng) -> String {
    if r.chance(4) {
        r.below(40).to_string()
    } else {
        r.pick(NUMS).to_string()
    }
}

const B64: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
fn b64(r: &Rng, n: usize) -> String {
    let mut s: String = (0..n).map(|_| B64[r.below(64)] as char).collect();
    match r.below(6) {
        0 => s.push('='),
        1 => s.push_str("=="),
        2 => s.push_str("==="),
        3 if !s.is_empty() => s.insert(r.below(s.len()), '='),
        _ => {}
    }
    s
}

fn rand_bytes(r: &Rng, n: usize) -> Vec<u8> {
    (0..n).map(|_| r.next() as u8).collect()
}

fn lens(r: &Rng) -> usize {
    match r.below(12) {
        0 => 2047,
        1 => 2048,
        2 => 2049,
        3 => 4095,
        4 => 4096,
        5 => 4097,
        6 => 8191,
        7 => 8192,
        8 => 8193,
        9 => 8 * 1024 * 1024 + r.below(3) - 1,
        _ => r.below(300),
    }
}

fn meta_kv(r: &Rng, keys: &[&str]) -> String {
    let n = r.below(8);
    let mut v = Vec::new();
    for _ in 0..n {
        let k = *r.pick(keys);
        let val = match r.below(6) {
            0 => num(r),
            1 => [
                "a", "b", "d", "p", "q", "T", "t", "f", "s", "?", "title", "body", "data", "alive", "close", "icon",
                "buttons",
            ][r.below(17)]
            .to_string(),
            2 => (0..r.below(200)).map(|_| (b'a' + r.below(26) as u8) as char).collect(),
            3 => String::new(),
            _ => r.below(3).to_string(),
        };
        v.push(format!("{k}={val}"));
    }
    v.join(if r.chance(10) { ";" } else { "," })
}

fn gfx_keys() -> Vec<&'static str> {
    vec![
        "a", "t", "f", "o", "s", "v", "S", "O", "i", "I", "p", "m", "q", "x", "y", "w", "h", "X", "Y", "c", "r", "C",
        "z", "U", "P", "Q", "H", "V", "d", "e", "Z", "b", "B", "R", "u", "j", "k",
    ]
}

fn gfx_apc(r: &Rng) -> Vec<u8> {
    let keys = gfx_keys();
    let mut meta = String::new();
    if r.below(4) != 0 {
        meta.push_str(
            [
                "a=T",
                "a=t",
                "a=p",
                "a=d",
                "a=q",
                "a=f",
                "a=a",
                "a=c",
                "a=T,f=32",
                "a=t,f=24",
                "a=T,f=100",
                "a=T,f=32,s=1,v=1",
            ][r.below(12)],
        );
        meta.push(',');
    }
    meta.push_str(&meta_kv(r, &keys));
    let mut out = b"\x1b_G".to_vec();
    out.extend(meta.bytes());
    if r.below(5) != 0 {
        out.push(b';');
        let n = r.below(3);
        match n {
            0 => out.extend(b64(r, r.below(64)).bytes()),
            1 => out.extend(b64(r, lens(r).min(70000)).bytes()),
            _ => out.extend(rand_bytes(r, r.below(50))),
        }
    }
    if r.below(10) != 0 {
        out.extend(b"\x1b\\");
    }
    out
}

fn osc(r: &Rng) -> Vec<u8> {
    let mut out = b"\x1b]".to_vec();
    let body: String = match r.below(14) {
        0 => format!(
            "99;{};{}",
            meta_kv(
                r,
                &["i", "d", "e", "p", "a", "o", "u", "w", "c", "s", "t", "f", "g", "n"]
            ),
            {
                let n = lens(r);
                if r.chance(2) {
                    b64(r, n)
                } else {
                    (0..n).map(|_| (b'a' + r.below(26) as u8) as char).collect()
                }
            }
        ),
        1 => format!(
            "99;i={}:p=?;",
            [
                "x",
                "ab-_+.",
                "a b",
                "a\x01b",
                "é",
                &"a".repeat(129),
                &"a".repeat(128),
                ""
            ][r.below(8)]
        ),
        2 => format!(
            "52;{};{}",
            ["c", "p", "s", "", "cp", "0", "7", "?"][r.below(8)],
            b64(r, lens(r).min(20000))
        ),
        3 => format!(
            "133;{}{}",
            ["A", "B", "C", "D", "L", "N", "P", "I", "A;aid=", "D;", "L;x", "Z"][r.below(12)],
            {
                let mut s = String::new();
                for _ in 0..r.below(6) {
                    s.push(';');
                    s.push_str(r.pick(&[
                        "aid=",
                        "k=i",
                        "k=s",
                        "k=r",
                        "k=c",
                        "redraw=0",
                        "redraw=1",
                        "redraw=last",
                        "click_events=1",
                        "cl=line",
                        "cl=m",
                        "cl=v",
                        "cl=w",
                        "cl=",
                        "x=y",
                    ]));
                    if r.chance(3) {
                        s.push_str(&num(r));
                    }
                }
                s
            }
        ),
        4 => format!(
            "633;{};{}",
            ["A", "B", "C", "D", "E"][r.below(5)],
            ["", "ls\\x3b -l", "\\\\x", &"x".repeat(900), "a;b;c"][r.below(5)]
        ),
        5 => format!(
            "7;{}",
            [
                "file://host/a%20b",
                "file://h/%ZZ",
                "file:///%00",
                "kitty-shell-cwd://h/x\u{0}y",
                "file://",
                "",
                "file://h",
                &format!("file://{}/x", "h".repeat(400)),
                &format!("file:///{}", "%41".repeat(3000))
            ][r.below(9)]
        ),
        6 => format!(
            "9;{}",
            [
                "4;1;50",
                "4;",
                "4;9;999",
                "4;1;-1",
                "4;300;5",
                "4;;",
                "1;2",
                "hello",
                "4;1;1;1;1;1",
                "99999999999"
            ][r.below(10)]
        ),
        7 => format!(
            "777;notify;{};{}",
            "t".repeat(lens(r).min(3000)),
            "b".repeat(lens(r).min(3000))
        ),
        8 => format!("21;{}", {
            let n = r.below(600);
            (0..n.max(1))
                .map(|_| {
                    [
                        "foreground=?",
                        "background=#fff",
                        "cursor=",
                        "0=rgb:1/2/3",
                        "255=?",
                        "256=?",
                        "-1=?",
                        "selection_background=?",
                        "x=y",
                        "1=#ggg",
                        "foreground=rgb:ff/ff/ff",
                        "",
                        "foreground= ? ",
                    ][r.below(13)]
                    .to_string()
                })
                .collect::<Vec<_>>()
                .join(";")
        }),
        9 => format!(
            "22;{}",
            [
                "pointer",
                "",
                "text",
                &"x".repeat(100),
                "pointer\x01",
                "bottom_right_corner"
            ][r.below(6)]
        ),
        10 => format!(
            "5522;{};{}",
            meta_kv(
                r,
                &["type", "id", "mime", "name", "pw", "loc", "status", "password", "dir"]
            ),
            {
                let n = lens(r).min(6000);
                if r.chance(2) {
                    b64(r, n)
                } else {
                    rand_bytes(r, n.min(300))
                        .iter()
                        .map(|b| (b % 90 + 32) as char)
                        .collect()
                }
            }
        ),
        11 => format!(
            "5522;type={}:id={}:mime={}:name={}:pw={};{}",
            ["read", "write", "wdata", "walias", "bogus"][r.below(5)],
            ["a", "b c", "", "a\x1b"][r.below(4)],
            ["dGV4dC9wbGFpbg==", "text/plain", "", "."][r.below(4)],
            b64(r, 8),
            b64(r, 8),
            b64(r, r.below(100))
        ),
        12 => format!("8;;{}", ["http://a", "", &"a".repeat(3000)][r.below(3)]),
        _ => String::from_utf8_lossy(&rand_bytes(r, r.below(100))).into_owned(),
    };
    out.extend(body.bytes());
    match r.below(8) {
        0 => {}
        1 => out.push(0x07),
        2 => out.extend(b"\x1b"),
        _ => out.extend(b"\x1b\\"),
    }
    out
}

fn dcs(r: &Rng) -> Vec<u8> {
    let mut out = b"\x1bP".to_vec();
    match r.below(8) {
        0 => out.extend(b"$q"),
        1 => out.extend(b"+q"),
        2 => out.extend(format!("{};{}$q", num(r), num(r)).bytes()),
        3 => out.extend(b"1$q"),
        4 => out.extend(b"$$q"),
        5 => out.extend(b"q"),
        6 => out.extend(rand_bytes(r, r.below(30)).iter().map(|b| b | 0x20)),
        _ => out.extend(b"+q"),
    }
    match r.below(6) {
        0 => out.extend(["m", "r", "s", " q", "mm", "mmm", "\"q", "x"][r.below(8)].bytes()),
        1 => {
            let names = [
                "544e",
                "436f",
                "5443",
                "524742",
                "tn",
                "zz",
                "696e646e",
                "Co",
                "colors",
                "7365746166",
                "00",
            ];
            for i in 0..r.below(120) {
                if i > 0 {
                    out.push(b';');
                }
                out.extend(r.pick(&names).bytes());
            }
        }
        2 => out.extend(vec![b'4'; lens(r).min(9000)]),
        3 => out.extend(rand_bytes(r, r.below(200))),
        4 => out.extend(vec![b'4'; 4095 + r.below(3)]),
        _ => {}
    }
    match r.below(6) {
        0 => {}
        1 => out.extend(b"\x1b"),
        2 => out.extend(b"\x18"),
        _ => out.extend(b"\x1b\\"),
    }
    out
}

fn csi(r: &Rng) -> Vec<u8> {
    let mut out = b"\x1b[".to_vec();
    if r.chance(3) {
        out.push(*r.pick(b"?><=!"));
    }
    for i in 0..r.below(10) {
        if i > 0 {
            out.push(b';');
        }
        out.extend(num(r).bytes().filter(|b| b.is_ascii_digit()));
    }
    if r.chance(4) {
        out.push(*r.pick(b"*#!$ \"':"));
    }
    if r.chance(12) {
        out.extend(vec![b'1'; 30 + r.below(30)]);
    }
    out.push(*r.pick(b"yhlnqpcrstJmHABCDKLMPSTXZ@dfgijuxz~ "));
    out
}

/// #598: 스캐너와 엔진이 다르게 읽던 바이트(C0, 8비트 ST, SOS·PM)를 섞는다
fn gap_bytes(r: &Rng) -> Vec<u8> {
    let mut v = Vec::new();
    for _ in 0..1 + r.below(6) {
        match r.below(8) {
            0 => v.push(*r.pick(&[0x00, 0x01, 0x05, 0x08, 0x0a, 0x0d, 0x17, 0x19, 0x1c, 0x1f])),
            1 => v.push(0x9c),
            2 => v.extend(b"\x1bX"),
            3 => v.extend(b"\x1b^"),
            4 => v.extend(b"\x1b\\"),
            5 => v.extend([0x1b, *r.pick(&[0x18u8, 0x1a, 0x05, 0x0a])]),
            6 => v.extend(*r.pick(&[
                &b"\x1b]133;A"[..],
                b"\x1bP$qm",
                b"\x1bP+q544e",
                b"\x1b[c",
                b"\x1b]9;hi\x07",
            ])),
            _ => v.extend(rand_bytes(r, r.below(8))),
        }
    }
    v
}

fn chunk(r: &Rng) -> Vec<u8> {
    match r.below(14) {
        12 | 13 => gap_bytes(r),
        0..=2 => gfx_apc(r),
        3..=5 => osc(r),
        6 | 7 => dcs(r),
        8 | 9 => csi(r),
        10 => rand_bytes(r, r.below(200)),
        _ => b"hello \x05\x1bc\x1b[2J\x1b[3J\x1b[!p\x1b[?2031h\x1b[?5522$p\r\n".to_vec(),
    }
}

fn placeholders(r: &Rng) -> Vec<u8> {
    let marks = [
        '\u{305}',
        '\u{30D}',
        '\u{30E}',
        '\u{310}',
        '\u{312}',
        '\u{33D}',
        '\u{33E}',
        '\u{33F}',
        '\u{346}',
        '\u{34A}',
        '\u{1D185}',
        '\u{FE20}',
        '\u{FFFF}',
        '\u{301}',
    ];
    let mut out = String::new();
    for _ in 0..1 + r.below(4) {
        out.push_str(&match r.below(4) {
            0 => format!("\x1b[38;5;{}m", r.below(300)),
            1 => format!("\x1b[38;2;{};{};{}m", r.below(300), r.below(300), r.below(300)),
            2 => format!("\x1b[58;5;{}m", r.below(10)),
            _ => "\x1b[0m".to_string(),
        });
        for _ in 0..r.below(60) {
            out.push('\u{10EEEE}');
            for _ in 0..r.below(4) {
                out.push(*r.pick(&marks));
            }
        }
        if r.chance(3) {
            out.push_str("\r\n");
        }
    }
    out.into_bytes()
}

fn emu_chunk(r: &Rng) -> Vec<u8> {
    match r.below(10) {
        0 => placeholders(r),
        1 => [
            "\x1b[?1049h",
            "\x1b[?1049l",
            "\x1b[?47h",
            "\x1b[2J",
            "\x1b[3J",
            "\x1bc",
            "\x1b[H",
            "\n\n\n\n",
            "\x1b[5S",
            "\x1b[5T",
            "\x1b[3;8r",
        ][r.below(11)]
        .as_bytes()
        .to_vec(),
        _ => chunk(r),
    }
}

fn buffer(r: &Rng) -> Vec<u8> {
    let mut v = Vec::new();
    for _ in 0..1 + r.below(6) {
        v.extend(chunk(r));
    }
    v
}

fn reply_ok(r: &[u8]) -> Result<(), String> {
    let mut i = 0;
    while i < r.len() {
        if r[i] != 0x1b {
            return Err(format!(
                "답이 ESC로 시작하지 않는다 @{i}: {:?}",
                &r[i.saturating_sub(4)..(i + 8).min(r.len())]
            ));
        }
        i += 1;
        let intro = r.get(i).copied();
        if !matches!(intro, Some(b']' | b'_' | b'P' | b'[')) {
            return Err(format!("알 수 없는 도입 {:?}", intro));
        }
        i += 1;
        let start = i;
        loop {
            match r.get(i) {
                None => {
                    return Err(format!(
                        "끝맺음 없음: {:?}",
                        &r[start.saturating_sub(2)..r.len().min(start + 60)]
                    ));
                }
                Some(0x07) => {
                    i += 1;
                    break;
                }
                Some(0x1b) if r.get(i + 1) == Some(&b'\\') => {
                    i += 2;
                    break;
                }
                Some(b) if (0x20..0x7f).contains(b) => i += 1,
                Some(b) => {
                    return Err(format!(
                        "답 안에 제어/비ASCII 바이트 {b:#x} @{i}: {:?}",
                        &r[start.saturating_sub(2)..r.len().min(i + 20)]
                    ));
                }
            }
        }
    }
    Ok(())
}

fn check_signal(sig: &Signal, whole_len: usize) -> Result<(), String> {
    match sig {
        Signal::Notify(n) => {
            if n.title.chars().count() > 120 {
                return Err(format!("제목 {}자", n.title.chars().count()));
            }
            if n.body.chars().count() > 500 {
                return Err(format!("본문 {}자", n.body.chars().count()));
            }
            if n.title.chars().chain(n.body.chars()).any(char::is_control) {
                return Err("알림에 제어 문자".into());
            }
        }
        Signal::Cwd { host, path } => {
            if host.chars().count() > 255 {
                return Err("호스트 255자 초과".into());
            }
            if host.chars().any(char::is_control) {
                return Err("host 제어 문자".into());
            }
            if path.as_os_str().len() > whole_len {
                return Err("경로가 입력보다 길다".into());
            }
        }
        Signal::CommandLine(c) => {
            if c.chars().count() > 500 {
                return Err("명령줄 500자 초과".into());
            }
            if c.chars().any(char::is_control) {
                return Err("명령줄 제어 문자".into());
            }
        }
        Signal::Progress { state, percent } => {
            if *state > 4 || percent.is_some_and(|p| p > 100) {
                return Err("진행 범위".into());
            }
        }
        Signal::Graphics(g) => {
            if g.len() > 8 * 1024 * 1024 {
                return Err("APC 8MiB 초과".into());
            }
        }
        Signal::KittyClipboard { packet, .. } => {
            if packet.len() > 8 * 1024 {
                return Err("5522 패킷 8KiB 초과".into());
            }
        }
        Signal::DcsQuery(bt_term::dcs::Query::XtGetTcap(b)) => {
            if b.len() > 4096 {
                return Err("XTGETTCAP 본문 4096 초과".into());
            }
        }
        Signal::KittyNotifyReply(r) => reply_ok(r)?,
        Signal::KittyColor { requests, .. } if requests.len() > bt_term::kitty_color::MAX_ITEMS => {
            return Err("OSC 21 항목 상한".into());
        }
        _ => {}
    }
    Ok(())
}

fn fail(what: &str, msg: &str, input: &[u8]) -> ! {
    let show = &input[..input.len().min(600)];
    panic!(
        "{what}: {msg}\n입력({}바이트, 앞 600): {:?}",
        input.len(),
        String::from_utf8_lossy(show)
    );
}

fn run_scanner_fuzz(scale: usize) {
    let r = Rng(std::cell::Cell::new(seed()));
    let mut sc = Scanner::default();
    let n = iters(300, scale);
    let mut slow = Duration::ZERO;
    for _ in 0..n {
        let buf = buffer(&r);
        let t = Instant::now();
        let sigs = sc.feed_at(&buf);
        slow = slow.max(t.elapsed());
        let mut last = 0;
        for (at, s) in &sigs {
            if *at > buf.len() || *at < last {
                fail("scanner", &format!("자리 {at} (이전 {last}, 길이 {})", buf.len()), &buf);
            }
            last = *at;
            if let Err(e) = check_signal(s, buf.len()) {
                fail("scanner", &e, &buf);
            }
        }
    }
    eprintln!("scanner: {n}회, 한 번 최대 {slow:?}");
}

/// 나눠 먹여도 신호가 같아야 한다
fn run_scanner_split_invariance(scale: usize) {
    let r = Rng(std::cell::Cell::new(seed() ^ 0xABCD));
    let n = iters(200, scale);
    for _ in 0..n {
        let buf = buffer(&r);
        let mut a = Scanner::default();
        let whole: Vec<String> = a.feed(&buf).iter().map(|s| format!("{s:?}")).collect();
        let mut b = Scanner::default();
        let mut parts = Vec::new();
        let mut pos = 0;
        while pos < buf.len() {
            let step = 1 + r.below(if r.chance(2) { 3 } else { 5000 });
            let end = (pos + step).min(buf.len());
            parts.extend(b.feed(&buf[pos..end]).iter().map(|s| format!("{s:?}")));
            pos = end;
        }
        if whole != parts {
            let i = whole
                .iter()
                .zip(&parts)
                .position(|(x, y)| x != y)
                .unwrap_or(whole.len().min(parts.len()));
            let a_s = whole.get(i).map(|s| s.chars().take(200).collect::<String>());
            let b_s = parts.get(i).map(|s| s.chars().take(200).collect::<String>());
            fail(
                "split",
                &format!(
                    "신호 {}개 vs {}개, 처음 다른 곳 {i}: {a_s:?} / {b_s:?}",
                    whole.len(),
                    parts.len()
                ),
                &buf,
            );
        }
    }
}

fn run_assembler_fuzz(scale: usize) {
    let r = Rng(std::cell::Cell::new(seed() ^ 0x99));
    let mut a = bt_term::kitty_notify::Assembler::default();
    let n = iters(3000, scale);
    let mut max_pending = 0;
    let mut max_held = 0;
    for _ in 0..n {
        let meta = meta_kv(
            &r,
            &["i", "d", "e", "p", "a", "o", "u", "w", "c", "s", "t", "f", "g", "n"],
        );
        let plen = lens(&r).min(5000);
        let payload: String = match r.below(3) {
            0 => b64(&r, plen),
            1 => (0..plen)
                .map(|_| char::from_u32(0x20 + r.below(0x3000) as u32).unwrap_or('x'))
                .collect(),
            _ => "\u{1}\u{85}한글🙂abc ".repeat(plen / 10),
        };
        let sep = if r.chance(8) { "" } else { ";" };
        let packet = format!("{meta}{sep}{payload}");
        let id_mix = format!(
            "i={}:{}",
            ["a", "b", "c", "d", &r.below(40).to_string()][r.below(5)],
            packet
        );
        let packet = if r.chance(2) { id_mix } else { packet };
        match a.packet(&packet, r.chance(2)) {
            bt_term::kitty_notify::Outcome::Reply(rep) => {
                if let Err(e) = reply_ok(&rep) {
                    fail("assembler reply", &e, packet.as_bytes());
                }
            }
            bt_term::kitty_notify::Outcome::Notify(nn)
                if nn.title.chars().count() > 120 || nn.body.chars().count() > 500 =>
            {
                fail("assembler", "알림 길이 상한", packet.as_bytes());
            }
            _ => {}
        }
        max_pending = max_pending.max(a.pending_len());
        max_held = max_held.max(a.held_chars());
        if a.pending_len() > 16 {
            fail("assembler", &format!("쌓임 {}개", a.pending_len()), packet.as_bytes());
        }
        if a.held_chars() > 16 * (121 + 501) {
            fail("assembler", &format!("쌓은 글자 {}", a.held_chars()), packet.as_bytes());
        }
    }
    eprintln!("assembler: {n}회, 최대 쌓임 {max_pending}개 {max_held}자");
}

fn cursor(r: &Rng) -> Cursor {
    Cursor {
        line: r.below(120) as i64 + if r.chance(8) { -50 } else { 0 },
        col: r.below(200),
        cell_w: 1 + r.below(40) as u32,
        cell_h: 1 + r.below(80) as u32,
        alt: r.chance(3),
        top: r.below(60) as i64,
        rows: 1 + r.below(100) as u32,
        cols: 2 + r.below(250) as u32,
    }
}

fn rgba_payload(r: &Rng, w: usize, h: usize) -> String {
    let raw = rand_bytes(r, w * h * 4);
    bt_term::kitty_clip::b64_encode(&raw)
}

fn png_bytes(r: &Rng, w: u32, h: u32) -> Vec<u8> {
    let mut out = Vec::new();
    {
        let mut enc = png::Encoder::new(&mut out, w, h);
        enc.set_color(
            [
                png::ColorType::Rgba,
                png::ColorType::Rgb,
                png::ColorType::Grayscale,
                png::ColorType::GrayscaleAlpha,
            ][r.below(4)],
        );
        enc.set_depth(png::BitDepth::Eight);
        let mut wr = enc.write_header().unwrap();
        let n = (w * h) as usize * 4;
        let _ = wr.write_image_data(&rand_bytes(r, n));
    }
    out
}

fn structured_gfx(r: &Rng) -> Vec<u8> {
    let id = 1 + r.below(10);
    let (w, h) = (1 + r.below(30) as u32, 1 + r.below(30) as u32);
    let cmd = match r.below(9) {
        0 => {
            let png = png_bytes(r, w, h);
            let mut data = png;
            let mut ex = String::new();
            if r.chance(3) {
                data = miniz_oxide::deflate::compress_to_vec_zlib(&data, 1);
                ex = ",o=z".into();
            }
            format!("a=T,f=100,i={id}{ex};{}", bt_term::kitty_clip::b64_encode(&data))
        }
        1 => {
            // 압축 폭탄: 0 바이트 N개
            static BOMBS: std::sync::OnceLock<Vec<String>> = std::sync::OnceLock::new();
            let bombs = BOMBS.get_or_init(|| {
                // 66MiB는 풀린 크기 상한(64MiB) 바로 위다. debug에서는 압축이 느려 1MiB만 쓴다
                let sizes: &[usize] = if cfg!(debug_assertions) {
                    &[1 << 20]
                } else {
                    &[1 << 20, 66 << 20]
                };
                sizes
                    .iter()
                    .map(|&n| {
                        bt_term::kitty_clip::b64_encode(&miniz_oxide::deflate::compress_to_vec_zlib(&vec![0u8; n], 1))
                    })
                    .collect()
            });
            format!(
                "a=T,f=32,s={},v={},i={id},o=z;{}",
                num(r),
                num(r),
                bombs[r.below(bombs.len())]
            )
        }
        2 => format!(
            "a=t,f=32,s={},v={},i={id}",
            [10000, 9999, 10001, 5000, 3000][r.below(5)],
            [10000, 9999, 1, 5000, 3000][r.below(5)]
        ),
        3 => {
            // 프레임 조각 전송
            let mut v = format!(
                "a=f,f=32,s={w},v={h},i={id},r={},m=1;{}",
                r.below(5),
                rgba_payload(r, w as usize, h as usize)
            );
            if r.chance(3) {
                v = format!("a=f,i={id},m=1,s={w},v={h};{}", b64(r, 8));
            }
            v
        }
        4 => format!(
            "a=c,i={id},r={},c={},x={},y={},w={},h={},X={},Y={},C={},v={},s={}",
            num(r),
            num(r),
            num(r),
            num(r),
            num(r),
            num(r),
            num(r),
            num(r),
            r.below(2),
            num(r),
            num(r)
        ),
        5 => format!(
            "a=a,i={id},s={},v={},r={},c={},z={},x={},y={}",
            r.below(4),
            r.below(4),
            num(r),
            num(r),
            num(r),
            num(r),
            num(r)
        ),
        6 => format!(
            "a=T,f=32,s={w},v={h},i={id},U=1,c={},r={};{}",
            r.below(40),
            r.below(40),
            rgba_payload(r, w as usize, h as usize)
        ),
        7 => format!(
            "a=p,i={id},p={},P={},Q={},H={},V={},c={},r={},z={}",
            r.below(6),
            1 + r.below(10),
            r.below(6),
            num(r),
            num(r),
            r.below(8),
            r.below(8),
            num(r)
        ),
        _ => format!(
            "a=q,i={id},f=32,s=1,v=1,t={};AAAA",
            ["d", "f", "t", "s", "z"][r.below(5)]
        ),
    };
    cmd.into_bytes()
}

fn gfx_raw(r: &Rng) -> Vec<u8> {
    let a = gfx_apc(r);
    let mut v = a[3..].to_vec();
    if v.ends_with(b"\x1b\\") {
        v.truncate(v.len() - 2);
    }
    // 구조 있는 정상 명령도 섞는다
    if r.chance(10) {
        return structured_gfx(r);
    }
    if r.chance(2) {
        let w = 1 + r.below(8);
        let h = 1 + r.below(8);
        let id = 1 + r.below(12);
        let cmd = match r.below(8) {
            0 => format!("a=T,f=32,s={w},v={h},i={id};{}", rgba_payload(r, w, h)),
            1 => format!("a=t,f=32,s={w},v={h},i={id},m=1;{}", rgba_payload(r, w, h)),
            2 => format!("m=0,i={id};{}", b64(r, r.below(40))),
            3 => format!(
                "a=f,f=32,s={w},v={h},i={id},r={},c={},z={};{}",
                num(r),
                num(r),
                num(r),
                rgba_payload(r, w, h)
            ),
            4 => format!(
                "a=p,i={id},p={},c={},r={},x={},y={},w={},h={},X={},Y={},z={},P={},Q={},H={},V={}",
                r.below(5),
                num(r),
                num(r),
                num(r),
                num(r),
                num(r),
                num(r),
                num(r),
                num(r),
                num(r),
                num(r),
                num(r),
                num(r),
                num(r)
            ),
            5 => format!(
                "a=T,f=32,s={w},v={h},i={id},U=1,c={},r={};{}",
                num(r),
                num(r),
                rgba_payload(r, w, h)
            ),
            6 => format!(
                "a=d,d={},i={id},p={},x={},y={},z={}",
                [
                    "a", "A", "i", "I", "p", "P", "q", "Q", "c", "C", "x", "X", "y", "Y", "z", "Z", "n", "N", "r", "R"
                ][r.below(20)],
                r.below(5),
                num(r),
                num(r),
                num(r)
            ),
            _ => format!(
                "a=a,i={id},r={},s={},v={},c={},z={},x={},y={}",
                num(r),
                r.below(4),
                r.below(4),
                num(r),
                num(r),
                num(r),
                num(r)
            ),
        };
        v = cmd.into_bytes();
    }
    v
}

fn run_graphics_fuzz(scale: usize) {
    let r = Rng(std::cell::Cell::new(seed() ^ 0x77));
    let n = iters(600, scale);
    for limit in [0usize, 20_000, 2_000_000] {
        let mut g = Graphics::with_limit(limit);
        let mut slow = (Duration::ZERO, Vec::new());
        for _ in 0..n {
            let raw = gfx_raw(&r);
            let cur = cursor(&r);
            let t = Instant::now();
            let out = g.handle(&raw, cur);
            let el = t.elapsed();
            if el > slow.0 {
                slow = (el, raw.clone());
            }
            if let Some(rep) = &out.reply {
                if let Err(e) = reply_ok(rep) {
                    fail("graphics reply", &e, &raw);
                }
                if !rep.starts_with(b"\x1b_G") {
                    fail("graphics reply", "ESC _ G 아님", &raw);
                }
            }
            if g.placements.len() > 1024 {
                fail("graphics", &format!("자리 {}개", g.placements.len()), &raw);
            }
            // 저장 총량
            let mut total = 0usize;
            let ids = (0..64u32).chain(2_147_483_647..2_147_483_647 + 64);
            for id in ids {
                if let Some(img) = g.image(id) {
                    total += img.rgba.len() + 4096 + img.animation().map_or(0, |a| a.frame_bytes());
                }
            }
            if total > limit {
                fail("graphics", &format!("저장 총량 {total} > 상한 {limit}"), &raw);
            }
        }
        eprintln!(
            "graphics 상한 {limit}: {n}회, 한 번 최대 {:?} (입력 앞 {:?})",
            slow.0,
            String::from_utf8_lossy(&slow.1[..slow.1.len().min(120)])
        );
    }
}

fn run_clip_engine_fuzz(scale: usize) {
    use bt_term::kitty_clip::{Action, Engine, Grants};
    let r = Rng(std::cell::Cell::new(seed() ^ 0x55));
    let n = iters(1500, scale);
    let grants = Arc::new(parking_lot_mutex());
    let mut e = Engine::new(grants.clone(), true, 1024 * 1024);
    for _ in 0..n {
        let packet = {
            let o = osc(&r);
            let s = String::from_utf8_lossy(&o).into_owned();
            let s = s.trim_start_matches("\x1b]").to_string();
            let s = s.strip_prefix("5522;").map(str::to_string).unwrap_or_else(|| {
                format!(
                    "type={}:id={}:mime={}:name={}:pw={};{}",
                    ["read", "write", "wdata", "walias"][r.below(4)],
                    ["a", "b", "z"][r.below(3)],
                    ["dGV4dC9wbGFpbg==", "dGV4dC9wbGFpbg", "!!", ""][r.below(4)],
                    b64(&r, 8),
                    b64(&r, 8),
                    b64(&r, r.below(100))
                )
            });
            s.trim_end_matches(['\x07', '\x1b', '\\']).to_string()
        };
        for a in e.handle(&packet, r.chance(2)) {
            match a {
                Action::Reply(rep) => {
                    if let Err(er) = reply_ok(&rep) {
                        fail("clip reply", &er, packet.as_bytes());
                    }
                }
                Action::Write(w) => {
                    let tot: usize = w.contents.iter().map(|(_, d)| d.len()).sum();
                    if tot > 1024 * 1024 {
                        fail("clip", &format!("쓰기 총량 {tot}"), packet.as_bytes());
                    }
                    if w.contents.len() > 64 {
                        fail("clip", "MIME 64 초과", packet.as_bytes());
                    }
                }
                Action::Read(_) => {}
            }
        }
        if grants.lock().len() > 32 {
            fail("clip", "허가 32 초과", packet.as_bytes());
        }
    }
    let _ = Grants::default();
    eprintln!("clip: {n}회");
}

fn parking_lot_mutex() -> parking_lot::Mutex<bt_term::kitty_clip::Grants> {
    parking_lot::Mutex::new(bt_term::kitty_clip::Grants::default())
}

#[derive(Clone, Copy)]
struct Null;
impl EventListener for Null {}
struct Sz(usize, usize);
impl Dimensions for Sz {
    fn total_lines(&self) -> usize {
        self.0
    }
    fn screen_lines(&self) -> usize {
        self.0
    }
    fn columns(&self) -> usize {
        self.1
    }
}

fn run_small_parsers_fuzz(scale: usize) {
    use bt_term::dcs;
    use bt_term::xt_checksum::{Flags, Request, compute, encode};
    let r = Rng(std::cell::Cell::new(seed() ^ 0x33));
    let n = iters(600, scale);
    let mut term = Term::new(Config::default(), &Sz(24, 80), Null);
    let theme = Theme::default();
    let (mut tcolor, mut tsum) = (Duration::ZERO, Duration::ZERO);
    for _ in 0..n {
        // OSC 133 / 21
        let s: String = {
            let o = osc(&r);
            String::from_utf8_lossy(&o).into_owned()
        };
        let body = s.trim_start_matches("\x1b]");
        let _ = bt_term::osc133::parse(body.strip_prefix("133;").unwrap_or(body));
        if let Some(reqs) = bt_term::kitty_color::parse(body.strip_prefix("21;").unwrap_or(body)) {
            if reqs.len() > bt_term::kitty_color::MAX_ITEMS {
                fail("color", "항목 상한", body.as_bytes());
            }
            if let Some(rep) = bt_term::kitty_color::apply(&mut term, &theme, &reqs, r.chance(2)) {
                if let Err(e) = reply_ok(&rep) {
                    fail("color reply", &e, body.as_bytes());
                }
            }
        }
        let t0 = Instant::now();
        let _ = bt_term::kitty_color::parse_color(body);
        tcolor += t0.elapsed();
        // DCS
        let d = dcs(&r);
        let inner = &d[2.min(d.len())..];
        if let Some(q) = dcs::parse_decrqss(inner) {
            let rep = dcs::encode_decrqss(q, &term);
            if let Err(e) = reply_ok(&rep) {
                fail("decrqss reply", &e, inner);
            }
        }
        let rep = dcs::xtgettcap_reply(inner, "xterm-256color");
        if let Err(e) = reply_ok(&rep) {
            fail("xtgettcap reply", &e, inner);
        }
        if rep.len() > 64 * 400 {
            fail("xtgettcap", &format!("답 {}바이트", rep.len()), inner);
        }
        // XTCHECKSUM
        let u = |r: &Rng| *r.pick(&[0u16, 1, 2, 24, 25, 80, 81, 255, 256, 65535]);
        let req = Request {
            id: u(&r),
            top: u(&r),
            left: u(&r),
            bottom: u(&r),
            right: u(&r),
        };
        let flags = Flags::from_bits(r.next() as u16);
        let t1 = Instant::now();
        let sum = compute(&term, req, flags);
        tsum += t1.elapsed();
        let _ = encode(req.id, sum);
        if r.chance(50) {
            term = Term::new(Config::default(), &Sz(1 + r.below(30), 2 + r.below(100)), Null);
        }
    }
    eprintln!("small: {n}회 parse_color {tcolor:?} compute {tsum:?}");
}

fn run_emulator_fuzz(scale: usize) {
    let r = Rng(std::cell::Cell::new(seed() ^ 0x11));
    let n = iters(250, scale);
    let mut e = Emulator::new(GridSize::new(40, 12), Theme::default());
    let mut slow = (Duration::ZERO, Vec::new());
    for i in 0..n {
        let mut buf = Vec::new();
        for _ in 0..1 + r.below(6) {
            buf.extend(emu_chunk(&r));
        }
        if r.chance(40) {
            e.resize(GridSize::new(2 + r.below(100) as u16, 1 + r.below(40) as u16));
        }
        let t = Instant::now();
        let sigs = e.feed(&buf);
        let el = t.elapsed();
        if el > slow.0 {
            slow = (el, buf.clone());
        }
        for s in &sigs {
            if let Err(er) = check_signal(s, buf.len()) {
                fail("emulator", &er, &buf);
            }
        }
        let rep = e.take_replies();
        if let Err(er) = reply_ok(&rep) {
            fail("emulator reply", &er, &buf);
        }
        if i % 5 == 0 {
            let s = e.snapshot_at(Instant::now() + Duration::from_millis(r.below(100000) as u64));
            let _ = s.text();
            let cells = (s.cols * s.rows) as i64;
            let area: i64 = s
                .images
                .iter()
                .map(|v| {
                    let vr = ((v.row as i64 + v.rows as i64).min(s.rows as i64) - (v.row as i64).max(0)).max(0);
                    let vc = ((v.col as i64 + v.cols as i64).min(s.cols as i64) - (v.col as i64).max(0)).max(0);
                    vr * vc
                })
                .sum();
            if area > 4 * cells + 1000 {
                fail("emulator", &format!("그림 넓이 {area} > 4배 {cells}"), &buf);
            }
        }
    }
    eprintln!("emulator: {n}회, 한 번 최대 {:?}", slow.0);
}

fn cur0() -> Cursor {
    Cursor {
        line: 0,
        col: 0,
        cell_w: 8,
        cell_h: 16,
        alt: false,
        top: 0,
        rows: 24,
        cols: 80,
    }
}

fn e64(s: &str) -> String {
    bt_term::kitty_clip::b64_encode(s.as_bytes())
}

fn run_clip_structured_fuzz(scale: usize) {
    use bt_term::kitty_clip::{Action, Engine, Grants};
    let r = Rng(std::cell::Cell::new(seed() ^ 0xC11B));
    let n = iters(3000, scale);
    let max = 64 * 1024;
    let grants = Arc::new(parking_lot::Mutex::new(Grants::default()));
    let mut e = Engine::new(grants.clone(), true, max);
    let (mut commits, mut errs, mut biggest) = (0, 0, 0usize);
    for _ in 0..n {
        let mime = |r: &Rng| e64(&format!("text/x-{}", r.below(if r.chance(2) { 5 } else { 90 })));
        let packet = match r.below(10) {
            0 => format!("type=write:id=i{}:name={}:pw={}", r.below(3), e64("n"), e64("p")),
            1..=5 => {
                let raw = rand_bytes(&r, r.below(if r.chance(5) { 6000 } else { 90 }));
                let mut b = bt_term::kitty_clip::b64_encode(&raw);
                if r.chance(4) {
                    b = b.trim_end_matches('=').to_string();
                }
                if r.chance(8) {
                    b.insert(r.below(b.len() + 1), '=');
                }
                let cut = r.below(b.len() + 1);
                let b = if r.chance(2) { b[..cut].to_string() } else { b };
                format!("type=wdata:mime={};{}", mime(&r), b)
            }
            6 => format!(
                "type=walias:mime={};{}",
                mime(&r),
                e64(&(0..r.below(100))
                    .map(|_| format!("a{} ", r.below(100)))
                    .collect::<String>())
            ),
            7 => "type=wdata".to_string(),
            8 => format!(
                "type=read:id=r{}:mime={};{}",
                r.below(4),
                mime(&r),
                e64(&(0..r.below(30))
                    .map(|_| format!("text/x-{} ", r.below(50)))
                    .collect::<String>())
            ),
            _ => format!("type=read:loc=primary:pw={}", e64(&format!("{}", r.below(10)))),
        };
        for a in e.handle(&packet, r.chance(2)) {
            match a {
                Action::Reply(rep) => {
                    if let Err(er) = reply_ok(&rep) {
                        fail("clip2 reply", &er, packet.as_bytes());
                    }
                    if String::from_utf8_lossy(&rep).contains("EFBIG")
                        || String::from_utf8_lossy(&rep).contains("EINVAL")
                    {
                        errs += 1;
                    }
                }
                Action::Write(w) => {
                    commits += 1;
                    let tot: usize = w.contents.iter().map(|(_, d)| d.len()).sum();
                    biggest = biggest.max(tot);
                    if tot > max {
                        fail("clip2", &format!("쓰기 총량 {tot} > {max}"), packet.as_bytes());
                    }
                    if w.contents.len() > 128 {
                        fail("clip2", &format!("종류 {}개", w.contents.len()), packet.as_bytes());
                    }
                }
                Action::Read(rr) => {
                    if rr.mimes.len() > 4 {
                        fail("clip2", "읽기 MIME 4 초과", packet.as_bytes());
                    }
                }
            }
        }
        if grants.lock().len() > 32 {
            fail("clip2", "허가", packet.as_bytes());
        }
    }
    eprintln!("clip2: {n}회, 커밋 {commits}, 오류 {errs}, 최대 총량 {biggest}");
}

fn assembler_directed_inner() {
    let mut a = bt_term::kitty_notify::Assembler::default();
    let big = "한".repeat(600);
    for i in 0..500 {
        a.packet(&format!("i=id{i}:d=0:p=title;{}", "t".repeat(300)), false);
        a.packet(&format!("i=id{i}:d=0:p=body;{big}"), true);
        a.packet(&format!("i=id{}:d=0:p=body;{big}", i % 7), false);
    }
    eprintln!(
        "asm: 쌓임 {}개, {}자 (상한 16개, {}자)",
        a.pending_len(),
        a.held_chars(),
        16 * (121 + 501)
    );
    assert!(a.pending_len() <= 16 && a.held_chars() <= 16 * 622);
    // 식별자 길이 경계
    for l in [127usize, 128, 129] {
        let r = a.packet(&format!("i={}:p=?;", "a".repeat(l)), false);
        eprintln!(
            "asm: id {l}자 질의 -> {:?}",
            matches!(r, bt_term::kitty_notify::Outcome::Reply(_))
        );
    }
}

use bt_term::{Session, SessionEvent, SpawnOptions};
use std::sync::mpsc::{Receiver, channel};
fn sspawn(script: &str) -> (Session, Receiver<SessionEvent>) {
    let (tx, rx) = channel();
    let tx = std::sync::Mutex::new(tx);
    let sink = Arc::new(move |e: SessionEvent| {
        let _ = tx.lock().unwrap().send(e);
    });
    let o = SpawnOptions {
        program: Some("/bin/sh".into()),
        args: vec!["-c".into(), script.into()],
        cwd: Some(std::env::temp_dir()),
        ..Default::default()
    };
    (
        Session::spawn(o, GridSize::new(100, 40), Theme::default(), sink).unwrap_or_else(|e| panic!("spawn: {e:#}")),
        rx,
    )
}

fn wait_exit(rx: &Receiver<SessionEvent>) -> Vec<SessionEvent> {
    let mut all = Vec::new();
    let deadline = Instant::now() + Duration::from_secs(10);
    loop {
        assert!(Instant::now() < deadline, "10초 안에 끝나지 않았다");
        match rx.recv_timeout(Duration::from_millis(100)) {
            Ok(SessionEvent::Exited(_)) => break,
            Ok(e) => all.push(e),
            Err(_) => {}
        }
    }
    all.extend(rx.try_iter());
    all
}

/// 셸이 PTY에서 읽은 바이트(답)를 `R[16진]`으로 찍는다
fn reader(setup: &str) -> String {
    format!(
        "stty raw -echo min 0 time 5; {setup}; sleep 0.3; \
         r=$(dd bs=512 count=1 2>/dev/null | od -An -v -tx1 | tr -d ' \\n'); printf 'R[%s]\\n' \"$r\""
    )
}

fn parse_reply(s: &Session) -> String {
    let text = s.snapshot().text();
    let start = text.find("R[").unwrap_or_else(|| panic!("답 줄이 없다:\n{text}")) + 2;
    let end = text[start..].find(']').expect("닫는 ]") + start;
    let hex: String = text[start..end].chars().filter(char::is_ascii_hexdigit).collect();
    let bytes: Vec<u8> = (0..hex.len() / 2)
        .map(|i| u8::from_str_radix(&hex[i * 2..i * 2 + 2], 16).unwrap())
        .collect();
    String::from_utf8(bytes).unwrap()
}

fn run(setup: &str) -> (String, Vec<SessionEvent>) {
    let (s, rx) = sspawn(&reader(setup));
    let events = wait_exit(&rx);
    (parse_reply(&s), events)
}

fn run_links_fuzz(scale: usize) {
    let r = Rng(std::cell::Cell::new(seed() ^ 0x1155));
    let n = iters(600, scale);
    let hosts = [
        "a.b",
        "[::1]",
        "x@y",
        "localhost",
        "",
        "é",
        "a b",
        "a:80",
        "a:b:c",
        "xn--a",
    ];
    for _ in 0..n {
        let url = match r.below(5) {
            0 => format!(
                "https://{}/{}",
                r.pick(&hosts),
                String::from_utf8_lossy(&rand_bytes(&r, r.below(40)))
            ),
            1 => format!(
                "file://{}/no-such-dir-73/{}",
                r.pick(&hosts),
                ["%41", "%zz", "%", "%2", "%00", "a%2fb", "..%2f"][r.below(7)]
            ),
            2 => String::from_utf8_lossy(&rand_bytes(&r, r.below(100))).into_owned(),
            3 => format!(
                "{}://{}",
                ["http", "HTTPS", "ftp", "javascript", "file"][r.below(5)],
                "é".repeat(r.below(50))
            ),
            _ => format!("https://a/{}", "한".repeat(r.below(9000))),
        };
        let _ = bt_term::links::classify(&url, &|h| h == "x");
        for max in [0usize, 1, 5, 12, 40, 200] {
            let h = bt_term::links::hint_text(&url, max);
            let _ = h;
        }
        let chars: Vec<char> = url.chars().collect();
        let _ = bt_term::links::find_urls(&chars);
    }
    // OSC 8 + link_at
    let mut e = Emulator::new(GridSize::new(30, 8), Theme::default());
    for _ in 0..iters(100, scale) {
        let mut b = Vec::new();
        for _ in 0..1 + r.below(5) {
            match r.below(4) {
                0 => b.extend(
                    format!(
                        "\x1b]8;id={};{}\x1b\\",
                        r.below(5),
                        [
                            "http://a.b/c",
                            "file:///x",
                            "",
                            &"u".repeat(r.below(3000)),
                            "https://a.b/\u{1}"
                        ][r.below(5)]
                    )
                    .bytes(),
                ),
                1 => b.extend(b"\x1b]8;;\x1b\\"),
                2 => b.extend(rand_bytes(&r, r.below(40)).iter().map(|b| b % 90 + 32)),
                _ => b.extend(b"see https://x.y/z)))] (http://a) \r\n"),
            }
        }
        e.feed(&b);
        let s = e.snapshot();
        for row in 0..s.rows {
            for col in 0..s.cols {
                if let Some(h) = bt_term::links::link_at(&s, col, row) {
                    if h.url.len() > 20000 {
                        fail("links", "URL 길이", &b);
                    }
                }
            }
        }
    }
    eprintln!("links: ok");
}

#[test]
fn scanner_fuzz() {
    run_scanner_fuzz(0);
}

#[test]
#[ignore = "3만 회 판(release로 돌린다)"]
fn scanner_fuzz_30000() {
    run_scanner_fuzz(30000);
}

#[test]
fn scanner_split_invariance() {
    run_scanner_split_invariance(0);
}

#[test]
#[ignore = "3만 회 판(release로 돌린다)"]
fn scanner_split_invariance_30000() {
    run_scanner_split_invariance(30000);
}

#[test]
fn assembler_fuzz() {
    run_assembler_fuzz(0);
}

#[test]
#[ignore = "3만 회 판(release로 돌린다)"]
fn assembler_fuzz_30000() {
    run_assembler_fuzz(30000);
}

#[test]
fn graphics_fuzz() {
    run_graphics_fuzz(0);
}

#[test]
#[ignore = "3만 회 판(release로 돌린다)"]
fn graphics_fuzz_30000() {
    run_graphics_fuzz(30000);
}

#[test]
fn clip_engine_fuzz() {
    run_clip_engine_fuzz(0);
}

#[test]
#[ignore = "3만 회 판(release로 돌린다)"]
fn clip_engine_fuzz_30000() {
    run_clip_engine_fuzz(30000);
}

#[test]
fn small_parsers_fuzz() {
    run_small_parsers_fuzz(0);
}

#[test]
#[ignore = "3만 회 판(release로 돌린다)"]
fn small_parsers_fuzz_30000() {
    run_small_parsers_fuzz(30000);
}

#[test]
fn emulator_fuzz() {
    run_emulator_fuzz(0);
}

#[test]
#[ignore = "3만 회 판(release로 돌린다)"]
fn emulator_fuzz_30000() {
    run_emulator_fuzz(30000);
}

#[test]
fn clip_structured_fuzz() {
    run_clip_structured_fuzz(0);
}

#[test]
#[ignore = "3만 회 판(release로 돌린다)"]
fn clip_structured_fuzz_30000() {
    run_clip_structured_fuzz(30000);
}

#[test]
fn assembler_directed() {
    assembler_directed_inner();
}

#[test]
fn links_fuzz() {
    run_links_fuzz(0);
}

#[test]
#[ignore = "3만 회 판(release로 돌린다)"]
fn links_fuzz_30000() {
    run_links_fuzz(30000);
}

// ---- #595: 스캐너가 문자열 끝을 엔진과 같게 본다 ----
// 기준: Ghostty Parser.zig 276·280·281줄(OSC·DCS·APC 상태를 나갈 때 끝낸다. DCS·APC는 바이트와 상관없이 unhook·apc_end,
// OSC는 `osc.zig` `Parser.end`가 CAN·SUB면 버린다(#763)), vte `advance_osc_string`(ESC에서 `osc_end`)

fn feed_all(b: &[u8]) -> Vec<Signal> {
    Scanner::default().feed(b)
}

#[test]
fn can_sub_end_osc_and_the_next_query_is_seen() {
    // CAN으로 끝난 OSC는 끝나되 명령은 버리고(#763, Ghostty 1.4.0 `osc.zig` `Parser.end`), 뒤따르는 DA1은 새 CSI다
    let got = feed_all(b"\x1b]9;hi\x18\x1b[c");
    assert_eq!(got, vec![Signal::Da1]);
    let got = feed_all(b"\x1b]0;t\x1a\x1b[c");
    assert_eq!(got, vec![Signal::Da1]);
}

#[test]
fn can_in_osc_does_not_swallow_following_text_until_bel() {
    // 예전에는 CAN 뒤 글자가 모두 OSC로 모이고 BEL까지 신호를 삼켰다
    let got = feed_all(b"\x1b]9;a\x18plain text\x1b[c plain \x07\x1b[c");
    assert_eq!(got.iter().filter(|s| **s == Signal::Da1).count(), 2, "{got:?}");
}

#[test]
fn esc_not_backslash_ends_osc_and_rereads_the_byte() {
    let got = feed_all(b"\x1b]9;hi\x1b[c");
    assert!(matches!(&got[0], Signal::Notify(n) if n.body == "hi"), "{got:?}");
    assert_eq!(got[1], Signal::Da1);
    // ESC ESC: 둘째 ESC가 새 이스케이프를 시작한다
    let got = feed_all(b"\x1b]9;hi\x1b\x1b[c");
    assert_eq!(got.last(), Some(&Signal::Da1), "{got:?}");
    // ESC ]: 새 OSC가 이어진다
    let got = feed_all(b"\x1b]9;one\x1b]9;two\x07");
    assert_eq!(got.len(), 2, "{got:?}");
}

#[test]
fn esc_ends_apc_and_dcs_and_rereads_the_byte() {
    let got = feed_all(b"\x1b_Ga=q,i=1\x1b[c");
    assert_eq!(got, vec![Signal::Graphics(b"a=q,i=1".to_vec()), Signal::Da1]);
    let got = feed_all(b"\x1b_Ga=q,i=1\x18\x1b[c");
    assert_eq!(got, vec![Signal::Graphics(b"a=q,i=1".to_vec()), Signal::Da1]);
    // 끊긴 DCS(DECRQSS)는 답하지 않지만 다음 CSI는 읽는다
    assert_eq!(feed_all(b"\x1bP$qm\x1b[c"), vec![Signal::Da1]);
    assert_eq!(feed_all(b"\x1bP$qm\x18\x1b[c"), vec![Signal::Da1]);
    // APC 안의 ESC ]는 새 OSC를 연다
    assert_eq!(feed_all(b"\x1b_Gx\x1b]9;n\x07").len(), 2);
}

#[test]
fn can_ends_csi_so_the_final_byte_is_text() {
    // 엔진은 `ESC [ 2 CAN J`의 J를 글자로 찍는다. 스캐너가 ED 2로 읽으면 그림만 지워진다
    assert!(feed_all(b"\x1b[2\x18J").is_empty());
    assert!(feed_all(b"\x1b[3\x1aJ").is_empty());
    assert_eq!(feed_all(b"\x1b[2J").len(), 1);
}

#[test]
fn can_ends_a_skipped_oversize_osc() {
    let mut v = b"\x1b]9;".to_vec();
    v.extend(vec![b'a'; 9000]);
    v.extend(b"\x18\x1b[c");
    assert_eq!(feed_all(&v), vec![Signal::Da1]);
}

#[test]
fn tmux_passthrough_double_esc_is_still_not_ours() {
    // 겹친 ESC 안의 질의는 우리 것이 아니다(기존 규칙을 그대로 지킨다)
    assert!(feed_all(b"\x1bPtmux;\x1b\x1bP$qm\x1b\x1b\\\x1b\\").is_empty());
}

/// Session 끝-끝: CAN·ESC로 끊긴 문자열 뒤의 DA1에 PTY가 답을 받는다
#[test]
fn session_answers_da1_after_interrupted_strings() {
    for pre in ["", "\\033]9;hi\\030", "\\033]9;hi", "\\033_Ga=q,i=1\\030", "\\033P1$r"] {
        let (reply, _) = run(&format!("printf '{pre}\\033[c'"));
        // APC가 CAN으로 끝나면 끝난 명령으로 읽혀 그림 답이 앞에 붙는다(Ghostty `apc_end`). DA1 답은 반드시 온다
        assert!(reply.ends_with("\x1b[?62;22c"), "앞 바이트 {pre:?}: {reply:?}");
    }
}

// ---- #596 ----

#[test]
fn x11_names_resolve_like_the_linear_scan() {
    use bt_term::kitty_color::parse_color;
    let table = include_str!("../../bt-app/src/x11_colors.txt");
    // 줄마다 훑던 옛 방식(첫 줄이 이긴다)
    let old = |name: &str| -> Option<(u8, u8, u8)> {
        let squeezed: String = name.chars().filter(|&c| c != ' ').collect();
        table.lines().find_map(|l| {
            let mut f = l.split_whitespace();
            let (r, g, b, n) = (f.next()?, f.next()?, f.next()?, f.next()?);
            if !n.eq_ignore_ascii_case(&squeezed) {
                return None;
            }
            Some((r.parse().ok()?, g.parse().ok()?, b.parse().ok()?))
        })
    };
    let mut checked = 0;
    for l in table.lines() {
        let Some(n) = l.split_whitespace().nth(3) else { continue };
        for v in [
            n.to_string(),
            n.to_uppercase(),
            format!(" {n} "),
            n.chars().rev().collect::<String>(),
        ] {
            let want = old(v.trim_matches(' '));
            if want.is_some() {
                assert_eq!(parse_color(&v), want, "{v:?}");
                checked += 1;
            }
        }
    }
    assert!(checked > 600, "{checked}");
    assert_eq!(parse_color("Yellow Green"), Some((154, 205, 50)));
    assert_eq!(parse_color("nosuchcolor"), None);
    assert_eq!(parse_color("été"), None);
}

/// 프레임 번호의 뜻은 그대로다: 1부터 세고, 0·끝 넘음은 끝, 1을 지우면 2가 뿌리가 된다, 지운 뒤 번호가 당겨진다
#[test]
fn delete_frame_keeps_numbering() {
    let r = Rng(std::cell::Cell::new(seed() ^ 0xDE1));
    let cur = cur0();
    for round in 0..40 {
        let mut g = Graphics::with_limit(100_000_000);
        let n = 1 + r.below(9);
        // 프레임 k의 첫 바이트가 k다(1x1 RGBA)
        g.handle(
            &format!("a=t,f=32,s=1,v=1,i=1;{}", e64_bytes(&[1, 0, 0, 255])).into_bytes(),
            cur,
        );
        for k in 2..=n {
            g.handle(
                &format!("a=f,f=32,s=1,v=1,i=1,c=0;{}", e64_bytes(&[k as u8, 0, 0, 255])).into_bytes(),
                cur,
            );
        }
        let mut model: Vec<u8> = (1..=n as u8).collect();
        for _ in 0..r.below(8) {
            let num = r.below(14);
            g.handle(format!("a=d,d=f,i=1,r={num}").as_bytes(), cur);
            if model.len() > 1 {
                let count = model.len();
                let number = num.min(count).max(1);
                model.remove(number - 1);
            }
            let img = g.image(1).unwrap();
            assert_eq!(img.frame_count() as usize, model.len().max(1), "회차 {round}");
            for (i, want) in model.iter().enumerate() {
                let got = img.frame_data(i as u32 + 1).map(|d| d[0]);
                assert_eq!(got, Some(*want), "회차 {round} 프레임 {}", i + 1);
            }
        }
    }
}

fn e64_bytes(b: &[u8]) -> String {
    bt_term::kitty_clip::b64_encode(b)
}

/// 앞에서부터 하나씩 지우는 출력이 끝까지 가는지만 본다(시간 단언은 하지 않는다. 잰 값은 CHANGELOG에 적었다)
#[test]
fn deleting_many_frames_from_the_front_finishes() {
    let cur = cur0();
    let mut g = Graphics::with_limit(320_000_000);
    g.handle(b"a=t,f=32,s=1,v=1,i=1;AAAAAA==", cur);
    for _ in 0..3000 {
        g.handle(b"a=f,f=32,s=1,v=1,i=1;AAAAAA==", cur);
    }
    for _ in 0..3000 {
        g.handle(b"a=d,d=f,i=1,r=1", cur);
    }
    assert_eq!(g.image(1).map(|i| i.frame_count()), Some(1));
}
