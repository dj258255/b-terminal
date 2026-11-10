//! kitty 클립보드 프로토콜(OSC 5522)(#325, ADR-030). 기준: Ghostty `src/terminal/kitty/clipboard*.zig`
//!
//! 이 모듈은 창도 PTY도 클립보드도 모른다. 패킷을 받아 요청·답 바이트를 만든다. 정책(`clipboard-read`·`clipboard-write`)과
//! 클립보드 읽고 쓰기는 앱이 한다. 상한은 모두 모으기(버퍼에 붙이기) 전에 본다.
//! 답에는 요청의 바이트를 그대로 넣지 않는다: id는 허용 글자만 남기고, MIME·암호는 base64로만 싣는다

use std::sync::Arc;

use parking_lot::Mutex;

/// id 길이 상한(kitty 소스와 같다)
pub const MAX_ID_LEN: usize = 512;
/// 풀어 놓은 암호 길이 상한. 넘으면 암호가 없는 것으로 본다
pub const MAX_PW_LEN: usize = 128;
/// 풀어 놓은 MIME 종류 길이 상한. 넘으면 패킷을 버린다
pub const MAX_MIME_LEN: usize = 256;
/// 풀어 놓은 이름 길이 상한
pub const MAX_NAME_LEN: usize = 256;
/// 쓰기 한 번에 담는 MIME 종류 수와 별칭 수 상한
pub const MAX_WRITE_MIMES: usize = 64;
pub const MAX_WRITE_ALIASES: usize = 64;
/// 읽기 답의 DATA 패킷 하나에 담는 원본 바이트(base64 이전)
pub const READ_CHUNK: usize = 4096;
/// 읽기 요청 하나가 받는 MIME 종류 수 상한(넘는 것은 답하지 않는다)
pub const MAX_READ_MIMES: usize = 4;
/// 붙여넣기 이벤트 목록에 적는 MIME 종류 수 상한
pub const MAX_LISTING_MIMES: usize = 16;
/// 일회용 암호 길이
pub const OTP_LEN: usize = 22;
/// 허가 목록 크기 상한(차면 가장 오래된 것을 버린다)
pub const MAX_GRANTS: usize = 32;
/// 종류 목록을 달라는 특별한 MIME
pub const TARGETS_MIME: &str = ".";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Op {
    Read,
    Walias,
    Wdata,
    Write,
}

impl Op {
    fn init(s: &str) -> Option<Self> {
        match s {
            "read" => Some(Self::Read),
            "walias" => Some(Self::Walias),
            "wdata" => Some(Self::Wdata),
            "write" => Some(Self::Write),
            _ => None,
        }
    }

