//! kitty 데스크톱 알림(OSC 99): 조각 합치기, base64, 지원 질의 답(#561).
//!
//! 기준은 kitty 명세(`docs/desktop-notifications.rst`)다. Ghostty 앱은 OSC 99를 처리하지 않으므로(libghostty-vt의 파서
//! `kitty_desktop_notification.zig`만 있다) 파서의 상한(평문 2048바이트, 인코딩 4096바이트)과 메타데이터 읽기 규칙을 따른다.
//!
//! 형태: `OSC 99 ; 메타데이터 ; 페이로드 (BEL | ST)`. 메타데이터는 `:`로 나뉜 `key=value`다.
//! 하는 키는 `i`(식별자) `d`(끝남) `e`(base64) `p`(페이로드 종류)뿐이고, 나머지 키(`a c f g n o s t u w`)는 읽지 않는다.
//! 버튼·아이콘·소리·닫기·살아 있는지·클릭 보고는 하지 않는다(`p=buttons|icon|close|alive`는 그 조각을 버린다).
//!
//! 쌓는 상태(`Assembler`)는 스캐너가 쥔다. 스캐너는 세션 읽기 스레드마다 하나이므로 쌓임도 세션마다 하나다.
//! 프로그램이 보낸 바이트는 믿지 않는다: 조각 크기는 읽기 전에, 쌓는 개수와 글자 수에는 상한을 둔다.

use crate::notify::{MAX_BODY, MAX_TITLE, Notification, clean};

/// 평문 조각 하나의 상한(바이트). Ghostty `MAX_PLAIN_PAYLOAD_BYTES`, kitty 명세 62줄
pub const MAX_PLAIN_PAYLOAD: usize = 2048;
/// base64 조각 하나의 상한(바이트). Ghostty `MAX_ENCODED_PAYLOAD_BYTES`, kitty 명세 62줄
pub const MAX_ENCODED_PAYLOAD: usize = 4096;
/// 식별자 길이 상한(바이트). 명세에는 길이 상한이 없다(권장은 UUID, 36바이트). 답에 되돌려 보내고 쌓임의 열쇠가 되므로 넉넉히 묶는다
pub const MAX_ID_LEN: usize = 128;
/// 세션마다 쌓아 둘 수 있는 미완성 알림 수. 사람이 한 번에 쌓는 알림은 한두 개이고, 진행 막대를 `i`로 갱신하는 프로그램도
/// 완성(d=1)한 뒤 낸다. 16개면 정상 사용에 넉넉하고, 최악의 메모리는 16 × (제목 121자 + 본문 501자) × 4바이트 = 약 40KB다.
/// 넘으면 가장 오래 만지지 않은 것을 버린다
pub const MAX_PENDING: usize = 16;

/// 메타데이터 값에 허용하는 글자. 식별자 글자에 Ghostty가 더한 것(`=`은 base64 패딩, `?`는 `p=?`)을 포함한다
const VALUE_EXTRA: &[u8] = b"/,(){}[]*&^%$#@!`~=?";

fn is_ident_byte(b: u8) -> bool {
    b.is_ascii_alphanumeric() || matches!(b, b'-' | b'_' | b'+' | b'.')
}

fn is_value_byte(b: u8) -> bool {
    is_ident_byte(b) || VALUE_EXTRA.contains(&b)
}

/// 메타데이터에서 `key`의 첫 값. Ghostty `ValueIterator`처럼 키·값 둘레의 공백을 자르고, 값 글자가 허용 밖이면 그 항목을 건너뛴다.
/// 같은 키가 여럿이면 처음 읽히는 값만 쓴다
fn field<'a>(meta: &'a str, key: &str) -> Option<&'a str> {
    meta.split(':').find_map(|kv| {
        let (k, v) = kv.split_once('=')?;
        let v = v.trim();
        (k.trim() == key && v.bytes().all(is_value_byte)).then_some(v)
    })
}

/// 0이나 1. 다른 값이면 기본값(Ghostty `parseBool`)
fn flag(meta: &str, key: &str, default: bool) -> bool {
    match field(meta, key) {
        Some("0") => false,
        Some("1") => true,
        _ => default,
    }
}

