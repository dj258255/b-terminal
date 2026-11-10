//! 화면 반전(DECSCNM ?5), 역방향 줄바꿈(?45·?1045), 저장만 하는 모드(?4·?8)와 ANSI 모드 답(2·12)(#302).
//! 기준: Ghostty Terminal.zig `cursorLeft`(2592~2696줄)와 그 시험(12408~12649줄), stream_handler.zig `setMode`(680~700줄),
//! terminal_render.zig(1063줄, 기본 글자색·바탕색만 맞바꿈), modes.zig(ANSI 2·4·12·20, DEC 4·5·8·45·1045)

use std::sync::Arc;
use std::sync::mpsc::{Receiver, channel};
use std::time::{Duration, Instant};

use bt_core::{Rgb, Theme};
use bt_term::{Emulator, GridSize, Screen, Session, SessionEvent, SpawnOptions};

fn emu(cols: u16, rows: u16) -> Emulator {
    Emulator::new(GridSize::new(cols, rows), Theme::default())
}

/// 바이트를 먹이고 줄마다 글(끝 공백 뺌)과 커서 자리를 돌려준다
fn run(cols: u16, rows: u16, bytes: &str) -> (Vec<String>, (usize, usize)) {
    let mut e = emu(cols, rows);
    e.feed(bytes.as_bytes());
    let s = e.snapshot();
    let lines = (0..s.rows).map(|r| s.row_text(r)).collect();
    (lines, (s.cursor.col, s.cursor.row))
}

// ---- 역방향 줄바꿈: Terminal.zig 시험을 옮긴 것 ----

#[test]
fn reverse_wrap_goes_up_only_over_soft_wrapped_line() {
    // "cursorLeft reverse wrap": ABCDE1 뒤 CUB 2, X
    let (l, _) = run(5, 5, "\x1b[?45hABCDE1\x1b[2DX");
    assert_eq!(&l[..2], ["ABCDX", "1"]);
    // 같은 일을 Backspace 두 번으로
    let (l, _) = run(5, 5, "\x1b[?45hABCDE1\x08\x08X");
    assert_eq!(&l[..2], ["ABCDX", "1"]);
}

#[test]
fn reverse_wrap_stops_at_hard_line_break() {
    // "with no soft wrap": CR LF로 내려간 줄에서는 윗줄로 넘지 않는다
    let (l, _) = run(5, 5, "\x1b[?45hABCDE\r\n1\x1b[2DX");
    assert_eq!(&l[..2], ["ABCDE", "X"]);
}

#[test]
fn reverse_wrap_with_pending_wrap_counts_one_step() {
    // "with pending wrap state": 끝 칸에서 CUB 1은 대기 상태만 풀고 안 움직인다
    let (l, _) = run(5, 5, "\x1b[?45hABCDE\x1b[DX");
    assert_eq!(l[0], "ABCDX");
    let (l, _) = run(5, 5, "\x1b[?1045hABCDE\x1b[DX");
    assert_eq!(l[0], "ABCDX");
}

#[test]
fn reverse_wrap_on_first_row_stops_at_top_left() {
    // "on first row": 위쪽 여백 위, 첫 줄에서는 왼쪽 위에 멈춘다
    let (_, cur) = run(5, 5, "\x1b[?45h\x1b[3;0r\x1b[1;2H\x1b[1000D");
    assert_eq!(cur, (0, 0));
    let (_, cur) = run(5, 5, "\x1b[?1045h\x1b[3;0r\x1b[1;2H\x1b[1000D");
    assert_eq!(cur, (0, 0));
}

#[test]
fn reverse_wrap_before_left_margin_above_top_margin() {
    // "before left margin": 위쪽 여백(3줄~)보다 위 첫 줄(왼쪽 끝)에서 CUB는 위쪽 여백의 왼쪽 위로 간다
    let (l, _) = run(5, 5, "\x1b[?45h\x1b[3;0r\x1b[DX");
    assert_eq!(l[2], "X");
}

#[test]
fn extended_reverse_wrap_crosses_hard_line_break() {
    // "extended reverse wrap": 하드 줄바꿈도 넘는다
    let (l, _) = run(5, 5, "\x1b[?1045hABCDE\r\n1\x1b[2DX");
    assert_eq!(&l[..2], ["ABCDX", "1"]);
}

#[test]
fn extended_reverse_wrap_wraps_from_top_to_bottom() {
    // "bottom wraparound": 맨 윗줄 왼쪽 끝에서 더 가면 아래 여백의 오른쪽 끝으로
    let (l, _) = run(5, 3, "\x1b[?1045hABCDE\r\n1\x1b[7DX");
    assert_eq!(l[..3], ["ABCDE", "1", "    X"]);
}

