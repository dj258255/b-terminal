//! DCS 질의(DECRQSS·XTGETTCAP) 스캐너 테스트(#265)

use super::*;
use crate::dcs::{Decrqss, Query};

fn feed(bytes: &[u8]) -> Vec<Signal> {
    Scanner::default().feed(bytes)
}

fn q(d: Decrqss) -> Vec<Signal> {
    vec![Signal::DcsQuery(Query::Decrqss(d))]
}

#[test]
fn decrqss_requests_become_signals() {
    assert_eq!(feed(b"\x1bP$qm\x1b\\"), q(Decrqss::Sgr));
    assert_eq!(feed(b"\x1bP$qr\x1b\\"), q(Decrqss::Decstbm));
    assert_eq!(feed(b"\x1bP$qs\x1b\\"), q(Decrqss::Decslrm));
    // `DCS $ q SP q ST`
    assert_eq!(feed(b"\x1bP$q q\x1b\\"), q(Decrqss::Decscusr));
    // 모르는 한두 글자와 빈 본문은 "모르는 설정"으로 읽는다(답은 DCS 0)
    assert_eq!(feed(b"\x1bP$qz\x1b\\"), q(Decrqss::None));
    assert_eq!(feed(b"\x1bP$qq \x1b\\"), q(Decrqss::None));
    assert_eq!(feed(b"\x1bP$q\x1b\\"), q(Decrqss::None));
    // 매개변수가 붙어도 Ghostty처럼 읽는다
    assert_eq!(feed(b"\x1bP1$qm\x1b\\"), q(Decrqss::Sgr));
}

#[test]
fn decrqss_longer_than_two_bytes_is_dropped() {
    assert!(feed(b"\x1bP$q\" q\x1b\\").is_empty());
    assert!(feed(b"\x1bP$qmmm\x1b\\").is_empty());
}

#[test]
fn xtgettcap_requests_keep_the_raw_body() {
    assert_eq!(
        feed(b"\x1bP+q544e\x1b\\"),
        vec![Signal::DcsQuery(Query::XtGetTcap(b"544e".to_vec()))]
    );
    assert_eq!(
        feed(b"\x1bP+q536D756C78;544E\x1b\\"),
        vec![Signal::DcsQuery(Query::XtGetTcap(b"536D756C78;544E".to_vec()))]
    );
}

#[test]
fn chunk_boundaries_do_not_matter() {
    let all = b"ab\x1bP+q544e;436f\x1b\\cd\x1bP$qm\x1b\\";
    let want = feed(all);
    assert_eq!(want.len(), 2);
    for cut in 1..all.len() {
        let mut s = Scanner::default();
        let mut got = s.feed(&all[..cut]);
        got.extend(s.feed(&all[cut..]));
        assert_eq!(got, want, "cut={cut}");
    }
}

#[test]
fn only_st_ends_a_dcs_and_bel_does_not() {
    let mut s = Scanner::default();
    assert!(s.feed(b"\x1bP+q54\x07").is_empty());
    assert_eq!(
        s.feed(b"63\x1b\\"),
        vec![Signal::DcsQuery(Query::XtGetTcap(b"54\x0763".to_vec()))]
    );
}

#[test]
fn oversized_xtgettcap_is_dropped_without_growing() {
    let mut s = Scanner::default();
    let mut big = b"\x1bP+q".to_vec();
    big.extend(std::iter::repeat_n(b'4', crate::dcs::MAX_XTGETTCAP + 100));
    big.extend_from_slice(b"\x1b\\");
    assert!(s.feed(&big).is_empty());
    assert!(s.dcs.capacity() <= crate::dcs::MAX_XTGETTCAP * 2);
    // 뒤따르는 질의는 받는다
    assert_eq!(s.feed(b"\x1bP$qm\x1b\\"), q(Decrqss::Sgr));
    // 상한 안의 가장 긴 것은 받는다
    let mut ok = b"\x1bP+q".to_vec();
    ok.extend(std::iter::repeat_n(b'4', crate::dcs::MAX_XTGETTCAP));
    ok.extend_from_slice(b"\x1b\\");
    assert_eq!(s.feed(&ok).len(), 1);
}

#[test]
fn other_dcs_are_left_alone() {
    // sixel
    assert!(feed(b"\x1bPq#0;2;0;0;0~~\x1b\\").is_empty());
    // tmux 패스스루: 안의 ESC는 두 번 쓴다. 안에 든 질의는 우리 것이 아니다
    assert!(feed(b"\x1bPtmux;\x1b\x1bP$qm\x1b\x1b\\\x1b\\").is_empty());
    // DECRQSS·XTGETTCAP이 아닌 끝 문자
    assert!(feed(b"\x1bP$pm\x1b\\").is_empty());
    assert!(feed(b"\x1bP+pm\x1b\\").is_empty());
    // 중간 바이트가 둘이면 우리 모양이 아니다
    assert!(feed(b"\x1bP$+qm\x1b\\").is_empty());
    // 다른 DCS 뒤의 OSC·질의는 정상이다
    let mut s = Scanner::default();
    assert!(s.feed(b"\x1bPq~~\x1b\\").is_empty());
    assert_eq!(s.feed(b"\x1b]9;hi\x07").len(), 1);
    assert_eq!(s.feed(b"\x1bP$qm\x1b\\"), q(Decrqss::Sgr));
}

#[test]
fn can_sub_and_new_escape_abort_the_dcs() {
    assert!(feed(b"\x1bP$qm\x18\x1b\\").is_empty());
    assert!(feed(b"\x1bP$qm\x1a\x1b\\").is_empty());
    // ESC 뒤가 \가 아니면 끊긴 것이다
    assert!(feed(b"\x1bP$qm\x1b[0m").is_empty());
    // 끊긴 뒤의 새 질의는 받는다
    assert_eq!(feed(b"\x1bP$q\x18ok\x1bP$qm\x1b\\"), q(Decrqss::Sgr));
}

#[test]
fn answers_written_by_us_are_not_queries() {
    // DECRQCRA 답(`DCS 1 ! ~ XXXX ST`)과 우리가 쓰는 DCS 답은 PTY 입력 쪽이다. 출력에 같은 글이 찍혀도 질의가 아니다
    assert!(feed(b"\x1bP1!~FEDA\x1b\\").is_empty());
    assert!(feed(b"\x1bP1$r0m\x1b\\").is_empty());
    assert!(feed(b"\x1bP1+r5463\x1b\\").is_empty());
}
