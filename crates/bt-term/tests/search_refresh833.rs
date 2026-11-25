//! #833: 찾기 막대가 열린 채 출력이 오면 개수를 다시 센다(Ghostty `search/Thread.zig`의 새로 고침).
//! 안 바뀌었으면 다시 찾지 않는다(`Search::recounts`로 센다).

use bt_core::Theme;
use bt_term::{Emulator, GridSize};

fn emu(cols: u16, rows: u16) -> Emulator {
    Emulator::new(GridSize::new(cols, rows), Theme::default())
}

#[test]
fn refresh_recounts_when_output_adds_a_match() {
    let mut e = emu(20, 4);
    e.feed(b"Fizz1\r\nFizz2\r\nFizz3\r\n");
    assert_eq!(e.search("fizz").unwrap().total, 3);
    assert!(e.search_refresh().is_none(), "안 바뀌었으면 세지 않는다");
    e.feed(b"Fizz4\r\n");
    let st = e.search_refresh().expect("바뀌었으니 다시 센다");
    assert_eq!(st.total, 4, "출력으로 늘어난 맞은 곳이 개수에 든다");
    e.feed(b"\x1b[2J\x1b[H");
    assert_eq!(
        e.search_refresh().expect("지워졌다").total,
        1,
        "화면에서 지워진 것은 빠진다(스크롤백은 남는다)"
    );
}

#[test]
fn refresh_follows_the_selected_match_while_output_scrolls() {
    let mut e = emu(20, 3);
    e.feed(b"Fizz1\r\nFizz2\r\nFizz3");
    e.search("fizz").unwrap();
    assert_eq!(e.search_step(true).unwrap().index, Some(3));
    for i in 4..8 {
        e.feed(format!("\r\nFizz{i}").as_bytes());
    }
    // 고른 것(Fizz3)은 스크롤백으로 밀렸어도 같은 내용을 따라가 3번째다
    let st = e.search_refresh().expect("바뀜");
    assert_eq!((st.index, st.total), (Some(3), 7));
}

#[test]
fn refresh_does_not_search_again_when_nothing_changed() {
    let mut e = emu(20, 3);
    e.feed(b"Fizz1\r\nFizz2");
    e.search("fizz").unwrap();
    let base = e.search_recounts();
    for _ in 0..50 {
        assert!(e.search_refresh().is_none());
    }
    assert_eq!(e.search_recounts(), base, "화면이 같으면 스크롤백을 다시 찾지 않는다");
    // 커서만 움직이는 글(화면 내용은 같다)도 다시 찾지 않는다
    e.feed(b"\x1b[1;1H");
    assert!(e.search_refresh().is_none());
    e.feed(b"x");
    assert!(e.search_refresh().is_some());
    assert_eq!(e.search_recounts(), base + 1);
}