#[test]
fn extended_reverse_wrap_has_priority_when_both_set() {
    let (l, _) = run(5, 3, "\x1b[?45h\x1b[?1045hABCDE\r\n1\x1b[7DX");
    assert_eq!(l[..3], ["ABCDE", "1", "    X"]);
}

#[test]
fn extended_reverse_wrap_above_top_scroll_region_ends_at_origin() {
    // "above top scroll region": 위쪽 여백 위에서 큰 수만큼 가도 (0,0)에서 멈춘다
    let (_, cur) = run(5, 5, "\x1b[?1045h\x1b[3;0r\x1b[2;1H\x1b[1000D");
    assert_eq!(cur, (0, 0));
}

#[test]
fn no_reverse_wrap_without_the_modes_or_with_wraparound_off() {
    let (l, cur) = run(5, 5, "ABCDE1\x1b[2DX");
    assert_eq!(
        (l[0].as_str(), l[1].as_str(), cur),
        ("ABCDE", "X", (1, 1)),
        "모드 없이는 줄을 넘지 않는다"
    );
    // 자동 줄바꿈(?7)이 꺼져 있으면 역방향도 하지 않는다(Ghostty는 wraparound가 꺼지면 .none)
    let (l, _) = run(5, 5, "\x1b[?45h\x1b[?7lABCDE\r\n1\x1b[2DX");
    assert_eq!(&l[..2], ["ABCDE", "X"]);
}

// ---- 화면 반전(DECSCNM) ----

#[test]
fn decscnm_swaps_default_colors_only() {
    let t = Theme::default();
    let mut e = emu(10, 2);
    e.feed(b"\x1b[?5hA\x1b[48;2;1;2;3mB\x1b[0m\x1b[31mC");
    let s = e.snapshot();
    // 기본 글자색·바탕색이 맞바뀐다
    assert_eq!((s.cell(0, 0).fg, s.cell(0, 0).bg), (t.background, t.foreground), "A");
    // 칸에 정한 바탕색은 그대로, 기본 글자색은 맞바뀐 쪽
    assert_eq!(
        (s.cell(1, 0).fg, s.cell(1, 0).bg),
        (t.background, Rgb::new(1, 2, 3)),
        "B"
    );
    // 칸에 정한 글자색(팔레트 1번)은 그대로
    assert_eq!((s.cell(2, 0).fg, s.cell(2, 0).bg), (t.palette[1], t.foreground), "C");
    // 빈 칸과 창 바탕도 기본 바탕색을 따른다
    assert_eq!(s.cell(5, 1).bg, t.foreground, "빈 칸");
    assert_eq!(s.background, t.foreground, "여백이 따르는 기본 바탕");
    // 끄면 그대로
    e.feed(b"\x1b[?5l");
    let s = e.snapshot();
    assert_eq!((s.cell(0, 0).fg, s.cell(0, 0).bg), (t.foreground, t.background));
    assert_eq!(s.background, t.background);
}

#[test]
fn decscnm_composes_with_sgr_inverse_and_osc_default_colors() {
    let t = Theme::default();
    let mut e = emu(10, 2);
    e.feed(b"\x1b[?5h\x1b[7mA");
    let s = e.snapshot();
    // 맞바뀐 기본색 위에서 SGR 7이 한 번 더 바꾼다
    assert_eq!((s.cell(0, 0).fg, s.cell(0, 0).bg), (t.foreground, t.background));
    // OSC 11로 바꾼 기본 바탕색도 같이 맞바뀐다
    let mut e = emu(10, 2);
    e.feed(b"\x1b]11;rgb:10/20/30\x1b\\\x1b[?5hA");
    let s = e.snapshot();
    assert_eq!(s.cell(0, 0).fg, Rgb::new(0x10, 0x20, 0x30));
    assert_eq!(s.cell(0, 0).bg, t.foreground);
    assert_eq!(s.background, t.foreground);
}

// ---- DECRQM ----

fn session_screen(script: &str) -> Screen {
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
    let s = Session::spawn(o, GridSize::new(80, 6), Theme::default(), sink).unwrap_or_else(|e| panic!("spawn: {e:#}"));
    wait_exit(&rx);
    s.snapshot()
}

fn wait_exit(rx: &Receiver<SessionEvent>) {
    let deadline = Instant::now() + Duration::from_secs(10);
    while Instant::now() < deadline {
        if let Ok(SessionEvent::Exited(_)) = rx.recv_timeout(Duration::from_millis(100)) {
            return;
        }
    }
    panic!("10초 안에 끝나지 않았다");
}

