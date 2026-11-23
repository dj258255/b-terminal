//! Glyph Protocol(APC `ESC _ 25a1 ; 동사 … ESC \`): 응용이 터미널에 글리프를 등록하고 글자가 덮였는지 묻는다(#779).
//!
//! Ghostty 1.4.0 앱이 하는 만큼만 한다. 등록을 받아 검사하고 칸(세션)마다 저장하고 답하지만 **그리지는 않는다**.
//! Ghostty도 렌더러와 글꼴이 글로서리를 부르지 않고(`src/renderer`·`src/font`), 래스터라이저는 시험에서만 쓰며, `width`도 엔진의
//! 글자 폭에 쓰이지 않는다. 옮긴 곳(Ghostty 1.4.0 `src/` 아래):
//! - `terminal/apc/glyph/request.zig`: 요청 해석과 옵션 읽기(`optionValue`는 같은 키가 여럿이면 마지막 값)
//! - `terminal/apc/glyph/response.zig`: 답 바이트(`formatWire`)
//! - `terminal/apc/glyph/execute.zig`: 동사별 실행과 오류 `reason`
//! - `terminal/apc/glyph/Glossary.zig`: PUA만, 1024개 상한(가장 오래된 것부터 밀려남), 같은 cp는 덮어쓰고 가장 새것이 됨
//! - `font/opentype/glyf.zig`: 단순 `glyf` 디코더(합성·힌팅은 거절). `terminal/fraction.zig`: `pad` 값
//!
//! 믿지 않는 출력이 메모리와 답을 만드는 입구라 상한이 셋이다: 요청 한 줄 1MiB(`apc.zig` 409줄, 스캐너가 넘기기 전에 자른다),
//! 등록 1024개, 디코더의 할당 한 번 64KiB. 답에는 요청의 글을 그대로 넣지 않고 숫자·고정 이름으로 다시 쓴다(ADR-078)

/// 요청 한 줄(식별자 `25a1;` 뒤)의 상한. Ghostty `CommandParser.init(alloc, 1024 * 1024)`
pub const MAX_REQUEST: usize = 1024 * 1024;
/// 칸 하나가 가질 수 있는 등록 수(Glossary.zig `max_entries`)
pub const MAX_ENTRIES: usize = 1024;
/// 디코더가 한 번에 할당할 수 있는 크기와 base64를 푼 자료의 상한(`max_glyf_allocation`, `max_payload_size`)
pub const MAX_ALLOC: usize = 64 * 1024;
/// 디코더가 쓰는 점 하나의 크기(`Glyf.Outline.Point`: i32 둘과 bool, 정렬 4바이트)
const POINT_BYTES: usize = 12;

// ---- 요청 해석(request.zig) ----

/// 해석한 요청. 옵션은 쓸 때 읽는다(Ghostty처럼 게으르게)
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Request {
    Support,
    Query(Vec<u8>),
    Register(Register),
    Clear(Vec<u8>),
}

/// 등록 요청: 옵션 구간 `raw[2..payload_idx]`과 마지막 `;` 뒤의 base64 자료
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Register {
    raw: Vec<u8>,
    payload_idx: usize,
}

/// 요청 꼴이 틀렸다(답하지 않는다)
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct InvalidFormat;

impl Request {
    /// `25a1;` 뒤의 바이트를 해석한다. Ghostty `CommandParser.complete` + `Request.parse`:
    /// 한 글자뿐이면 `;`를 붙이고, 둘째 바이트가 `;`가 아니면 꼴이 틀린 것이다
    pub fn parse(data: &[u8]) -> Result<Request, InvalidFormat> {
        let mut raw = data.to_vec();
        if raw.len() == 1 {
            raw.push(b';');
        }
        if raw.len() < 2 || raw[1] != b';' {
            return Err(InvalidFormat);
        }
        match raw[0] {
            b's' => Ok(Request::Support),
            b'q' => Ok(Request::Query(raw)),
            b'r' => {
                // 마지막 `;`가 자료의 앞이다. 첫 `;`(동사 뒤) 하나뿐이면 자료가 없는 꼴이라 틀렸다
                let payload_idx = raw.iter().rposition(|&b| b == b';').ok_or(InvalidFormat)?;
                if payload_idx <= 1 {
                    return Err(InvalidFormat);
                }
                Ok(Request::Register(Register { raw, payload_idx }))
            }
            b'c' => Ok(Request::Clear(raw)),
            _ => Err(InvalidFormat),
        }
    }
}

/// `key=value`가 여러 개면 마지막 값(request.zig `optionValue`)
fn option_value<'a>(raw: &'a [u8], key: &str) -> Option<&'a [u8]> {
    let mut found = None;
    for seg in raw.split(|&b| b == b';') {
        if let Some(eq) = seg.iter().position(|&b| b == b'=') {
            if &seg[..eq] == key.as_bytes() {
                found = Some(&seg[eq + 1..]);
            }
        }
    }
    found
}

/// Zig `std.fmt.parseInt`의 읽기: 앞의 `+`·`-`(음수 부호는 값이 0일 때만 통과), 숫자 사이의 `_`, 그 밖의 글자는 거절.
/// 부호 없는 형이라 `max`를 넘으면 거절한다
fn parse_int(value: &[u8], radix: u32, max: u64) -> Option<u64> {
    let (neg, digits) = match value.first()? {
        b'+' => (false, &value[1..]),
        b'-' => (true, &value[1..]),
        _ => (false, value),
    };
    if digits.is_empty() || digits[0] == b'_' || digits[digits.len() - 1] == b'_' {
        return None;
    }
    let mut x: u64 = 0;
    let mut prev_us = false;
    for &c in digits {
        if c == b'_' {
            if prev_us {
                return None;
            }
            prev_us = true;
            continue;
        }
        prev_us = false;
        let d = (c as char).to_digit(radix)? as u64;
        x = x.checked_mul(radix as u64)?.checked_add(d)?;
        if x > max {
            return None;
        }
    }
    if neg && x != 0 {
        return None;
    }
    Some(x)
}

fn parse_cp(value: &[u8]) -> Option<u32> {
    parse_int(value, 16, 0x1f_ffff).map(|v| v as u32)
}

/// `terminal/fraction.zig` `parse`: `[0, 1]`의 소수. 지수·16진 꼴은 받지 않고, 15자리까지만 센다
fn parse_fraction(value: &[u8]) -> Option<f64> {
    let (neg, v) = match value.first() {
        Some(b'+') => (false, &value[1..]),
        Some(b'-') => (true, &value[1..]),
        _ => (false, value),
    };
    let mut i = 0;
    let mut int_part = 0f64;
    while i < v.len() && v[i] != b'.' {
        if !v[i].is_ascii_digit() {
            return None;
        }
        int_part = int_part * 10.0 + f64::from(v[i] - b'0');
        i += 1;
    }
    let mut digits = i;
    let mut frac = 0u64;
    let mut scale = 1u64;
    if i < v.len() {
        i += 1;
        while i < v.len() {
            if !v[i].is_ascii_digit() {
                return None;
            }
            if scale < 1_000_000_000_000_000 {
                frac = frac * 10 + u64::from(v[i] - b'0');
                scale *= 10;
            }
            digits += 1;
            i += 1;
        }
    }
    if digits == 0 {
        return None;
    }
    let magnitude = int_part + frac as f64 / scale as f64;
    let result = if neg { -magnitude } else { magnitude };
    if (0.0..=1.0).contains(&result) {
        Some(result)
    } else {
        None
    }
}

/// 등록 자료의 형식
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Format {
    Glyf,
    ColrV0,
    ColrV1,
}

/// 답을 얼마나 할지: 0 없음, 1 성공·실패(기본), 2 실패만. 읽히지 않는 값은 1
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Reply {
    None,
    All,
    Failures,
}

