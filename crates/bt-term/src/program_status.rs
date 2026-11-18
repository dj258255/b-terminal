//! 프로그램 상태 프로토콜(OSC 7501, 명세 0.4): 프로그램이 "일하는 중·사용자를 기다림·끝남"을 터미널에 알린다(#618).
//! <https://www.superlogical.com/rex/docs/build/program-status>
//!
//! 모양은 `OSC 7501 ; state=working:id=a/b:progress=40 ST`(보고)와 `OSC 7501 ; ? ST`(지원 질의)다.
//! 이 모듈은 보고 하나를 읽고 검증하는 것(`parse`)과 칸이 쥐는 기록(`Records`)을 맡는다.
//! 검증은 Ghostty libghostty-vt의 `program_status.zig`와 같게 한다: 짝 하나가 틀리면 그 짝만 건너뛰고,
//! 상태가 없거나 한도를 넘거나 id·글이 틀리면 보고 전체를 버린다. 보낸 글은 모두 믿지 않는다

/// 보고 한 줄 전체(`ESC ]`부터 끝맺음까지)의 상한(바이트)
pub const MAX_SEQUENCE: usize = 4096;
/// `7501;` 뒤 본문의 상한. 끝맺음이 긴 `ESC \\`일 때를 기준으로 한다(BEL은 한 바이트 더 짧게 제한된다)
pub const MAX_BODY: usize = MAX_SEQUENCE - "\x1b]7501;".len() - "\x1b\\".len();
const MAX_KEY: usize = 16;
const MAX_APP: usize = 32;
const MAX_ID: usize = 128;
const MAX_ID_SEGMENT: usize = 32;
const MAX_ID_DEPTH: usize = 8;
const MAX_TITLE_ENCODED: usize = 256;
const MAX_TITLE: usize = 192;
const MAX_MSG_ENCODED: usize = 2732;
const MAX_MSG: usize = 2048;
/// 칸 하나가 쥐는 기록 수. 명세는 64 이상을 지원하라고 한다. 가득 차면 가장 오래 갱신되지 않은 것을 버린다
pub const MAX_RECORDS: usize = 64;

/// `state` 값
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum State {
    Idle,
    Working,
    Done,
    Blocked,
    Error,
    /// 상태가 아니라 지우라는 명령: id의 기록과 그 아래를 지운다. id가 없으면 모두 지운다
    Clear,
}

/// `kind` 값(`blocked`일 때만): 사용자에게 무엇이 필요한가
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    Permission,
    Question,
    Auth,
}

/// 검증을 통과한 보고 하나. 글(`title`·`msg`)은 이미 풀었고 제어 문자가 없다
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Report {
    pub state: State,
    /// 없으면 뿌리 기록(프로그램 자신)
    pub id: Option<String>,
    pub kind: Option<Kind>,
    /// 0~100. 없으면 끝을 모른다
    pub progress: Option<u8>,
    pub app: Option<String>,
    pub title: Option<String>,
    pub msg: Option<String>,
}

/// 읽은 OSC 7501 하나
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Command {
    Query,
    Report(Report),
}

/// 지원 질의에 돌려줄 바이트(질의와 같은 끝맺음). 명세는 `?`만 요구하고, 상태·종류 목록을 넣지 않으면 기본값(명세의 전부)이다.
/// 기록의 내용은 답에 넣지 않는다
pub fn query_reply(bel: bool) -> Vec<u8> {
    if bel {
        b"\x1b]7501;?\x07".to_vec()
    } else {
        b"\x1b]7501;?\x1b\\".to_vec()
    }
}

const NAME_BYTES: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789_.+-";

fn is_name(s: &[u8]) -> bool {
    !s.is_empty() && s.iter().all(|b| NAME_BYTES.contains(b))
}

fn is_value(s: &[u8]) -> bool {
    s.iter()
        .all(|b| NAME_BYTES.contains(b) || matches!(b, b',' | b'/' | b'='))
}

fn trim(mut s: &[u8]) -> &[u8] {
    while let [first, rest @ ..] = s {
        if !first.is_ascii_whitespace() {
            break;
        }
        s = rest;
    }
    while let [rest @ .., last] = s {
        if !last.is_ascii_whitespace() {
            break;
        }
        s = rest;
    }
    s
}

