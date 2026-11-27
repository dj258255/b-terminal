//! #929: kitty 마커(`toggle_marker`·`remove_marker`·`scroll_to_mark`). 엔진에 바이트를 먹이는 시험(PTY 없음).
//! 문법은 kitty `kitty/options/utils.py`(`parse_marker_spec`·`toggle_marker`·`scroll_to_mark`)와 `docs/marks.rst`를 따른다.

use bt_core::Theme;
use bt_term::marks::{Marker, MarkerSpec, parse_scroll_args, shlex_split};
use bt_term::screen::CellFlags;
use bt_term::{Emulator, GridSize};

fn emu(cols: u16, rows: u16, bytes: &[u8]) -> Emulator {
    let mut e = Emulator::new(GridSize::new(cols, rows), Theme::default());
    e.feed(bytes);
    e
}

fn spec(s: &str) -> MarkerSpec {
    MarkerSpec::parse(s).unwrap()
}

fn mark_of(f: CellFlags) -> u8 {
    [CellFlags::MARK1, CellFlags::MARK2, CellFlags::MARK3]
        .iter()
        .position(|m| f.contains(*m))
        .map_or(0, |i| i as u8 + 1)
}

fn row_marks(e: &mut Emulator, row: usize) -> Vec<u8> {
    let s = e.snapshot();
    (0..s.cols).map(|c| mark_of(s.cell(c, row).flags)).collect()
}

#[test]
fn shlex_matches_kitty_lexer() {
    assert_eq!(shlex_split("1 ERROR").unwrap(), ["1", "ERROR"]);
    assert_eq!(
        shlex_split(r"1 \\bERROR\\b").unwrap(),
        ["1", r"\bERROR\b"],
        "백슬래시는 다음 글자를 그대로 낸다"
    );
    assert_eq!(shlex_split("1 'a b' 2 \"c\\\"d\"").unwrap(), ["1", "a b", "2", "c\"d"]);
    assert_eq!(shlex_split("1 ''").unwrap(), ["1", ""], "빈 따옴표 낱말");
    assert!(shlex_split("1 'open").is_err(), "닫히지 않은 따옴표");
    assert!(shlex_split("1 a\\").is_err(), "끝의 백슬래시");
}

#[test]
fn parse_follows_kitty_parse_marker_spec() {
    let s = spec("text 1 ERROR");
    assert_eq!(s, MarkerSpec::parse("text   1   ERROR").unwrap());
    // 색 번호는 1~3으로 맞춘다(max(1, min(n, 3)))
    assert_eq!(spec("regex 9 a").groups(), &[(3, "a".to_string())]);
    assert_eq!(spec("regex 0 a").groups(), &[(1, "a".to_string())]);
    assert_eq!(spec("regex 1 a 2 b").groups().len(), 2);
    // text는 정규식 글자를 벗긴다
    assert_eq!(spec("text 1 a.b").groups(), &[(1, r"a\.b".to_string())]);
    for bad in [
        "text 1",        // 짝이 안 맞음
        "text",          // 명세 없음
        "text x ERROR",  // 색이 정수가 아님
        "function a.py", // 함수 마커는 하지 않는다
        "bogus 1 a",     // 모르는 종류
        "",
    ] {
        assert!(MarkerSpec::parse(bad).is_err(), "{bad:?}");
    }
    assert!(
        MarkerSpec::parse("regex 1 (unclosed").unwrap().compile().is_err(),
        "컴파일에서 막는다"
    );
}

#[test]
fn scroll_args_follow_kitty() {
    // 인자 없음: 이전, 모든 색
    assert_eq!(parse_scroll_args("").unwrap(), (true, 0));
    assert_eq!(parse_scroll_args("prev").unwrap(), (true, 0));
    assert_eq!(parse_scroll_args("previous").unwrap(), (true, 0));
    assert_eq!(parse_scroll_args("NEXT").unwrap(), (false, 0));
    assert_eq!(parse_scroll_args("2").unwrap(), (true, 2));
    assert_eq!(parse_scroll_args("9").unwrap(), (true, 3));
    assert_eq!(parse_scroll_args("next 1").unwrap(), (false, 1));
    assert_eq!(parse_scroll_args("prev 3").unwrap(), (true, 3));
    assert!(parse_scroll_args("sideways").is_err());
    assert!(parse_scroll_args("next x").is_err());
}