/// 메타데이터의 `i`
#[derive(Debug, PartialEq, Eq)]
enum Id<'a> {
    /// `i`가 없거나 비었다
    Absent,
    /// 명세의 식별자 글자(`[a-zA-Z0-9-_+.]`)와 길이를 지킨다
    Valid(&'a str),
    /// 첫 `i`가 식별자가 아니다. 알림 조각은 `i`가 없는 것으로 본다. 질의는 답하지 않는다(명세 끝 `identifier` 절: 입력 주입 방지)
    Bad,
}

fn identifier(meta: &str) -> Id<'_> {
    for kv in meta.split(':') {
        let Some((k, v)) = kv.split_once('=') else { continue };
        if k.trim() != "i" {
            continue;
        }
        let v = v.trim();
        return if v.is_empty() {
            Id::Absent
        } else if v.len() <= MAX_ID_LEN && v.bytes().all(is_ident_byte) {
            Id::Valid(v)
        } else {
            Id::Bad
        };
    }
    Id::Absent
}

#[derive(Debug, PartialEq, Eq)]
enum Kind {
    Title,
    Body,
    Query,
    /// `buttons` `icon` `close` `alive`, 모르는 값: 이 조각은 버린다
    Other,
}

fn kind(meta: &str) -> Kind {
    match field(meta, "p") {
        None | Some("title") => Kind::Title,
        Some("body") => Kind::Body,
        Some("?") => Kind::Query,
        Some(_) => Kind::Other,
    }
}

/// base64(RFC 4648 표준 글자)를 UTF-8 글로 푼다. 패딩은 있어도 없어도 되고(명세 base64 절), 가운데의 `=`나
/// 글자 수가 4로 나눠 1이 남는 것은 틀린 것이다. 글자가 틀리거나 UTF-8이 아니면 `None`
fn decode(payload: &str) -> Option<String> {
    let body = payload.trim_end_matches('=');
    if payload.len() - body.len() > 2 || body.contains('=') || body.len() % 4 == 1 {
        return None;
    }
    let bytes = crate::graphics::base64_decode(body.as_bytes())?;
    String::from_utf8(bytes).ok()
}

/// 제어 문자를 공백으로 바꾼다(앞뒤를 자르지 않는다: 조각 경계의 공백은 글의 일부다)
fn sanitize(s: &str) -> String {
    s.chars().map(|c| if c.is_control() { ' ' } else { c }).collect()
}

#[derive(Debug, Default)]
struct Pending {
    id: String,
    title: String,
    body: String,
}

impl Pending {
    /// 글을 이어 붙인다. 최종 상한보다 한 글자 더(넘쳤는지 알아야 `…`를 붙인다)까지만 쥐고 나머지는 버린다
    fn append(&mut self, kind: &Kind, text: &str) {
        let (dst, max) = match kind {
            Kind::Body => (&mut self.body, MAX_BODY),
            _ => (&mut self.title, MAX_TITLE),
        };
        let room = (max + 1).saturating_sub(dst.chars().count());
        dst.extend(text.chars().take(room));
    }

    fn into_notification(self) -> Option<Notification> {
        let title = clean(&self.title, MAX_TITLE);
        let body = clean(&self.body, MAX_BODY);
        (!title.is_empty() || !body.is_empty()).then_some(Notification { title, body })
    }
}

/// 조각 하나를 먹인 결과
#[derive(Debug, PartialEq, Eq)]
pub enum Outcome {
    /// 할 일이 없다(쌓았거나 버렸다)
    Nothing,
    Notify(Notification),
    /// 지원 질의의 답(끝맺음까지)
    Reply(Vec<u8>),
}

/// 세션 하나의 쌓임. `i`마다 제목·본문 조각을 모았다가 `d=1`(또는 `d` 없음)에서 알림 하나로 낸다
#[derive(Debug, Default)]
pub struct Assembler {
    /// 오래 만진 순서: 맨 앞이 가장 오래된 것
    pending: Vec<Pending>,
}