    fn name(self) -> &'static str {
        match self {
            Self::Read => "read",
            Self::Walias => "walias",
            Self::Wdata => "wdata",
            Self::Write => "write",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Status {
    Data,
    Done,
    Ebusy,
    Efbig,
    Einval,
    Eio,
    Enosys,
    Eperm,
    Ok,
}

impl Status {
    fn name(self) -> &'static str {
        match self {
            Self::Data => "DATA",
            Self::Done => "DONE",
            Self::Ebusy => "EBUSY",
            Self::Efbig => "EFBIG",
            Self::Einval => "EINVAL",
            Self::Eio => "EIO",
            Self::Enosys => "ENOSYS",
            Self::Eperm => "EPERM",
            Self::Ok => "OK",
        }
    }
}

/// 패킷 메타데이터(풀고 걸러 낸 것)
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Meta {
    pub op: Op,
    pub primary: bool,
    pub id: String,
    pub mime: String,
    pub pw: String,
    pub name: String,
}

/// 메타데이터 값이 UTF-8이 아니거나 base64가 틀렸거나 상한을 넘었다
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct InvalidValue;

/// 페이로드의 base64가 틀렸다
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Invalid;

// ---------- base64(엄격: RFC 4648 표준 글자, 패딩 필수, 공백·줄바꿈을 건너뛰지 않는다) ----------

const ALPHA: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";

fn b64_val(c: u8) -> Option<u8> {
    match c {
        b'A'..=b'Z' => Some(c - b'A'),
        b'a'..=b'z' => Some(c - b'a' + 26),
        b'0'..=b'9' => Some(c - b'0' + 52),
        b'+' => Some(62),
        b'/' => Some(63),
        _ => None,
    }
}

pub fn b64_encode(d: &[u8]) -> String {
    let mut out = String::with_capacity(d.len().div_ceil(3) * 4);
    for c in d.chunks(3) {
        let n =
            (u32::from(c[0]) << 16) | (u32::from(*c.get(1).unwrap_or(&0)) << 8) | u32::from(*c.get(2).unwrap_or(&0));
        for i in 0..4 {
            if i <= c.len() {
                out.push(ALPHA[((n >> (18 - 6 * i)) & 63) as usize] as char);
            } else {
                out.push('=');
            }
        }
    }
    out
}

/// 한 번에 다 풀기. 잘못되면 `None`
pub fn b64_decode(s: &[u8]) -> Option<Vec<u8>> {
    let mut st = B64Stream::default();
    let mut out = Vec::with_capacity(s.len() / 4 * 3);
    st.feed(s, &mut out).ok()?;
    st.finish().ok()?;
    Some(out)
}

/// 조각으로 오는 base64 한 줄기를 푼다. 조각 경계는 어디여도 된다.
/// 패딩으로 끝난 네 글자 묶음이 조각의 맨 끝이면 줄기가 새로 시작한다(kitty와 같다). 패딩 뒤에 같은 조각 안에서 글자가 더 오면 틀린 것이다
#[derive(Debug, Default)]
pub struct B64Stream {
    buf: [u8; 4],
    n: usize,
}

impl B64Stream {
    pub fn feed(&mut self, input: &[u8], out: &mut Vec<u8>) -> Result<(), Invalid> {
        for (i, &b) in input.iter().enumerate() {
            if b != b'=' && b64_val(b).is_none() {
                return Err(Invalid);
            }
            self.buf[self.n] = b;
            self.n += 1;
            if self.n < 4 {
                continue;
            }
            self.n = 0;
            let g = self.buf;
            let padded = g.contains(&b'=');
            Self::group(g, out)?;
            if padded && i + 1 < input.len() {
                return Err(Invalid);
            }
        }
        Ok(())
    }

    /// 줄기가 네 글자 묶음 경계에서 끝났는가
    pub fn finish(&mut self) -> Result<(), Invalid> {
        let ok = self.n == 0;
        self.n = 0;
        if ok { Ok(()) } else { Err(Invalid) }
    }

    fn group(g: [u8; 4], out: &mut Vec<u8>) -> Result<(), Invalid> {
        let v = |i: usize| b64_val(g[i]).ok_or(Invalid);
        if g[0] == b'=' || g[1] == b'=' {
            return Err(Invalid);
        }
        let (v0, v1) = (v(0)?, v(1)?);
        if g[2] == b'=' {
            // `xx==`: 남는 비트는 0이어야 한다
            if g[3] != b'=' || v1 & 0xf != 0 {
                return Err(Invalid);
            }
            out.push(v0 << 2 | v1 >> 4);
            return Ok(());
        }
        let v2 = v(2)?;
        if g[3] == b'=' {
            if v2 & 0x3 != 0 {
                return Err(Invalid);
            }
            out.extend([v0 << 2 | v1 >> 4, v1 << 4 | v2 >> 2]);
            return Ok(());
        }
        let v3 = v(3)?;
        out.extend([v0 << 2 | v1 >> 4, v1 << 4 | v2 >> 2, v2 << 6 | v3]);
        Ok(())
    }
}

// ---------- 메타데이터 ----------

/// id에서 `[a-zA-Z0-9-_+.]` 밖의 글자를 지우고 512바이트로 자른다
fn sanitize_id(v: &str) -> String {
    v.chars()
        .filter(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '+' | '.'))
        .take(MAX_ID_LEN)
        .collect()
}

enum ValueErr {
    Overflow,
    Invalid,
}

/// 메타데이터 값 하나를 푼다(base64된 UTF-8). 풀기 전에 길이부터 본다
fn decode_value(v: &str, max: usize) -> Result<String, ValueErr> {
    if v.len() > max.div_ceil(3) * 4 {
        return Err(ValueErr::Overflow);
    }
    let bytes = b64_decode(v.as_bytes()).ok_or(ValueErr::Invalid)?;
    let s = String::from_utf8(bytes).map_err(|_| ValueErr::Invalid)?;
    if s.len() > max {
        return Err(ValueErr::Overflow);
    }
    Ok(s)
}