/// `키=값` 짝들(공백을 자른다). `=`가 없는 짝은 건너뛴다
fn pairs(body: &[u8]) -> impl Iterator<Item = (&[u8], &[u8])> {
    body.split(|&b| b == b':').filter_map(|p| {
        let eq = p.iter().position(|&b| b == b'=')?;
        Some((trim(&p[..eq]), trim(&p[eq + 1..])))
    })
}

/// 그 키의 마지막 값. 값 글자 범위를 벗어난 값은 건너뛴다
fn last_value<'a>(body: &'a [u8], key: &str) -> Option<&'a [u8]> {
    pairs(body)
        .filter(|(k, v)| *k == key.as_bytes() && is_value(v))
        .map(|(_, v)| v)
        .last()
}

fn raw_values<'a>(body: &'a [u8], key: &'a str) -> impl Iterator<Item = &'a [u8]> {
    pairs(body).filter(move |(k, _)| *k == key.as_bytes()).map(|(_, v)| v)
}

/// id: 이름을 `/` 하나로 이은 것. 틀리면 `false`
fn valid_id(v: &[u8]) -> bool {
    v.len() <= MAX_ID && v.split(|&b| b == b'/').count() <= MAX_ID_DEPTH && {
        v.split(|&b| b == b'/').all(|s| s.len() <= MAX_ID_SEGMENT && is_name(s))
    }
}

/// 0~100의 정수만. 나머지는 없는 것으로 본다(`+1`·`4.5`·`1_0`·음수 포함)
fn parse_progress(v: &[u8]) -> Option<u8> {
    if v.is_empty() {
        return None;
    }
    let mut n: u16 = 0;
    for &c in v {
        if !c.is_ascii_digit() {
            return None;
        }
        n = n * 10 + u16::from(c - b'0');
        if n > 100 {
            return None;
        }
    }
    Some(n as u8)
}

/// base64 글(제목·본문)을 푼다. 패딩은 있어도 없어도 된다. 풀기 전에 인코딩된 길이를 이미 보았어야 한다.
/// 글자가 틀리거나 UTF-8이 아니거나 제어 문자(U+0000~001F, 007F~009F, 줄바꿈·탭 포함)가 있으면 `None`
fn decode_text(v: &[u8], max: usize) -> Option<String> {
    let body = {
        let n = v.iter().rposition(|&b| b != b'=').map_or(0, |i| i + 1);
        let pad = v.len() - n;
        // 패딩이 있으면 4의 배수여야 하고, 가운데 `=`는 틀리다
        if pad > 2 || v[..n].contains(&b'=') || (pad > 0 && !v.len().is_multiple_of(4)) || n % 4 == 1 {
            return None;
        }
        &v[..n]
    };
    let bytes = crate::graphics::base64_decode(body)?;
    if bytes.len() > max {
        return None;
    }
    let s = String::from_utf8(bytes).ok()?;
    (!s.chars().any(char::is_control)).then_some(s)
}

/// `7501;` 뒤의 본문 하나를 읽는다. 틀린 보고는 `None`(아무것도 적용하지 않는다)
pub fn parse(body: &[u8]) -> Option<Command> {
    if body.len() > MAX_BODY {
        return None;
    }
    if body == b"?" {
        return Some(Command::Query);
    }
    // 아는 키든 모르는 키든 키가 너무 길면 버린다
    if pairs(body).any(|(k, _)| k.len() > MAX_KEY) {
        return None;
    }
    // 같은 키가 뒤에서 덮여도 모든 값이 한도를 지켜야 한다. 값 글자 범위는 걸러 내지 않는다:
    // 걸러 내면 틀린 id가 "id 없음"으로 읽혀 뿌리 기록으로 떨어진다(`clear`면 전부 지운다)
    if raw_values(body, "id").any(|v| !valid_id(v)) {
        return None;
    }
    for (key, max) in [("app", MAX_APP), ("title", MAX_TITLE_ENCODED), ("msg", MAX_MSG_ENCODED)] {
        if raw_values(body, key).any(|v| v.len() > max) {
            return None;
        }
    }
    let title = match last_value(body, "title") {
        Some(v) => Some(decode_text(v, MAX_TITLE)?),
        None => None,
    };
    let msg = match last_value(body, "msg") {
        Some(v) => Some(decode_text(v, MAX_MSG)?),
        None => None,
    };
    // 모르는 상태는 짐작하지 않고 버린다
    let state = match last_value(body, "state")? {
        b"idle" => State::Idle,
        b"working" => State::Working,
        b"done" => State::Done,
        b"blocked" => State::Blocked,
        b"error" => State::Error,
        b"clear" => State::Clear,
        _ => return None,
    };
    let id = last_value(body, "id").map(|v| String::from_utf8_lossy(v).into_owned());
    let kind = match (state, last_value(body, "kind")) {
        (State::Blocked, Some(b"permission")) => Some(Kind::Permission),
        (State::Blocked, Some(b"question")) => Some(Kind::Question),
        (State::Blocked, Some(b"auth")) => Some(Kind::Auth),
        _ => None,
    };
    let progress = match state {
        State::Working | State::Blocked => last_value(body, "progress").and_then(parse_progress),
        _ => None,
    };
    let app = last_value(body, "app")
        .filter(|v| is_name(v))
        .map(|v| String::from_utf8_lossy(v).into_owned());
    Some(Command::Report(Report {
        state,
        id,
        kind,
        progress,
        app,
        title: title.filter(|s| !s.is_empty()),
        msg: msg.filter(|s| !s.is_empty()),
    }))
}

