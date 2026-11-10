//! kitty 클립보드 프로토콜(OSC 5522)의 해석·상한·허가·답 바이트(#325, ADR-030).
//! 시험 값은 Ghostty `clipboard_command.zig`·`clipboard_write.zig`·`clipboard_response.zig`·`clipboard_grants.zig`의 시험에서 옮겼다.
//! 사용자 클립보드는 건드리지 않는다(이 파일은 바이트만 다룬다)

use std::sync::Arc;

use bt_term::kitty_clip::*;
use parking_lot::Mutex;

fn meta(raw: &str) -> Meta {
    parse_meta(raw).expect("값 오류 없음").expect("패킷이 버려지지 않음")
}

// ---------- 메타데이터 ----------

#[test]
fn metadata_without_equals_or_type_is_dropped() {
    assert_eq!(parse_meta(""), Ok(None));
    assert_eq!(parse_meta("type=read:bare"), Ok(None));
    assert_eq!(parse_meta("bare:type=read"), Ok(None));
    assert_eq!(parse_meta("loc=primary"), Ok(None));
    assert_eq!(parse_meta("type=bobr"), Ok(None));
    assert_eq!(parse_meta("type="), Ok(None));
}

#[test]
fn metadata_duplicate_keys_keep_the_last_and_unknown_keys_are_ignored() {
    assert_eq!(meta("type=bobr:type=read").op, Op::Read);
    assert_eq!(parse_meta("type=read:type=bobr"), Ok(None));
    assert_eq!(meta("type=read:bobr=kurwa").op, Op::Read);
}

#[test]
fn metadata_loc_primary_only_when_exact() {
    assert!(meta("type=read:loc=primary").primary);
    assert!(!meta("type=read:loc=bobr").primary);
    assert!(!meta("type=read").primary);
}

#[test]
fn metadata_id_is_sanitized_and_truncated() {
    assert_eq!(meta("type=read:id=abc-123_x.Y+z").id, "abc-123_x.Y+z");
    assert_eq!(meta("type=read:id=*4 2*").id, "42");
    // 제어 문자는 id를 거쳐 답에 들어가지 못한다
    assert_eq!(meta("type=read:id=a\u{1b}]52;c;x\u{7}b").id, "a52cxb");
    let long = format!("type=read:id={}", "a".repeat(MAX_ID_LEN + 100));
    assert_eq!(meta(&long).id.len(), MAX_ID_LEN);
}

#[test]
fn metadata_mime_pw_name_are_strict_base64_utf8() {
    let m = meta("type=wdata:mime=dGV4dC9wbGFpbg==");
    assert_eq!(m.mime, "text/plain");
    let m = meta("type=read:pw=c2VjcmV0:name=YXBw");
    assert_eq!((m.pw.as_str(), m.name.as_str()), ("secret", "app"));
    assert_eq!(meta("type=read:pw=c2VjcmV0:name=").name, "");
    for bad in [
        "type=wdata:mime=!!!",
        "type=wdata:mime=dGV4dC9wbGFpbg",
        "type=wdata:mime=dGV4dC9wbGFpbg=!",
        "type=wdata:mime=dGV4dC9w\nbGFpbg==",
        // 0xff 0xfe는 UTF-8이 아니다
        "type=wdata:mime=//4=",
    ] {
        assert_eq!(parse_meta(bad), Err(InvalidValue), "{bad}");
        assert_eq!(meta_operation(bad), Some(Op::Wdata), "{bad}");
    }
}

#[test]
fn metadata_overlong_name_mime_are_reported_and_overlong_pw_is_ignored() {
    let long = |n: usize| b64_encode("n".repeat(n).as_bytes());
    assert_eq!(
        parse_meta(&format!("type=read:name={}", long(MAX_NAME_LEN + 1))),
        Err(InvalidValue)
    );
    assert!(parse_meta(&format!("type=read:name={}", long(MAX_NAME_LEN))).is_ok());
    assert_eq!(
        parse_meta(&format!("type=wdata:mime={}", long(MAX_MIME_LEN + 1))),
        Err(InvalidValue)
    );
    // 암호가 너무 길면 없는 것과 같다(어떤 허가와도 맞을 수 없다)
    let m = meta(&format!("type=read:pw={}", long(MAX_PW_LEN + 1)));
    assert_eq!(m.pw, "");
}