#[derive(Default)]
struct Raw<'a> {
    op: Option<&'a str>,
    loc: &'a str,
    id: &'a str,
    mime: &'a str,
    pw: &'a str,
    name: &'a str,
}

fn parse_raw(raw: &str) -> Option<Raw<'_>> {
    let mut r = Raw::default();
    // 빈 문자열도 빈 기록 하나로 나뉘고 `=`가 없으니 버려진다. 구조를 다 확인한 뒤에 풀므로 같은 열쇠는 마지막 것이 이긴다
    for rec in raw.split(':') {
        let (k, v) = rec.split_once('=')?;
        match k {
            "type" => r.op = Some(v),
            "loc" => r.loc = v,
            "id" => r.id = v,
            "mime" => r.mime = v,
            "pw" => r.pw = v,
            "name" => r.name = v,
            _ => {}
        }
    }
    Some(r)
}

/// 메타데이터를 읽는다. `Ok(None)`은 조용히 버리는 패킷, `Err`은 값이 틀린 패킷(쓰기 중이면 거래를 끊는다)
pub fn parse_meta(raw: &str) -> Result<Option<Meta>, InvalidValue> {
    let Some(f) = parse_raw(raw) else { return Ok(None) };
    let Some(op) = f.op.and_then(Op::init) else {
        return Ok(None);
    };
    let bad = |_: ValueErr| InvalidValue;
    let mime = decode_value(f.mime, MAX_MIME_LEN).map_err(bad)?;
    let pw = match decode_value(f.pw, MAX_PW_LEN) {
        Ok(s) => s,
        // 너무 긴 암호는 없는 것과 같다
        Err(ValueErr::Overflow) => String::new(),
        Err(ValueErr::Invalid) => return Err(InvalidValue),
    };
    let name = decode_value(f.name, MAX_NAME_LEN).map_err(bad)?;
    Ok(Some(Meta {
        op,
        primary: f.loc == "primary",
        id: sanitize_id(f.id),
        mime,
        pw,
        name,
    }))
}

/// 값을 풀지 않고 연산만 읽는다(값이 틀린 패킷이 쓰기 거래를 끊어야 하는지 가르려고)
pub fn meta_operation(raw: &str) -> Option<Op> {
    parse_raw(raw)?.op.and_then(Op::init)
}

// ---------- 답 ----------

fn terminator(bel: bool) -> &'static [u8] {
    if bel { b"\x07" } else { b"\x1b\\" }
}

struct Packet<'a> {
    op: Op,
    status: Status,
    primary: bool,
    id: &'a str,
    mime: Option<&'a str>,
    pw: Option<&'a str>,
}

impl Packet<'_> {
    /// 열쇠 차례는 kitty와 같다: type, status, loc, id, mime, pw
    fn metadata(&self, out: &mut Vec<u8>) {
        out.extend(format!("\x1b]5522;type={}:status={}", self.op.name(), self.status.name()).bytes());
        if self.primary {
            out.extend(b":loc=primary");
        }
        if !self.id.is_empty() {
            out.extend(b":id=");
            out.extend(sanitize_id(self.id).bytes());
        }
        if let Some(m) = self.mime {
            out.extend(b":mime=");
            out.extend(b64_encode(m.as_bytes()).bytes());
        }
        if let Some(p) = self.pw {
            out.extend(b":pw=");
            out.extend(b64_encode(p.as_bytes()).bytes());
        }
    }

    fn encode(&self, payload: &[u8], bel: bool, out: &mut Vec<u8>) {
        self.metadata(out);
        // 빈 페이로드는 `;` 구간을 아예 보내지 않는다
        if !payload.is_empty() {
            out.push(b';');
            out.extend(b64_encode(payload).bytes());
        }
        out.extend(terminator(bel));
    }
}

/// 상태 하나만 알리는 답
pub fn status_reply(op: Op, status: Status, id: &str, bel: bool) -> Vec<u8> {
    let mut out = Vec::new();
    Packet {
        op,
        status,
        primary: false,
        id,
        mime: None,
        pw: None,
    }
    .encode(&[], bel, &mut out);
    out
}

