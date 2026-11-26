//! 동아시아 모호 폭 글자를 두 칸으로 세는 설정(`ambiguous-width = wide`, #857).
//! PTY에 `/bin/sh`와 `printf`로 글을 찍고 칸 배치를 잰다. 폭은 엔진이 한 곳에서 정하고 셀의 `WIDE` 표시가 따른다

use std::sync::Arc;
use std::sync::mpsc::{Receiver, channel};
use std::time::{Duration, Instant};

use bt_core::Theme;
use bt_term::{CellFlags, GridSize, Screen, SelectKind, Session, SessionEvent, SpawnOptions};

fn spawn(script: &str, cols: u16, wide: bool) -> (Session, Receiver<SessionEvent>) {
    let (tx, rx) = channel();
    let tx = std::sync::Mutex::new(tx);
    let sink = Arc::new(move |e: SessionEvent| {
        let _ = tx.lock().unwrap().send(e);
    });
    let o = SpawnOptions {
        program: Some("/bin/sh".into()),
        args: vec!["-c".into(), script.into()],
        cwd: Some(std::env::temp_dir()),
        ambiguous_wide: wide,
        ..Default::default()
    };
    let s =
        Session::spawn(o, GridSize::new(cols, 6), Theme::default(), sink).unwrap_or_else(|e| panic!("spawn: {e:#}"));
    (s, rx)
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

fn run(script: &str, cols: u16, wide: bool) -> (Session, Screen) {
    let (s, rx) = spawn(script, cols, wide);
    wait_exit(&rx);
    let snap = s.snapshot();
    (s, snap)
}

/// `text` 뒤에 표지 `x`를 찍고 `x`가 놓인 칸 번호
fn x_col(text: &str, wide: bool) -> usize {
    let (_s, s) = run(&format!("printf '{text}x'"), 20, wide);
    let col = (0..20)
        .find(|&c| s.cell(c, 0).ch == 'x')
        .unwrap_or_else(|| panic!("표지가 없다: {:?}", s.row_text(0)));
    assert_eq!(s.cursor.col, col + 1, "{text:?}");
    col
}

/// 모호 폭 글자 몇 개: 동그라미 숫자, 참고 표시, 원, 상자 그리기, 뒤집힌 느낌표, 불릿
const AMBIGUOUS: [&str; 6] = ["\u{2460}", "\u{203B}", "\u{25CB}", "\u{2500}", "\u{A1}", "\u{2022}"];

#[test]
fn ambiguous_char_takes_two_cells_when_on() {
    for a in AMBIGUOUS {
        assert_eq!(x_col(a, true), 2, "{a:?} 켬");
    }
}

/// 그리스·키릴 글자는 `unicode-width`의 `width_cjk`가 한 칸으로 둔다(모호 폭 표의 일부만 두 칸이 된다)
#[test]
fn greek_and_cyrillic_stay_one_cell() {
    for a in ["\u{03A9}", "\u{0416}", "\u{03B1}"] {
        assert_eq!(x_col(a, true), 1, "{a:?}");
    }
}

#[test]
fn ambiguous_char_takes_one_cell_when_off() {
    for a in AMBIGUOUS {
        assert_eq!(x_col(a, false), 1, "{a:?} 끔");
    }
}

#[test]
fn wide_cell_is_marked_wide_for_the_renderer() {
    let (_s, s) = run("printf '\\342\\221\\240x'", 20, true);
    assert!(s.cell(0, 0).flags.contains(CellFlags::WIDE), "① 칸에 WIDE 표시");
    assert!(s.cell(1, 0).flags.contains(CellFlags::SPACER), "뒤 칸은 SPACER");
    let (_s, s) = run("printf '\\342\\221\\240x'", 20, false);
    assert!(!s.cell(0, 0).flags.contains(CellFlags::WIDE));
}

#[test]
fn other_characters_are_unchanged() {
    for wide in [false, true] {
        // 한글·넓은 이모지는 두 칸, 라틴 글자·결합 문자·좁은 이모지 글자는 그대로
        assert_eq!(x_col("\u{D55C}", wide), 2, "한글 {wide}");
        assert_eq!(x_col("\u{1F600}", wide), 2, "이모지 {wide}");
        assert_eq!(x_col("a", wide), 1, "ASCII {wide}");
        assert_eq!(x_col("a\u{301}", wide), 1, "결합 문자 {wide}");
        assert_eq!(x_col("\u{FF21}", wide), 2, "전각 {wide}");
        // 결합 문자는 모호 폭 글자 칸에도 붙는다(폭 0)
        assert_eq!(
            x_col("\u{2460}\u{301}", wide),
            if wide { 2 } else { 1 },
            "①+결합 {wide}"
        );
    }
}

#[test]
fn mode_2027_and_legacy_agree_on_ambiguous() {
    // 묶음 폭(2027)을 켠 채와 끈 채 모두 같은 값을 따른다
    for pre in ["", "\\033[?2027l", "\\033[?2027h"] {
        assert_eq!(x_col(&format!("{pre}\u{2460}"), true), 2, "{pre} 켬");
        assert_eq!(x_col(&format!("{pre}\u{2460}"), false), 1, "{pre} 끔");
    }
    // ① ZWJ 😀는 ①가 이모지 바탕이 아니라 😀가 새 묶음이다: 켜면 2+2, 끄면 1+2
    assert_eq!(x_col("\u{2460}\u{200D}\u{1F600}", true), 4);
    assert_eq!(x_col("\u{2460}\u{200D}\u{1F600}", false), 3);
}

#[test]
fn cursor_movement_follows_the_wide_cell() {
    // ①가 0~1칸. CUB 1은 뒤 칸(SPACER)에서 앞 칸으로 돌아가고, 거기에 덮어쓰면 ①가 지워진다
    let (_s, s) = run("printf '\\342\\221\\240\\033[Dy'", 20, true);
    assert_eq!(s.cell(0, 0).ch, ' ', "{:?}", s.row_text(0));
    assert_eq!(s.cell(1, 0).ch, 'y');
    // 끄면 한 칸이라 CUB 1은 ①를 그대로 두지 않고 그 칸에 쓴다
    let (_s, s) = run("printf '\\342\\221\\240\\033[Dy'", 20, false);
    assert_eq!(s.cell(0, 0).ch, 'y');
    // CHA 3: 셋째 칸(0부터 2)은 켜면 ① 바로 뒤
    let (_s, s) = run("printf '\\342\\221\\240\\033[3Gx'", 20, true);
    assert_eq!(s.cell(0, 0).ch, '\u{2460}');
    assert_eq!(s.cell(2, 0).ch, 'x');
}

#[test]
fn wide_ambiguous_wraps_at_the_line_end() {
    // 4칸 줄에서 abc 다음 ①는 한 칸이 남아도 들어가지 않아 다음 줄 맨 앞에 쓴다
    let (_s, s) = run("printf 'abc\\342\\221\\240x'", 4, true);
    assert_eq!(s.row_text(0), "abc");
    assert_eq!(s.row_text(1), "\u{2460}x");
    // 끄면 한 칸이라 같은 줄에 들어간다
    let (_s, s) = run("printf 'abc\\342\\221\\240x'", 4, false);
    assert_eq!(s.row_text(0), "abc\u{2460}");
    assert_eq!(s.row_text(1), "x");
}

#[test]
fn selection_and_copy_treat_the_pair_as_one_char() {
    let (s, _) = run("printf 'a\\342\\221\\240b'", 20, true);
    s.select_start(0, 0, SelectKind::Simple);
    s.select_update(3, 0);
    // 글: a ① b (① 뒤 칸을 빈칸으로 복사하지 않는다)
    assert_eq!(s.selection_text().as_deref(), Some("a\u{2460}b"));
    // 뒤 칸(2번)에서 시작해도 ① 전체를 품는다
    s.select_clear();
    s.select_start(2, 0, SelectKind::Simple);
    s.select_update(3, 0);
    assert_eq!(s.selection_text().as_deref(), Some("\u{2460}b"));
}

#[test]
fn search_finds_the_wide_char() {
    let (s, _) = run("printf 'a\\342\\221\\240b'", 20, true);
    let st = s.search("\u{2460}b").expect("찾기");
    assert_eq!(st.total, 1);
}

#[test]
fn reload_applies_to_new_text_only() {
    // 켜지 않고 시작해 ①a를 찍고, 설정을 켠 뒤 ①b를 찍는다. 이미 찍힌 첫 줄은 그대로
    let (s, rx) = spawn(
        "printf '\\342\\221\\240a\\n'; sleep 1; printf '\\342\\221\\240b\\n'",
        20,
        false,
    );
    std::thread::sleep(Duration::from_millis(400));
    s.set_ambiguous_wide(true);
    wait_exit(&rx);
    let snap = s.snapshot();
    assert_eq!(
        snap.cell(1, 0).ch,
        'a',
        "첫 줄은 한 칸 폭 그대로: {:?}",
        snap.row_text(0)
    );
    assert_eq!(snap.cell(2, 1).ch, 'b', "둘째 줄은 두 칸: {:?}", snap.row_text(1));
}
