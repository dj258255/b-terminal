//! 글자 묶음 폭(mode 2027, #180). PTY에 `/bin/sh`와 `printf`로 글을 찍고 묶음이 차지한 칸 수를 잰다

use std::sync::Arc;
use std::sync::mpsc::{Receiver, channel};
use std::time::{Duration, Instant};

use bt_core::Theme;
use bt_term::{GridSize, Screen, Session, SessionEvent, SpawnOptions};

fn run(script: &str, cols: u16, opts: impl FnOnce(&mut SpawnOptions)) -> Screen {
    let (tx, rx) = channel();
    let tx = std::sync::Mutex::new(tx);
    let sink = Arc::new(move |e: SessionEvent| {
        let _ = tx.lock().unwrap().send(e);
    });
    let mut o = SpawnOptions {
        program: Some("/bin/sh".into()),
        args: vec!["-c".into(), script.into()],
        cwd: Some(std::env::temp_dir()),
        ..Default::default()
    };
    opts(&mut o);
    let s = Session::spawn(o, GridSize::new(cols, 6), Theme::default(), sink).expect("spawn");
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

/// `pre`(이스케이프)와 `text` 뒤에 표지 `x`를 찍고 `x`가 놓인 칸 번호를 돌려준다(묶음이 차지한 칸 수)
fn width_of(pre: &str, text: &str, opts: impl FnOnce(&mut SpawnOptions)) -> usize {
    let s = run(&format!("printf '{pre}{text}x'"), 20, opts);
    let col = (0..20)
        .find(|&c| s.cell(c, 0).ch == 'x')
        .unwrap_or_else(|| panic!("표지가 없다: {:?}", s.row_text(0)));
    // 표지를 찍은 뒤 커서는 한 칸 더 간 자리
    assert_eq!(s.cursor.col, col + 1, "{text:?}");
    col
}

const FAMILY: &str = "\u{1F468}\u{200D}\u{1F469}\u{200D}\u{1F467}";
const HEART: &str = "\u{2764}\u{FE0F}";
const FLAG_KR: &str = "\u{1F1F0}\u{1F1F7}";
const THUMB_TONE: &str = "\u{1F44D}\u{1F3FD}";

#[test]
fn clusters_take_one_cluster_width_by_default() {
    for (name, text) in [
        ("ZWJ 가족", FAMILY),
        ("하트 + FE0F", HEART),
        ("국기", FLAG_KR),
        ("피부색", THUMB_TONE),
    ] {
        assert_eq!(width_of("", text, |_| {}), 2, "{name}");
    }
}

#[test]
fn ordinary_text_is_unchanged() {
    // 결합 문자는 앞 글자 칸에, 한글·ASCII·단독 이모지는 그대로
    assert_eq!(width_of("", "a\u{301}", |_| {}), 1);
    assert_eq!(width_of("", "\u{D55C}\u{AE00}", |_| {}), 4);
    assert_eq!(width_of("", "\u{1F600}\u{1F600}", |_| {}), 4);
    assert_eq!(width_of("", "\u{2764}", |_| {}), 1);
    // 국기 둘은 4칸(세 번째 지역 표시 글자부터는 새 묶음)
    assert_eq!(width_of("", &format!("{FLAG_KR}{FLAG_KR}"), |_| {}), 4);
}

#[test]
fn mode_2027_reset_counts_each_codepoint() {
    let off = "\\033[?2027l";
    assert_eq!(width_of(off, FAMILY, |_| {}), 6);
    assert_eq!(width_of(off, HEART, |_| {}), 1);
    assert_eq!(width_of(off, FLAG_KR, |_| {}), 2);
    assert_eq!(width_of(off, THUMB_TONE, |_| {}), 4);
    // 다시 켜면 묶음 폭
    assert_eq!(width_of(&format!("{off}\\033[?2027h"), FAMILY, |_| {}), 2);
}

#[test]
fn cluster_at_line_end_wraps_like_a_wide_char() {
    // 4칸 줄: a b [가족 2칸] 다음 글자는 다음 줄 맨 앞
    let s = run(&format!("printf 'ab{FAMILY}x'"), 4, |_| {});
    assert_eq!(s.row_text(0), format!("ab{FAMILY}"));
    assert_eq!(s.row_text(1), "x");
}

#[test]
fn modifier_at_line_start_does_not_join_the_previous_line() {
    // 가족을 찍고 CR로 맨 앞에 돌아와 피부색 수식자만 찍어도, 수식자는 칸 위의 글자에 붙지 않고 제 칸에 쓴다
    let s = run("printf '\\360\\237\\221\\250\\r\\360\\237\\217\\275x'", 20, |_| {});
    assert_eq!(s.cell(0, 0).ch, '\u{1F3FD}');
    assert!(s.cell(0, 0).combining.is_empty());
}

#[test]
fn legacy_setting_starts_with_mode_2027_off() {
    use bt_core::config::GraphemeWidthMethod::Legacy;
    let legacy = |o: &mut SpawnOptions| o.grapheme_width_method = Legacy;
    assert_eq!(width_of("", FAMILY, legacy), 6);
    assert_eq!(width_of("", HEART, legacy), 1);
    // 프로그램이 켜면 그때부터 묶음 폭
    assert_eq!(width_of("\\033[?2027h", FAMILY, legacy), 2);
    assert_eq!(width_of("\\033[?2027h", HEART, legacy), 2);
}

// ---- DECRQM ----

/// 질의를 찍고 PTY로 돌아온 바이트를 16진수로 화면에 적는 /bin/sh 스크립트
fn query_script(query: &str) -> String {
    format!(
        "stty raw -echo min 0 time 5; printf '{query}'; sleep 0.3; \
         r=$(dd bs=256 count=1 2>/dev/null | od -An -v -tx1 | tr -d ' \\n'); printf 'R[%s]\\n' \"$r\""
    )
}

fn reply_to(query: &str, opts: impl FnOnce(&mut SpawnOptions)) -> String {
    let s = run(&query_script(query), 80, opts);
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
fn decrqm_2027_reports_the_mode_state() {
    use bt_core::config::GraphemeWidthMethod::Legacy;
    // 켜진 채(Ghostty 기본)
    assert_eq!(reply_to("\\033[?2027$p", |_| {}), "\x1b[?2027;1$y");
    // 프로그램이 끄면 꺼짐, 다시 켜면 켜짐
    assert_eq!(reply_to("\\033[?2027l\\033[?2027$p", |_| {}), "\x1b[?2027;2$y");
    assert_eq!(
        reply_to("\\033[?2027l\\033[?2027h\\033[?2027$p", |_| {}),
        "\x1b[?2027;1$y"
    );
    // 설정이 legacy면 꺼진 채 시작
    assert_eq!(
        reply_to("\\033[?2027$p", |o| o.grapheme_width_method = Legacy),
        "\x1b[?2027;2$y"
    );
    // 모르는 모드는 그대로 '지원 안 함'
    assert_eq!(reply_to("\\033[?2028$p", |_| {}), "\x1b[?2028;0$y");
}

/// 한 칸에 이어 붙이는 결합 문자에는 상한이 있다. 상한이 없으면 이어 붙일 때마다 묶음 전체를 다시 나눠
/// 결합 문자 n개에 n² 일이 든다(찍힌 글 하나로 엔진을 붙잡을 수 있다)
#[test]
fn joining_stops_at_the_per_cell_cap() {
    let long: String = std::iter::once("\u{1F468}")
        .chain(std::iter::repeat_n("\u{200D}\u{1F469}", 20))
        .collect();
    let w = width_of("", &long, |_| {});
    assert!(w > 2, "상한을 넘은 뒤의 이모지는 새 칸에 찍힌다: {w}칸");
}