// ---------- base64 ----------

#[test]
fn base64_is_strict() {
    assert_eq!(b64_decode(b"R2hvc3R0eQ=="), Some(b"Ghostty".to_vec()));
    assert_eq!(b64_decode(b""), Some(Vec::new()));
    assert_eq!(b64_decode(b"!!!"), None);
    assert_eq!(b64_decode(b"dGV4dC9wbGFpbg"), None, "패딩이 빠짐");
    assert_eq!(b64_decode(b"dGV4dC9w\nbGFpbg=="), None, "공백은 건너뛰지 않는다");
    assert_eq!(b64_decode(b"YR=="), None, "남는 비트가 0이 아님");
    assert_eq!(b64_encode(b"text/plain"), "dGV4dC9wbGFpbg==");
    assert_eq!(b64_encode(b""), "");
    assert_eq!(b64_encode(b"a"), "YQ==");
    assert_eq!(b64_encode(b"ab"), "YWI=");
    assert_eq!(b64_encode(b"abc"), "YWJj");
}

// ---------- 답 바이트 ----------

#[test]
fn status_reply_matches_ghostty_bytes() {
    assert_eq!(
        status_reply(Op::Write, Status::Done, "", false),
        b"\x1b]5522;type=write:status=DONE\x1b\\"
    );
    assert_eq!(
        status_reply(Op::Write, Status::Eperm, "42", true),
        b"\x1b]5522;type=write:status=EPERM:id=42\x07"
    );
    assert_eq!(
        status_reply(Op::Read, Status::Enosys, "", false),
        b"\x1b]5522;type=read:status=ENOSYS\x1b\\"
    );
}

#[test]
fn read_success_empty_request_is_ok_then_done() {
    let r = ReadSuccess {
        id: "7",
        ..Default::default()
    };
    assert_eq!(
        r.encode(),
        b"\x1b]5522;type=read:status=OK:id=7\x1b\\\x1b]5522;type=read:status=DONE:id=7\x1b\\".to_vec()
    );
}

#[test]
fn read_success_targets_listing_and_data() {
    let avail = vec!["text/plain".to_string()];
    let r = ReadSuccess {
        list: true,
        available: &avail,
        ..Default::default()
    };
    assert_eq!(
        r.encode(),
        b"\x1b]5522;type=read:status=OK\x1b\\\
          \x1b]5522;type=read:status=DATA:mime=Lg==;dGV4dC9wbGFpbgo=\x1b\\\
          \x1b]5522;type=read:status=DONE\x1b\\"
            .to_vec()
    );
    // 빈 목록도 DATA 패킷은 나가고 payload 구간만 없다
    let r = ReadSuccess {
        list: true,
        ..Default::default()
    };
    assert_eq!(
        r.encode(),
        b"\x1b]5522;type=read:status=OK\x1b\\\
          \x1b]5522;type=read:status=DATA:mime=Lg==\x1b\\\
          \x1b]5522;type=read:status=DONE\x1b\\"
            .to_vec()
    );
    // 여러 종류는 공백으로 잇고 끝에 줄바꿈 하나("text/plain text/html\n")
    let avail = vec!["text/plain".to_string(), "text/html".to_string()];
    let r = ReadSuccess {
        list: true,
        available: &avail,
        ..Default::default()
    };
    let text = String::from_utf8(r.encode()).unwrap();
    assert!(
        text.contains(&format!("mime=Lg==;{}", b64_encode(b"text/plain text/html\n"))),
        "{text}"
    );
}