impl Assembler {
    /// `OSC 99 ;` 뒤의 `메타데이터;페이로드`를 먹인다. `bel`은 받은 끝맺음이 BEL인가(답도 같게 한다)
    pub fn packet(&mut self, rest: &str, bel: bool) -> Outcome {
        // 메타데이터와 페이로드 사이 `;`가 없으면 무효다(Ghostty `parse`의 "missing semicolon before payload", #764)
        let Some((meta, payload)) = rest.split_once(';') else {
            return Outcome::Nothing;
        };
        let encoded = flag(meta, "e", false);
        // 읽기 전에 크기부터 본다(Ghostty 파서와 같다)
        let limit = if encoded {
            MAX_ENCODED_PAYLOAD
        } else {
            MAX_PLAIN_PAYLOAD
        };
        if payload.len() > limit {
            return Outcome::Nothing;
        }
        let kind = kind(meta);
        match kind {
            Kind::Other => Outcome::Nothing,
            Kind::Query => match identifier(meta) {
                Id::Bad => Outcome::Nothing,
                Id::Absent => Outcome::Reply(query_reply(None, bel)),
                Id::Valid(id) => Outcome::Reply(query_reply(Some(id), bel)),
            },
            Kind::Title | Kind::Body => {
                let Some(text) = (if encoded {
                    decode(payload)
                } else {
                    Some(payload.to_string())
                }) else {
                    return Outcome::Nothing;
                };
                let done = flag(meta, "d", true);
                match identifier(meta) {
                    Id::Valid(id) => self.chunk(id, &kind, &sanitize(&text), done),
                    // `i`가 없으면 갱신하지 않는다(명세 188줄). 명세는 `i` 없는 d=0에 대해 말이 없어 바로 알린다
                    Id::Absent | Id::Bad => {
                        let mut one = Pending::default();
                        one.append(&kind, &sanitize(&text));
                        one.into_notification().map_or(Outcome::Nothing, Outcome::Notify)
                    }
                }
            }
        }
    }

    fn chunk(&mut self, id: &str, kind: &Kind, text: &str, done: bool) -> Outcome {
        let mut entry = match self.pending.iter().position(|p| p.id == id) {
            Some(i) => self.pending.remove(i),
            None => Pending {
                id: id.to_string(),
                ..Pending::default()
            },
        };
        entry.append(kind, text);
        if done {
            return entry.into_notification().map_or(Outcome::Nothing, Outcome::Notify);
        }
        if self.pending.len() >= MAX_PENDING {
            self.pending.remove(0);
        }
        self.pending.push(entry);
        Outcome::Nothing
    }

    /// 쌓인 미완성 알림 수
    pub fn pending_len(&self) -> usize {
        self.pending.len()
    }

    /// 쌓아 둔 글자 수(제목과 본문 합계)
    pub fn held_chars(&self) -> usize {
        self.pending
            .iter()
            .map(|p| p.title.chars().count() + p.body.chars().count())
            .sum()
    }
}

/// 지원 질의의 답: `OSC 99 ; i=<id> : p=? ; o=always : p=title,body,? <끝맺음>`.
/// 실제로 하는 것만 적는다: 제목·본문(`p`), `o`는 따르지 않고 늘 알리므로 `o=always`(명세 402줄: 하나도 안 하면 `o=always`).
/// `a`(클릭 동작)·`c`(닫힘 보고)·`s`(소리)·`u`(긴급도)·`w`(자동 닫힘)는 하지 않으므로 뺀다.
/// `id`는 호출하는 쪽이 식별자 글자만 남긴 것이어야 한다(PTY 입력 주입 방지)
fn query_reply(id: Option<&str>, bel: bool) -> Vec<u8> {
    debug_assert!(id.is_none_or(|i| i.bytes().all(is_ident_byte)));
    let mut out = String::from("\x1b]99;");
    if let Some(id) = id {
        out.push_str("i=");
        out.push_str(id);
        out.push(':');
    }
    out.push_str("p=?;o=always:p=title,body,?");
    out.push_str(if bel { "\x07" } else { "\x1b\\" });
    out.into_bytes()
}