/// 터미널 밖(데스크톱 알림 등)에 보일 글을 다듬는다: 제어 문자는 공백으로, 방향을 바꾸는 글자와 보이지 않는 서식 글자는 지우고
/// 앞뒤 공백을 자른 뒤 글자 수를 자른다. 글은 마크업이 아니라 글자 그대로 쓴다
pub fn safe_text(s: &str, max: usize) -> String {
    let kept: String = s.chars().filter(|&c| !is_invisible_format(c)).collect();
    crate::notify::clean(&kept, max)
}

/// 양방향 덮어쓰기·표지, 폭 없는 글자, 단어 붙임·보이지 않는 연산자 같은 서식 글자, 줄·문단 구분자, 한글 채움 글자
fn is_invisible_format(c: char) -> bool {
    matches!(c,
        '\u{00AD}' | '\u{034F}' | '\u{061C}' | '\u{180E}'
        // 한글 채움 글자: 빈 칸처럼 보인다
        | '\u{115F}' | '\u{1160}' | '\u{3164}' | '\u{FFA0}'
        | '\u{200B}'..='\u{200F}'
        // 줄·문단 구분자: 알림 제목이 줄바꿈된 것처럼 보인다
        | '\u{2028}' | '\u{2029}'
        | '\u{202A}'..='\u{202E}'
        | '\u{2060}'..='\u{206F}'
        | '\u{FEFF}'
        | '\u{FFF9}'..='\u{FFFB}'
        | '\u{1D173}'..='\u{1D17A}'
        | '\u{E0000}'..='\u{E007F}')
}

/// 칸이 쥐는 기록 하나
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Record {
    /// 뿌리 기록은 빈 글
    pub id: String,
    pub state: State,
    pub kind: Option<Kind>,
    pub progress: Option<u8>,
    pub app: Option<String>,
    pub title: Option<String>,
    pub msg: Option<String>,
    /// 마지막으로 갱신된 차례(가득 찼을 때 가장 오래된 것을 버리는 기준)
    seq: u64,
}

/// 기록을 새로 만들거나 `blocked`·`done`·`error`로 바뀌었을 때 앱에 알리는 값(데스크톱 알림의 재료)
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Alert {
    pub state: State,
    pub kind: Option<Kind>,
    /// 조상에게서 물려받은 것을 포함한다
    pub app: Option<String>,
    pub title: Option<String>,
    pub msg: Option<String>,
}

/// 탭에 보일 가장 급한 상태
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Summary {
    pub state: State,
    pub progress: Option<u8>,
}

/// 칸 하나의 기록들. 보고는 그 id의 기록을 통째로 바꾼다
#[derive(Debug, Default)]
pub struct Records {
    list: Vec<Record>,
    seq: u64,
}

fn rank(s: State) -> u8 {
    match s {
        State::Blocked => 4,
        State::Error => 3,
        State::Working => 2,
        State::Done => 1,
        State::Idle | State::Clear => 0,
    }
}

impl Records {
    pub fn len(&self) -> usize {
        self.list.len()
    }

