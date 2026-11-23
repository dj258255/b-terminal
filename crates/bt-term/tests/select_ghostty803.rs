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

// "selectLine does not join lines across a recycled row"(12803)는 아래 파일에서 본다
const DEF: &str = bt_core::config::DEFAULT_WORD_CHARS;

// "Screen: selectOutput"(10815)

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