#[test]
fn spans_text_itext_regex_and_alternation_order() {
    let m = |s: &str| Marker::new(&spec(s)).unwrap();
    assert_eq!(m("text 1 ERROR").spans("an ERROR, an error"), [(3, 8, 1)]);
    assert_eq!(m("itext 1 ERROR").spans("an ERROR, an error"), [(3, 8, 1), (13, 18, 1)]);
    assert_eq!(m(r"regex 1 \\bERROR\\b").spans("ERRORS ERROR"), [(7, 12, 1)]);
    assert_eq!(
        m(r"iregex 1 \\bERROR\\b 2 \\bWARNING\\b").spans("error Warning"),
        [(0, 5, 1), (6, 13, 2)]
    );
    // 같은 자리에서는 앞 정규식이 이긴다(kitty는 `(?P<mcg0>..)|(?P<mcg1>..)` 한 정규식)
    assert_eq!(m("regex 1 ab 2 abc").spans("abc"), [(0, 2, 1)]);
    // 빈 맞음은 칠하지 않는다
    assert!(m("regex 1 x*").spans("abc").is_empty());
}

#[test]
fn toggle_marks_visible_cells_and_second_toggle_removes() {
    let mut e = emu(20, 4, b"ERROR one\r\nok\r\nxx ERROR");
    assert!(e.toggle_marker(&spec("text 1 ERROR")).unwrap(), "켜짐");
    assert_eq!(row_marks(&mut e, 0)[..9], [1, 1, 1, 1, 1, 0, 0, 0, 0]);
    assert!(row_marks(&mut e, 1).iter().all(|m| *m == 0));
    assert_eq!(row_marks(&mut e, 2)[..8], [0, 0, 0, 1, 1, 1, 1, 1]);
    // 같은 명세를 다시: 끈다
    assert!(!e.toggle_marker(&spec("text 1 ERROR")).unwrap());
    assert!(row_marks(&mut e, 0).iter().all(|m| *m == 0));
    // 다른 명세는 바꾼다(마커는 칸마다 하나)
    e.toggle_marker(&spec("text 1 ERROR")).unwrap();
    e.toggle_marker(&spec("text 2 one")).unwrap();
    assert_eq!(row_marks(&mut e, 0)[..9], [0, 0, 0, 0, 0, 0, 2, 2, 2]);
    assert!(e.remove_marker());
    assert!(!e.remove_marker(), "없으면 아무 일도 없다");
    assert!(row_marks(&mut e, 0).iter().all(|m| *m == 0));
}

#[test]
fn marks_do_not_change_the_grid() {
    let mut e = emu(20, 3, b"\x1b[31mERROR\x1b[0m one\r\n");
    let before = e.snapshot();
    e.toggle_marker(&spec("text 3 ERROR")).unwrap();
    let after = e.snapshot();
    for r in 0..before.rows {
        for c in 0..before.cols {
            let (a, b) = (before.cell(c, r), after.cell(c, r));
            assert_eq!((a.ch, a.fg, a.bg), (b.ch, b.fg, b.bg), "({c},{r}) 글자·색은 그대로");
            let strip = |f: CellFlags| {
                (
                    f.contains(CellFlags::BOLD),
                    f.contains(CellFlags::UNDERLINE),
                    f.contains(CellFlags::SELECTED),
                )
            };
            assert_eq!(strip(a.flags), strip(b.flags));
        }
    }
    assert_eq!(mark_of(after.cell(0, 0).flags), 3);
}

#[test]
fn wide_chars_mark_both_cells() {
    let mut e = emu(20, 2, "한글 ERR".as_bytes());
    e.toggle_marker(&spec("text 2 글")).unwrap();
    // 한(0,1) 글(2,3)
    assert_eq!(row_marks(&mut e, 0)[..6], [0, 0, 2, 2, 0, 0]);
    e.toggle_marker(&spec("text 1 ERR")).unwrap();
    assert_eq!(row_marks(&mut e, 0)[5..8], [1, 1, 1], "넓은 글자 뒤 칸 수가 맞다");
}

#[test]
fn matching_is_per_row_not_across_soft_wrap() {
    // kitty docs/marks.rst: 한 줄 안에서만 맞춘다
    let mut e = emu(10, 3, b"abcdefgHIJKLM");
    e.toggle_marker(&spec("text 1 HIJKLM")).unwrap();
    assert!(row_marks(&mut e, 0).iter().all(|m| *m == 0) || row_marks(&mut e, 0)[7..].iter().all(|m| *m == 1));
    e.toggle_marker(&spec("text 1 JKL")).unwrap();
    assert!(row_marks(&mut e, 0).iter().all(|m| *m == 0), "줄을 넘는 맞음은 없다");
    assert!(row_marks(&mut e, 1).iter().all(|m| *m == 0));
}

