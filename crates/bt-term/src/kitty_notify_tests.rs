//! kitty 알림(OSC 99)의 조각 합치기·base64·지원 질의(#561). 기준은 kitty 명세(`desktop-notifications.rst`)다

use super::*;
use crate::kitty_notify::{MAX_ID_LEN, MAX_PENDING};

fn feed(s: &mut Scanner, osc: &str) -> Vec<Signal> {
    s.feed(osc.as_bytes())
}

fn n(title: &str, body: &str) -> Signal {
    Signal::Notify(Notification {
        title: title.into(),
        body: body.into(),
    })
}

fn st(meta_payload: &str) -> String {
    format!("\x1b]99;{meta_payload}\x1b\\")
}

fn bel(meta_payload: &str) -> String {
    format!("\x1b]99;{meta_payload}\x07")
}

fn b64(data: &[u8]) -> String {
    crate::kitty_clip::b64_encode(data)
}

#[test]
fn spec_example_title_chunk_then_body_is_one_notification() {
    let mut s = Scanner::default();
    assert!(feed(&mut s, &st("i=1:d=0;Hello world")).is_empty());
    assert_eq!(
        feed(&mut s, &st("i=1:p=body;This is cool")),
        vec![n("Hello world", "This is cool")]
    );
    // 알림을 낸 뒤 같은 i가 다시 오면 새 알림이다(쌓인 것은 비워졌다)
    assert_eq!(feed(&mut s, &st("i=1:p=body;again")), vec![n("", "again")]);
}

#[test]
fn different_ids_interleaved_each_make_one_notification() {
    let mut s = Scanner::default();
    assert!(feed(&mut s, &st("i=a:d=0;AAA")).is_empty());
    assert!(feed(&mut s, &st("i=b:d=0;BBB")).is_empty());
    assert_eq!(feed(&mut s, &st("i=b:p=body;b-body")), vec![n("BBB", "b-body")]);
    assert_eq!(feed(&mut s, &st("i=a:p=body;a-body")), vec![n("AAA", "a-body")]);
}

#[test]
fn title_chunks_are_concatenated() {
    let mut s = Scanner::default();
    assert!(feed(&mut s, &st("i=x:d=0;Hel")).is_empty());
    assert!(feed(&mut s, &st("i=x:d=0;lo wo")).is_empty());
    assert_eq!(feed(&mut s, &st("i=x:d=1;rld")), vec![n("Hello world", "")]);
}

#[test]
fn chunks_without_i_are_not_accumulated() {
    let mut s = Scanner::default();
    // i가 없으면 갱신하지 않는다(명세 188줄): 조각마다 따로 알린다. d=0이어도 같다(명세가 침묵하는 자리, 보고에 적는다)
    assert_eq!(feed(&mut s, &st("d=0;one")), vec![n("one", "")]);
    assert_eq!(feed(&mut s, &st("p=body;two")), vec![n("", "two")]);
    assert_eq!(feed(&mut s, &st(";plain")), vec![n("plain", "")]);
}

#[test]
fn other_payload_kinds_are_dropped_but_keep_pending() {
    let mut s = Scanner::default();
    assert!(feed(&mut s, &st("i=1:d=0;T")).is_empty());
    for p in ["buttons", "icon", "close", "alive", "query", "nope", ""] {
        assert!(feed(&mut s, &st(&format!("i=1:p={p};zzz"))).is_empty(), "p={p}");
    }
    assert_eq!(feed(&mut s, &st("i=1:p=body;B")), vec![n("T", "B")]);
}

#[test]
fn unknown_keys_are_ignored_and_first_valid_value_wins() {
    let mut s = Scanner::default();
    assert_eq!(feed(&mut s, &st("zz=1:i=7:u=2:a=report:f=Zm9v;hi")), vec![n("hi", "")]);
    // 같은 키가 여럿이면 첫 값(Ghostty ValueIterator는 `it.next()` 하나만 읽는다)
    assert!(feed(&mut s, &st("i=k:i=other:d=0:d=1;a")).is_empty());
    assert_eq!(feed(&mut s, &st("i=k;b")), vec![n("ab", "")]);
    // 키 둘레의 공백은 잘라 낸다
    assert!(feed(&mut s, &st(" i = w : d = 0 ;x")).is_empty());
    assert_eq!(feed(&mut s, &st("i=w;y")), vec![n("xy", "")]);
}

#[test]
fn base64_title_and_body_with_hangul() {
    let mut s = Scanner::default();
    let t = b64("빌드 끝".as_bytes());
    let b = b64("모두 통과했습니다".as_bytes());
    assert!(feed(&mut s, &st(&format!("i=1:e=1:d=0;{t}"))).is_empty());
    assert_eq!(
        feed(&mut s, &st(&format!("i=1:e=1:p=body;{b}"))),
        vec![n("빌드 끝", "모두 통과했습니다")]
    );
    // 패딩이 없는 끝 조각도 받는다(명세 base64 절)
    let t = b64(b"ab").trim_end_matches('=').to_string();
    assert_eq!(feed(&mut s, &st(&format!("e=1;{t}"))), vec![n("ab", "")]);
}