/// 질의를 찍고 PTY로 돌아온 바이트를 읽어 글로 돌려준다
fn reply_to(query: &str) -> String {
    let script = format!(
        "stty raw -echo min 0 time 5; printf '{query}'; sleep 0.3; \
         r=$(dd bs=256 count=1 2>/dev/null | od -An -v -tx1 | tr -d ' \\n'); printf 'R[%s]\\n' \"$r\""
    );
    let s = session_screen(&script);
    let text = s.text();
    let start = text.find("R[").unwrap_or_else(|| panic!("답 줄이 없다:\n{text}")) + 2;
    let end = text[start..].find(']').expect("닫는 ]") + start;
    let hex = &text[start..end];
    let bytes: Vec<u8> = (0..hex.len() / 2)
        .map(|i| u8::from_str_radix(&hex[i * 2..i * 2 + 2], 16).unwrap())
        .collect();
    String::from_utf8(bytes).unwrap()
}

#[test]
fn decrqm_dec_modes_default_reset_and_follow_set_reset() {
    // Ghostty 기본은 모두 꺼짐(modes.zig 377~386줄). 켜면 1, 끄면 2
    for m in [5, 45, 1045, 4, 8] {
        assert_eq!(
            reply_to(&format!("\\033[?{m}$p")),
            format!("\x1b[?{m};2$y"),
            "?{m} 기본"
        );
        assert_eq!(
            reply_to(&format!("\\033[?{m}h\\033[?{m}$p")),
            format!("\x1b[?{m};1$y"),
            "?{m} 켬"
        );
        assert_eq!(
            reply_to(&format!("\\033[?{m}h\\033[?{m}l\\033[?{m}$p")),
            format!("\x1b[?{m};2$y"),
            "?{m} 끔"
        );
    }
}

#[test]
fn decrqm_ansi_srm_defaults_on_and_kam_follows_the_stored_mode() {
    // SRM(12): Ghostty 기본 켬(modes.zig 371줄), 끌 수 있다
    assert_eq!(reply_to("\\033[12$p"), "\x1b[12;1$y");
    assert_eq!(reply_to("\\033[12l\\033[12$p"), "\x1b[12;2$y");
    assert_eq!(reply_to("\\033[12l\\033[12h\\033[12$p"), "\x1b[12;1$y");
    // KAM(2): Ghostty는 모드를 늘 저장하고 vt-kam-allowed는 키 입력에서만 본다(Surface.zig 2842~2845줄).
    // 설정이 꺼져 있어도(기본) 상태를 답한다
    assert_eq!(reply_to("\\033[2$p"), "\x1b[2;2$y");
    assert_eq!(reply_to("\\033[2h\\033[2$p"), "\x1b[2;1$y");
    assert_eq!(reply_to("\\033[2h\\033[2l\\033[2$p"), "\x1b[2;2$y");
    // 이미 있던 ANSI 4(IRM)·20(LNM)은 그대로
    assert_eq!(reply_to("\\033[4h\\033[4$p"), "\x1b[4;1$y");
    assert_eq!(reply_to("\\033[20$p"), "\x1b[20;2$y");
}

#[test]
fn soft_reset_clears_reverse_wrap_and_kam_but_not_the_stored_only_modes() {
    // Ghostty softReset(5918~5931줄)은 reverse_wrap·reverse_wrap_extended·disable_keyboard를 기본값으로 되돌린다
    assert_eq!(reply_to("\\033[?45h\\033[!p\\033[?45$p"), "\x1b[?45;2$y");
    assert_eq!(reply_to("\\033[?1045h\\033[!p\\033[?1045$p"), "\x1b[?1045;2$y");
    assert_eq!(reply_to("\\033[2h\\033[!p\\033[2$p"), "\x1b[2;2$y");
    // 목록에 없는 모드(?5 SRM)는 소프트 리셋이 건드리지 않는다
    assert_eq!(reply_to("\\033[?5h\\033[!p\\033[?5$p"), "\x1b[?5;1$y");
    assert_eq!(reply_to("\\033[12l\\033[!p\\033[12$p"), "\x1b[12;2$y");
}

#[test]
fn full_reset_restores_defaults() {
    assert_eq!(reply_to("\\033[?5h\\033[?45h\\033[12l\\033c\\033[?5$p"), "\x1b[?5;2$y");
    assert_eq!(reply_to("\\033[12l\\033c\\033[12$p"), "\x1b[12;1$y");
}
