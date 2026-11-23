//! 세 번 누르기(#803)·두 번 누르기(#804)·전체 고르기(#805)를 Ghostty 1.4.0 `Screen.zig`의 시험과 같은 입력·기대로 견준다.
//! 시험 이름 뒤 숫자는 Ghostty `src/terminal/Screen.zig`의 줄 번호다. 한 시험 안의 여러 경우를 모아 두었다가 한꺼번에 알린다.
//! OSC 133 표시는 PTY 읽기 길로만 엔진에 닿으므로 `mkp`는 `/bin/sh`의 `cat`으로 넣는다(PTY 시험은 `/bin/sh`와 `sleep`·`cat`만 쓴다).
//! 대응: Ghostty 시험의 `cursorSetSemanticContent(.prompt)`는 `A`(줄 중간이면 `P`), `.input = .clear_explicit`는 `B`(`Terminal.semanticPrompt`
//! 2231줄), `.clear_eol`은 `I`(2239줄), `.output`은 `C`다.

use std::sync::Arc;

use bt_core::Theme;
use bt_term::{GridSize, SelectKind, Session, SessionEvent, SpawnOptions};

const A: &str = "\x1b]133;A\x07"; // prompt
const B: &str = "\x1b]133;B\x07"; // input
const C: &str = "\x1b]133;C\x07"; // output
const P: &str = "\x1b]133;P\x07"; // 줄 끝이 아닌 자리에서 프롬프트로(A는 먼저 새 줄로 넘어간다, `Terminal.semanticPrompt` 2161줄)

fn mk(cols: u16, rows: u16, input: &str) -> Session {
    mkw(cols, rows, input, None)
}
fn mkw(cols: u16, rows: u16, input: &str, word: Option<&str>) -> Session {
    let opts = SpawnOptions {
        program: Some("/bin/sh".into()),
        args: vec!["-c".into(), "sleep 120".into()],
        cwd: Some(std::env::temp_dir()),
        scrollback: 1000,
        word_chars: Some(word.unwrap_or(bt_core::config::DEFAULT_WORD_CHARS).to_string()),
        ..Default::default()
    };
    let sink = Arc::new(|_e: SessionEvent| {});
    let s = Session::spawn(opts, GridSize::new(cols, rows), Theme::default(), sink).expect("spawn");
    s.feed_local(input.replace('\n', "\r\n").as_bytes());
    s
}

/// 실제 PTY 길(`cat`)로 바이트를 넣는다: OSC 133 표시(`Signal::Semantic`)는 PTY 읽기 길에서만 엔진에 닿는다
fn mkp(cols: u16, rows: u16, input: &str) -> Session {
    use std::sync::atomic::{AtomicUsize, Ordering};
    static N: AtomicUsize = AtomicUsize::new(0);
    let path = std::env::temp_dir().join(format!(
        "bt-select803-{}-{}",
        std::process::id(),
        N.fetch_add(1, Ordering::SeqCst)
    ));
    std::fs::write(&path, input.replace('\n', "\r\n")).unwrap();
    let opts = SpawnOptions {
        program: Some("/bin/sh".into()),
        args: vec!["-c".into(), format!("cat {}; sleep 120", path.display())],
        cwd: Some(std::env::temp_dir()),
        scrollback: 1000,
        word_chars: Some(bt_core::config::DEFAULT_WORD_CHARS.to_string()),
        ..Default::default()
    };
    let sink = Arc::new(|_e: SessionEvent| {});
    let s = Session::spawn(opts, GridSize::new(cols, rows), Theme::default(), sink).expect("spawn");
    let mut prev = String::new();
    for _ in 0..60 {
        std::thread::sleep(std::time::Duration::from_millis(100));
        let t = s.plain_text(false);
        if t == prev && !t.is_empty() {
            break;
        }
        prev = t;
    }
    let _ = std::fs::remove_file(&path);
    s
}

/// 고른 칸의 처음·끝(줄 우선 순서). 없으면 None
fn span(s: &Session, cols: usize, rows: usize) -> Option<((usize, usize), (usize, usize))> {
    let mut first = None;
    let mut last = None;
    for r in 0..rows {
        for c in 0..cols {
            if s.selection_contains(c, r) {
                if first.is_none() {
                    first = Some((c, r));
                }
                last = Some((c, r));
            }
        }
    }
    first.zip(last)
}