#[test]
fn read_success_chunks_data_at_4096_and_never_sends_empty_data() {
    let data = vec![b'x'; 5000];
    let contents = vec![
        ("text/plain".to_string(), data.clone()),
        ("text/html".to_string(), Vec::new()),
    ];
    let out = String::from_utf8(
        ReadSuccess {
            contents: &contents,
            ..Default::default()
        }
        .encode(),
    )
    .unwrap();
    let packets: Vec<&str> = out.split("\x1b\\").filter(|p| !p.is_empty()).collect();
    // OK, DATA(4096), DATA(904), DONE. 빈 종류는 패킷이 없다
    assert_eq!(packets.len(), 4, "{out}");
    let payload = |p: &str| b64_decode(p.rsplit_once(';').unwrap().1.as_bytes()).unwrap();
    assert_eq!(payload(packets[1]).len(), 4096);
    assert_eq!(payload(packets[2]).len(), 904);
    assert!(packets[1].contains(":mime=dGV4dC9wbGFpbg=="));
    assert!(!out.contains(&b64_encode(b"text/html")));
}

#[test]
fn read_success_listing_drops_types_that_do_not_fit_one_chunk() {
    let avail: Vec<String> = (0..10).map(|i| format!("x/{}", i.to_string().repeat(1000))).collect();
    let out = String::from_utf8(
        ReadSuccess {
            list: true,
            available: &avail,
            ..Default::default()
        }
        .encode(),
    )
    .unwrap();
    let listing = out.split("\x1b\\").nth(1).unwrap();
    let raw = b64_decode(listing.rsplit_once(';').unwrap().1.as_bytes()).unwrap();
    assert!(raw.len() <= READ_CHUNK);
    assert!(raw.ends_with(b"\n"));
}

#[test]
fn paste_event_bytes_carry_the_otp_in_every_packet() {
    let avail = vec!["text/plain".to_string()];
    assert_eq!(
        paste_event(false, "abc", &avail),
        b"\x1b]5522;type=read:status=OK:pw=YWJj\x1b\\\
          \x1b]5522;type=read:status=DATA:mime=Lg==:pw=YWJj;dGV4dC9wbGFpbgo=\x1b\\\
          \x1b]5522;type=read:status=DONE:pw=YWJj\x1b\\"
            .to_vec()
    );
    let primary = String::from_utf8(paste_event(true, "abc", &avail)).unwrap();
    assert!(
        primary.starts_with("\x1b]5522;type=read:status=OK:loc=primary:pw=YWJj"),
        "{primary}"
    );
}

#[test]
fn paste_event_lists_at_most_sixteen_types() {
    let avail: Vec<String> = (0..40).map(|i| format!("a/{i}")).collect();
    let out = String::from_utf8(paste_event(false, "abc", &avail)).unwrap();
    let listing = out.split("\x1b\\").nth(1).unwrap();
    let raw = String::from_utf8(b64_decode(listing.rsplit_once(';').unwrap().1.as_bytes()).unwrap()).unwrap();
    assert_eq!(raw.trim_end().split(' ').count(), MAX_LISTING_MIMES, "{raw}");
}

// ---------- 허가 ----------

#[test]
fn grants_one_time_is_consumed_only_in_the_used_direction() {
    let mut g = Grants::default();
    g.grant("pw1", Dir::Read, true);
    // 방향이 맞지 않는 확인은 허가를 닳게 하지 않는다(쓰인 방향의 허가만 사라진다)
    assert!(!g.consume("pw1", Dir::Write));
    assert!(g.consume("pw1", Dir::Read));
    assert!(!g.consume("pw1", Dir::Read));
    g.grant("pw2", Dir::Read, true);
    assert!(g.consume("pw2", Dir::Read));
    assert!(!g.consume("pw2", Dir::Read), "일회용은 한 번만");
    assert!(!g.consume("", Dir::Read));
    assert!(!g.consume("nope", Dir::Read));
}