#[test]
fn bad_base64_or_non_utf8_is_dropped() {
    let mut s = Scanner::default();
    assert!(feed(&mut s, &st("i=1:d=0;keep")).is_empty());
    assert!(feed(&mut s, &st("i=1:e=1;@@@@")).is_empty());
    assert!(feed(&mut s, &st("i=1:e=1;ab=cd")).is_empty());
    assert!(feed(&mut s, &st("i=1:e=1;ab=c")).is_empty(), "가운데의 =");
    assert!(feed(&mut s, &st("i=1:e=1;YQ===")).is_empty(), "패딩이 셋");
    assert!(feed(&mut s, &st("i=1:e=1;abcde")).is_empty());
    // 0xff 0xfe는 UTF-8이 아니다
    assert!(feed(&mut s, &st(&format!("i=1:e=1;{}", b64(&[0xff, 0xfe])))).is_empty());
    // 버린 조각은 쌓인 것을 건드리지 않는다
    assert_eq!(feed(&mut s, &st("i=1:p=body;b")), vec![n("keep", "b")]);
}

#[test]
fn plain_utf8_that_is_not_valid_is_dropped() {
    let mut s = Scanner::default();
    assert!(s.feed(b"\x1b]99;;ab\xff\xfecd\x1b\\").is_empty());
    assert_eq!(s.feed(b"\x1b]99;;ok\x1b\\"), vec![n("ok", "")]);
}

#[test]
fn control_chars_are_cleaned_and_long_text_is_capped() {
    let mut s = Scanner::default();
    // base64로 푼 글의 제어 문자는 공백이 되고, 앞뒤 공백은 잘린다
    let t = b64(b"a\x1b[31mb\nc");
    assert_eq!(feed(&mut s, &st(&format!("e=1;{t}"))), vec![n("a [31mb c", "")]);
    // 제목은 MAX_TITLE까지, 본문은 MAX_BODY까지. 넘치면 …로 줄인다
    assert!(feed(&mut s, &st(&format!("i=1:d=0;{}", "가".repeat(600)))).is_empty());
    assert!(feed(&mut s, &st(&format!("i=1:d=0;{}", "가".repeat(600)))).is_empty());
    assert!(feed(&mut s, &st(&format!("i=1:d=0:p=body;{}", "나".repeat(600)))).is_empty());
    let out = feed(&mut s, &st(&format!("i=1:p=body;{}", "나".repeat(600))));
    let [Signal::Notify(note)] = &out[..] else {
        panic!("{out:?}")
    };
    assert_eq!(note.title.chars().count(), crate::notify::MAX_TITLE);
    assert!(note.title.ends_with('…'));
    assert_eq!(note.body.chars().count(), crate::notify::MAX_BODY);
    assert!(note.body.ends_with('…'));
}

#[test]
fn accumulated_text_stays_bounded_in_memory() {
    let mut s = Scanner::default();
    for _ in 0..200 {
        feed(&mut s, &st(&format!("i=1:d=0;{}", "x".repeat(2000))));
        feed(&mut s, &st(&format!("i=1:d=0:p=body;{}", "y".repeat(2000))));
    }
    let held = s.kitty_notify.held_chars();
    assert!(held > 0, "쌓이지 않았다");
    assert!(
        held <= crate::notify::MAX_TITLE + crate::notify::MAX_BODY + 2,
        "쌓인 글자 {held}"
    );
}

#[test]
fn oversized_chunks_are_dropped() {
    let mut s = Scanner::default();
    // 평문 2048바이트까지, 그 이상은 버린다
    assert_eq!(feed(&mut s, &st(&format!(";{}", "a".repeat(2048)))).len(), 1);
    assert!(feed(&mut s, &st(&format!(";{}", "a".repeat(2049)))).is_empty());
    // 인코딩은 4096바이트까지(글 길이는 MAX_TITLE로 잘리므로 알림은 난다)
    let ok = "QUJD".repeat(1024); // 4096바이트
    assert_eq!(feed(&mut s, &st(&format!("e=1;{ok}"))).len(), 1);
    let big = "QUJD".repeat(1024) + "QUJD";
    assert!(feed(&mut s, &st(&format!("e=1;{big}"))).is_empty());
    // 큰 조각을 버려도 쌓인 것은 그대로
    assert!(feed(&mut s, &st("i=2:d=0;head")).is_empty());
    assert!(feed(&mut s, &st(&format!("i=2:d=0;{}", "a".repeat(2049)))).is_empty());
    assert_eq!(feed(&mut s, &st("i=2:p=body;b")), vec![n("head", "b")]);
}