fn numbered(lines: usize) -> Emulator {
    let mut e = emu(20, 5, b"");
    for i in 0..lines {
        let tag = match i {
            3 => " ERROR",
            12 => " WARN",
            20 => " ERROR",
            _ => "",
        };
        e.feed(format!("line{i}{tag}\r\n").as_bytes());
    }
    e
}

fn first_word(s: &str) -> String {
    s.split_whitespace().next().unwrap_or("").to_string()
}

fn top_text(e: &mut Emulator) -> String {
    let s = e.snapshot();
    first_word(&(0..s.cols).map(|c| s.cell(c, 0).ch).collect::<String>())
}

#[test]
fn scroll_to_mark_prev_and_next_put_the_mark_line_at_the_edge() {
    let mut e = numbered(30);
    assert!(!e.scroll_to_mark(true, 0), "마커가 없으면 아무 일도 없다");
    e.toggle_marker(&spec("regex 1 ERROR 2 WARN")).unwrap();
    // 맨 아래에서 위로: 보이는 줄 위의 가장 가까운 마크 줄이 맨 윗줄에 온다
    assert!(e.scroll_to_mark(true, 0));
    assert_eq!(top_text(&mut e), "line20");
    assert!(e.scroll_to_mark(true, 0));
    assert_eq!(top_text(&mut e), "line12");
    assert!(e.scroll_to_mark(true, 0));
    assert_eq!(top_text(&mut e), "line3");
    assert!(!e.scroll_to_mark(true, 0), "더 위에 없다");
    assert_eq!(top_text(&mut e), "line3", "못 찾으면 화면을 옮기지 않는다");
    // 아래로: 보이는 마지막 줄 밑의 마크 줄이 맨 아랫줄에 온다
    assert!(e.scroll_to_mark(false, 0));
    let s = e.snapshot();
    let bottom = first_word(&(0..s.cols).map(|c| s.cell(c, s.rows - 1).ch).collect::<String>());
    assert_eq!(bottom, "line12");
}

#[test]
fn scroll_to_mark_filters_by_mark_number() {
    let mut e = numbered(30);
    e.toggle_marker(&spec("regex 1 ERROR 2 WARN")).unwrap();
    assert!(e.scroll_to_mark(true, 2));
    assert_eq!(top_text(&mut e), "line12");
    assert!(!e.scroll_to_mark(true, 2), "2번 마크는 더 없다");
    assert!(e.scroll_to_mark(true, 1));
    assert_eq!(top_text(&mut e), "line3");
}

#[test]
fn scroll_to_mark_does_nothing_on_alt_screen() {
    let mut e = numbered(30);
    e.toggle_marker(&spec("text 1 ERROR")).unwrap();
    e.feed(b"\x1b[?1049h");
    assert!(!e.scroll_to_mark(true, 0), "kitty: 대체 화면에서는 하지 않는다");
}

#[test]
fn scroll_to_mark_scan_has_a_limit() {
    let mut e = emu(20, 5, b"");
    e.feed(b"far ERROR\r\n");
    for i in 0..300 {
        e.feed(format!("l{i}\r\n").as_bytes());
    }
    e.toggle_marker(&spec("text 1 ERROR")).unwrap();
    assert!(
        !e.scroll_to_mark_within(true, 0, 100),
        "상한(100줄) 밖의 마크는 찾지 않는다"
    );
    assert!(e.scroll_to_mark_within(true, 0, 10_000));
    assert_eq!(top_text(&mut e), "far");
}

#[test]
fn marker_work_is_bounded_by_visible_rows() {
    // 스크롤백이 길어도 화면을 만들 때 검사하는 줄은 보이는 줄뿐이다
    let mut e = emu(40, 6, b"");
    for i in 0..2000 {
        e.feed(format!("ERROR {i}\r\n").as_bytes());
    }
    e.toggle_marker(&spec("text 1 ERROR")).unwrap();
    let _ = e.snapshot();
    assert!(e.marker_rows_checked() <= 6 + 1, "{}", e.marker_rows_checked());
}