#[derive(Default)]
struct Diffs(Vec<String>);
impl Diffs {
    fn eq<T: PartialEq + std::fmt::Debug>(&mut self, label: &str, got: T, want: T) {
        if got != want {
            self.0.push(format!("{label}: 우리 {got:?} / Ghostty {want:?}"));
        }
    }
    fn done(self) {
        assert!(self.0.is_empty(), "\n차이 {}개\n{}", self.0.len(), self.0.join("\n"));
    }
}

/// 두 번 누르기(낱말)
fn word(s: &Session, cols: usize, rows: usize, p: (usize, usize)) -> Option<((usize, usize), (usize, usize))> {
    s.select_start(p.0, p.1, SelectKind::Word);
    s.select_update(p.0, p.1);
    span(s, cols, rows)
}
/// 세 번 누르기(줄)
fn line(s: &Session, cols: usize, rows: usize, p: (usize, usize)) -> Option<((usize, usize), (usize, usize))> {
    s.select_start(p.0, p.1, SelectKind::Line);
    s.select_update(p.0, p.1);
    span(s, cols, rows)
}
fn line_text(s: &Session, p: (usize, usize)) -> Option<String> {
    s.select_start(p.0, p.1, SelectKind::Line);
    s.select_update(p.0, p.1);
    s.selection_text()
}

type Sp = ((usize, usize), (usize, usize));
fn lt(s: &Session, cols: usize, rows: usize, p: (usize, usize)) -> (Option<Sp>, Option<String>) {
    s.select_start(p.0, p.1, SelectKind::Line);
    s.select_update(p.0, p.1);
    (span(s, cols, rows), s.selection_text())
}
fn some(sp: Sp, t: &str) -> (Option<Sp>, Option<String>) {
    (Some(sp), Some(t.to_string()))
}

#[test]
fn select_line() {
    let mut d = Diffs::default();
    let s = mk(10, 10, "ABC  DEF\n 123\n456");
    for x in [0, 7, 3, 9] {
        d.eq(
            &format!("selectLine x={x}"),
            lt(&s, 10, 10, (x, 0)),
            some(((0, 0), (7, 0)), "ABC  DEF"),
        );
    }
    // 앞 공백(Ghostty는 첫 글자부터): 같은 줄 시험에는 없지만 아래 soft-wrap 시험이 start x=1로 보인다
    d.eq("leading space", lt(&s, 10, 10, (2, 1)), some(((1, 1), (3, 1)), "123"));
    d.done();
}

// "selectLine across soft-wrap"(9628) / "across full soft-wrap"(9655) / "ignores blank lines"(9681)
#[test]
fn select_line_soft_wrap() {
    let mut d = Diffs::default();
    let s = mk(5, 10, " 12 34012   \n 123");
    d.eq("soft-wrap", lt(&s, 5, 10, (1, 0)), some(((1, 0), (3, 1)), "12 34012"));
    let s = mk(5, 5, "1ABCD2EFGH\n3IJKL");
    d.eq(
        "full soft-wrap",
        lt(&s, 5, 5, (2, 1)),
        some(((0, 0), (4, 1)), "1ABCD2EFGH"),
    );
    let s = mk(5, 10, " 12 34012             \n 123");
    d.eq("blank fwd", lt(&s, 5, 10, (1, 0)), some(((1, 0), (3, 1)), "12 34012"));
    d.eq("blank back", lt(&s, 5, 10, (1, 1)), some(((1, 0), (3, 1)), "12 34012"));
    d.eq("blank both", lt(&s, 5, 10, (3, 0)), some(((1, 0), (3, 1)), "12 34012"));
    d.done();
}

// "selectLine disabled whitespace trimming"(9742)는 설정 없는 선택지라 대응 없음(우리는 늘 Ghostty 기본값 꼴)
// "selectLine with scrollback"(9792)
#[test]
fn select_line_scrollback() {
    let mut d = Diffs::default();
    let s = mk(2, 3, "1A\n2B\n3C\n4D\n5E");
    d.eq("first", lt(&s, 2, 3, (0, 0)), some(((0, 0), (1, 0)), "3C"));
    d.eq("last", lt(&s, 2, 3, (0, 2)), some(((0, 2), (1, 2)), "5E"));
    d.done();
}

