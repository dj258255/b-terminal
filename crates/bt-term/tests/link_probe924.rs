//! #924: 링크 hover가 화면 전체 복사 없이 필요한 줄만 읽어도 옛 길(전체 `snapshot` → `link_at_with`)과 같은 링크를 찾는지.
//! 엔진에 바이트를 먹여 만든다(PTY 없음). 모든 칸·모든 `LinkOptions` 조합에서 옛 길과 새 길을 견준다.

use bt_core::Theme;
use bt_term::links::{LinkOptions, link_at_with};
use bt_term::{Emulator, GridSize};

fn emu(cols: u16, rows: u16, bytes: &[u8]) -> Emulator {
    let mut e = Emulator::new(GridSize::new(cols, rows), Theme::default());
    e.feed(bytes);
    e
}

/// 모든 칸·옵션에서 옛 길과 새 길이 같다. 링크가 하나라도 찾아졌는지(시험이 빈 시험이 아닌지)를 돌려준다
fn assert_same(e: &mut Emulator) -> usize {
    let snap = e.snapshot();
    let mut found = 0;
    for opts in [
        LinkOptions { url: true, osc8: true },
        LinkOptions { url: true, osc8: false },
        LinkOptions { url: false, osc8: true },
        LinkOptions {
            url: false,
            osc8: false,
        },
    ] {
        for r in 0..snap.rows + 1 {
            for c in 0..snap.cols + 1 {
                let old = link_at_with(&snap, c, r, opts);
                let new = e.link_probe(c, r, opts);
                assert_eq!(new.hit, old, "칸 ({c},{r}) {opts:?}");
                if old.is_some() {
                    found += 1;
                }
            }
        }
    }
    found
}

#[test]
fn soft_wrapped_url_over_three_rows() {
    let mut e = emu(
        20,
        8,
        b"go https://example.com/aaaaaaaaaaaaaaaaaaaaaaaaaaaa/b end\r\nnext https://x.y/z\r\n",
    );
    assert!(assert_same(&mut e) > 20);
}

#[test]
fn osc8_link_split_over_rows_and_second_link() {
    let mut e = emu(
        16,
        8,
        b"a \x1b]8;;https://one.example/p\x07click this long label\x1b]8;;\x07 b \x1b]8;;https://two.example\x07two\x1b]8;;\x07\r\nplain https://u.v/w\r\n",
    );
    assert!(assert_same(&mut e) > 20);
    // 명시 링크는 보이는 글과 주소가 달라도 명시로 나온다
    let r = e.link_probe(3, 0, LinkOptions::default());
    let hit = r.hit.expect("hit");
    assert!(hit.explicit);
    assert_eq!(hit.url, "https://one.example/p");
    assert!(hit.spans.len() >= 2);
}

#[test]
fn osc8_same_uri_in_two_places_is_one_link() {
    let mut e = emu(
        30,
        6,
        b"\x1b]8;;https://s.example\x07aa\x1b]8;;\x07 mid \x1b]8;;https://s.example\x07bb\x1b]8;;\x07\r\n\r\n\x1b]8;id=1;https://s.example\x07cc\x1b]8;;\x07\r\n",
    );
    assert!(assert_same(&mut e) > 5);
    let hit = e.link_probe(0, 0, LinkOptions::default()).hit.unwrap();
    assert_eq!(hit.spans.len(), 3);
}

#[test]
fn scrolled_back_view() {
    let mut bytes = Vec::new();
    for i in 0..60 {
        bytes.extend_from_slice(
            format!("line {i} https://h{i}.example/p{i} \x1b]8;;https://o{i}.example\x07o\x1b]8;;\x07\r\n").as_bytes(),
        );
    }
    let mut e = emu(40, 10, &bytes);
    assert!(assert_same(&mut e) > 20);
    e.scroll(7);
    assert!(assert_same(&mut e) > 20);
    e.scroll(30);
    assert!(assert_same(&mut e) > 20);
}