#[test]
fn grants_ignore_empty_and_overlong_passwords_and_evict_the_oldest() {
    let mut g = Grants::default();
    g.grant("", Dir::Read, false);
    g.grant(&"p".repeat(MAX_PW_LEN + 1), Dir::Read, false);
    assert!(g.is_empty());
    for i in 0..MAX_GRANTS + 1 {
        g.grant(&format!("pw{i}"), Dir::Read, false);
    }
    assert_eq!(g.len(), MAX_GRANTS);
    assert!(!g.consume("pw0", Dir::Read), "가장 오래된 것은 밀려났다");
    assert!(g.consume(&format!("pw{MAX_GRANTS}"), Dir::Read));
    g.clear();
    assert!(g.is_empty());
}

#[test]
fn otp_is_22_chars_from_the_unambiguous_alphabet_and_differs_each_time() {
    let a = generate_otp().expect("OS 난수");
    let b = generate_otp().expect("OS 난수");
    assert_eq!(a.len(), OTP_LEN);
    assert_ne!(a, b);
    let alphabet = "23456789abcdefghijkmnopqrstuvwxyzABCDEFGHJKLMNPQRSTUVWXYZ";
    assert!(a.chars().all(|c| alphabet.contains(c)), "{a}");
}

// ---------- 쓰기 거래 ----------

fn wmeta(mime: &str) -> Meta {
    Meta {
        op: Op::Wdata,
        primary: false,
        id: String::new(),
        mime: mime.into(),
        pw: String::new(),
        name: String::new(),
    }
}

fn begin(id: &str, max: usize) -> WriteState {
    let mut m = wmeta("");
    m.op = Op::Write;
    m.id = id.into();
    WriteState::new(&m, max)
}

#[test]
fn write_basic_chunked_and_multi_mime() {
    let mut s = begin("42", 1 << 20);
    s.data(&wmeta("text/plain"), b"R2hvc3R0eQ==").unwrap();
    let c = s.commit().unwrap();
    assert_eq!(c.id, "42");
    assert_eq!(c.contents, vec![("text/plain".to_string(), b"Ghostty".to_vec())]);

    let mut s = begin("", 1 << 20);
    s.data(&wmeta("text/plain"), b"SGVsbG8=").unwrap();
    s.data(&wmeta("text/plain"), b"V29ybGQ=").unwrap();
    assert_eq!(s.commit().unwrap().contents[0].1, b"HelloWorld");

    let mut s = begin("", 1 << 20);
    s.data(&wmeta("text/plain"), b"YQ==").unwrap();
    s.data(&wmeta("text/html"), b"Yg==").unwrap();
    s.data(&wmeta("text/plain"), b"Yw==").unwrap();
    let c = s.commit().unwrap();
    assert_eq!(
        c.contents,
        vec![
            ("text/plain".to_string(), b"c".to_vec()),
            ("text/html".to_string(), b"b".to_vec())
        ],
        "다시 쓴 종류는 자리는 두고 내용만 바뀐다"
    );
}

#[test]
fn write_invalid_base64_aborts() {
    for bad in [
        &b"!!!"[..],
        b"SGVs!!!bG8=",
        b"\nZGF0YSB3aXRoIGEgbmV3bGluZQ==",
        b"Z29vZA==bW9yZQ==",
    ] {
        let mut s = begin("", 1 << 20);
        assert_eq!(s.data(&wmeta("text/plain"), bad), Err(WriteError::Invalid), "{bad:?}");
    }
    let mut s = begin("", 1 << 20);
    s.data(&wmeta("text/plain"), b"Z29vZA==").unwrap();
    assert_eq!(s.data(&wmeta("text/plain"), b"SGVs!!!bG8="), Err(WriteError::Invalid));
}

#[test]
fn write_chunks_split_one_base64_stream_anywhere() {
    let encoded = b"c29tZSBkYXRh";
    for splits in [vec![3, 7], (1..12).collect::<Vec<usize>>()] {
        let mut s = begin("", 1 << 20);
        let mut prev = 0;
        for end in splits {
            s.data(&wmeta("text/plain"), &encoded[prev..end]).unwrap();
            prev = end;
        }
        s.data(&wmeta("text/plain"), &encoded[prev..]).unwrap();
        assert_eq!(s.commit().unwrap().contents[0].1, b"some data");
    }
}