// "selectLine semantic prompt boundary"(9837)
#[test]
fn select_line_semantic_prompt_boundary() {
    let mut d = Diffs::default();
    let s = mkp(5, 10, &format!("ABCDE\n{A}A    {C}> "));
    d.eq("string", line_text(&s, (1, 1)), Some("A".to_string()));
    d.eq("output", line(&s, 5, 10, (1, 2)), Some(((0, 2), (0, 2))));
    d.done();
}

// "semantic boundary around unwritten cells"(9888): 쓰지 않은 칸은 null
#[test]
fn select_line_semantic_unwritten() {
    let mut d = Diffs::default();
    let s = mkp(5, 1, &format!("{A}x\x1b[2Cm"));
    for x in 1..3 {
        d.eq(&format!("unwritten x={x}"), lt(&s, 5, 1, (x, 0)), (None, None));
    }
    d.done();
}

// "prompt to input boundary"(9907)
#[test]
fn select_line_prompt_to_input() {
    let mut d = Diffs::default();
    let s = mkp(10, 5, &format!("{A}$>{B}command"));
    d.eq("prompt", lt(&s, 10, 5, (0, 0)), some(((0, 0), (1, 0)), "$>"));
    d.eq("input", lt(&s, 10, 5, (5, 0)), some(((2, 0), (8, 0)), "command"));
    d.done();
}

// "input to output boundary"(9957)
#[test]
fn select_line_input_to_output() {
    let mut d = Diffs::default();
    let s = mkp(10, 5, &format!("{B}ls -la\n{C}file.txt"));
    d.eq("input", line_text(&s, (2, 0)), Some("ls -la".to_string()));
    d.eq("output", line_text(&s, (2, 1)), Some("file.txt".to_string()));
    d.done();
}

// "mid-row boundary"(10003)
#[test]
fn select_line_mid_row() {
    let mut d = Diffs::default();
    let s = mkp(10, 5, &format!("{C}out{P}$>{B}cmd"));
    d.eq("output", lt(&s, 10, 5, (1, 0)), some(((0, 0), (2, 0)), "out"));
    d.eq("prompt", lt(&s, 10, 5, (3, 0)), some(((3, 0), (4, 0)), "$>"));
    d.eq("input", lt(&s, 10, 5, (6, 0)), some(((5, 0), (7, 0)), "cmd"));
    d.done();
}

// "soft-wrap with mid-row transition"(10072)
#[test]
fn select_line_soft_wrap_mid_row() {
    let mut d = Diffs::default();
    let s = mkp(5, 5, &format!("{A}$ {B}cmd12{C}out"));
    d.eq("in row0", line_text(&s, (3, 0)), Some("cmd12".to_string()));
    d.eq("in row1", line_text(&s, (0, 1)), Some("cmd12".to_string()));
    d.eq("out row1", line_text(&s, (3, 1)), Some("out".to_string()));
    d.done();
}

// "semantic boundary disabled"(10142): 설정이 없는 선택지(우리는 경계 없음 = 전체 줄)라 대응 없음
// "semantic boundary first cell of row"(10175)
#[test]
fn select_line_first_cell_of_row() {
    let mut d = Diffs::default();
    let s = mkp(5, 5, &format!("{B}12345{C}ABCDE"));
    d.eq("input", lt(&s, 5, 5, (2, 0)), some(((0, 0), (4, 0)), "12345"));
    d.eq("output", lt(&s, 5, 5, (2, 1)), some(((0, 1), (4, 1)), "ABCDE"));
    d.done();
}

// "semantic all same content"(10264)
#[test]
fn select_line_all_same() {
    let mut d = Diffs::default();
    let s = mkp(5, 5, &format!("{A}prompt text"));
    d.eq("string", line_text(&s, (2, 1)), Some("prompt text".to_string()));
    d.done();
}

// "selectLine does not join lines across a recycled row"(12803)는 아래 파일에서 본다
const DEF: &str = bt_core::config::DEFAULT_WORD_CHARS;