#[test]
fn pending_count_is_capped_and_oldest_is_dropped() {
    let mut s = Scanner::default();
    let cap = MAX_PENDING;
    for i in 0..cap + 4 {
        assert!(feed(&mut s, &st(&format!("i=id{i}:d=0;t{i}"))).is_empty());
    }
    assert_eq!(s.kitty_notify.pending_len(), cap);
    // 가장 오래된 넷(id0~id3)은 버려졌다: 본문만 가진 새 알림이 된다
    assert_eq!(feed(&mut s, &st("i=id0:p=body;late")), vec![n("", "late")]);
    // 남은 것은 합쳐진다
    assert_eq!(
        feed(&mut s, &st(&format!("i=id{}:p=body;late", cap + 3))),
        vec![n(&format!("t{}", cap + 3), "late")]
    );
    // 같은 i로 다시 오면 개수는 늘지 않는다
    let before = s.kitty_notify.pending_len();
    feed(&mut s, &st(&format!("i=id{}:d=0;more", cap + 2)));
    assert_eq!(s.kitty_notify.pending_len(), before);
}

const REPLY_BODY: &str = "o=always:p=title,body,?";

#[test]
fn query_answers_with_the_same_terminator() {
    let mut s = Scanner::default();
    assert_eq!(
        feed(&mut s, &st("i=probe-1:p=?;")),
        vec![Signal::KittyNotifyReply(
            format!("\x1b]99;i=probe-1:p=?;{REPLY_BODY}\x1b\\").into_bytes()
        )]
    );
    assert_eq!(
        feed(&mut s, &bel("i=probe-1:p=?;")),
        vec![Signal::KittyNotifyReply(
            format!("\x1b]99;i=probe-1:p=?;{REPLY_BODY}\x07").into_bytes()
        )]
    );
    // 질의는 알림도 쌓임도 건드리지 않는다
    assert!(feed(&mut s, &st("i=q:d=0;T")).is_empty());
    feed(&mut s, &st("i=q:p=?;"));
    assert_eq!(feed(&mut s, &st("i=q:p=body;B")), vec![n("T", "B")]);
}

#[test]
fn query_without_id_omits_i() {
    let mut s = Scanner::default();
    assert_eq!(
        feed(&mut s, &st("p=?;")),
        vec![Signal::KittyNotifyReply(
            format!("\x1b]99;p=?;{REPLY_BODY}\x1b\\").into_bytes()
        )]
    );
}

#[test]
fn query_with_bad_id_gets_no_reply_and_nothing_is_injected() {
    let mut s = Scanner::default();
    // `;`는 메타데이터를 끝내므로 `i=a;b:p=?`의 `b:p=?`는 페이로드다: 질의가 아니라 제목 글이다
    assert_eq!(feed(&mut s, &st("i=a;b:p=?")), vec![n("b:p=?", "")]);
    assert_eq!(
        feed(&mut s, &st("i=a:p=?;b:c")),
        vec![Signal::KittyNotifyReply(
            format!("\x1b]99;i=a:p=?;{REPLY_BODY}\x1b\\").into_bytes()
        )]
    );
    for bad in [
        "i=a b:p=?;",
        "i=a/b:p=?;",
        "i=a,b:p=?;",
        "i=a=b:p=?;",
        "i=한글:p=?;",
        "i=a%:p=?;",
        "i=a?:p=?;",
        "p=?:i=a.b?c;",
    ] {
        assert!(feed(&mut s, &st(bad)).is_empty(), "{bad}");
    }
    // ESC는 OSC를 끊으므로 답이 없다
    assert!(s.feed(b"\x1b]99;i=\x1b[31m:p=?;\x1b\\").is_empty());
    // 아주 긴 i
    let long = format!("i={}:p=?;", "a".repeat(MAX_ID_LEN + 1));
    assert!(feed(&mut s, &st(&long)).is_empty());
    let longest = format!("i={}:p=?;", "a".repeat(MAX_ID_LEN));
    assert_eq!(feed(&mut s, &st(&longest)).len(), 1);
    // 답에 나가는 i는 언제나 명세 글자만이다
    let got = feed(&mut s, &st("i=A-z_0+9.:p=?;"));
    assert_eq!(got.len(), 1);
    let Signal::KittyNotifyReply(b) = &got[0] else { panic!() };
    let text = String::from_utf8(b.clone()).unwrap();
    assert!(text.starts_with("\x1b]99;i=A-z_0+9.:p=?;"));
    assert_eq!(text.matches('\x1b').count(), 2);
}

#[test]
fn oversized_query_payload_is_dropped_without_reply() {
    let mut s = Scanner::default();
    assert!(feed(&mut s, &st(&format!("i=1:p=?;{}", "a".repeat(2049)))).is_empty());
}