#[test]
fn write_padding_rules() {
    // 패딩이 모자란 줄기는 commit(또는 종류 바꿈)에서 걸린다
    let mut s = begin("", 1 << 20);
    s.data(&wmeta("text/plain"), b"SGVsbG8").unwrap();
    assert_eq!(s.commit(), Err(WriteError::Invalid));
    let mut s = begin("", 1 << 20);
    s.data(&wmeta("text/plain"), b"SGVsbG8").unwrap();
    assert_eq!(s.data(&wmeta("text/html"), b"PGI+aGk8L2I+"), Err(WriteError::Invalid));
    // 패딩으로 끝나는 패킷은 줄기를 새로 시작한다
    let mut s = begin("", 1 << 20);
    s.data(&wmeta("text/plain"), b"Z29vZA==").unwrap();
    s.data(&wmeta("text/plain"), b"bW9yZQ==").unwrap();
    assert_eq!(s.commit().unwrap().contents[0].1, b"goodmore");
}

#[test]
fn write_aliases_resolve_at_commit() {
    let mut s = begin("", 1 << 20);
    s.data(&wmeta("text/plain"), b"R2hvc3R0eQ==").unwrap();
    s.alias(&wmeta("text/plain"), b"VEVYVCBVVEY4X1NUUklORw==").unwrap();
    let c = s.commit().unwrap();
    assert_eq!(c.contents.len(), 3);
    assert_eq!(
        (c.contents[1].0.as_str(), c.contents[1].1.as_slice()),
        ("TEXT", &b"Ghostty"[..])
    );
    assert_eq!(c.contents[2].0, "UTF8_STRING");
    // 알맹이 없는 별칭은 버려진다
    let mut s = begin("", 1 << 20);
    s.alias(&wmeta("text/plain"), b"VEVYVA==").unwrap();
    assert!(s.commit().unwrap().contents.is_empty());
    // 별칭 목록은 UTF-8이어야 한다
    let mut s = begin("", 1 << 20);
    assert_eq!(s.alias(&wmeta("text/plain"), b"/w=="), Err(Invalid));
}

#[test]
fn write_size_limit_covers_all_data_and_is_checked_before_keeping_it() {
    let mut s = begin("", 8);
    assert_eq!(
        s.data(&wmeta("text/plain"), b"SGVsbG9Xb3JsZA=="),
        Err(WriteError::TooLarge)
    );
    let mut s = begin("", 8);
    s.data(&wmeta("text/plain"), b"SGVsbG8=").unwrap();
    assert_eq!(s.data(&wmeta("text/plain"), b"V29ybGQ="), Err(WriteError::TooLarge));
    let mut s = begin("", 5);
    s.data(&wmeta("text/plain"), b"SGVsbG8=").unwrap();
    assert_eq!(s.commit().unwrap().contents[0].1, b"Hello");
    let mut s = begin("", 8);
    s.data(&wmeta("text/plain"), b"SGVsbG8=").unwrap();
    assert_eq!(
        s.data(&wmeta("image/png"), b"V29ybGQ="),
        Err(WriteError::TooLarge),
        "종류가 달라도 합쳐서 센다"
    );
}

#[test]
fn write_mime_count_is_capped() {
    let mut s = begin("", 1 << 20);
    for i in 0..MAX_WRITE_MIMES + 10 {
        s.data(&wmeta(&format!("x/{i}")), b"YQ==").unwrap();
    }
    assert_eq!(s.commit().unwrap().contents.len(), MAX_WRITE_MIMES);
}