// "Screen: selectWord"(10299)
#[test]
fn select_word() {
    let mut d = Diffs::default();
    let s = mk(10, 10, "ABC  DEF\n 123\n456");
    d.eq("fwd", word(&s, 10, 10, (0, 0)), Some(((0, 0), (2, 0))));
    d.eq("back", word(&s, 10, 10, (2, 0)), Some(((0, 0), (2, 0))));
    d.eq("both", word(&s, 10, 10, (1, 0)), Some(((0, 0), (2, 0))));
    d.eq("whitespace run", word(&s, 10, 10, (3, 0)), Some(((3, 0), (4, 0))));
    d.eq("whitespace single", word(&s, 10, 10, (0, 1)), Some(((0, 1), (0, 1))));
    d.eq("end of screen", word(&s, 10, 10, (1, 2)), Some(((0, 2), (2, 2))));
    d.done();
}

// "selectWord at hard line breaks"(10424): 경계 {0,' '}
#[test]
fn select_word_hard_breaks() {
    let mut d = Diffs::default();
    for (cols, text) in [(5u16, "abcde\nfghij"), (5, "     \n     "), (1, "a\nb"), (1, " \n ")] {
        let s = mkw(cols, 2, text, Some(" "));
        for y in 0..2 {
            for x in 0..cols as usize {
                d.eq(
                    &format!("{text:?} ({x},{y})"),
                    word(&s, cols as usize, 2, (x, y)),
                    Some(((0, y), (cols as usize - 1, y))),
                );
            }
        }
    }
    d.done();
}

// "selectWord across soft-wrap at right edge"(10464)
#[test]
fn select_word_soft_wrap_right_edge() {
    let mut d = Diffs::default();
    let s = mkw(5, 3, "abcdefghij\nklmno", Some(" "));
    for y in 0..2 {
        for x in 0..5 {
            d.eq(&format!("({x},{y})"), word(&s, 5, 3, (x, y)), Some(((0, 0), (4, 1))));
        }
    }
    d.done();
}

// "selectWord wide characters"(10496)
#[test]
fn select_word_wide() {
    let mut d = Diffs::default();
    // (text, cols, boundary, start, end, expected)
    let cases: Vec<(&str, usize, &str, usize, usize, &str)> = vec![
        ("日本語", 10, " ", 0, 5, "日本語"),
        ("a日b語c", 10, " ", 0, 6, "a日b語c"),
        (" 日本語 ", 10, " ", 1, 6, "日本語"),
        ("日本語", 4, " ", 0, 5, "日本語"),
        ("日本語", 5, " ", 0, 6, "日本語"),
        ("日本\n語文", 4, " ", 0, 3, "日本"),
        ("日本\n語文", 4, " ", 4, 7, "語文"),
        ("a語b", 10, "語", 0, 0, "a"),
        ("a語b", 10, "語", 1, 2, "語"),
        ("a語b", 10, "語", 3, 3, "b"),
        ("abcd語ef", 5, "語", 0, 3, "abcd"),
        ("abcd語ef", 5, "語", 4, 6, "語"),
        ("abcd語ef", 5, "語", 7, 8, "ef"),
    ];
    for (text, cols, bound, start, end, expected) in cases {
        let s = mkw(cols as u16, 4, text, Some(bound));
        for off in start..=end {
            let p = (off % cols, off / cols);
            s.select_start(p.0, p.1, SelectKind::Word);
            s.select_update(p.0, p.1);
            let got = (span(&s, cols, 4), s.selection_text());
            let want = (
                Some(((start % cols, start / cols), (end % cols, end / cols))),
                Some(expected.to_string()),
            );
            d.eq(&format!("{text:?} cols={cols} bound={bound:?} off={off}"), got, want);
        }
    }
    d.done();
}

// "selectWord across soft-wrap"(10558) / "whitespace across soft-wrap"(10634)
#[test]
fn select_word_soft_wrap() {
    let mut d = Diffs::default();
    let s = mkw(5, 10, " 1234012\n 123", Some(DEF));
    d.eq("fwd", word(&s, 5, 10, (1, 0)), Some(((1, 0), (2, 1))));
    d.eq("back", word(&s, 5, 10, (1, 1)), Some(((1, 0), (2, 1))));
    d.eq("both", word(&s, 5, 10, (3, 0)), Some(((1, 0), (2, 1))));
    let s = mkw(5, 10, "1       1\n 123", Some(DEF));
    d.eq("ws fwd", word(&s, 5, 10, (1, 0)), Some(((1, 0), (2, 1))));
    d.eq("ws back", word(&s, 5, 10, (1, 1)), Some(((1, 0), (2, 1))));
    d.eq("ws both", word(&s, 5, 10, (3, 0)), Some(((1, 0), (2, 1))));
    d.done();
}