#[test]
fn wide_chars_and_wide_at_wrap_edge() {
    // 한글 바로 뒤의 주소, 줄 끝 칸에 못 들어가 접힌 넓은 글자, 주소 안의 넓은 글자
    let mut e = emu(
        19,
        8,
        "한글https://a.b/c 가나다라마바사아자차카타파 https://a.b/가나다/x 끝\r\n이모지😀https://q.r/s😀t\r\n"
            .as_bytes(),
    );
    assert!(assert_same(&mut e) > 20);
}

#[test]
fn alternate_screen() {
    let mut e = emu(
        24,
        6,
        b"main https://main.example\r\n\x1b[?1049h\x1b[2J\x1b[3;2Hhttps://alt.example/x\x1b[5;1Habc",
    );
    assert!(assert_same(&mut e) > 10);
    let hit = e.link_probe(3, 2, LinkOptions::default()).hit.unwrap();
    assert_eq!(hit.url, "https://alt.example/x");
}

#[test]
fn unsafe_looking_urls_get_same_result() {
    // 거르는 규칙은 links.rs 그대로 쓰는지: 제어 문자·방향 문자·여러 스킴이 섞인 글
    let mut e = emu(
        40,
        6,
        "x javascript:alert(1) https://a.b/\u{202e}evil file:///etc/passwd (https://p.q/r) <mailto:a@b.c> ssh://h/p\r\n".as_bytes(),
    );
    assert!(assert_same(&mut e) > 10);
}

#[test]
fn reads_fewer_cells_than_the_whole_screen() {
    let e = emu(80, 24, b"top\r\nplain https://example.com/abc here\r\nbottom\r\n");
    let total = 80 * 24;
    // 한 줄짜리 글 링크: 그 줄만 복사
    let r = e.link_probe(10, 1, LinkOptions::default());
    assert!(r.hit.is_some());
    assert!(r.cells_copied <= 80, "{r:?}");
    assert!(r.cells_copied < total / 10);
    // 링크 없는 빈 칸
    let r = e.link_probe(70, 20, LinkOptions::default());
    assert!(r.hit.is_none());
    assert!(r.cells_copied <= 80, "{r:?}");
}

#[test]
fn wrapped_logical_line_reads_only_its_rows() {
    let e = emu(
        20,
        24,
        b"head\r\nhttps://example.com/aaaaaaaaaaaaaaaaaaaaaaaaaaaa/bbbbbbbbbbbbbbbbbbbbb\r\ntail\r\n",
    );
    let r = e.link_probe(5, 1, LinkOptions::default());
    assert!(r.hit.is_some());
    // 4줄에 접힌 주소: 4줄(80칸)이면 되고 화면 전체(480칸)가 아니다
    assert!(r.cells_copied == 4 * 20, "{r:?}");
}

#[test]
fn osc8_hit_does_not_copy_the_screen() {
    let e = emu(80, 24, b"x \x1b]8;;https://one.example\x07label\x1b]8;;\x07 y\r\n");
    let r = e.link_probe(3, 0, LinkOptions::default());
    assert!(r.hit.as_ref().is_some_and(|h| h.explicit));
    assert!(r.cells_copied <= 1, "{r:?}");
}

#[test]
fn out_of_range_reads_nothing() {
    let e = emu(20, 5, b"https://a.b/c");
    let r = e.link_probe(20, 0, LinkOptions::default());
    assert!(r.hit.is_none() && r.cells_copied == 0);
    let r = e.link_probe(0, 5, LinkOptions::default());
    assert!(r.hit.is_none() && r.cells_copied == 0);
}

#[test]
fn more_than_the_link_cap_on_one_screen_matches_the_old_path() {
    // 칸마다 다른 링크 5000개: 옛 길은 4096개째까지만 링크로 친다. 그 뒤 칸은 링크가 아니다
    let mut bytes = Vec::new();
    for i in 0..5000 {
        bytes.extend_from_slice(format!("\x1b]8;;https://h.example/{i}\x07x\x1b]8;;\x07").as_bytes());
    }
    let mut e = emu(100, 50, &bytes);
    let snap = e.snapshot();
    assert!(snap.links.len() >= 4096);
    let opts = LinkOptions::default();
    for r in 0..50 {
        for c in (0..100).step_by(3) {
            assert_eq!(
                e.link_probe(c, r, opts).hit,
                link_at_with(&snap, c, r, opts),
                "({c},{r})"
            );
        }
    }
}