/// 읽기 성공 답 전체: OK, (요청했으면) 종류 목록, 종류마다 DATA 조각, DONE
#[derive(Debug, Default)]
pub struct ReadSuccess<'a> {
    pub primary: bool,
    pub id: &'a str,
    /// 붙여넣기 이벤트의 일회용 암호. 모든 패킷에 싣는다
    pub pw: Option<&'a str>,
    pub list: bool,
    pub available: &'a [String],
    pub contents: &'a [(String, Vec<u8>)],
    pub bel: bool,
}

impl ReadSuccess<'_> {
    pub fn encode(&self) -> Vec<u8> {
        let mut out = Vec::new();
        let p = |status, mime, primary| Packet {
            op: Op::Read,
            status,
            primary,
            id: self.id,
            mime,
            pw: self.pw,
        };
        p(Status::Ok, None, self.primary).encode(&[], self.bel, &mut out);
        if self.list {
            // 종류는 공백으로 잇고 비지 않으면 끝에 줄바꿈. DATA 조각 하나의 크기를 넘는 종류는 버린다
            let listing = p(Status::Data, Some(TARGETS_MIME), false);
            listing.metadata(&mut out);
            if !self.available.is_empty() {
                let mut raw: Vec<u8> = Vec::new();
                for (i, m) in self.available.iter().enumerate() {
                    let sep = usize::from(i > 0);
                    if raw.len() + sep + m.len() + 1 > READ_CHUNK {
                        break;
                    }
                    if i > 0 {
                        raw.push(b' ');
                    }
                    raw.extend(m.bytes());
                }
                raw.push(b'\n');
                out.push(b';');
                out.extend(b64_encode(&raw).bytes());
            }
            out.extend(terminator(self.bel));
        }
        for (mime, data) in self.contents {
            for chunk in data.chunks(READ_CHUNK) {
                p(Status::Data, Some(mime), false).encode(chunk, self.bel, &mut out);
            }
        }
        p(Status::Done, None, false).encode(&[], self.bel, &mut out);
        out
    }
}

/// 붙여넣기 이벤트(모드 5522): 종류 목록과 일회용 암호만 싣는다. 클립보드 내용은 싣지 않는다
pub fn paste_event(primary: bool, otp: &str, available: &[String]) -> Vec<u8> {
    let n = available.len().min(MAX_LISTING_MIMES);
    ReadSuccess {
        primary,
        pw: Some(otp),
        list: true,
        available: &available[..n],
        ..Default::default()
    }
    .encode()
}

// ---------- 허가 ----------

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Dir {
    Read,
    Write,
}

#[derive(Debug)]
struct Grant {
    pw: String,
    read: bool,
    write: bool,
    one_time: bool,
}

/// 암호로 묶는 허가. 일회용은 확인하는 순간 사라진다(방향이 달라도)
#[derive(Debug, Default)]
pub struct Grants {
    entries: Vec<Grant>,
}

impl Grants {
    pub fn grant(&mut self, pw: &str, dir: Dir, one_time: bool) {
        if pw.is_empty() || pw.len() > MAX_PW_LEN {
            return;
        }
        let idx = match self.entries.iter().position(|e| e.pw == pw) {
            Some(i) => {
                self.entries[i].one_time &= one_time;
                i
            }
            None => {
                if self.entries.len() >= MAX_GRANTS {
                    self.entries.remove(0);
                }
                self.entries.push(Grant {
                    pw: pw.to_string(),
                    read: false,
                    write: false,
                    one_time,
                });
                self.entries.len() - 1
            }
        };
        match dir {
            Dir::Read => self.entries[idx].read = true,
            Dir::Write => self.entries[idx].write = true,
        }
    }

    pub fn consume(&mut self, pw: &str, dir: Dir) -> bool {
        if pw.is_empty() {
            return false;
        }
        let Some(i) = self.entries.iter().position(|e| e.pw == pw) else {
            return false;
        };
        let allowed = match dir {
            Dir::Read => self.entries[i].read,
            Dir::Write => self.entries[i].write,
        };
        if self.entries[i].one_time {
            self.entries.swap_remove(i);
        }
        allowed
    }