// "selectWord with character boundary"(10704)
#[test]
fn select_word_character_boundary() {
    let mut d = Diffs::default();
    let cases = [
        " 'abc' \n123",
        " \"abc\" \n123",
        " │abc│ \n123",
        " `abc` \n123",
        " |abc| \n123",
        " :abc: \n123",
        " ;abc; \n123",
        " ,abc, \n123",
        " (abc( \n123",
        " )abc) \n123",
        " [abc[ \n123",
        " ]abc] \n123",
        " {abc{ \n123",
        " }abc} \n123",
        " <abc< \n123",
        " >abc> \n123",
        " $abc$ \n123",
    ];
    for c in cases {
        let s = mkw(20, 10, c, Some(DEF));
        d.eq(&format!("{c:?} fwd"), word(&s, 20, 10, (2, 0)), Some(((2, 0), (4, 0))));
        d.eq(&format!("{c:?} back"), word(&s, 20, 10, (4, 0)), Some(((2, 0), (4, 0))));
        d.eq(&format!("{c:?} bidi"), word(&s, 20, 10, (3, 0)), Some(((2, 0), (4, 0))));
        d.eq(
            &format!("{c:?} on quote"),
            word(&s, 20, 10, (1, 0)),
            Some(((0, 0), (1, 0))),
        );
    }
    d.done();
}

// "Screen: selectOutput"(10815)

// 10496 selectWord wide characters: 경계 글자에 넓은 글자가 있고 공백도 경계일 때(실제 설정에서 나올 수 있는 꼴)
#[test]
fn select_word_wide_boundary_with_space() {
    let mut d = Diffs::default();
    let cases: Vec<(&str, usize, &str, usize, usize, &str)> = vec![
        ("a語b", 10, " 語", 0, 0, "a"),
        ("a語b", 10, " 語", 1, 2, "語"),
        ("a語b", 10, " 語", 3, 3, "b"),
        ("abcd語ef", 5, " 語", 0, 3, "abcd"),
        ("abcd語ef", 5, " 語", 4, 6, "語"),
        ("abcd語ef", 5, " 語", 7, 8, "ef"),
    ];
    for (text, cols, bound, start, end, expected) in cases {
        let s = mkw(cols as u16, 4, text, Some(bound));
        for off in start..=end {
            let p = (off % cols, off / cols);
            s.select_start(p.0, p.1, SelectKind::Word);
            s.select_update(p.0, p.1);
            let got = (span(&s, cols, 4), s.selection_text());
            let want = (
                Some(((start % cols, start / cols), (end % cols, end / cols))),
                Some(expected.to_string()),
            );
            d.eq(&format!("{text:?} cols={cols} bound={bound:?} off={off}"), got, want);
        }
    }
    d.done();
}

// ---- 전체 고르기(#805) ----

/// 앱의 `select_all` 동작이 부르는 세션 함수
fn select_all(s: &Session) -> bool {
    s.select_all()
}

// "Screen: selectAll"(9509)
#[test]
fn select_all_text_and_span() {
    let mut d = Diffs::default();
    let s = mk(10, 10, "ABC  DEF\n 123\n456");
    d.eq("ok1", select_all(&s), true);
    d.eq("text1", s.selection_text(), Some("ABC  DEF\n 123\n456".to_string()));
    d.eq("span1", span(&s, 10, 10), Some(((0, 0), (2, 2))));
    s.feed_local("\r\nFOO\r\n BAR\r\n BAZ\r\n QWERTY\r\n 12345678".as_bytes());
    select_all(&s);
    d.eq("span2", span(&s, 10, 10), Some(((0, 0), (8, 7))));
    d.done();
}

// #805 재현: `seq`처럼 화면보다 긴 출력은 스크롤백까지 통째로 고른다
#[test]
fn select_all_includes_scrollback() {
    let mut d = Diffs::default();
    let mut input = String::new();
    for i in 0..30 {
        input.push_str(&format!("line{i}\n"));
    }
    let s = mk(10, 5, input.trim_end());
    select_all(&s);
    let t = s.selection_text().unwrap_or_default();
    d.eq("starts at line0", t.starts_with("line0\nline1\n"), true);
    d.eq("ends at line29", t.ends_with("\nline29"), true);
    d.eq("lines", t.lines().count(), 30);
    d.done();
}