    pub fn is_empty(&self) -> bool {
        self.list.is_empty()
    }

    pub fn records(&self) -> &[Record] {
        &self.list
    }

    /// 보고를 적용한다. 데스크톱 알림을 낼 만한 바뀜(이 id의 기록이 새로 `blocked`·`done`·`error`가 됨)이면 그 값을 돌려준다
    pub fn apply(&mut self, report: Report) -> Option<Alert> {
        let id = report.id.clone().unwrap_or_default();
        if report.state == State::Clear {
            if id.is_empty() {
                self.list.clear();
            } else {
                let prefix = format!("{id}/");
                self.list.retain(|r| r.id != id && !r.id.starts_with(&prefix));
            }
            return None;
        }
        self.seq += 1;
        let previous = self.list.iter().position(|r| r.id == id).map(|i| self.list.remove(i));
        if previous.is_none() && self.list.len() >= MAX_RECORDS {
            if let Some(oldest) = self.list.iter().enumerate().min_by_key(|(_, r)| r.seq).map(|(i, _)| i) {
                self.list.remove(oldest);
            }
        }
        let changed = previous.as_ref().is_none_or(|p| p.state != report.state);
        self.list.push(Record {
            id: id.clone(),
            state: report.state,
            kind: report.kind,
            progress: report.progress,
            app: report.app,
            title: report.title,
            msg: report.msg,
            seq: self.seq,
        });
        let alertable = matches!(report.state, State::Blocked | State::Done | State::Error);
        (alertable && changed).then(|| {
            let r = self.list.last().expect("방금 넣었다");
            Alert {
                state: r.state,
                kind: r.kind,
                app: self.app_of(&id),
                title: r.title.clone(),
                msg: r.msg.clone(),
            }
        })
    }

    /// 그 기록의 `app`. 없으면 가장 가까운 조상의 것(뿌리까지)
    pub fn app_of(&self, id: &str) -> Option<String> {
        let mut cur = id;
        loop {
            if let Some(app) = self.list.iter().find(|r| r.id == cur).and_then(|r| r.app.clone()) {
                return Some(app);
            }
            if cur.is_empty() {
                return None;
            }
            cur = cur.rfind('/').map_or("", |i| &cur[..i]);
        }
    }

    /// 새 셸 프롬프트(OSC 133 A)나 프로세스 끝: `working`·`blocked`는 사라지고, `idle`도 버린다. `done`·`error`는 남는다
    pub fn drop_live(&mut self) -> bool {
        let before = self.list.len();
        self.list.retain(|r| matches!(r.state, State::Done | State::Error));
        self.list.len() != before
    }

    /// 사용자가 본 `done`·`error` 기록을 지운다. 지운 것이 있으면 참
    pub fn clear_seen(&mut self) -> bool {
        let before = self.list.len();
        self.list.retain(|r| !matches!(r.state, State::Done | State::Error));
        self.list.len() != before
    }

    /// 초기화(RIS): 모두 지운다
    pub fn clear_all(&mut self) {
        self.list.clear();
    }