    pub fn len(&self) -> usize {
        self.entries.len()
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    pub fn clear(&mut self) {
        self.entries.clear();
    }
}

const OTP_ALPHABET: &[u8] = b"23456789abcdefghijkmnopqrstuvwxyzABCDEFGHJKLMNPQRSTUVWXYZ";

/// 붙여넣기 이벤트의 일회용 암호. 운영체제 난수(`/dev/urandom`)에서 치우침 없이 뽑는다.
/// 암호를 아는 프로그램은 확인 없이 클립보드를 읽을 수 있으므로 난수를 못 얻으면 만들지 않는다(대신할 값이 없다)
pub fn generate_otp() -> Option<String> {
    use std::io::Read;
    let mut f = std::fs::File::open("/dev/urandom").ok()?;
    let limit = 256 / OTP_ALPHABET.len() * OTP_ALPHABET.len();
    let mut out = String::with_capacity(OTP_LEN);
    while out.len() < OTP_LEN {
        let mut raw = [0u8; 2 * OTP_LEN];
        f.read_exact(&mut raw).ok()?;
        for b in raw {
            if usize::from(b) >= limit {
                continue;
            }
            out.push(OTP_ALPHABET[usize::from(b) % OTP_ALPHABET.len()] as char);
            if out.len() == OTP_LEN {
                break;
            }
        }
    }
    Some(out)
}

/// 실제로 읽고 쓸 수 있는 글 종류인가(arboard는 글만 다룬다)
pub fn is_text_mime(mime: &str) -> bool {
    let m: String = mime
        .chars()
        .filter(|c| !c.is_whitespace())
        .collect::<String>()
        .to_ascii_lowercase();
    m == "text/plain" || m == "text/plain;charset=utf-8"
}

// ---------- 쓰기 거래 ----------

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WriteError {
    TooLarge,
    Invalid,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Committed {
    pub primary: bool,
    pub id: String,
    pub pw: String,
    pub name: String,
    pub contents: Vec<(String, Vec<u8>)>,
}

#[derive(Debug)]
struct Entry {
    mime: String,
    start: usize,
    len: usize,
}

/// 쓰기 거래 하나: `type=write`부터 commit까지의 wdata·walias를 모은다
#[derive(Debug)]
pub struct WriteState {
    primary: bool,
    pub id: String,
    pw: String,
    name: String,
    max_size: usize,
    spool: Vec<u8>,
    entries: Vec<Entry>,
    aliases: Vec<(String, String)>,
    current: Option<usize>,
    dec: B64Stream,
}

impl WriteState {
    /// `max_size`: 한 거래가 모을 수 있는 풀린 바이트의 총합
    pub fn new(meta: &Meta, max_size: usize) -> Self {
        Self {
            primary: meta.primary,
            id: meta.id.clone(),
            pw: meta.pw.clone(),
            name: meta.name.clone(),
            max_size,
            spool: Vec::new(),
            entries: Vec::new(),
            aliases: Vec::new(),
            current: None,
            dec: B64Stream::default(),
        }
    }

    /// wdata 조각 하나(`meta.mime`은 비어 있지 않다). 상한은 모으기 전에 본다
    pub fn data(&mut self, meta: &Meta, payload: &[u8]) -> Result<(), WriteError> {
        let same = self.current.is_some_and(|i| self.entries[i].mime == meta.mime);
        if !same {
            // 앞 종류의 줄기는 온전한 묶음에서 끝나야 한다
            if let Some(i) = self.current {
                self.dec.finish().map_err(|_| WriteError::Invalid)?;
                self.entries[i].len = self.spool.len() - self.entries[i].start;
            }
            if let Some(i) = self.entries.iter().position(|e| e.mime == meta.mime) {
                // 이미 쓴 종류를 다시 쓰면 새 구간으로 덮는다
                self.entries[i].start = self.spool.len();
                self.entries[i].len = 0;
                self.current = Some(i);
            } else if self.entries.len() >= MAX_WRITE_MIMES {
                self.current = None;
                return Ok(());
            } else {
                self.entries.push(Entry {
                    mime: meta.mime.clone(),
                    start: self.spool.len(),
                    len: 0,
                });
                self.current = Some(self.entries.len() - 1);
            }
        }
        let mut tmp = Vec::new();
        self.dec.feed(payload, &mut tmp).map_err(|_| WriteError::Invalid)?;
        if tmp.len() > self.max_size.saturating_sub(self.spool.len()) {
            return Err(WriteError::TooLarge);
        }
        self.spool.extend(tmp);
        Ok(())
    }

    /// walias: `meta.mime`이 데이터를 든 대상, 페이로드는 base64된 공백 나눈 별칭 목록
    pub fn alias(&mut self, meta: &Meta, payload: &[u8]) -> Result<(), Invalid> {
        let bytes = b64_decode(payload).ok_or(Invalid)?;
        let text = std::str::from_utf8(&bytes).map_err(|_| Invalid)?;
        for name in text
            .split(|c: char| matches!(c, ' ' | '\t'..='\r'))
            .filter(|s| !s.is_empty())
        {
            if name.len() > MAX_MIME_LEN {
                continue;
            }
            if let Some(a) = self.aliases.iter_mut().find(|a| a.0 == name) {
                a.1 = meta.mime.clone();
            } else if self.aliases.len() >= MAX_WRITE_ALIASES {
                return Ok(());
            } else {
                self.aliases.push((name.to_string(), meta.mime.clone()));
            }
        }
        Ok(())
    }

    /// 거래를 끝낸다(MIME 없는 wdata). 마지막 줄기의 패딩이 틀렸으면 `Invalid`
    pub fn commit(&mut self) -> Result<Committed, Invalid> {
        if let Some(i) = self.current.take() {
            self.dec.finish()?;
            self.entries[i].len = self.spool.len() - self.entries[i].start;
        }
        let mut contents: Vec<(String, Vec<u8>)> = self
            .entries
            .iter()
            .map(|e| (e.mime.clone(), self.spool[e.start..e.start + e.len].to_vec()))
            .collect();
        // 별칭은 적은 차례대로 지금까지의 표에 적용한다. 대상이 없으면 버리고, 이미 있는 이름이면 덮는다
        for (alias, target) in &self.aliases {
            let Some(data) = contents.iter().find(|c| &c.0 == target).map(|c| c.1.clone()) else {
                continue;
            };
            match contents.iter_mut().find(|c| &c.0 == alias) {
                Some(c) => c.1 = data,
                None => contents.push((alias.clone(), data)),
            }
        }
        Ok(Committed {
            primary: self.primary,
            id: self.id.clone(),
            pw: self.pw.clone(),
            name: self.name.clone(),
            contents,
        })
    }
}

// ---------- 엔진 ----------

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReadRequest {
    pub primary: bool,
    pub id: String,
    /// 데이터를 달라는 종류(목록 `.`은 빼고, 최대 4개)
    pub mimes: Vec<String>,
    pub list: bool,
    /// 이름이 있을 때만 암호를 쓴다
    pub pw: String,
    pub name: String,
    /// 암호가 허가를 가리켜 확인을 건너뛰는가
    pub granted: bool,
    pub bel: bool,
}

impl ReadRequest {
    /// 확인 없이 답해도 되는 요청인가: 데이터 종류가 하나도 없는(목록만 묻는) 요청
    pub fn prompt_exempt(&self) -> bool {
        self.mimes.is_empty()
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WriteRequest {
    pub primary: bool,
    pub id: String,
    pub pw: String,
    pub name: String,
    pub contents: Vec<(String, Vec<u8>)>,
    pub granted: bool,
    pub bel: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Action {
    /// PTY에 바로 쓸 답
    Reply(Vec<u8>),
    Read(ReadRequest),
    Write(WriteRequest),
}

/// 칸 하나의 OSC 5522 상태: 진행 중인 쓰기 거래와 허가
pub struct Engine {
    grants: Arc<Mutex<Grants>>,
    write: Option<WriteState>,
    write_enabled: bool,
    max_write: usize,
}

impl Engine {
    /// `write_enabled`가 꺼져 있으면(clipboard-write = deny) 쓰기 거래는 시작하자마자 EPERM으로 끝낸다.
    /// `max_write`: 쓰기 한 번의 풀린 바이트 상한
    pub fn new(grants: Arc<Mutex<Grants>>, write_enabled: bool, max_write: usize) -> Self {
        Self {
            grants,
            write: None,
            write_enabled,
            max_write,
        }
    }

    fn finish(&mut self, status: Status, bel: bool) -> Vec<Action> {
        let id = self.write.take().map(|w| w.id).unwrap_or_default();
        vec![Action::Reply(status_reply(Op::Write, status, &id, bel))]
    }

    /// `5522;` 뒤의 `메타데이터[;페이로드]` 한 패킷을 처리한다
    pub fn handle(&mut self, packet: &str, bel: bool) -> Vec<Action> {
        let (md, payload) = packet.split_once(';').unwrap_or((packet, ""));
        let meta = match parse_meta(md) {
            Ok(Some(m)) => m,
            Ok(None) => return Vec::new(),
            Err(InvalidValue) => {
                if self.write.is_some() && matches!(meta_operation(md), Some(Op::Wdata | Op::Walias)) {
                    return self.finish(Status::Einval, bel);
                }
                return Vec::new();
            }
        };
        match meta.op {
            Op::Read => self.read(&meta, payload, bel),
            Op::Write => {
                // 새 write는 진행 중이던 거래를 말없이 바꾼다
                self.write = None;
                if !self.write_enabled {
                    return vec![Action::Reply(status_reply(Op::Write, Status::Eperm, &meta.id, bel))];
                }
                self.write = Some(WriteState::new(&meta, self.max_write));
                Vec::new()
            }
            Op::Wdata => {
                let Some(state) = self.write.as_mut() else {
                    return Vec::new();
                };
                if meta.mime.is_empty() {
                    return self.commit(bel);
                }
                match state.data(&meta, payload.as_bytes()) {
                    Ok(()) => Vec::new(),
                    Err(WriteError::TooLarge) => self.finish(Status::Efbig, bel),
                    Err(WriteError::Invalid) => self.finish(Status::Einval, bel),
                }
            }
            Op::Walias => {
                let Some(state) = self.write.as_mut() else {
                    return Vec::new();
                };
                if meta.mime.is_empty() {
                    return self.finish(Status::Einval, bel);
                }
                match state.alias(&meta, payload.as_bytes()) {
                    Ok(()) => Vec::new(),
                    Err(Invalid) => self.finish(Status::Einval, bel),
                }
            }
        }
    }

    fn commit(&mut self, bel: bool) -> Vec<Action> {
        let Some(mut state) = self.write.take() else {
            return Vec::new();
        };
        let Ok(c) = state.commit() else {
            return vec![Action::Reply(status_reply(Op::Write, Status::Einval, &state.id, bel))];
        };
        let pw = if c.name.is_empty() { String::new() } else { c.pw };
        let granted = self.grants.lock().consume(&pw, Dir::Write);
        vec![Action::Write(WriteRequest {
            primary: c.primary,
            id: c.id,
            pw,
            name: c.name,
            contents: c.contents,
            granted,
            bel,
        })]
    }

    fn read(&mut self, meta: &Meta, payload: &str, bel: bool) -> Vec<Action> {
        // 풀 수 없는 요청은 답 없이 버린다(읽기에는 오류 상태가 없다)
        let Some(bytes) = b64_decode(payload.as_bytes()) else {
            return Vec::new();
        };
        let Ok(text) = String::from_utf8(bytes) else {
            return Vec::new();
        };
        let mut mimes = Vec::new();
        let mut list = false;
        for m in text
            .split(|c: char| matches!(c, ' ' | '\t'..='\r'))
            .filter(|s| !s.is_empty())
        {
            if m == TARGETS_MIME {
                list = true;
            } else if mimes.len() < MAX_READ_MIMES {
                mimes.push(m.to_string());
            }
        }
        let pw = if meta.name.is_empty() {
            String::new()
        } else {
            meta.pw.clone()
        };
        // 목록만 묻는 요청은 허가를 쓰지 않는다(붙여넣기 이벤트의 일회용 암호가 뒤따르는 데이터 읽기 전에 닳지 않게)
        let granted = !mimes.is_empty() && self.grants.lock().consume(&pw, Dir::Read);
        vec![Action::Read(ReadRequest {
            primary: meta.primary,
            id: meta.id.clone(),
            mimes,
            list,
            pw,
            name: meta.name.clone(),
            granted,
            bel,
        })]
    }
}