// 글이 하나도 없으면 아무것도 고르지 않고(Ghostty `selectAll`이 null), 앞서 고른 것도 그대로 둔다(`Surface.select_all`은 null이면 건드리지 않는다)
#[test]
fn select_all_empty_keeps_previous() {
    let mut d = Diffs::default();
    let s = mk(10, 5, "");
    d.eq("empty", select_all(&s), false);
    d.eq("nothing", span(&s, 10, 5), None);
    let s = mk(10, 5, "   \n  ");
    d.eq("blank only", select_all(&s), false);
    s.select_start(1, 0, SelectKind::Simple);
    s.select_update(3, 0);
    select_all(&s);
    d.eq("kept", span(&s, 10, 5), Some(((1, 0), (3, 0))));
    d.done();
}

// ---- 끌어서 늘리기(Ghostty `SelectionGesture.dragSelectionWord`·`dragSelectionLine`) ----

fn drag(s: &Session, kind: SelectKind, a: (usize, usize), b: (usize, usize), cols: usize, rows: usize) -> Option<Sp> {
    s.select_start(a.0, a.1, kind);
    s.select_update(b.0, b.1);
    span(s, cols, rows)
}

#[test]
fn drag_by_word_and_line() {
    let mut d = Diffs::default();
    let s = mk(20, 5, "foo bar baz\n  qux  quux\n\nlast");
    // 낱말 끌기: 누른 낱말 처음부터 끈 낱말 끝까지, 거꾸로 끌어도 두 낱말을 다 품는다
    d.eq(
        "word fwd",
        drag(&s, SelectKind::Word, (1, 0), (9, 0), 20, 5),
        Some(((0, 0), (10, 0))),
    );
    d.eq(
        "word back",
        drag(&s, SelectKind::Word, (9, 0), (1, 0), 20, 5),
        Some(((0, 0), (10, 0))),
    );
    d.eq(
        "word next line",
        drag(&s, SelectKind::Word, (5, 0), (3, 1), 20, 5),
        Some(((4, 0), (4, 1))),
    );
    // 줄 끌기: 누른 줄과 끈 줄을 앞뒤 공백 빼고 함께
    d.eq(
        "line fwd",
        drag(&s, SelectKind::Line, (1, 0), (5, 1), 20, 5),
        Some(((0, 0), (10, 1))),
    );
    d.eq(
        "line back",
        drag(&s, SelectKind::Line, (5, 1), (1, 0), 20, 5),
        Some(((0, 0), (10, 1))),
    );
    // 빈 줄을 누르면 고르지 않는다. 글 있는 줄에서 빈 줄로 끌면 Ghostty는 앞서 고른 것을 두지만(`dragSelectionLine`의 null),
    // 우리는 앞서 값이 없어 그 빈 줄 전체까지 고른다(알려진 차이)
    d.eq("blank press", drag(&s, SelectKind::Line, (0, 2), (0, 2), 20, 5), None);
    d.eq(
        "drag onto blank",
        drag(&s, SelectKind::Line, (1, 0), (3, 2), 20, 5),
        Some(((0, 0), (19, 2))),
    );
    d.done();
}

// `selection-word-chars`: 사용자가 정한 경계 글자도 같은 규칙(경계 글자를 누르면 이어진 경계 구간)
#[test]
fn custom_word_chars() {
    let mut d = Diffs::default();
    let s = mkw(20, 2, "a-b--c d", Some(" -"));
    d.eq("word", word(&s, 20, 2, (0, 0)), Some(((0, 0), (0, 0))));
    d.eq("run", word(&s, 20, 2, (3, 0)), Some(((3, 0), (4, 0))));
    d.eq("space", word(&s, 20, 2, (6, 0)), Some(((6, 0), (6, 0))));
    d.done();
}

// 세 번 누른 줄을 복사한 글에는 앞 공백이 없다(#803 재현 절차)
#[test]
fn line_text_has_no_leading_space() {
    let mut d = Diffs::default();
    let s = mk(20, 3, "  들여쓴 줄  \n끝");
    d.eq("text", line_text(&s, (5, 0)), Some("들여쓴 줄".to_string()));
    d.done();
}