#[test]
fn write_alias_count_is_capped() {
    let mut s = begin("", 1 << 20);
    s.data(&wmeta("text/plain"), b"YQ==").unwrap();
    let names: Vec<String> = (0..MAX_WRITE_ALIASES + 10).map(|i| format!("a/{i}")).collect();
    s.alias(&wmeta("text/plain"), b64_encode(names.join(" ").as_bytes()).as_bytes())
        .unwrap();
    assert_eq!(s.commit().unwrap().contents.len(), 1 + MAX_WRITE_ALIASES);
}

// ---------- 엔진(패킷 → 요청·답) ----------

fn engine(write_enabled: bool, max: usize) -> (Engine, Arc<Mutex<Grants>>) {
    let g = Arc::new(Mutex::new(Grants::default()));
    (Engine::new(g.clone(), write_enabled, max), g)
}

fn packet(md: &str, payload: &[u8]) -> String {
    format!("{md};{}", b64_encode(payload))
}

#[test]
fn engine_read_targets_only_is_a_listing_request() {
    let (mut e, _) = engine(true, 1 << 20);
    let out = e.handle(&packet("type=read:id=7", b"."), false);
    assert_eq!(
        out,
        vec![Action::Read(ReadRequest {
            primary: false,
            id: "7".into(),
            mimes: vec![],
            list: true,
            pw: String::new(),
            name: String::new(),
            granted: false,
            bel: false,
        })]
    );
}

#[test]
fn engine_read_mime_list_is_capped_and_whitespace_split() {
    let (mut e, _) = engine(true, 1 << 20);
    let out = e.handle(&packet("type=read", b"text/plain  text/html\n. a/1 a/2 a/3 a/4"), true);
    let Action::Read(r) = &out[0] else { panic!("{out:?}") };
    assert_eq!(
        r.mimes,
        vec!["text/plain", "text/html", "a/1", "a/2"],
        "데이터 종류는 {MAX_READ_MIMES}개까지"
    );
    assert!(r.list && r.bel);
}

#[test]
fn engine_read_with_bad_payload_or_metadata_is_dropped_without_reply() {
    let (mut e, _) = engine(true, 1 << 20);
    assert!(e.handle("type=read;!!!", false).is_empty());
    assert!(e.handle(&packet("type=read", &[0xff, 0xfe]), false).is_empty());
    assert!(e.handle("type=read:bare;", false).is_empty());
    assert!(e.handle("type=read:mime=!!!;", false).is_empty());
    assert!(e.handle("", false).is_empty());
}

#[test]
fn engine_read_password_needs_a_name_and_a_grant_and_is_one_time() {
    let (mut e, g) = engine(true, 1 << 20);
    g.lock().grant("secret", Dir::Read, true);
    // 이름이 없으면 암호도 없는 것
    let out = e.handle(&packet("type=read:pw=c2VjcmV0", b"text/plain"), false);
    let Action::Read(r) = &out[0] else { panic!() };
    assert!(!r.granted);
    assert_eq!(r.pw, "");
    // 이름이 있으면 허가를 쓴다. 한 번뿐
    let out = e.handle(&packet("type=read:pw=c2VjcmV0:name=YXBw", b"text/plain"), false);
    let Action::Read(r) = &out[0] else { panic!() };
    assert!(r.granted);
    let out = e.handle(&packet("type=read:pw=c2VjcmV0:name=YXBw", b"text/plain"), false);
    let Action::Read(r) = &out[0] else { panic!() };
    assert!(!r.granted, "일회용 암호는 다시 쓸 수 없다");
}

#[test]
fn engine_listing_only_read_never_burns_the_one_time_password() {
    let (mut e, g) = engine(true, 1 << 20);
    g.lock().grant("secret", Dir::Read, true);
    let out = e.handle(&packet("type=read:pw=c2VjcmV0:name=YXBw", b"."), false);
    let Action::Read(r) = &out[0] else { panic!() };
    assert!(!r.granted);
    assert_eq!(g.lock().len(), 1, "목록만 묻는 요청은 암호를 쓰지 않는다");
    let out = e.handle(&packet("type=read:pw=c2VjcmV0:name=YXBw", b"text/plain"), false);
    let Action::Read(r) = &out[0] else { panic!() };
    assert!(r.granted);
}