/// 글자 칸 수(`width`): 1 또는 2
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Width {
    Narrow = 1,
    Wide = 2,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Size {
    Height,
    Advance,
    Contain,
    Cover,
    Stretch,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AlignH {
    Start,
    Center,
    End,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AlignV {
    Start,
    Center,
    End,
    Baseline,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Align {
    pub horizontal: AlignH,
    pub vertical: AlignV,
}

impl Default for Align {
    fn default() -> Self {
        Align {
            horizontal: AlignH::Center,
            vertical: AlignV::Center,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct Pad {
    pub top: f64,
    pub right: f64,
    pub bottom: f64,
    pub left: f64,
}

fn parse_align(value: &[u8]) -> Option<Align> {
    let mut it = value.split(|&b| b == b',');
    let h = match it.next()? {
        b"start" => AlignH::Start,
        b"center" => AlignH::Center,
        b"end" => AlignH::End,
        _ => return None,
    };
    let v = match it.next()? {
        b"start" => AlignV::Start,
        b"center" => AlignV::Center,
        b"end" => AlignV::End,
        b"baseline" => AlignV::Baseline,
        _ => return None,
    };
    if it.next().is_some() {
        return None;
    }
    Some(Align {
        horizontal: h,
        vertical: v,
    })
}

/// `pad=위,오른,아래,왼`. 합이 1 이상이면(퇴화) 안쪽 여백 없음으로 본다(명세 8.5.2)
fn parse_pad(value: &[u8]) -> Option<Pad> {
    let mut it = value.split(|&b| b == b',');
    let top = parse_fraction(it.next()?)?;
    let right = parse_fraction(it.next()?)?;
    let bottom = parse_fraction(it.next()?)?;
    let left = parse_fraction(it.next()?)?;
    if it.next().is_some() {
        return None;
    }
    if left + right >= 1.0 || top + bottom >= 1.0 {
        return Some(Pad::default());
    }
    Some(Pad {
        top,
        right,
        bottom,
        left,
    })
}

impl Register {
    fn options(&self) -> &[u8] {
        &self.raw[2..self.payload_idx]
    }

    /// 마지막 `;` 뒤의 base64 자료(없으면 빈 글)
    pub fn payload(&self) -> &[u8] {
        &self.raw[self.payload_idx + 1..]
    }

    pub fn cp(&self) -> Option<u32> {
        parse_cp(option_value(self.options(), "cp")?)
    }

    /// 빠졌으면 `glyf`, 알 수 없는 이름이면 `None`
    pub fn format(&self) -> Option<Format> {
        match option_value(self.options(), "fmt") {
            None => Some(Format::Glyf),
            Some(b"glyf") => Some(Format::Glyf),
            Some(b"colrv0") => Some(Format::ColrV0),
            Some(b"colrv1") => Some(Format::ColrV1),
            Some(_) => None,
        }
    }

    pub fn reply(&self) -> Reply {
        match option_value(self.options(), "reply") {
            Some(b"0") => Reply::None,
            Some(b"2") => Reply::Failures,
            _ => Reply::All,
        }
    }

    fn u32_option(&self, key: &str) -> Option<u32> {
        parse_int(option_value(self.options(), key)?, 10, u64::from(u32::MAX)).map(|v| v as u32)
    }

    /// 빠졌으면 1000
    pub fn upm(&self) -> Option<u32> {
        match option_value(self.options(), "upm") {
            None => Some(1000),
            Some(_) => self.u32_option("upm"),
        }
    }

    /// 빠졌으면 `upm`
    fn aw_or_lh(&self, key: &str) -> Option<u32> {
        match option_value(self.options(), key) {
            None => self.upm(),
            Some(_) => self.u32_option(key),
        }
    }

    pub fn width(&self) -> Option<Width> {
        match option_value(self.options(), "width") {
            None | Some(b"1") => Some(Width::Narrow),
            Some(b"2") => Some(Width::Wide),
            Some(_) => None,
        }
    }

    pub fn size(&self) -> Option<Size> {
        match option_value(self.options(), "size") {
            None | Some(b"height") => Some(Size::Height),
            Some(b"advance") => Some(Size::Advance),
            Some(b"contain") => Some(Size::Contain),
            Some(b"cover") => Some(Size::Cover),
            Some(b"stretch") => Some(Size::Stretch),
            Some(_) => None,
        }
    }

    pub fn align(&self) -> Option<Align> {
        match option_value(self.options(), "align") {
            None => Some(Align::default()),
            Some(v) => parse_align(v),
        }
    }

    pub fn pad(&self) -> Option<Pad> {
        match option_value(self.options(), "pad") {
            None => Some(Pad::default()),
            Some(v) => parse_pad(v),
        }
    }
}

fn clear_cp(raw: &[u8]) -> Option<u32> {
    parse_cp(option_value(&raw[2..], "cp")?)
}

fn clear_has_cp(raw: &[u8]) -> bool {
    option_value(&raw[2..], "cp").is_some()
}

// ---- 답(response.zig) ----

/// 등록 오류의 `reason` 이름(고정 글. 요청의 글은 들어가지 않는다)
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Reason {
    OutOfNamespace,
    CompositeUnsupported,
    HintingUnsupported,
    MalformedPayload,
    PayloadTooLarge,
}

impl Reason {
    pub fn name(self) -> &'static str {
        match self {
            Reason::OutOfNamespace => "out_of_namespace",
            Reason::CompositeUnsupported => "composite_unsupported",
            Reason::HintingUnsupported => "hinting_unsupported",
            Reason::MalformedPayload => "malformed_payload",
            Reason::PayloadTooLarge => "payload_too_large",
        }
    }
}

/// 실행 결과. `wire`가 PTY로 갈 바이트를 만든다
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Response {
    /// 지원하는 형식. Ghostty 앱은 `glyf`만 알린다(`execute.zig` `supported_formats`)
    Support {
        glyf: bool,
    },
    /// `glossary`는 이 칸의 등록이 덮는가. `system`은 앱이 채우지 않아 늘 뺀다
    Query {
        cp: u32,
        glossary: bool,
    },
    Register {
        cp: u32,
        status: u8,
        reason: Option<Reason>,
    },
    Clear {
        status: u8,
        reason: Option<&'static str>,
    },
}

impl Response {
    /// `ESC _ 25a1 ; 동사 ; … ESC \`. cp는 소문자 16진수(앞 0 없음), status는 10진수로 다시 쓴다
    pub fn wire(&self) -> Vec<u8> {
        let mut s = String::from("\x1b_25a1;");
        match *self {
            Response::Support { glyf } => {
                s.push_str("s;fmt=");
                if glyf {
                    s.push_str("glyf");
                }
            }
            Response::Query { cp, glossary } => {
                s.push_str(&format!("q;cp={cp:x};status="));
                if glossary {
                    s.push_str("glossary");
                }
            }
            Response::Register { cp, status, reason } => {
                s.push_str(&format!("r;cp={cp:x};status={status}"));
                if let Some(r) = reason {
                    s.push_str(";reason=");
                    s.push_str(r.name());
                }
            }
            Response::Clear { status, reason } => {
                s.push_str(&format!("c;status={status}"));
                if let Some(r) = reason {
                    s.push_str(";reason=");
                    s.push_str(r);
                }
            }
        }
        s.push_str("\x1b\\");
        s.into_bytes()
    }
}

// ---- glyf 디코더(font/opentype/glyf.zig) ----

/// 디코드한 윤곽. 점은 등록한 `upm` 좌표계(Y 위쪽)이다
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Outline {
    /// 윤곽마다 마지막 점의 번호(포함)
    pub contours: Vec<u16>,
    pub points: Vec<Point>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Point {
    pub x: i32,
    pub y: i32,
    pub on_curve: bool,
}

impl Outline {
    /// `index`번 윤곽의 점들(`Glyf.Outline.contour`)
    pub fn contour(&self, index: usize) -> &[Point] {
        let start = if index == 0 {
            0
        } else {
            usize::from(self.contours[index - 1]) + 1
        };
        &self.points[start..=usize::from(self.contours[index])]
    }
}

/// 디코드 오류. 이름은 Ghostty `DecodeError`와 같다
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DecodeError {
    /// base64나 glyf 꼴이 틀렸다(자료가 모자람, 끝점 순서, 점 수 초과, 좌표 넘침 포함)
    Malformed,
    /// 풀면 64KiB가 넘거나 디코더가 한 번에 64KiB를 넘게 할당해야 한다
    TooLarge,
    Composite,
    Hinting,
}

/// 앞에서부터 읽는 글: 모자라면 `Malformed`(Zig `EndOfStream`)
struct Cursor<'a> {
    data: &'a [u8],
    pos: usize,
}

impl Cursor<'_> {
    fn byte(&mut self) -> Result<u8, DecodeError> {
        let b = *self.data.get(self.pos).ok_or(DecodeError::Malformed)?;
        self.pos += 1;
        Ok(b)
    }

    fn u16(&mut self) -> Result<u16, DecodeError> {
        Ok(u16::from_be_bytes([self.byte()?, self.byte()?]))
    }

    fn i16(&mut self) -> Result<i16, DecodeError> {
        Ok(self.u16()? as i16)
    }
}

/// 한 번의 할당이 한도를 넘는가(`LimitedAllocator`는 합계가 아니라 요청마다 본다)
fn check_alloc(bytes: usize) -> Result<(), DecodeError> {
    if bytes > MAX_ALLOC {
        Err(DecodeError::TooLarge)
    } else {
        Ok(())
    }
}

/// 단순 glyf 한 개를 디코드한다(`Glyf.Entry.decode`). 머리 10바이트 뒤가 `data`이다
pub fn decode_glyf(entry: &[u8]) -> Result<Outline, DecodeError> {
    if entry.len() < 10 {
        return Err(DecodeError::Malformed);
    }
    let number_of_contours = i16::from_be_bytes([entry[0], entry[1]]);
    // 음수는 합성 글리프
    if number_of_contours < 0 {
        return Err(DecodeError::Composite);
    }
    let data = &entry[10..];
    let num_contours = number_of_contours as usize;
    // 윤곽이 없는 글리프는 머리뿐일 수 있다
    if num_contours == 0 && data.len() < 2 {
        return Ok(Outline {
            contours: Vec::new(),
            points: Vec::new(),
        });
    }
    let mut r = Cursor { data, pos: 0 };
    check_alloc(num_contours * 2)?;
    if num_contours == 0 {
        if r.u16()? > 0 {
            return Err(DecodeError::Hinting);
        }
        return Ok(Outline {
            contours: Vec::new(),
            points: Vec::new(),
        });
    }
    let mut contours = Vec::with_capacity(num_contours);
    let mut prev: i32 = -1;
    for _ in 0..num_contours {
        let index = r.u16()?;
        if i32::from(index) <= prev {
            return Err(DecodeError::Malformed);
        }
        prev = i32::from(index);
        contours.push(index);
    }
    let point_count = usize::from(contours[num_contours - 1]) + 1;
    // 힌팅 명령은 받지 않는다
    if r.u16()? > 0 {
        return Err(DecodeError::Hinting);
    }
    // 점 배열이 가장 큰 할당이다. 읽기 전에 본다
    check_alloc(point_count * POINT_BYTES)?;
    // 깃발(점마다 하나, 반복 표시 포함)
    let mut flags = Vec::with_capacity(point_count);
    while flags.len() < point_count {
        let flag = r.byte()?;
        flags.push(flag);
        if flag & 0x08 != 0 {
            let repeat = usize::from(r.byte()?);
            if flags.len() + repeat > point_count {
                return Err(DecodeError::Malformed);
            }
            for _ in 0..repeat {
                flags.push(flag);
            }
        }
    }
    let mut points = vec![
        Point {
            x: 0,
            y: 0,
            on_curve: false
        };
        point_count
    ];
    // x는 짧은 값(0x02, 부호 0x10), 같음(0x10만), 16비트 차이. y는 0x04·0x20
    let mut x: i32 = 0;
    for (flag, p) in flags.iter().zip(points.iter_mut()) {
        let dx = if flag & 0x02 != 0 {
            let v = i32::from(r.byte()?);
            if flag & 0x10 != 0 { v } else { -v }
        } else if flag & 0x10 == 0 {
            i32::from(r.i16()?)
        } else {
            0
        };
        x = x.checked_add(dx).ok_or(DecodeError::Malformed)?;
        p.x = x;
    }
    let mut y: i32 = 0;
    for (flag, p) in flags.iter().zip(points.iter_mut()) {
        let dy = if flag & 0x04 != 0 {
            let v = i32::from(r.byte()?);
            if flag & 0x20 != 0 { v } else { -v }
        } else if flag & 0x20 == 0 {
            i32::from(r.i16()?)
        } else {
            0
        };
        y = y.checked_add(dy).ok_or(DecodeError::Malformed)?;
        p.y = y;
        p.on_curve = flag & 0x01 != 0;
    }
    Ok(Outline { contours, points })
}

/// 표준 base64(채움 필수, 끝 비트는 0)를 푼다. Zig `std.base64.standard.Decoder`와 같게 엄격하다.
/// 풀기 전에 크기를 먼저 셈해 64KiB를 넘으면 `TooLarge`(글이 틀려도 크기가 먼저)
pub fn decode_payload(src: &[u8]) -> Result<Vec<u8>, DecodeError> {
    if !src.len().is_multiple_of(4) {
        return Err(DecodeError::Malformed);
    }
    let pads = src.iter().rev().take(2).take_while(|&&b| b == b'=').count();
    let size = src.len() / 4 * 3 - pads;
    if size > MAX_ALLOC {
        return Err(DecodeError::TooLarge);
    }
    fn value(b: u8) -> Option<u32> {
        match b {
            b'A'..=b'Z' => Some(u32::from(b - b'A')),
            b'a'..=b'z' => Some(u32::from(b - b'a') + 26),
            b'0'..=b'9' => Some(u32::from(b - b'0') + 52),
            b'+' => Some(62),
            b'/' => Some(63),
            _ => None,
        }
    }
    let mut out = Vec::with_capacity(size);
    let mut acc: u32 = 0;
    let mut acc_len: u32 = 0;
    let mut i = 0;
    while i < src.len() {
        let Some(v) = value(src[i]) else { break };
        acc = (acc << 6) | v;
        acc_len += 6;
        if acc_len >= 8 {
            acc_len -= 8;
            out.push((acc >> acc_len) as u8);
            acc &= (1 << acc_len) - 1;
        }
        i += 1;
    }
    // 남은 비트가 4보다 많거나 0이 아니면 정규 base64가 아니다
    if acc_len > 4 || acc != 0 {
        return Err(DecodeError::Malformed);
    }
    // 채움 글자는 필요한 개수만큼 끝에만 온다
    let rest = &src[i..];
    if rest.len() != (acc_len / 2) as usize || rest.iter().any(|&b| b != b'=') {
        return Err(DecodeError::Malformed);
    }
    Ok(out)
}

// ---- 글로서리(Glossary.zig) ----

/// 등록 하나. 윤곽과 등록한 값들을 그대로 둔다(그리지는 않는다)
#[derive(Debug, Clone, PartialEq)]
pub struct Entry {
    pub outline: Outline,
    pub units_per_em: u32,
    pub advance_width: u32,
    pub line_height: u32,
    pub width: Width,
    pub size: Size,
    pub align: Align,
    pub pad: Pad,
}

/// 등록이 안 된 까닭
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum EntryError {
    /// 옵션 값이 틀렸거나 지원하지 않는 형식이다(답은 `malformed_payload`)
    Invalid,
    Decode(DecodeError),
}

impl Entry {
    /// 옵션을 검사하고 자료를 디코드한다(`Glossary.Entry.init`). 순서도 같다: 옵션 → 자료
    fn init(reg: &Register) -> Result<Entry, EntryError> {
        let fmt = reg.format().ok_or(EntryError::Invalid)?;
        let units_per_em = reg.upm().ok_or(EntryError::Invalid)?;
        let advance_width = reg.aw_or_lh("aw").ok_or(EntryError::Invalid)?;
        let line_height = reg.aw_or_lh("lh").ok_or(EntryError::Invalid)?;
        if units_per_em == 0 || advance_width == 0 || line_height == 0 {
            return Err(EntryError::Invalid);
        }
        let width = reg.width().ok_or(EntryError::Invalid)?;
        let size = reg.size().ok_or(EntryError::Invalid)?;
        let align = reg.align().ok_or(EntryError::Invalid)?;
        let pad = reg.pad().ok_or(EntryError::Invalid)?;
        let outline = match fmt {
            Format::Glyf => {
                let data = decode_payload(reg.payload()).map_err(EntryError::Decode)?;
                decode_glyf(&data).map_err(EntryError::Decode)?
            }
            Format::ColrV0 | Format::ColrV1 => return Err(EntryError::Invalid),
        };
        Ok(Entry {
            outline,
            units_per_em,
            advance_width,
            line_height,
            width,
            size,
            align,
            pad,
        })
    }
}

/// 유니코드 사용자 지정 영역 셋(BMP, 보충 A, 보충 B)
pub fn is_private_use(cp: u32) -> bool {
    (0xE000..=0xF8FF).contains(&cp) || (0xF0000..=0xFFFFD).contains(&cp) || (0x100000..=0x10FFFD).contains(&cp)
}

/// 칸 하나의 등록. 등록한 차례를 지킨다(가장 오래된 것이 앞)
#[derive(Debug, Default)]
pub struct Glossary {
    entries: Vec<(u32, Entry)>,
}

impl Glossary {
    pub fn contains(&self, cp: u32) -> bool {
        self.entries.iter().any(|(c, _)| *c == cp)
    }

    pub fn get(&self, cp: u32) -> Option<&Entry> {
        self.entries.iter().find(|(c, _)| *c == cp).map(|(_, e)| e)
    }

    pub fn len(&self) -> usize {
        self.entries.len()
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// 등록 순서대로의 cp
    pub fn codepoints(&self) -> impl Iterator<Item = u32> + '_ {
        self.entries.iter().map(|(c, _)| *c)
    }

    /// 전체 초기화(RIS) 때 모두 지운다. 메모리도 돌려준다
    pub fn clear_all(&mut self) {
        self.entries = Vec::new();
    }

    fn register(&mut self, cp: u32, entry: Entry) -> Result<(), ()> {
        if !is_private_use(cp) {
            return Err(());
        }
        // 같은 cp는 덮어쓰고 가장 새것이 된다
        if let Some(i) = self.entries.iter().position(|(c, _)| *c == cp) {
            self.entries.remove(i);
        }
        self.entries.push((cp, entry));
        if self.entries.len() > MAX_ENTRIES {
            self.entries.remove(0);
        }
        Ok(())
    }

    fn delete(&mut self, cp: u32) -> Result<(), ()> {
        if !is_private_use(cp) {
            return Err(());
        }
        if let Some(i) = self.entries.iter().position(|(c, _)| *c == cp) {
            self.entries.remove(i);
        }
        Ok(())
    }

    /// `25a1;` 뒤의 바이트 하나를 처리하고 PTY로 갈 답을 돌려준다. 해석할 수 없는 요청과 답이 없는 요청은 `None`
    pub fn handle(&mut self, data: &[u8]) -> Option<Vec<u8>> {
        let req = Request::parse(data).ok()?;
        self.execute(&req).map(|r| r.wire())
    }

    /// `execute.zig` `execute`
    pub fn execute(&mut self, req: &Request) -> Option<Response> {
        match req {
            Request::Support => Some(Response::Support { glyf: true }),
            Request::Query(raw) => {
                let cp = parse_cp(option_value(&raw[2..], "cp")?)?;
                Some(Response::Query {
                    cp,
                    glossary: self.contains(cp),
                })
            }
            Request::Register(reg) => self.register_request(reg),
            Request::Clear(raw) => Some(self.clear_request(raw)),
        }
    }

    fn register_request(&mut self, reg: &Register) -> Option<Response> {
        let reply = reg.reply();
        let result = match reg.cp() {
            None => Err(Reason::MalformedPayload),
            Some(cp) => match Entry::init(reg) {
                Err(EntryError::Invalid) => Err(Reason::MalformedPayload),
                Err(EntryError::Decode(e)) => Err(match e {
                    DecodeError::Malformed => Reason::MalformedPayload,
                    DecodeError::TooLarge => Reason::PayloadTooLarge,
                    DecodeError::Composite => Reason::CompositeUnsupported,
                    DecodeError::Hinting => Reason::HintingUnsupported,
                }),
                // 자료까지 맞아도 PUA가 아니면 마지막에 거절한다(Glossary.register가 entry를 받은 뒤 본다)
                Ok(entry) => self
                    .register(cp, entry)
                    .map(|()| cp)
                    .map_err(|()| Reason::OutOfNamespace),
            },
        };
        match result {
            Ok(cp) => match reply {
                Reply::All => Some(Response::Register {
                    cp,
                    status: 0,
                    reason: None,
                }),
                Reply::None | Reply::Failures => None,
            },
            Err(reason) => match reply {
                Reply::None => None,
                Reply::All | Reply::Failures => Some(Response::Register {
                    cp: reg.cp().unwrap_or(0),
                    status: 1,
                    reason: Some(reason),
                }),
            },
        }
    }

    fn clear_request(&mut self, raw: &[u8]) -> Response {
        if let Some(cp) = clear_cp(raw) {
            if self.delete(cp).is_err() {
                return Response::Clear {
                    status: 1,
                    reason: Some("out_of_namespace"),
                };
            }
        } else if clear_has_cp(raw) {
            return Response::Clear {
                status: 1,
                reason: Some("malformed_payload"),
            };
        } else {
            self.clear_all();
        }
        Response::Clear {
            status: 0,
            reason: None,
        }
    }
}

// ---- 시험 ----
// 옮긴 곳과 줄 번호는 Ghostty 1.4.0 `src/` 기준. 글꼴 파일(Cozette·JetBrains Mono)을 읽는 시험은 글꼴을 싣지 않아
// 같은 오류를 내는 손으로 만든 자료로 바꿨다(각 시험에 적음)

#[cfg(test)]
mod tests {
    use super::*;

    fn parse(data: &str) -> Request {
        Request::parse(data.as_bytes()).unwrap_or_else(|_| panic!("{data:?}"))
    }

    fn reg(data: &str) -> Register {
        match parse(data) {
            Request::Register(r) => r,
            other => panic!("{other:?}"),
        }
    }

    // ---- request.zig ----

    /// request.zig 612줄 `support command`
    #[test]
    fn support_command() {
        assert_eq!(parse("s"), Request::Support);
    }

    /// request.zig 621줄 `query command`
    #[test]
    fn query_command() {
        match parse("q;cp=E0A0") {
            Request::Query(raw) => assert_eq!(clear_cp(&raw), Some(0xE0A0)),
            other => panic!("{other:?}"),
        }
    }

    /// request.zig 631줄 `register command with payload`
    #[test]
    fn register_command_with_payload() {
        let r = reg("r;cp=e0a0;fmt=glyf;upm=1000;reply=2;QQ==");
        assert_eq!(r.cp(), Some(0xE0A0));
        assert_eq!(r.format(), Some(Format::Glyf));
        assert_eq!(r.upm(), Some(1000));
        assert_eq!(r.reply(), Reply::Failures);
        assert_eq!(r.payload(), b"QQ==");
    }

    /// request.zig 648줄 `register command with sizing and placement options`
    #[test]
    fn register_command_with_sizing_and_placement_options() {
        let r =
            reg("r;cp=e0a0;upm=2048;aw=1024;lh=1536;width=2;size=contain;align=end,baseline;pad=0.1,0.2,0.3,0.4;QQ==");
        assert_eq!(r.upm(), Some(2048));
        assert_eq!(r.aw_or_lh("aw"), Some(1024));
        assert_eq!(r.aw_or_lh("lh"), Some(1536));
        assert_eq!(r.width(), Some(Width::Wide));
        assert_eq!(r.size(), Some(Size::Contain));
        assert_eq!(
            r.align(),
            Some(Align {
                horizontal: AlignH::End,
                vertical: AlignV::Baseline
            })
        );
        assert_eq!(
            r.pad(),
            Some(Pad {
                top: 0.1,
                right: 0.2,
                bottom: 0.3,
                left: 0.4
            })
        );
        assert_eq!(r.payload(), b"QQ==");
    }

    /// request.zig 676줄 `register option defaults`와 692줄 `register command defaults`
    #[test]
    fn register_command_defaults() {
        let r = reg("r;cp=e0a0;QQ==");
        assert_eq!(r.cp(), Some(0xE0A0));
        assert_eq!(r.format(), Some(Format::Glyf));
        assert_eq!(r.upm(), Some(1000));
        assert_eq!(r.aw_or_lh("aw"), Some(1000));
        assert_eq!(r.aw_or_lh("lh"), Some(1000));
        assert_eq!(r.width(), Some(Width::Narrow));
        assert_eq!(r.size(), Some(Size::Height));
        assert_eq!(r.align(), Some(Align::default()));
        assert_eq!(r.pad(), Some(Pad::default()));
        assert_eq!(r.reply(), Reply::All);
        // cp는 기본이 없다
        assert_eq!(reg("r;QQ==;QQ==").cp(), None);
    }

    /// request.zig 711줄 `register command aw and lh default to upm`
    #[test]
    fn aw_and_lh_default_to_upm() {
        let r = reg("r;cp=e0a0;upm=2048;QQ==");
        assert_eq!(r.aw_or_lh("aw"), Some(2048));
        assert_eq!(r.aw_or_lh("lh"), Some(2048));
    }

    /// request.zig 722줄 `register command invalid sizing and placement options`
    #[test]
    fn invalid_sizing_and_placement_options() {
        let r = reg("r;cp=e0a0;width=3;size=invalid;align=center,middle;pad=0,1.2,0,0;QQ==");
        assert_eq!(r.width(), None);
        assert_eq!(r.size(), None);
        assert_eq!(r.align(), None);
        assert_eq!(r.pad(), None);
    }

    /// request.zig 738줄 `register command degenerate padding defaults to no padding`
    #[test]
    fn degenerate_padding_defaults_to_no_padding() {
        assert_eq!(reg("r;cp=e0a0;pad=0.4,0.2,0.6,0.1;QQ==").pad(), Some(Pad::default()));
    }

    /// request.zig 751줄 `register command invalid reply falls back to reply=1`
    #[test]
    fn invalid_reply_falls_back_to_all() {
        assert_eq!(reg("r;cp=e0a0;reply=9;QQ==").reply(), Reply::All);
    }

    /// request.zig 761줄 `register command duplicate options use the last value`
    #[test]
    fn duplicate_options_use_the_last_value() {
        assert_eq!(reg("r;cp=e0a0;reply=1;reply=2;QQ==").reply(), Reply::Failures);
    }

    /// request.zig 771줄 `register command with invalid payload`
    #[test]
    fn invalid_payload_is_kept_as_is() {
        let r = reg("r;cp=e0a0;fmt=glyf;%%%not-base64%%%");
        assert_eq!(r.cp(), Some(0xE0A0));
        assert_eq!(r.format(), Some(Format::Glyf));
        assert_eq!(r.payload(), b"%%%not-base64%%%");
    }

    /// request.zig 786줄 `register command rejects missing payload separator`
    #[test]
    fn register_rejects_missing_payload_separator() {
        for data in ["r", "r;cp=e0a0", "r;foo"] {
            assert_eq!(Request::parse(data.as_bytes()), Err(InvalidFormat), "{data}");
        }
    }

    /// request.zig 797줄 `register decodes glyf payload`
    #[test]
    fn register_decodes_glyf_payload() {
        let r = reg("r;cp=e0a0;AAAAAAAAAAAAAA==");
        let data = decode_payload(r.payload()).unwrap();
        let outline = decode_glyf(&data).unwrap();
        assert_eq!(outline.points.len(), 0);
        assert_eq!(outline.contours.len(), 0);
    }

    /// request.zig 813줄 `register rejects malformed glyf payload`
    #[test]
    fn register_rejects_malformed_payload() {
        let r = reg("r;cp=e0a0;%%%not-base64%%%");
        assert_eq!(decode_payload(r.payload()), Err(DecodeError::Malformed));
    }

    /// request.zig 825줄 `register response without payload`: 요청은 마지막 조각을 늘 자료로 본다
    #[test]
    fn register_response_text_is_treated_as_payload() {
        let r = reg("r;cp=E0A0;status=4;reason=out_of_namespace");
        assert_eq!(r.cp(), Some(0xE0A0));
        assert_eq!(r.payload(), b"reason=out_of_namespace");
    }

    /// request.zig 841줄 `clear command`, 852줄 `clear command tracks malformed cp presence`
    #[test]
    fn clear_command_tracks_cp_presence() {
        match parse("c;cp=e0a0") {
            Request::Clear(raw) => {
                assert!(clear_has_cp(&raw));
                assert_eq!(clear_cp(&raw), Some(0xE0A0));
            }
            other => panic!("{other:?}"),
        }
        for data in ["c;cp=zz", "c;cp=", "c;cp=200000"] {
            match parse(data) {
                Request::Clear(raw) => {
                    assert!(clear_has_cp(&raw), "{data}");
                    assert_eq!(clear_cp(&raw), None, "{data}");
                }
                other => panic!("{other:?}"),
            }
        }
    }

    /// request.zig 865줄 `invalid command`
    #[test]
    fn invalid_command() {
        assert_eq!(Request::parse(b"x"), Err(InvalidFormat));
        assert_eq!(Request::parse(b""), Err(InvalidFormat));
        assert_eq!(Request::parse(b"sx"), Err(InvalidFormat));
    }

    /// Zig `parseInt`이 받는 꼴(앞 부호, 숫자 사이 `_`)과 거절하는 꼴. 우리가 직접 정한 경계라 시험으로 고정한다
    #[test]
    fn cp_reads_like_zig_parse_int() {
        assert_eq!(parse_cp(b"E0A0"), Some(0xE0A0));
        assert_eq!(parse_cp(b"000e0a0"), Some(0xE0A0));
        assert_eq!(parse_cp(b"+e0a0"), Some(0xE0A0));
        assert_eq!(parse_cp(b"e0_a0"), Some(0xE0A0));
        assert_eq!(parse_cp(b"-0"), Some(0));
        assert_eq!(parse_cp(b"-1"), None);
        assert_eq!(parse_cp(b"_e0"), None);
        assert_eq!(parse_cp(b"e0_"), None);
        assert_eq!(parse_cp(b"e__0"), None);
        assert_eq!(parse_cp(b""), None);
        assert_eq!(parse_cp(b"zz"), None);
        assert_eq!(parse_cp(b"1fffff"), Some(0x1f_ffff));
        assert_eq!(parse_cp(b"200000"), None);
        assert_eq!(parse_cp(b"ffffffffffffffffffffff"), None);
    }

    // ---- fraction.zig 시험 `parse`(79줄) ----

    #[test]
    fn fraction_parse() {
        for (s, want) in [
            ("0", Some(0.0)),
            ("1", Some(1.0)),
            ("0.5", Some(0.5)),
            ("0.25", Some(0.25)),
            ("1.0", Some(1.0)),
            ("1.000000", Some(1.0)),
            ("0.0", Some(0.0)),
            (".5", Some(0.5)),
            ("1.", Some(1.0)),
            ("00.00", Some(0.0)),
            ("+0.5", Some(0.5)),
            ("-0", Some(0.0)),
            ("-0.000", Some(0.0)),
            ("-0.5", None),
            ("-1", None),
            ("0.3", Some(0.3)),
            ("0.123456789012345", Some(0.123456789012345)),
            ("1.0000001", None),
            ("2", None),
            ("255", None),
            ("", None),
            (".", None),
            ("+", None),
            ("-", None),
            ("+.", None),
            ("abc", None),
            ("0.5x", None),
            ("0x0.8", None),
            ("0..5", None),
            ("0.5.5", None),
            (" 0.5", None),
            ("0.5 ", None),
            ("1e-1", None),
            ("nan", None),
            ("inf", None),
        ] {
            assert_eq!(parse_fraction(s.as_bytes()), want, "{s:?}");
        }
        // 15자리를 넘는 자리는 무시한다
        let long = format!("0.{}", "3".repeat(398));
        assert_eq!(parse_fraction(long.as_bytes()), parse_fraction(b"0.333333333333333"));
        assert_eq!(parse_fraction("1".repeat(400).as_bytes()), None);
    }

    // ---- response.zig ----

    /// response.zig 218줄 `support formats default to no advertised formats`, 225줄 `response support formatWire`(우리는 glyf만 알린다)와 236줄 `with no formats`
    #[test]
    fn response_support_wire() {
        assert_eq!(Response::Support { glyf: true }.wire(), b"\x1b_25a1;s;fmt=glyf\x1b\\");
        assert_eq!(Response::Support { glyf: false }.wire(), b"\x1b_25a1;s;fmt=\x1b\\");
    }

    /// response.zig 247줄 `response query formatWire`(`system`은 앱이 채우지 않는다)와 258줄 `with no coverage`
    #[test]
    fn response_query_wire() {
        assert_eq!(
            Response::Query {
                cp: 0xE0A0,
                glossary: true
            }
            .wire(),
            b"\x1b_25a1;q;cp=e0a0;status=glossary\x1b\\"
        );
        assert_eq!(
            Response::Query {
                cp: 0xE0A0,
                glossary: false
            }
            .wire(),
            b"\x1b_25a1;q;cp=e0a0;status=\x1b\\"
        );
    }

    /// response.zig 281줄 `response register success formatWire`, 292줄 `error formatWire`, 303줄 `arbitrary status formatWire`
    #[test]
    fn response_register_wire() {
        assert_eq!(
            Response::Register {
                cp: 0xE0A0,
                status: 0,
                reason: None
            }
            .wire(),
            b"\x1b_25a1;r;cp=e0a0;status=0\x1b\\"
        );
        assert_eq!(
            Response::Register {
                cp: 0xE0A0,
                status: 1,
                reason: Some(Reason::OutOfNamespace)
            }
            .wire(),
            b"\x1b_25a1;r;cp=e0a0;status=1;reason=out_of_namespace\x1b\\"
        );
        assert_eq!(
            Response::Register {
                cp: 0xE0A0,
                status: 37,
                reason: Some(Reason::PayloadTooLarge)
            }
            .wire(),
            b"\x1b_25a1;r;cp=e0a0;status=37;reason=payload_too_large\x1b\\"
        );
    }

    /// response.zig 314줄 `register reason names`
    #[test]
    fn register_reason_names() {
        assert_eq!(Reason::OutOfNamespace.name(), "out_of_namespace");
        assert_eq!(Reason::CompositeUnsupported.name(), "composite_unsupported");
        assert_eq!(Reason::HintingUnsupported.name(), "hinting_unsupported");
        assert_eq!(Reason::MalformedPayload.name(), "malformed_payload");
        assert_eq!(Reason::PayloadTooLarge.name(), "payload_too_large");
    }

    /// response.zig 326줄 `response clear formatWire`와 337줄 `response clear error formatWire`
    #[test]
    fn response_clear_wire() {
        assert_eq!(
            Response::Clear {
                status: 0,
                reason: None
            }
            .wire(),
            b"\x1b_25a1;c;status=0\x1b\\"
        );
        assert_eq!(
            Response::Clear {
                status: 1,
                reason: Some("out_of_namespace")
            }
            .wire(),
            b"\x1b_25a1;c;status=1;reason=out_of_namespace\x1b\\"
        );
    }

    // ---- glyf.zig ----

    fn put16(buf: &mut Vec<u8>, v: i16) {
        buf.extend_from_slice(&v.to_be_bytes());
    }

    fn header(buf: &mut Vec<u8>, contours: i16, x_min: i16, y_min: i16, x_max: i16, y_max: i16) {
        for v in [contours, x_min, y_min, x_max, y_max] {
            put16(buf, v);
        }
    }

    /// glyf.zig 707줄 `glyf: decode triangle`
    #[test]
    fn glyf_decode_triangle() {
        let mut buf = Vec::new();
        header(&mut buf, 1, 100, 100, 900, 900);
        put16(&mut buf, 2); // endPtsOfContours[0]
        put16(&mut buf, 0); // instructionLength
        buf.extend_from_slice(&[1, 1, 1]);
        for v in [500, -400, 800, 900, -800, 0] {
            put16(&mut buf, v);
        }
        let o = decode_glyf(&buf).unwrap();
        assert_eq!(o.contours.len(), 1);
        assert_eq!(o.points.len(), 3);
        let c = o.contour(0);
        assert_eq!(c.len(), 3);
        assert_eq!(
            c[0],
            Point {
                x: 500,
                y: 900,
                on_curve: true
            }
        );
        assert_eq!(
            c[1],
            Point {
                x: 100,
                y: 100,
                on_curve: true
            }
        );
        assert_eq!(
            c[2],
            Point {
                x: 900,
                y: 100,
                on_curve: true
            }
        );
        // Glossary.zig의 시험 자료(같은 삼각형을 base64로)도 같다
        let data = decode_payload(b"AAEAZABkA4QDhAACAAABAQEB9P5wAyADhPzgAAA=").unwrap();
        assert_eq!(decode_glyf(&data).unwrap().points, o.points);
    }

    /// glyf.zig 742줄 `glyf: decode multiple contours`
    #[test]
    fn glyf_decode_multiple_contours() {
        let mut buf = Vec::new();
        header(&mut buf, 2, 0, 0, 30, 10);
        put16(&mut buf, 1);
        put16(&mut buf, 3);
        put16(&mut buf, 0);
        buf.extend_from_slice(&[1, 1, 1, 1]);
        for dx in [0, 10, 10, 10] {
            put16(&mut buf, dx);
        }
        for dy in [0, 0, 10, 0] {
            put16(&mut buf, dy);
        }
        let o = decode_glyf(&buf).unwrap();
        assert_eq!(o.contours, vec![1, 3]);
        assert_eq!(o.points.len(), 4);
        assert_eq!(o.contour(0).len(), 2);
        assert_eq!(o.contour(1).len(), 2);
        assert_eq!(o.contour(0).as_ptr(), o.points[0..2].as_ptr());
        assert_eq!(o.contour(1).as_ptr(), o.points[2..4].as_ptr());
    }

    /// glyf.zig 771줄 `glyf: decode repeat and short vector flags`
    #[test]
    fn glyf_decode_repeat_and_short_vector_flags() {
        let mut buf = Vec::new();
        header(&mut buf, 1, 0, -16, 16, 0);
        put16(&mut buf, 3);
        put16(&mut buf, 0);
        buf.push(0x01 | 0x02 | 0x04 | 0x08 | 0x10);
        buf.push(3);
        buf.extend_from_slice(&[1, 2, 4, 8]);
        buf.extend_from_slice(&[1, 2, 4, 8]);
        let o = decode_glyf(&buf).unwrap();
        let want = [(1, -1), (3, -3), (7, -7), (15, -15)];
        assert_eq!(o.points.len(), 4);
        for (p, (x, y)) in o.points.iter().zip(want) {
            assert_eq!(*p, Point { x, y, on_curve: true });
        }
    }

    /// glyf.zig 797줄 `glyf: decode off curve and same coordinate flags`
    #[test]
    fn glyf_decode_off_curve_and_same_coordinate_flags() {
        let mut buf = Vec::new();
        header(&mut buf, 1, 0, 0, 7, 9);
        put16(&mut buf, 1);
        put16(&mut buf, 0);
        buf.push(0x10 | 0x20);
        buf.push(0x01 | 0x02 | 0x04 | 0x10 | 0x20);
        buf.push(7);
        buf.push(9);
        let o = decode_glyf(&buf).unwrap();
        assert_eq!(
            o.points[0],
            Point {
                x: 0,
                y: 0,
                on_curve: false
            }
        );
        assert_eq!(
            o.points[1],
            Point {
                x: 7,
                y: 9,
                on_curve: true
            }
        );
    }

    /// glyf.zig 820줄 `glyf: decode one-point contour`
    #[test]
    fn glyf_decode_one_point_contour() {
        let mut buf = Vec::new();
        header(&mut buf, 1, 0, 0, 0, 0);
        put16(&mut buf, 0);
        put16(&mut buf, 0);
        buf.push(0x01 | 0x10 | 0x20);
        let o = decode_glyf(&buf).unwrap();
        assert_eq!(o.points.len(), 1);
        assert_eq!(o.contours, vec![0]);
        assert_eq!(
            o.contour(0)[0],
            Point {
                x: 0,
                y: 0,
                on_curve: true
            }
        );
    }

    fn repeated_points(end_points: &[u16]) -> Vec<u8> {
        let mut buf = Vec::new();
        header(&mut buf, end_points.len() as i16, 0, 0, 0, 0);
        for &e in end_points {
            buf.extend_from_slice(&e.to_be_bytes());
        }
        put16(&mut buf, 0);
        let mut remaining = usize::from(*end_points.last().unwrap()) + 1;
        while remaining > 0 {
            let run = remaining.min(256);
            if run == 1 {
                buf.push(0x31);
            } else {
                buf.extend_from_slice(&[0x39, (run - 1) as u8]);
            }
            remaining -= run;
        }
        buf
    }

    /// glyf.zig 842줄 `glyf: decode contour ending at max point index`(65536점). 점 배열이 64KiB를 넘으므로 여기서는 거절된다.
    /// Ghostty도 `decode`는 받지만 프로토콜의 할당 한도(`Glossary.zig` `Entry init rejects glyf allocation over limit`)가 거절한다
    #[test]
    fn glyf_contour_ending_at_max_point_index_is_over_the_limit() {
        let buf = repeated_points(&[u16::MAX]);
        assert_eq!(decode_glyf(&buf), Err(DecodeError::TooLarge));
    }

    /// glyf.zig 874줄 `glyf: reject glyphs with instructions and composite glyphs`(글꼴 대신 손으로 만든 자료)
    #[test]
    fn glyf_rejects_instructions_and_composite() {
        let mut buf = Vec::new();
        header(&mut buf, 1, 0, 0, 0, 0);
        put16(&mut buf, 0);
        put16(&mut buf, 4); // instructionLength > 0
        buf.extend_from_slice(&[0, 0, 0, 0, 0x31]);
        assert_eq!(decode_glyf(&buf), Err(DecodeError::Hinting));
        let mut buf = Vec::new();
        header(&mut buf, -1, 0, 0, 0, 0);
        buf.extend_from_slice(&[0; 12]);
        assert_eq!(decode_glyf(&buf), Err(DecodeError::Composite));
    }

    /// glyf.zig 911줄 `glyf: reject truncated`(글꼴 대신: 점 자료를 한 바이트 잘랐다)
    #[test]
    fn glyf_rejects_truncated() {
        let data = decode_payload(b"AAEAZABkA4QDhAACAAABAQEB9P5wAyADhPzgAAA=").unwrap();
        for cut in 1..data.len() {
            assert_eq!(decode_glyf(&data[..cut]), Err(DecodeError::Malformed), "{cut}바이트");
        }
        assert!(decode_glyf(&data).is_ok());
    }

    /// glyf.zig 932줄 `glyf: reject endpoints out of order`
    #[test]
    fn glyf_rejects_endpoints_out_of_order() {
        let mut buf = Vec::new();
        header(&mut buf, 2, 0, 0, 0, 0);
        put16(&mut buf, 3);
        put16(&mut buf, 0); // 앞 끝점보다 크지 않다
        put16(&mut buf, 0);
        buf.extend_from_slice(&[0x31; 8]);
        assert_eq!(decode_glyf(&buf), Err(DecodeError::Malformed));
        // 같은 끝점 둘도 순서가 아니다
        let mut buf = Vec::new();
        header(&mut buf, 2, 0, 0, 0, 0);
        put16(&mut buf, 1);
        put16(&mut buf, 1);
        put16(&mut buf, 0);
        buf.extend_from_slice(&[0x31; 4]);
        assert_eq!(decode_glyf(&buf), Err(DecodeError::Malformed));
    }

    /// glyf.zig 961줄 `glyf: reject too many points`(반복 표시가 끝점이 정한 점 수를 넘는다)
    #[test]
    fn glyf_rejects_too_many_points() {
        let mut buf = Vec::new();
        header(&mut buf, 1, 0, 0, 0, 0);
        put16(&mut buf, 2); // 점 셋
        put16(&mut buf, 0);
        buf.extend_from_slice(&[0x31 | 0x08, 0xFF]);
        buf.extend_from_slice(&[0; 8]);
        assert_eq!(decode_glyf(&buf), Err(DecodeError::Malformed));
    }

    /// glyf.zig 989줄 `glyf: zero-contour glyph can be header-only`와 1009줄 `can include instruction length`
    #[test]
    fn glyf_zero_contour_glyphs() {
        let mut buf = Vec::new();
        header(&mut buf, 0, 0, 0, 0, 0);
        let o = decode_glyf(&buf).unwrap();
        assert!(o.points.is_empty() && o.contours.is_empty());
        put16(&mut buf, 0);
        let o = decode_glyf(&buf).unwrap();
        assert!(o.points.is_empty() && o.contours.is_empty());
    }

    /// glyf.zig 1028줄 `glyf: zero-contour glyph rejects instructions`
    #[test]
    fn glyf_zero_contour_rejects_instructions() {
        let mut buf = Vec::new();
        header(&mut buf, 0, 0, 0, 0, 0);
        put16(&mut buf, 1);
        assert_eq!(decode_glyf(&buf), Err(DecodeError::Hinting));
    }

    /// 머리(10바이트)보다 짧으면 틀린 자료다(`Entry.init`의 `EndOfStream`)
    #[test]
    fn glyf_shorter_than_a_header_is_malformed() {
        assert_eq!(decode_glyf(&[]), Err(DecodeError::Malformed));
        assert_eq!(decode_glyf(&[0; 9]), Err(DecodeError::Malformed));
    }

    /// 좌표 차이가 쌓여 i32를 넘으면 거절한다(`CoordinateOverflow`). 점마다 +32767씩 65536개는 점 한도에 먼저 걸려 거절되므로,
    /// 5461점(한도 안)으로는 넘치지 않는 것도 본다
    #[test]
    fn glyf_coordinate_sum_stays_within_i32_for_allowed_sizes() {
        let mut buf = Vec::new();
        header(&mut buf, 1, 0, 0, 0, 0);
        put16(&mut buf, 5460);
        put16(&mut buf, 0);
        buf.extend(std::iter::repeat_n(0x01, 5461)); // x·y 모두 16비트 차이
        for _ in 0..5461 {
            put16(&mut buf, i16::MAX);
        }
        for _ in 0..5461 {
            put16(&mut buf, i16::MIN);
        }
        let o = decode_glyf(&buf).unwrap();
        assert_eq!(o.points[5460].x, 5461 * 32767);
        assert_eq!(o.points[5460].y, 5461 * -32768);
    }

    // ---- Glossary.zig ----

    const TRIANGLE: &str = "AAEAZABkA4QDhAACAAABAQEB9P5wAyADhPzgAAA=";

    fn entry_request(cp: u32) -> Register {
        reg(&format!(
            "r;cp={cp:x};upm=2048;aw=1024;lh=1536;width=2;size=stretch;align=end,start;pad=0.1,0.2,0.3,0.4;{TRIANGLE}"
        ))
    }

    /// Glossary.zig 342줄 `Entry init decodes glyf payload and applies register fields`
    #[test]
    fn entry_init_decodes_glyf_payload_and_applies_register_fields() {
        let e = Entry::init(&entry_request(0xE000)).unwrap();
        assert_eq!(e.units_per_em, 2048);
        assert_eq!(e.advance_width, 1024);
        assert_eq!(e.line_height, 1536);
        assert_eq!(e.width, Width::Wide);
        assert_eq!(e.size, Size::Stretch);
        assert_eq!(e.align.horizontal, AlignH::End);
        assert_eq!(e.align.vertical, AlignV::Start);
        assert_eq!(
            e.pad,
            Pad {
                top: 0.1,
                right: 0.2,
                bottom: 0.3,
                left: 0.4
            }
        );
        assert_eq!(e.outline.points.len(), 3);
        assert_eq!(e.outline.contours.len(), 1);
    }

    /// Glossary.zig 368줄 `Entry init rejects invalid register payload`
    #[test]
    fn entry_init_rejects_invalid_payload() {
        let r = reg("r;cp=e000;%%%not-base64%%%");
        assert_eq!(Entry::init(&r), Err(EntryError::Decode(DecodeError::Malformed)));
    }

    /// Glossary.zig 378줄 `Entry init rejects glyf allocation over limit`: 5462점은 점 배열이 65544바이트
    #[test]
    fn entry_init_rejects_glyf_allocation_over_limit() {
        let buf = repeated_points(&[5461]);
        assert_eq!(decode_glyf(&buf), Err(DecodeError::TooLarge));
    }

    /// Glossary.zig 389줄 `Entry init accepts largest glyf points allocation under limit`: 5461점은 65532바이트
    #[test]
    fn entry_init_accepts_largest_glyf_points_allocation_under_limit() {
        let buf = repeated_points(&[0, 5460]);
        let o = decode_glyf(&buf).unwrap();
        assert_eq!(o.points.len(), 5461);
        assert_eq!(o.contours.len(), 2);
    }

    /// 형식·옵션 오류는 자료를 풀기 전에 나온다. `colrv0`·`colrv1`은 알려진 이름이지만 지원하지 않는다
    #[test]
    fn entry_init_option_errors() {
        for opt in [
            "fmt=colrv0",
            "fmt=colrv1",
            "fmt=png",
            "upm=0",
            "upm=abc",
            "aw=0",
            "lh=0",
            "width=3",
            "size=x",
            "align=a,b",
            "pad=1,1,1,1,1",
        ] {
            let r = reg(&format!("r;cp=e000;{opt};{TRIANGLE}"));
            assert_eq!(Entry::init(&r), Err(EntryError::Invalid), "{opt}");
        }
    }

    fn glossary_with(cps: &[u32]) -> Glossary {
        let mut g = Glossary::default();
        for &cp in cps {
            g.register(cp, Entry::init(&entry_request(cp)).unwrap()).unwrap();
        }
        g
    }

    /// Glossary.zig 403줄 `Glossary register overwrites and moves entry to newest position`
    #[test]
    fn register_overwrites_and_moves_entry_to_newest_position() {
        let mut g = glossary_with(&[0xE000, 0xE001]);
        g.register(0xE000, Entry::init(&entry_request(0xE000)).unwrap())
            .unwrap();
        assert_eq!(g.len(), 2);
        assert_eq!(g.codepoints().collect::<Vec<_>>(), vec![0xE001, 0xE000]);
    }

    /// Glossary.zig 419줄 `Glossary register evicts oldest entry`
    #[test]
    fn register_evicts_oldest_entry() {
        let cps: Vec<u32> = (0..=MAX_ENTRIES as u32).map(|i| 0xE000 + i).collect();
        let g = glossary_with(&cps);
        assert_eq!(g.len(), MAX_ENTRIES);
        assert!(!g.contains(0xE000));
        assert!(g.contains(0xE001));
        assert!(g.contains(0xE000 + MAX_ENTRIES as u32));
    }

    /// Glossary.zig 437줄 `Glossary register rejects non-PUA codepoint`
    #[test]
    fn register_rejects_non_pua_codepoint() {
        let mut g = Glossary::default();
        let e = Entry::init(&entry_request(0xE000)).unwrap();
        assert!(g.register(u32::from(b'A'), e).is_err());
        assert!(g.is_empty());
    }

    /// Glossary.zig 449줄 `Glossary delete removes one PUA slot and ignores empty PUA slot`
    #[test]
    fn delete_removes_one_slot_and_ignores_empty_slot() {
        let mut g = glossary_with(&[0xE000, 0xE001]);
        g.delete(0xE000).unwrap();
        assert_eq!(g.len(), 1);
        assert!(!g.contains(0xE000));
        assert!(g.contains(0xE001));
        g.delete(0xE000).unwrap();
        assert_eq!(g.len(), 1);
        assert!(g.contains(0xE001));
    }

    /// Glossary.zig 469줄 `Glossary delete rejects non-PUA codepoint`
    #[test]
    fn delete_rejects_non_pua_codepoint() {
        let mut g = glossary_with(&[0xE000]);
        assert!(g.delete(u32::from(b'A')).is_err());
        assert_eq!(g.len(), 1);
        assert!(g.contains(0xE000));
    }

    /// Glossary.zig 482줄 `Glossary clearAndFree removes all slots and remains reusable`
    #[test]
    fn clear_all_removes_all_slots_and_remains_reusable() {
        let mut g = glossary_with(&[0xE000, 0xE001]);
        g.clear_all();
        assert_eq!(g.len(), 0);
        assert!(!g.contains(0xE000));
        assert!(!g.contains(0xE001));
        g.register(0xE002, Entry::init(&entry_request(0xE002)).unwrap())
            .unwrap();
        assert_eq!(g.len(), 1);
        assert!(g.contains(0xE002));
    }

    /// Glossary.zig 502줄 `Glossary contains reports registered slots`
    #[test]
    fn contains_reports_registered_slots() {
        let mut g = Glossary::default();
        assert!(!g.contains(0xE000));
        g.register(0xE000, Entry::init(&entry_request(0xE000)).unwrap())
            .unwrap();
        assert!(g.contains(0xE000));
        assert!(!g.contains(0xE001));
    }

    // ---- base64(Zig `std.base64.standard.Decoder`와 같은 엄격함) ----

    #[test]
    fn base64_is_strict_like_zig_standard() {
        assert_eq!(decode_payload(b""), Ok(vec![]));
        assert_eq!(decode_payload(b"QQ=="), Ok(b"A".to_vec()));
        assert_eq!(decode_payload(b"QUI="), Ok(b"AB".to_vec()));
        assert_eq!(decode_payload(b"QUJD"), Ok(b"ABC".to_vec()));
        // 채움이 없거나 길이가 4의 배수가 아니다
        assert_eq!(decode_payload(b"QQ"), Err(DecodeError::Malformed));
        assert_eq!(decode_payload(b"QUJ"), Err(DecodeError::Malformed));
        // 끝 비트가 0이 아니다
        assert_eq!(decode_payload(b"QR=="), Err(DecodeError::Malformed));
        // 채움이 모자라거나 넘치거나 가운데에 있다
        assert_eq!(decode_payload(b"QQ=A"), Err(DecodeError::Malformed));
        assert_eq!(decode_payload(b"Q==="), Err(DecodeError::Malformed));
        assert_eq!(decode_payload(b"===="), Err(DecodeError::Malformed));
        assert_eq!(decode_payload(b"QUJD=QQ="), Err(DecodeError::Malformed));
        // URL 안전 글자와 공백은 표준이 아니다
        assert_eq!(decode_payload(b"Q-J_"), Err(DecodeError::Malformed));
        assert_eq!(decode_payload(b"QU JD"), Err(DecodeError::Malformed));
    }

    /// 풀면 64KiB를 넘는 자료는 풀기 전에 거절한다(글이 틀려도 크기가 먼저)
    #[test]
    fn payload_over_64kib_is_rejected_before_decoding() {
        let ok = "A".repeat(65536 / 3 * 4 - 4 + 4); // 87380자 = 65535바이트
        assert!(decode_payload(ok.as_bytes()).is_ok());
        let big = "A".repeat(87384); // 65538바이트
        assert_eq!(decode_payload(big.as_bytes()), Err(DecodeError::TooLarge));
        let garbage = "%".repeat(87384);
        assert_eq!(decode_payload(garbage.as_bytes()), Err(DecodeError::TooLarge));
    }

    // ---- execute.zig 중 글로서리를 직접 부르는 것(전체는 tests/glyph_ghostty779.rs) ----

    /// 등록이 실패하면 글로서리는 그대로다
    #[test]
    fn failed_register_leaves_the_glossary_unchanged() {
        let mut g = Glossary::default();
        assert!(g.handle(format!("r;cp=e0a0;{TRIANGLE}").as_bytes()).is_some());
        for bad in ["r;cp=e0a0;%%%", "r;cp=41;AAAAAAAAAAAAAA==", "r;cp=e0a0;fmt=colrv0;AAAA"] {
            g.handle(bad.as_bytes());
            assert_eq!(g.len(), 1, "{bad}");
        }
        assert_eq!(g.get(0xE0A0).unwrap().outline.points.len(), 3, "덮이지 않았다");
    }

    /// 합성·힌팅·너무 큰 자료의 `reason`
    #[test]
    fn register_reasons_for_decode_errors() {
        let mut g = Glossary::default();
        let b64 = |bytes: &[u8]| -> String {
            const T: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
            let mut s = String::new();
            for ch in bytes.chunks(3) {
                let n = (u32::from(ch[0]) << 16)
                    | (u32::from(*ch.get(1).unwrap_or(&0)) << 8)
                    | u32::from(*ch.get(2).unwrap_or(&0));
                s.push(T[(n >> 18) as usize & 63] as char);
                s.push(T[(n >> 12) as usize & 63] as char);
                s.push(if ch.len() > 1 {
                    T[(n >> 6) as usize & 63] as char
                } else {
                    '='
                });
                s.push(if ch.len() > 2 { T[n as usize & 63] as char } else { '=' });
            }
            s
        };
        let mut composite = Vec::new();
        header(&mut composite, -1, 0, 0, 0, 0);
        composite.extend_from_slice(&[0; 12]);
        let mut hinted = Vec::new();
        header(&mut hinted, 1, 0, 0, 0, 0);
        put16(&mut hinted, 0);
        put16(&mut hinted, 2);
        hinted.extend_from_slice(&[0, 0, 0x31]);
        let large = repeated_points(&[5461]);
        for (data, reason) in [
            (composite, "composite_unsupported"),
            (hinted, "hinting_unsupported"),
            (large, "payload_too_large"),
        ] {
            let got = g.handle(format!("r;cp=e0a0;{}", b64(&data)).as_bytes()).unwrap();
            assert_eq!(
                got,
                format!("\x1b_25a1;r;cp=e0a0;status=1;reason={reason}\x1b\\").into_bytes()
            );
        }
        assert!(g.is_empty());
    }
}