    /// 탭에 보일 가장 급한 상태(`blocked` > `error` > `working` > `done`, `idle`은 없다). 같으면 나중에 갱신된 것
    pub fn summary(&self) -> Option<Summary> {
        self.list
            .iter()
            .filter(|r| rank(r.state) > 0)
            .max_by_key(|r| (rank(r.state), r.seq))
            .map(|r| Summary {
                state: r.state,
                progress: r.progress,
            })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn report(body: &str) -> Report {
        match parse(body.as_bytes()) {
            Some(Command::Report(r)) => r,
            other => panic!("보고여야 한다: {body:?} -> {other:?}"),
        }
    }

    fn dropped(body: &str) -> bool {
        parse(body.as_bytes()).is_none()
    }

    #[test]
    fn query_is_recognized_and_replied_with_the_same_terminator() {
        assert_eq!(parse(b"?"), Some(Command::Query));
        assert_eq!(query_reply(false), b"\x1b]7501;?\x1b\\");
        assert_eq!(query_reply(true), b"\x1b]7501;?\x07");
    }

    #[test]
    fn minimal_report_has_only_a_state() {
        let r = report("state=idle");
        assert_eq!(r.state, State::Idle);
        assert!(r.id.is_none() && r.kind.is_none() && r.progress.is_none());
        assert!(r.app.is_none() && r.title.is_none() && r.msg.is_none());
    }

    #[test]
    fn all_keys_are_read() {
        let r = report(
            "state=blocked:kind=permission:progress=42:id=tf/plan:app=terraform:title=UGxhbg==:msg=QXBwbHkgMyB0byBhZGQsIDEgdG8gY2hhbmdlLCAwIHRvIGRlc3Ryb3k/",
        );
        assert_eq!(r.state, State::Blocked);
        assert_eq!(r.kind, Some(Kind::Permission));
        assert_eq!(r.progress, Some(42));
        assert_eq!(r.id.as_deref(), Some("tf/plan"));
        assert_eq!(r.app.as_deref(), Some("terraform"));
        assert_eq!(r.title.as_deref(), Some("Plan"));
        assert_eq!(r.msg.as_deref(), Some("Apply 3 to add, 1 to change, 0 to destroy?"));
    }

    #[test]
    fn base64_padding_is_optional() {
        assert_eq!(report("state=done:msg=UGxhbg").msg.as_deref(), Some("Plan"));
        assert_eq!(report("state=done:msg=UGxhbg==").msg.as_deref(), Some("Plan"));
    }

    #[test]
    fn whitespace_around_keys_and_values_is_stripped() {
        let r = report(" state = working :\tapp=cargo\t");
        assert_eq!(r.state, State::Working);
        assert_eq!(r.app.as_deref(), Some("cargo"));
    }

    #[test]
    fn last_value_wins() {
        let r = report("state=idle:app=a:state=done:app=b");
        assert_eq!(r.state, State::Done);
        assert_eq!(r.app.as_deref(), Some("b"));
    }

    #[test]
    fn malformed_pairs_and_unknown_keys_are_skipped() {
        let r = report("garbage:=x:Bad=1:app=b@d:future=yes:state=working:app=ok:::");
        assert_eq!(r.state, State::Working);
        assert_eq!(r.app.as_deref(), Some("ok"));
    }

    #[test]
    fn kind_and_progress_only_apply_to_some_states() {
        let r = report("state=working:kind=auth:progress=10");
        assert_eq!((r.kind, r.progress), (None, Some(10)));
        let r = report("state=done:kind=auth:progress=10");
        assert_eq!((r.kind, r.progress), (None, None));
        assert_eq!(report("state=blocked:kind=dance").kind, None);
        assert_eq!(report("state=blocked:kind=auth").kind, Some(Kind::Auth));
    }

    #[test]
    fn out_of_range_progress_is_absent() {
        let cases: [(&str, Option<u8>); 11] = [
            ("0", Some(0)),
            ("100", Some(100)),
            ("101", None),
            ("256", None),
            ("99999999999999999999", None),
            ("-1", None),
            ("+1", None),
            ("4.5", None),
            ("1_0", None),
            ("007", Some(7)),
            ("", None),
        ];
        for (v, want) in cases {
            assert_eq!(report(&format!("state=working:progress={v}")).progress, want, "{v:?}");
        }
    }

    #[test]
    fn invalid_app_is_ignored() {
        assert_eq!(report("state=idle:app=a/b").app, None);
    }

    #[test]
    fn valid_ids_are_kept() {
        let long = "a".repeat(MAX_ID_SEGMENT);
        for id in ["a", "build/test", "A-Z_0.9+", long.as_str(), "a/b/c/d/e/f/g/h"] {
            assert_eq!(report(&format!("state=clear:id={id}")).id.as_deref(), Some(id));
        }
    }

    #[test]
    fn invalid_ids_discard_the_report_and_never_fall_back_to_root() {
        let seg33 = "a".repeat(MAX_ID_SEGMENT + 1);
        let seg31 = format!("{}/", "a".repeat(31));
        let over_128 = format!("{}a", seg31.repeat(4));
        let bad = [
            "",
            "/",
            "a/",
            "/a",
            "a//b",
            "a,b",
            "a=b",
            "a@b",
            "a b",
            "a%20b",
            seg33.as_str(),
            "a/b/c/d/e/f/g/h/i",
            over_128.as_str(),
        ];
        for id in bad {
            assert!(dropped(&format!("state=clear:id={id}")), "{id:?}");
            // 뒤에 올바른 id가 와도 틀린 id는 되살아나지 않는다
            assert!(dropped(&format!("state=clear:id={id}:id=a")), "{id:?}");
        }
    }

    #[test]
    fn limits_apply_to_values_with_disallowed_bytes() {
        let body = format!("state=idle:app=@{}", "a".repeat(MAX_APP));
        assert!(dropped(&body));
    }

    #[test]
    fn text_must_decode_to_safe_utf8() {
        for body in [
            "state=done:msg=a",   // base64가 아니다
            "state=done:msg=QQ=", // 패딩이 틀리다
            "state=done:title=a",
            "state=done:msg=YQpi", // "a\\nb": 제어 문자
            "state=done:msg=wps=", // U+009B: C1 제어 문자
            "state=done:msg=/w==", // UTF-8이 아니다
            "state=done:msg=YQlj", // "a\\tc": 탭
        ] {
            assert!(dropped(body), "{body:?}");
        }
        assert_eq!(report("state=done:msg=5a6J5YWo").msg.as_deref(), Some("安全"));
    }

    #[test]
    fn empty_text_is_absent() {
        let r = report("state=done:msg=:title=");
        assert!(r.msg.is_none() && r.title.is_none());
    }

    #[test]
    fn missing_or_unknown_state_discards_the_report() {
        for body in ["", "app=cargo", "state=sleeping", "state=", "state=IDLE"] {
            assert!(dropped(body), "{body:?}");
        }
    }

    #[test]
    fn size_limits_discard_the_whole_report() {
        let title = "QUFB".repeat(64); // 인코딩 256바이트, 192바이트로 풀림
        let main = "QUFB".repeat(MAX_MSG / 3);
        let fill = MAX_BODY - "state=idle:".len();
        let cases: Vec<(String, bool)> = vec![
            (format!("state=idle:{}=1", "a".repeat(MAX_KEY)), true),
            (format!("state=idle:{}=1", "a".repeat(MAX_KEY + 1)), false),
            (format!("state=idle:app={}", "a".repeat(MAX_APP + 1)), false),
            (format!("state=idle:app={}:app=ok", "a".repeat(MAX_APP + 1)), false),
            (format!("state=idle:title={title}"), true),
            (format!("state=idle:title={title}QUFB"), false),
            (format!("state=idle:msg={main}QUE"), true),
            (format!("state=idle:msg={main}QUFB"), false),
            (format!("state=idle:{}", "x".repeat(fill)), true),
            (format!("state=idle:{}", "x".repeat(fill + 1)), false),
        ];
        for (body, ok) in cases {
            assert_eq!(parse(body.as_bytes()).is_some(), ok, "{}", &body[..body.len().min(40)]);
        }
    }

    #[test]
    fn invisible_and_bidi_characters_are_removed_for_display() {
        assert_eq!(safe_text("a\u{202E}b\u{200B}c\u{2066}d\u{FEFF}", 50), "abcd");
        // 줄·문단 구분자와 빈 칸처럼 보이는 채움 글자도 지운다(보안 검토 Low, #618). 이모지 변이 선택자는 남긴다
        assert_eq!(
            safe_text(
                "a\u{2028}b\u{2029}c\u{3164}d\u{115F}\u{1160}e\u{FFA0}f\u{034F}g\u{1D173}h",
                50
            ),
            "abcdefgh"
        );
        assert_eq!(safe_text("\u{2764}\u{FE0F}", 50), "\u{2764}\u{FE0F}");
        assert_eq!(safe_text("줄\u{1}바꿈", 50), "줄 바꿈");
        assert_eq!(safe_text("가나다라마", 3), "가나…");
    }

    fn apply(rs: &mut Records, body: &str) -> Option<Alert> {
        rs.apply(report(body))
    }

    #[test]
    fn a_report_replaces_its_record_completely() {
        let mut rs = Records::default();
        apply(&mut rs, "state=working:progress=40:app=x:id=a");
        apply(&mut rs, "state=working:id=a");
        assert_eq!(rs.len(), 1);
        assert_eq!(rs.records()[0].progress, None);
        assert_eq!(rs.records()[0].app, None);
    }

    #[test]
    fn clear_removes_the_record_and_its_descendants_or_everything() {
        let mut rs = Records::default();
        for id in ["a", "a/b", "a/b/c", "ab", "z"] {
            apply(&mut rs, &format!("state=working:id={id}"));
        }
        apply(&mut rs, "state=working");
        apply(&mut rs, "state=clear:id=a");
        let ids: Vec<&str> = rs.records().iter().map(|r| r.id.as_str()).collect();
        assert_eq!(ids, ["ab", "z", ""]);
        apply(&mut rs, "state=clear");
        assert!(rs.is_empty());
    }

    #[test]
    fn app_is_inherited_from_the_nearest_ancestor() {
        let mut rs = Records::default();
        apply(&mut rs, "state=working:app=root");
        apply(&mut rs, "state=working:id=a:app=mid");
        let alert = apply(&mut rs, "state=blocked:id=a/b/c").unwrap();
        assert_eq!(alert.app.as_deref(), Some("mid"));
        let alert = apply(&mut rs, "state=blocked:id=z").unwrap();
        assert_eq!(alert.app.as_deref(), Some("root"));
    }

    #[test]
    fn full_store_evicts_the_least_recently_updated() {
        let mut rs = Records::default();
        for i in 0..MAX_RECORDS {
            apply(&mut rs, &format!("state=working:id=r{i}"));
        }
        // r0을 갱신하면 r1이 가장 오래된 것이 된다
        apply(&mut rs, "state=working:id=r0:progress=5");
        apply(&mut rs, "state=working:id=new");
        assert_eq!(rs.len(), MAX_RECORDS);
        let ids: Vec<&str> = rs.records().iter().map(|r| r.id.as_str()).collect();
        assert!(ids.contains(&"r0") && ids.contains(&"new") && !ids.contains(&"r1"));
    }

    #[test]
    fn prompt_and_exit_drop_live_records_but_keep_done_and_error() {
        let mut rs = Records::default();
        for (id, st) in [
            ("a", "working"),
            ("b", "blocked"),
            ("c", "idle"),
            ("d", "done"),
            ("e", "error"),
        ] {
            apply(&mut rs, &format!("state={st}:id={id}"));
        }
        assert!(rs.drop_live());
        let ids: Vec<&str> = rs.records().iter().map(|r| r.id.as_str()).collect();
        assert_eq!(ids, ["d", "e"]);
        assert!(!rs.drop_live());
    }

    #[test]
    fn clear_seen_removes_only_done_and_error() {
        let mut rs = Records::default();
        for (id, st) in [("a", "working"), ("d", "done"), ("e", "error")] {
            apply(&mut rs, &format!("state={st}:id={id}"));
        }
        assert!(rs.clear_seen());
        assert_eq!(rs.len(), 1);
        assert!(!rs.clear_seen());
    }

    #[test]
    fn summary_picks_the_most_urgent_state() {
        let mut rs = Records::default();
        assert_eq!(rs.summary(), None);
        apply(&mut rs, "state=idle:id=i");
        assert_eq!(rs.summary(), None, "idle은 아무것도 보이지 않는다");
        apply(&mut rs, "state=done:id=d");
        assert_eq!(rs.summary().unwrap().state, State::Done);
        apply(&mut rs, "state=working:id=w:progress=30");
        assert_eq!(
            rs.summary(),
            Some(Summary {
                state: State::Working,
                progress: Some(30)
            })
        );
        apply(&mut rs, "state=error:id=e");
        assert_eq!(rs.summary().unwrap().state, State::Error);
        apply(&mut rs, "state=blocked:id=b:kind=question");
        assert_eq!(rs.summary().unwrap().state, State::Blocked);
    }

    #[test]
    fn alerts_fire_only_when_a_record_becomes_blocked_done_or_error() {
        let mut rs = Records::default();
        assert!(apply(&mut rs, "state=working").is_none());
        let a = apply(&mut rs, "state=blocked:kind=permission:title=UGxhbg").unwrap();
        assert_eq!(
            (a.state, a.kind, a.title.as_deref()),
            (State::Blocked, Some(Kind::Permission), Some("Plan"))
        );
        // 같은 상태의 되풀이는 다시 알리지 않는다
        assert!(apply(&mut rs, "state=blocked:progress=3").is_none());
        assert!(apply(&mut rs, "state=working").is_none());
        assert_eq!(apply(&mut rs, "state=done").unwrap().state, State::Done);
        assert!(apply(&mut rs, "state=idle").is_none());
    }
}