fn write_flow(e: &mut Engine, md_begin: &str, mime: &str, data: &[u8]) -> Vec<Action> {
    assert!(e.handle(md_begin, false).is_empty());
    let mime_b64 = b64_encode(mime.as_bytes());
    assert!(
        e.handle(&packet(&format!("type=wdata:mime={mime_b64}"), data), false)
            .is_empty()
    );
    e.handle("type=wdata", false)
}

#[test]
fn engine_write_commit_yields_one_request() {
    let (mut e, _) = engine(true, 1 << 20);
    let out = write_flow(&mut e, "type=write:id=42", "text/plain", b"Ghostty");
    assert_eq!(
        out,
        vec![Action::Write(WriteRequest {
            primary: false,
            id: "42".into(),
            pw: String::new(),
            name: String::new(),
            contents: vec![("text/plain".into(), b"Ghostty".to_vec())],
            granted: false,
            bel: false,
        })]
    );
    // 거래는 끝났다: 또 commit해도 조용히 무시
    assert!(e.handle("type=wdata", false).is_empty());
}

#[test]
fn engine_write_denied_at_begin_answers_eperm_and_ignores_the_rest() {
    let (mut e, _) = engine(false, 1 << 20);
    let out = e.handle("type=write:id=9", true);
    assert_eq!(
        out,
        vec![Action::Reply(status_reply(Op::Write, Status::Eperm, "9", true))]
    );
    assert!(
        e.handle(&packet("type=wdata:mime=dGV4dC9wbGFpbg==", b"x"), false)
            .is_empty()
    );
    assert!(e.handle("type=wdata", false).is_empty());
}

#[test]
fn engine_wdata_without_transaction_is_ignored_and_new_write_replaces_old() {
    let (mut e, _) = engine(true, 1 << 20);
    assert!(
        e.handle(&packet("type=wdata:mime=dGV4dC9wbGFpbg==", b"x"), false)
            .is_empty()
    );
    assert!(e.handle("type=wdata", false).is_empty());
    assert!(e.handle("type=walias:mime=dGV4dC9wbGFpbg==;VEVYVA==", false).is_empty());
    assert!(e.handle("type=write:id=1", false).is_empty());
    assert!(
        e.handle(&packet("type=wdata:mime=dGV4dC9wbGFpbg==", b"old"), false)
            .is_empty()
    );
    let out = write_flow(&mut e, "type=write:id=2", "text/plain", b"new");
    let Action::Write(w) = &out[0] else { panic!("{out:?}") };
    assert_eq!((w.id.as_str(), w.contents[0].1.as_slice()), ("2", &b"new"[..]));
}

#[test]
fn engine_write_errors_abort_with_the_opening_id() {
    let (mut e, _) = engine(true, 8);
    // 상한 넘음: EFBIG
    e.handle("type=write:id=5", false);
    let out = e.handle(&packet("type=wdata:id=zzz:mime=dGV4dC9wbGFpbg==", b"HelloWorld"), true);
    assert_eq!(
        out,
        vec![Action::Reply(status_reply(Op::Write, Status::Efbig, "5", true))]
    );
    assert!(e.handle("type=wdata", false).is_empty(), "거래는 사라졌다");
    // 잘못된 base64: EINVAL
    e.handle("type=write:id=6", false);
    let out = e.handle("type=wdata:mime=dGV4dC9wbGFpbg==;!!!", false);
    assert_eq!(
        out,
        vec![Action::Reply(status_reply(Op::Write, Status::Einval, "6", false))]
    );
    // 값이 잘못된 wdata·walias도 거래를 끊는다. read는 거래를 건드리지 않는다
    e.handle("type=write:id=7", false);
    assert!(e.handle("type=read:mime=!!!;", false).is_empty());
    let out = e.handle("type=wdata:mime=!!!;", false);
    assert_eq!(
        out,
        vec![Action::Reply(status_reply(Op::Write, Status::Einval, "7", false))]
    );
    e.handle("type=write:id=8", false);
    let out = e.handle("type=walias;VEVYVA==", false);
    assert_eq!(
        out,
        vec![Action::Reply(status_reply(Op::Write, Status::Einval, "8", false))],
        "대상 종류 없는 walias"
    );
    // 패딩이 모자란 줄기는 commit에서 EINVAL
    e.handle("type=write:id=9", false);
    e.handle("type=wdata:mime=dGV4dC9wbGFpbg==;SGVsbG8", false);
    let out = e.handle("type=wdata", false);
    assert_eq!(
        out,
        vec![Action::Reply(status_reply(Op::Write, Status::Einval, "9", false))]
    );
}

#[test]
fn engine_write_password_grant_marks_the_request() {
    let (mut e, g) = engine(true, 1 << 20);
    g.lock().grant("secret", Dir::Write, false);
    let out = write_flow(&mut e, "type=write:pw=c2VjcmV0:name=YXBw", "text/plain", b"x");
    let Action::Write(w) = &out[0] else { panic!() };
    assert!(w.granted);
    // 이름 없는 암호는 암호가 아니다
    let out = write_flow(&mut e, "type=write:pw=c2VjcmV0", "text/plain", b"x");
    let Action::Write(w) = &out[0] else { panic!() };
    assert!(!w.granted);
    assert_eq!(w.pw, "");
}

#[test]
fn text_mime_names() {
    assert!(is_text_mime("text/plain"));
    assert!(is_text_mime("text/plain;charset=utf-8"));
    assert!(!is_text_mime("text/html"));
    assert!(!is_text_mime("image/png"));
    assert!(!is_text_mime("."));
}

// ---------- 보안 검토 세 건(#325) ----------

#[test]
fn one_time_is_kept_per_direction() {
    // 읽기 일회용 허가에 같은 암호의 쓰기 허가가 와도 읽기 허가의 수명은 그대로다
    let mut g = Grants::default();
    g.grant("pw", Dir::Read, true);
    g.grant("pw", Dir::Write, false);
    assert!(g.consume("pw", Dir::Read));
    assert!(!g.consume("pw", Dir::Read), "읽기 허가는 여전히 일회용이다");
    assert!(g.consume("pw", Dir::Write), "쓰기 허가는 남아 있다");
    assert!(g.consume("pw", Dir::Write));
}

#[test]
fn aliases_count_against_the_write_limit() {
    // 별칭이 대상 데이터를 복사하므로 별칭까지 합한 크기가 상한을 넘으면 거래를 끝낸다(메모리 증폭 방지)
    let mut s = begin("", 10);
    s.data(&wmeta("text/plain"), b"SGVsbG8=").unwrap(); // 5바이트
    s.alias(&wmeta("text/plain"), b64_encode(b"a/1 a/2 a/3").as_bytes())
        .unwrap();
    assert_eq!(
        s.commit(),
        Err(WriteError::TooLarge),
        "5 + 5 x 3 = 20바이트가 상한 10을 넘는다"
    );
    // 상한 안이면 된다
    let mut s = begin("", 10);
    s.data(&wmeta("text/plain"), b"SGVsbG8=").unwrap();
    s.alias(&wmeta("text/plain"), b64_encode(b"a/1").as_bytes()).unwrap();
    assert_eq!(s.commit().unwrap().contents.len(), 2);
}

#[test]
fn grant_is_scoped_to_the_location_it_was_made_for() {
    // 붙여넣기 이벤트의 암호는 표준 클립보드용이다. loc=primary 읽기에는 쓸 수 없다
    let (mut e, g) = engine(true, 1 << 20);
    g.lock().grant("secret", Dir::Read, true);
    let out = e.handle(
        &packet("type=read:loc=primary:pw=c2VjcmV0:name=YXBw", b"text/plain"),
        false,
    );
    let Action::Read(r) = &out[0] else { panic!() };
    assert!(!r.granted, "다른 위치에는 허가가 없다");
}
