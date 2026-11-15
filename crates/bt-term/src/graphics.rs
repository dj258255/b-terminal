//! kitty 그래픽 프로토콜(#76): <https://sw.kovidgoyal.net/kitty/graphics-protocol/>
//!
//! 터미널에 찍힌 글이 보내는 그림이다. 믿지 않는다: 파일·공유 메모리 전송(t=f·t·s)은 받지 않고(터미널이
//! 임의 파일을 읽게 되는 길), 받는 바이트·그림 크기·저장 총량에 상한을 둔다. 여기서는 VT 엔진과 상관없이
//! 명령을 읽고 그림을 저장하며, 무엇을 답하고 커서를 얼마나 옮길지만 정한다. 줄은 스크롤백 맨 위를 0으로 센다

use std::collections::HashMap;
use std::sync::Arc;

use bt_core::kitty_geom::Geom;

/// 한 명령(조각을 다 모은 것)의 base64 글 상한
const MAX_PAYLOAD: usize = 48 * 1024 * 1024;
/// 그림 한 장을 푼 RGBA 바이트 상한(예: 4096×4096)
const MAX_IMAGE_BYTES: usize = 64 * 1024 * 1024;
/// 그림 한 변의 상한(px)
const MAX_SIDE: u32 = 10_000;
/// 저장 상한의 기본(바이트). Ghostty의 `image-storage-limit` 기본과 같다(전에는 256MiB 고정, ADR-013).
/// 넘으면 오래된 그림부터 버린다
pub const DEFAULT_STORE: usize = 320_000_000;
/// 한 화면에 놓는 그림 수 상한
const MAX_PLACEMENTS: usize = 1024;
/// 저장하는 그림 수 상한(작은 그림을 많이 보내 바이트 상한을 피하지 못하게, 보안 검토)
const MAX_IMAGES: usize = 4096;
/// 그림마다 총량에 더하는 고정 몫(바이트). 아주 작은 그림도 0으로 세지 않는다
const IMAGE_OVERHEAD: usize = 4096;

/// 저장한 그림
#[derive(Debug, Clone)]
pub struct Image {
    pub width: u32,
    pub height: u32,
    /// 곱하지 않은 알파 RGBA
    pub rgba: Arc<Vec<u8>>,
    /// 저장한 차례(오래된 것부터 버리려고)
    seq: u64,
    /// 그림 번호(`I=`, 0이면 없음)
    pub number: u32,
}

/// 화면에 놓인 그림
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Placement {
    pub image: u32,
    /// 왼쪽 위 칸: 스크롤백 맨 위를 0으로 센 줄, 칸
    pub line: i64,
    pub col: usize,
    /// 놓을 때 정한 차지하는 칸 수(잘라낸 크기와 오프셋을 계산에 넣었다). 칸 크기가 바뀌면 그리는 쪽이 `geom`으로 다시 푼다
    pub cols: u32,
    pub rows: u32,
    /// 대체 화면(vim 등)에 놓였다. 대체 화면을 나오면 지운다
    pub alt: bool,
    /// 자리 번호(`p=`). 0이면 내부 번호: 같은 그림에 몇 개든 놓고 서로 바꾸지 않는다.
    /// 0이 아니면 (그림, 자리 번호) 쌍마다 하나이고 같은 쌍으로 다시 놓으면 옛 자리를 바꾼다
    pub pid: u32,
    /// 잘라내기(`x y w h`)·칸 수 요청(`c r`)·칸 안 오프셋(`X Y`)
    pub geom: Geom,
    /// z 순서(`z=`). 음수면 글자 아래, `i32::MIN / 2`보다 작으면 칸 바탕 아래까지(`bt_core::kitty_geom::layer_of_z`)
    pub z: i32,
    /// 자리 번호가 0일 때 저장소가 붙인 내부 번호(0이 아님). 자리 번호가 있으면 0. 부모 자리를 가리킬 때와 가상 배치를 고를 때 쓴다
    pub internal: u32,
    /// 자리의 종류: 커서에 놓인 자리, 가상 배치(`U=1`), 부모 자리 기준 상대 배치(`P Q H V`, #553)
    pub loc: Loc,
}

/// 자리의 종류(Ghostty `Placement.Location`: pin·virtual·relative)
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Loc {
    /// 놓을 때 커서가 있던 칸에 붙는다. `line`·`col`이 그 칸이다
    Pin,
    /// 자리가 없다. 화면의 자리표시 글자(U+10EEEE)가 그 칸에 그림을 보여 준다(`kitty_unicode`)
    Virtual,
    /// 부모 자리에서 칸 오프셋만큼 떨어진 곳. 부모가 지워지면 함께 지운다. `line`·`col`은 쓰지 않는다
    Relative(Relative),
}

/// 상대 배치의 부모와 칸 오프셋(`H`, `V`)
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Relative {
    pub parent: PlacementKey,
    pub h: i32,
    pub v: i32,
}

/// 자리 하나를 가리키는 열쇠: 같은 화면(`alt`)의 (그림, 자리 번호 또는 내부 번호)
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct PlacementKey {
    pub image: u32,
    /// 자리 번호(`p=`). 0이면 내부 번호(`internal`)가 가리킨다
    pub pid: u32,
    pub internal: u32,
    pub alt: bool,
}

impl Placement {
    pub fn key(&self) -> PlacementKey {
        PlacementKey {
            image: self.image,
            pid: self.pid,
            internal: self.internal,
            alt: self.alt,
        }
    }

    /// 커서에 놓인 자리인가(가상 배치와 상대 배치가 아니다)
    pub fn is_pin(&self) -> bool {
        matches!(self.loc, Loc::Pin)
    }
}

impl PlacementKey {
    /// 고를 자리가 여럿일 때 쓰는 정해진 차례(Ghostty `PlacementId.preferredOver`): 자리 번호가 있는 쪽, 같으면 낮은 번호
    pub fn preferred_over(&self, other: &PlacementKey) -> bool {
        match (self.pid != 0, other.pid != 0) {
            (true, false) => true,
            (false, true) => false,
            (true, true) => self.pid < other.pid,
            (false, false) => self.internal < other.internal,
        }
    }
}

/// 명령을 처리한 결과
#[derive(Debug, Default, PartialEq, Eq)]
pub struct Outcome {
    /// PTY에 쓸 답(`ESC _ G … ESC \`)
    pub reply: Option<Vec<u8>>,
    /// 그림을 놓은 뒤 커서를 옮길 만큼(아래 줄 수, 오른쪽 칸 수)
    pub advance: Option<(u32, u32)>,
}

/// 명령을 놓을 때 필요한 지금 화면 상태
#[derive(Debug, Clone, Copy)]
pub struct Cursor {
    pub line: i64,
    pub col: usize,
    /// 셀 크기(px)
    pub cell_w: u32,
    pub cell_h: u32,
    pub alt: bool,
    /// 활성 화면 맨 위 줄(스크롤백 맨 위를 0으로 센 줄)과 활성 화면의 줄·칸 수(지우기 `d=`가 쓴다)
    pub top: i64,
    pub rows: u32,
    pub cols: u32,
}

/// 조각(m=1)으로 오는 중인 명령
struct Pending {
    control: HashMap<u8, String>,
    payload: Vec<u8>,
}

pub struct Graphics {
    images: HashMap<u32, Image>,
    pub placements: Vec<Placement>,
    pending: Option<Pending>,
    seq: u64,
    /// 저장한 그림의 총량(바이트, 고정 몫 포함). 넣고 뺄 때마다 고친다
    stored: usize,
    /// 저장 상한(바이트, 설정 image-storage-limit). 0이면 그림 프로토콜을 쓰지 않는다
    limit: usize,
    /// 번호 없이 보낸 그림에 줄 다음 내부 번호(Ghostty처럼 u32 가운데에서 시작해 프로그램이 고르는 낮은 번호와 겹치지 않게)
    next_implicit: u32,
    /// 자리 번호 없이 놓은 자리에 줄 다음 내부 번호(1부터)
    next_internal: u32,
}

impl Default for Graphics {
    fn default() -> Self {
        Self::with_limit(DEFAULT_STORE)
    }
}

impl Graphics {
    /// 저장 상한(바이트)을 정해 만든다. 0이면 그림 프로토콜을 쓰지 않는다
    pub fn with_limit(limit: usize) -> Self {
        Self {
            images: HashMap::new(),
            placements: Vec::new(),
            pending: None,
            seq: 0,
            stored: 0,
            limit,
            next_implicit: 2_147_483_647,
            next_internal: 1,
        }
    }
}

fn cost(img: &Image) -> usize {
    img.rgba.len() + IMAGE_OVERHEAD
}

fn parse_control(s: &[u8]) -> HashMap<u8, String> {
    let mut m = HashMap::new();
    for kv in s.split(|&b| b == b',') {
        if let [k, b'=', v @ ..] = kv {
            m.insert(*k, String::from_utf8_lossy(v).into_owned());
        }
    }
    m
}

fn num(c: &HashMap<u8, String>, k: u8) -> u32 {
    c.get(&k).and_then(|v| v.parse().ok()).unwrap_or(0)
}

/// 부호 있는 값(`z`). 읽지 못하면 0
fn signed(c: &HashMap<u8, String>, k: u8) -> i32 {
    c.get(&k).and_then(|v| v.parse().ok()).unwrap_or(0)
}

/// base64를 푼다(표준 알파벳, 덧붙임 = 허용, 공백 무시). 잘못된 글자가 있으면 없다
pub fn base64_decode(s: &[u8]) -> Option<Vec<u8>> {
    let val = |c: u8| -> Option<u32> {
        Some(match c {
            b'A'..=b'Z' => c - b'A',
            b'a'..=b'z' => c - b'a' + 26,
            b'0'..=b'9' => c - b'0' + 52,
            b'+' => 62,
            b'/' => 63,
            _ => return None,
        } as u32)
    };
    let mut out = Vec::with_capacity(s.len() / 4 * 3);
    let (mut acc, mut bits) = (0u32, 0u32);
    for &c in s {
        if c == b'=' {
            break;
        }
        if c.is_ascii_whitespace() {
            continue;
        }
        acc = (acc << 6) | val(c)?;
        bits += 6;
        if bits >= 8 {
            bits -= 8;
            out.push((acc >> bits) as u8);
            acc &= (1 << bits) - 1;
        }
    }
    Some(out)
}

/// PNG를 RGBA로 푼다(크기 상한 안에서)
fn decode_png(data: &[u8]) -> Result<(u32, u32, Vec<u8>), &'static str> {
    let limits = png::Limits { bytes: MAX_IMAGE_BYTES };
    let mut dec = png::Decoder::new_with_limits(data, limits);
    dec.set_transformations(png::Transformations::normalize_to_color8());
    let mut reader = dec.read_info().map_err(|_| "PNG를 읽지 못했습니다")?;
    let (w, h) = (reader.info().width, reader.info().height);
    if w == 0 || h == 0 || w > MAX_SIDE || h > MAX_SIDE {
        return Err("그림 크기가 상한을 넘습니다");
    }
    let mut buf = vec![0u8; reader.output_buffer_size()];
    let info = reader.next_frame(&mut buf).map_err(|_| "PNG를 풀지 못했습니다")?;
    buf.truncate(info.buffer_size());
    let px = (w * h) as usize;
    let rgba = match info.color_type {
        png::ColorType::Rgba => buf,
        png::ColorType::Rgb => buf.chunks_exact(3).flat_map(|p| [p[0], p[1], p[2], 255]).collect(),
        png::ColorType::Grayscale => buf.iter().flat_map(|&g| [g, g, g, 255]).collect(),
        png::ColorType::GrayscaleAlpha => buf.chunks_exact(2).flat_map(|p| [p[0], p[0], p[0], p[1]]).collect(),
        png::ColorType::Indexed => return Err("팔레트 PNG를 풀지 못했습니다"),
    };
    if rgba.len() != px * 4 {
        return Err("PNG 크기가 맞지 않습니다");
    }
    Ok((w, h, rgba))
}

impl Graphics {
    pub fn image(&self, id: u32) -> Option<&Image> {
        self.images.get(&id)
    }

    /// `ESC _ G` 뒤부터 `ESC \` 앞까지(제어;자료)를 처리한다
    pub fn handle(&mut self, raw: &[u8], cur: Cursor) -> Outcome {
        // 상한 0은 그림 프로토콜을 쓰지 않는다는 뜻이다(Ghostty): 질의에도 답하지 않아 지원하지 않는 터미널처럼 보인다
        if self.limit == 0 {
            self.pending = None;
            return Outcome::default();
        }
        let (ctl, payload) = match raw.iter().position(|&b| b == b';') {
            Some(i) => (&raw[..i], &raw[i + 1..]),
            None => (raw, &[][..]),
        };
        let control = parse_control(ctl);
        let more = num(&control, b'm') == 1;
        let act = Self::action(&control);
        // 기능 질의는 자료도 조각도 보지 않고 바로 답한다
        if act == "q" && control.contains_key(&b'e') {
            return self.feature_query(&control);
        }
        // 조각 전송 중에 온 명령 나누기(Ghostty graphics_exec.zig, #15184)
        if self.pending.is_some() {
            match act {
                // 질의·놓기는 받던 전송을 건드리지 않고 따로 실행한다. m 값은 보지 않고 한 번에 실행한다
                "q" | "p" => return self.execute(&control, payload.to_vec(), cur),
                // 직접 전송의 다음 조각: 아래에서 잇는다
                "t" | "T" if Self::medium(&control) == "d" => {}
                // 지우기와 직접 전송이 아닌 전송: 받던 전송을 버리고 새 명령으로 처리한다
                "d" | "t" | "T" => self.abort_pending(),
                // 모르는 동작은 받던 전송을 건드리지 않고 지금처럼 답한다
                _ => return self.error(&control, "EINVAL", "모르는 동작입니다"),
            }
        }
        // 조각 모으기: 첫 조각의 제어를 쓰고 자료를 잇는다
        let (control, payload) = match self.pending.take() {
            Some(mut p) => {
                if p.payload.len() + payload.len() > MAX_PAYLOAD {
                    // 상한을 넘으면 이 전송은 버린다. 바꾸려던 옛 그림은 지운다(Ghostty)
                    self.remove_replaced(&p.control);
                    return self.error(&p.control, "EFBIG", "그림 자료가 상한을 넘습니다");
                }
                p.payload.extend_from_slice(payload);
                if more {
                    self.pending = Some(p);
                    return Outcome::default();
                }
                (p.control, p.payload)
            }
            None => {
                if payload.len() > MAX_PAYLOAD {
                    return self.error(&control, "EFBIG", "그림 자료가 상한을 넘습니다");
                }
                if more {
                    self.pending = Some(Pending {
                        control,
                        payload: payload.to_vec(),
                    });
                    return Outcome::default();
                }
                (control, payload.to_vec())
            }
        };
        self.execute(&control, payload, cur)
    }

    fn action(c: &HashMap<u8, String>) -> &str {
        c.get(&b'a').map(String::as_str).unwrap_or("t")
    }

    fn medium(c: &HashMap<u8, String>) -> &str {
        c.get(&b't').map(String::as_str).unwrap_or("d")
    }

    /// 받던 조각 전송을 버리고, 그 전송이 바꾸려던 옛 그림(첫 조각의 `i=`)을 지운다
    fn abort_pending(&mut self) {
        if let Some(p) = self.pending.take() {
            self.remove_replaced(&p.control);
        }
    }

    /// 번호(`i=`, 0이 아님)가 있으면 그 옛 그림과 놓인 자리를 지운다. `I=`만 있으면 지우지 않는다
    fn remove_replaced(&mut self, control: &HashMap<u8, String>) {
        let id = num(control, b'i');
        if id != 0 {
            self.replace_image(id);
        }
    }

    /// 다 모은 명령 하나를 실행한다
    fn execute(&mut self, control: &HashMap<u8, String>, payload: Vec<u8>, cur: Cursor) -> Outcome {
        let action = Self::action(control);
        let control = control.clone();
        // 그림 번호(i)와 이미지 번호(I)는 동작과 상관없이 함께 쓸 수 없다(명세, Ghostty graphics_exec.zig). 동작보다 먼저 거른다
        if num(&control, b'i') > 0 && num(&control, b'I') > 0 {
            return self.error(
                &control,
                "EINVAL",
                "그림 번호(i)와 이미지 번호(I)는 함께 쓸 수 없습니다",
            );
        }
        match action {
            "q" => match self.decode(&control, &payload) {
                // 질의: 그림을 저장하지 않고 받을 수 있는지만 답한다
                Ok(_) => self.ok(&control),
                Err((code, why)) => self.error(&control, code, why),
            },
            "t" | "T" => {
                let id_in = num(&control, b'i');
                let number = num(&control, b'I');
                // 명세: 같은 번호로 다시 보내면 옛 그림과 자리를 지운다. 성공이든 실패든 지우고(Ghostty #15184),
                // 지우는 때는 조각(m=1)을 다 모은 뒤다(kitty는 첫 조각에서 지우지만 Ghostty와 같은 때)
                let mut image = match self.transmitted(&control, &payload) {
                    Ok(img) => img,
                    Err((code, why)) => {
                        // 실패 길
                        if id_in != 0 {
                            self.replace_image(id_in);
                        }
                        return self.error(&control, code, why);
                    }
                };
                // 번호가 없으면 정한다: 이미지 번호(I)만 있으면 쓰지 않은 가장 낮은 번호, 둘 다 없으면 내부 번호(답하지 않는다)
                let implicit = id_in == 0 && number == 0;
                let id = if id_in != 0 {
                    id_in
                } else {
                    self.next_image_id(implicit)
                };
                // 성공 길
                self.replace_image(id);
                image.number = number;
                self.store(id, image);
                let out = if action == "T" {
                    self.place(id, &control, cur)
                } else {
                    self.ok_id(&control, id)
                };
                if implicit {
                    Outcome {
                        reply: None,
                        advance: out.advance,
                    }
                } else {
                    out
                }
            }
            "p" => {
                let (id_in, number) = (num(&control, b'i'), num(&control, b'I'));
                if id_in == 0 && number == 0 {
                    return self.error(&control, "EINVAL", "그림 번호(i)나 이미지 번호(I)가 필요합니다");
                }
                // 가상 배치는 부모를 가질 수 없다: 그림을 찾기 전에 거른다(#553)
                if num(&control, b'U') != 0 && num(&control, b'P') > 0 {
                    return self.error(&control, "EINVAL", "가상 배치는 부모를 가질 수 없습니다");
                }
                // 이미지 번호면 그 번호로 가장 나중에 보낸 그림
                let id = if id_in != 0 {
                    Some(id_in).filter(|id| self.images.contains_key(id))
                } else {
                    self.id_by_number(number)
                };
                match id {
                    Some(id) => self.place(id, &control, cur),
                    None => self.error(&control, "ENOENT", "그런 그림이 없습니다"),
                }
            }
            "d" => {
                self.delete(&control, cur);
                // 지운 자리가 부모였으면 자식(상대 배치)도 함께 간다. 대문자 지우기는 그 일로 자리가 없어진 그림 자료도 지운다
                let upper = control
                    .get(&b'd')
                    .is_some_and(|d| d.len() == 1 && d.as_bytes()[0].is_ascii_uppercase());
                self.reap_orphans(upper);
                Outcome::default()
            }
            _ => self.error(&control, "EINVAL", "모르는 동작입니다"),
        }
    }

    /// 이미지 번호(`I=`)로 가장 나중에 보낸 그림의 번호
    fn id_by_number(&self, number: u32) -> Option<u32> {
        if number == 0 {
            return None;
        }
        self.images
            .iter()
            .filter(|(_, i)| i.number == number)
            .max_by_key(|(_, i)| i.seq)
            .map(|(id, _)| *id)
    }

    /// 번호를 안 준 그림에 줄 번호(Ghostty `nextImageId`): 0이 아니고 쓰는 중이 아닌 가장 낮은 번호(`implicit`이면 내부 번호 구간부터)
    fn next_image_id(&mut self, implicit: bool) -> u32 {
        let mut id = if implicit { self.next_implicit } else { 1 };
        for _ in 0..self.images.len() + 2 {
            if id != 0 && !self.images.contains_key(&id) {
                break;
            }
            id = id.wrapping_add(1);
        }
        if id == 0 {
            id = 1;
        }
        if implicit {
            self.next_implicit = id.wrapping_add(1);
        }
        id
    }

    /// 기능 질의(`a=q`에 `e=`, kitty 규약 "querying support for protocol features")에 답한다.
    /// 자료는 풀지 않고 그림도 pending도 건드리지 않는다. 답에는 이 터미널이 실제로 따르는 키만, 검사한 값으로 다시 써서 넣는다
    /// (요청의 바이트를 되돌리지 않는다). 키 차례는 `a`~`z` 다음 `A`~`Z`
    fn feature_query(&self, c: &HashMap<u8, String>) -> Outcome {
        let id = num(c, b'i');
        let e = c.get(&b'e').map(String::as_bytes).unwrap_or_default();
        let action = match e {
            [b] if b.is_ascii_alphabetic() => *b,
            _ => return self.error(c, "EINVAL", "기능 질의의 동작(e)은 ASCII 글자 하나여야 합니다"),
        };
        if id == 0 {
            return self.error(c, "EINVAL", "그림 번호(i)가 필요합니다");
        }
        if Self::quiet(c) >= 1 {
            return Outcome::default();
        }
        let val = |k: u8| c.get(&k).map(String::as_str);
        let nonzero = |k: u8| Some(num(c, k)).filter(|&n| n != 0);
        let mut keys: Vec<(u8, String)> = Vec::new();
        let mut put = |k: u8, v: String| keys.push((k, v));
        // 전송(t T q)과 놓기(T p) 키. 우리는 직접 전송(d)·zlib(z)·f 24·32·100·크기 상한 안의 s·v, 칸 수 c·r, 커서 고정 C=1만 따른다
        let transmit = matches!(action, b't' | b'T' | b'q');
        let place = matches!(action, b'T' | b'p');
        if transmit {
            if let Some(f) = Some(num(c, b'f')).filter(|f| matches!(f, 24 | 32 | 100)) {
                put(b'f', f.to_string());
            }
            if val(b'o') == Some("z") {
                put(b'o', "z".into());
            }
            for k in *b"sv" {
                if let Some(n) = nonzero(k).filter(|&n| n <= MAX_SIDE) {
                    put(k, n.to_string());
                }
            }
            if val(b't') == Some("d") {
                put(b't', "d".into());
            }
        }
        if place {
            // 칸 수(c r), 잘라내기(x y w h), 칸 안 오프셋(X Y), 자리 번호(p), 커서 고정 C=1, z 순서(아래 U P Q H V는 따로)
            for k in *b"crxywhXYp" {
                if let Some(n) = nonzero(k) {
                    put(k, n.to_string());
                }
            }
            if signed(c, b'z') != 0 {
                put(b'z', signed(c, b'z').to_string());
            }
            if num(c, b'C') == 1 {
                put(b'C', "1".into());
            }
            // 유니코드 자리표시(U, 0이 아니면 가상 배치)와 상대 배치(부모 P·Q, 칸 오프셋 H·V, #553)
            if num(c, b'U') != 0 {
                put(b'U', "1".into());
            }
            for k in *b"PQ" {
                if let Some(n) = nonzero(k) {
                    put(k, n.to_string());
                }
            }
            for k in *b"HV" {
                if signed(c, k) != 0 {
                    put(k, signed(c, k).to_string());
                }
            }
        }
        if action == b'd' {
            // 지우기 종류: 애니메이션 프레임(f·F)만 빼고 모두
            let known = "aAiIpPcCxXyYzZrRnNqQ";
            if let Some(d) = val(b'd').filter(|d| d.len() == 1 && known.contains(*d)) {
                put(b'd', d.to_string());
            }
        }
        // 지원하지 않는 동작(애니메이션 f·a·c 등)은 e= 없이 보통 OK
        let known = transmit || place || action == b'd';
        let mut reply = format!("\x1b_Gi={id}");
        if known {
            reply.push_str(&format!(",e={}", action as char));
            keys.sort_by_key(|(k, _)| (k.is_ascii_uppercase(), *k));
            for (k, v) in keys {
                reply.push_str(&format!(",{}={v}", k as char));
            }
        }
        reply.push_str(";OK\x1b\\");
        Outcome {
            reply: Some(reply.into_bytes()),
            advance: None,
        }
    }

    /// `t`/`T`로 받은 자료를 저장해도 되는 그림으로 만든다(번호·크기 상한 검사 포함)
    fn transmitted(&self, c: &HashMap<u8, String>, payload: &[u8]) -> Result<Image, (&'static str, &'static str)> {
        let image = self.decode(c, payload)?;
        // 한 장이 저장 상한보다 크면 받지 않는다: 지금 넣은 것은 남기므로 상한을 넘겨 두게 된다
        if cost(&image) > self.limit {
            return Err(("ENOSPC", "그림이 저장 상한(image-storage-limit)을 넘습니다"));
        }
        Ok(image)
    }

    /// 자료를 RGBA 그림으로 푼다
    fn decode(&self, c: &HashMap<u8, String>, payload: &[u8]) -> Result<Image, (&'static str, &'static str)> {
        let medium = c.get(&b't').map(String::as_str).unwrap_or("d");
        if medium != "d" {
            return Err(("EINVAL", "직접 전송(t=d)만 받습니다"));
        }
        let mut data = base64_decode(payload).ok_or(("EINVAL", "base64가 아닙니다"))?;
        if c.get(&b'o').map(String::as_str) == Some("z") {
            data = miniz_oxide::inflate::decompress_to_vec_zlib_with_limit(&data, MAX_IMAGE_BYTES)
                .map_err(|_| ("EINVAL", "zlib을 풀지 못했습니다"))?;
        }
        let format = num(c, b'f');
        let (w, h, rgba) = match format {
            100 => decode_png(&data).map_err(|why| ("EBADPNG", why))?,
            0 | 24 | 32 => {
                let (w, h) = (num(c, b's'), num(c, b'v'));
                if w == 0 || h == 0 || w > MAX_SIDE || h > MAX_SIDE {
                    return Err(("EINVAL", "너비(s)·높이(v)가 없거나 상한을 넘습니다"));
                }
                let bpp = if format == 24 { 3 } else { 4 };
                let need = w as usize * h as usize * bpp;
                if data.len() < need {
                    return Err(("ENODATA", "자료가 너비·높이보다 짧습니다"));
                }
                let rgba = if bpp == 4 {
                    data[..need].to_vec()
                } else {
                    data[..need]
                        .chunks_exact(3)
                        .flat_map(|p| [p[0], p[1], p[2], 255])
                        .collect()
                };
                (w, h, rgba)
            }
            _ => return Err(("EINVAL", "모르는 형식입니다(f=24·32·100)")),
        };
        if rgba.len() > MAX_IMAGE_BYTES {
            return Err(("EFBIG", "그림이 상한을 넘습니다"));
        }
        Ok(Image {
            width: w,
            height: h,
            rgba: Arc::new(rgba),
            seq: 0,
            number: 0,
        })
    }

    fn store(&mut self, id: u32, mut image: Image) {
        self.seq += 1;
        image.seq = self.seq;
        self.stored += cost(&image);
        if let Some(old) = self.images.insert(id, image) {
            self.stored -= cost(&old);
        }
        // 총량이나 그림 수를 넘으면 오래된 그림부터 버린다(지금 넣은 것은 남긴다)
        while self.stored > self.limit || self.images.len() > MAX_IMAGES {
            let Some((&old, _)) = self.images.iter().filter(|(k, _)| **k != id).min_by_key(|(_, i)| i.seq) else {
                break;
            };
            self.remove_image(old);
        }
    }

    fn remove_image(&mut self, id: u32) {
        self.drop_image(id);
        self.reap_orphans(false);
    }

    /// 같은 번호로 다시 보내서 옛 그림을 바꾼다(Ghostty `deleteReplacedImage`: `d=I`와 같다). 옛 그림의 자리와 그 자식(상대 배치)이 가고,
    /// 그 일로 자리가 하나도 없어진 다른 그림의 자료도 함께 지운다
    fn replace_image(&mut self, id: u32) {
        self.drop_image(id);
        self.reap_orphans(true);
    }

    fn drop_image(&mut self, id: u32) {
        if let Some(img) = self.images.remove(&id) {
            self.stored -= cost(&img);
        }
        self.placements.retain(|p| p.image != id);
    }

    /// 커서 자리에 그림을 놓는다. 칸 수·크기는 `Geom::resolve`가 Ghostty와 같은 규칙으로 정한다(잘라낸 크기와 오프셋이 들어간다).
    /// 자리 번호(`p=`)가 있으면 같은 (그림, 자리 번호)의 옛 자리를 바꾼다.
    /// `U`가 0이 아니면 가상 배치(자리가 없고 커서도 옮기지 않는다), `P`가 있으면 부모 자리 기준 상대 배치(커서를 옮기지 않는다, #553)
    fn place(&mut self, id: u32, c: &HashMap<u8, String>, cur: Cursor) -> Outcome {
        let is_virtual = num(c, b'U') != 0;
        let parent_image = num(c, b'P');
        // 가상 배치는 부모를 가질 수 없다. 그림을 찾기 전에 거른다(Ghostty graphics_exec.zig, kitty와 같은 차례)
        if is_virtual && parent_image > 0 {
            return self.error(c, "EINVAL", "가상 배치는 부모를 가질 수 없습니다");
        }
        let Some(img) = self.images.get(&id) else {
            return self.error(c, "ENOENT", "그런 그림이 없습니다");
        };
        // 칸 수는 상한(1000)으로 자른다(보안 검토: 그릴 양)
        let geom = Geom {
            img_w: img.width,
            img_h: img.height,
            src: [num(c, b'x'), num(c, b'y'), num(c, b'w'), num(c, b'h')],
            want_cols: num(c, b'c').min(1000),
            want_rows: num(c, b'r').min(1000),
            off: [num(c, b'X'), num(c, b'Y')],
            piece: None,
        };
        let pid = num(c, b'p');
        let loc = if is_virtual {
            Loc::Virtual
        } else if parent_image == 0 {
            Loc::Pin
        } else {
            // 부모 자리를 찾아 순환·깊이를 검사한다. 지금 놓는 자리를 가리킬 수 있을 때(자리 번호가 있을 때)만 자기 자신·순환을 본다
            let child = (pid != 0).then_some(PlacementKey {
                image: id,
                pid,
                internal: 0,
                alt: cur.alt,
            });
            match crate::kitty_unicode::resolve_parent(
                &self.placements,
                self.images.contains_key(&parent_image),
                cur.alt,
                child,
                parent_image,
                num(c, b'Q'),
            ) {
                Ok(parent) => Loc::Relative(Relative {
                    parent,
                    h: signed(c, b'H'),
                    v: signed(c, b'V'),
                }),
                Err(e) => return self.error(c, e.code(), "부모 자리를 정하지 못했습니다"),
            }
        };
        let r = geom.resolve(cur.cell_w, cur.cell_h);
        let (cols, rows) = (r.cols.min(1000), r.rows.min(1000));
        if pid != 0 {
            self.placements
                .retain(|p| !(p.image == id && p.pid == pid && p.alt == cur.alt));
        }
        let mut evicted = false;
        if self.placements.len() >= MAX_PLACEMENTS {
            self.placements.remove(0);
            evicted = true;
        }
        let internal = if pid == 0 {
            let n = self.next_internal;
            self.next_internal = n.wrapping_add(1).max(1);
            n
        } else {
            0
        };
        self.placements.push(Placement {
            image: id,
            line: cur.line,
            col: cur.col,
            cols,
            rows,
            alt: cur.alt,
            pid,
            geom: Geom {
                off: [r.off_x, r.off_y],
                ..geom
            },
            z: signed(c, b'z'),
            internal,
            loc,
        });
        // 바꾸거나 밀어낸 자리가 부모였으면 그 자식이 남아 있을 수 있다(바꾼 자리는 같은 열쇠라 자식이 이어진다)
        if evicted {
            self.reap_orphans(false);
        }
        let mut out = self.ok_id(c, id);
        // C=1이면 커서를 옮기지 않는다. 아니면 그림의 마지막 줄, 그림 오른쪽 칸으로(kitty와 같다).
        // 가상 배치와 상대 배치는 C와 상관없이 커서를 옮기지 않는다(Ghostty)
        if num(c, b'C') != 1 && matches!(loc, Loc::Pin) {
            out.advance = Some((rows.saturating_sub(1), cols));
        }
        out
    }

    /// 부모 자리가 없어진 상대 배치를 지운다(Ghostty `removeOrphans`: 부모 사슬이 끊기면 자식도 함께 간다. 자식의 자식도 거듭).
    /// `free_unused`면 그 일로 자리가 없어진 그림의 자료까지 지운다(대문자 지우기)
    fn reap_orphans(&mut self, free_unused: bool) {
        let ids = crate::kitty_unicode::reap_orphans(&mut self.placements);
        if free_unused {
            self.free_unused(ids);
        }
    }

    /// 쓰지 않게 된 그림(어느 화면에도 자리가 없는 그림)의 자료를 지운다(Ghostty `deleteIfUnused`)
    fn delete_if_unused(&mut self, id: u32) {
        // 자리가 하나도 없는 그림이라 자식(상대 배치)이 걸린 자리도 없다. 지운 자리의 자식은 부르는 쪽이 `reap_orphans`로 정리한다
        if self.images.contains_key(&id) && !self.placements.iter().any(|p| p.image == id) {
            self.drop_image(id);
        }
    }

    /// 조건에 맞는 자리를 지우고 그 자리들의 그림 번호를 돌려준다
    fn drop_where(&mut self, f: impl Fn(&Placement) -> bool) -> Vec<u32> {
        let mut hit = Vec::new();
        self.placements.retain(|p| {
            if f(p) {
                hit.push(p.image);
                false
            } else {
                true
            }
        });
        hit
    }

    /// 지운 자리의 그림 가운데 쓰지 않게 된 것을 지운다(대문자 지우기)
    fn free_unused(&mut self, mut ids: Vec<u32>) {
        ids.sort_unstable();
        ids.dedup();
        for id in ids {
            self.delete_if_unused(id);
        }
    }

    /// `d=i`·`d=n`: 그림의 자리를 지운다. 자리 번호가 있으면 그 자리만, 없으면 이 화면의 그 그림 자리 모두.
    /// 자리 번호를 줬는데 맞는 자리가 없으면 그림 자료도 건드리지 않는다(명세)
    fn delete_by_id(&mut self, id: u32, pid: u32, upper: bool, alt: bool) {
        let matched = if pid == 0 {
            self.drop_where(|p| p.image == id && p.alt == alt);
            true
        } else {
            !self
                .drop_where(|p| p.image == id && p.pid == pid && p.alt == alt)
                .is_empty()
        };
        if upper && matched {
            self.delete_if_unused(id);
        }
    }

    /// 지우기 `d=`(Ghostty `graphics_storage.zig`의 `delete`). 소문자는 놓인 자리만, 대문자는 쓰지 않게 된 그림 자료까지 지운다.
    /// 지우는 대상은 지금 화면(주 화면 또는 대체 화면)의 자리다. 좌표(`x`·`y`)는 활성 화면 맨 위 왼쪽을 (1, 1)로 센다.
    /// 애니메이션 프레임 지우기(`f`·`F`)는 프레임이 없으니 소문자는 아무것도 하지 않고 대문자는 그림을 지운다(Ghostty가 프레임 없는 그림에 하는 일)
    fn delete(&mut self, c: &HashMap<u8, String>, cur: Cursor) {
        let what = c.get(&b'd').map(String::as_bytes).unwrap_or(b"a");
        let &[kind] = what else { return };
        let upper = kind.is_ascii_uppercase();
        let alt = cur.alt;
        let (x, y) = (num(c, b'x'), num(c, b'y'));
        let (cell_w, cell_h) = (cur.cell_w, cur.cell_h);
        // 자리가 덮는 칸 직사각형: (왼쪽 칸, 오른쪽 칸, 위 줄, 아래 줄). 칸 수를 모르면 없다
        let rect = move |p: &Placement| -> Option<(usize, usize, i64, i64)> {
            // 가상 배치와 상대 배치는 자기 칸이 없어 칸으로 고르지 못한다(Ghostty `Placement.rect`)
            if !p.is_pin() {
                return None;
            }
            let r = p.geom.resolve(cell_w, cell_h);
            if r.cols == 0 || r.rows == 0 {
                return None;
            }
            let last_col = (cur.cols.max(1) - 1) as usize;
            let right = p.col.saturating_add(r.cols as usize - 1).min(last_col);
            Some((p.col, right, p.line, p.line + i64::from(r.rows) - 1))
        };
        // 활성 화면의 (x, y)(1부터) 칸. 0이거나 화면 밖이면 없다
        let cell = |x: u32, y: u32| -> Option<(usize, i64)> {
            (x > 0 && y > 0 && x <= cur.cols && y <= cur.rows).then(|| ((x - 1) as usize, cur.top + i64::from(y - 1)))
        };
        let covering = move |p: &Placement, col: usize, line: i64| {
            p.alt == alt && rect(p).is_some_and(|(l, r, t, b)| l <= col && col <= r && t <= line && line <= b)
        };
        match kind.to_ascii_lowercase() {
            // 활성 화면에 보이는 자리만(스크롤백으로 올라간 자리는 남는다)
            b'a' => {
                // 가상 배치와 상대 배치는 고르지 않는다(Ghostty `deleteVisiblePlacements`: 상대 배치는 부모와 함께 간다)
                let hit = self.drop_where(|p| {
                    p.alt == alt && p.is_pin() && {
                        p.line >= cur.top || rect(p).is_some_and(|(_, _, _, bottom)| bottom >= cur.top)
                    }
                });
                if upper {
                    self.free_unused(hit);
                }
            }
            b'i' => self.delete_by_id(num(c, b'i'), num(c, b'p'), upper, alt),
            b'n' => {
                if let Some(id) = self.id_by_number(num(c, b'I')) {
                    self.delete_by_id(id, num(c, b'p'), upper, alt);
                }
            }
            b'c' => {
                let (col, line) = (cur.col, cur.line);
                let hit = self.drop_where(|p| covering(p, col, line));
                if upper {
                    self.free_unused(hit);
                }
            }
            b'p' => {
                if let Some((col, line)) = cell(x, y) {
                    let hit = self.drop_where(|p| covering(p, col, line));
                    if upper {
                        self.free_unused(hit);
                    }
                }
            }
            b'q' => {
                if let Some((col, line)) = cell(x, y) {
                    let z = signed(c, b'z');
                    let hit = self.drop_where(|p| p.z == z && covering(p, col, line));
                    if upper {
                        self.free_unused(hit);
                    }
                }
            }
            // 줄과 상관없이 그 칸을 덮는 자리(스크롤백의 것도)
            b'x' => {
                if x > 0 {
                    let col = (x - 1) as usize;
                    let hit =
                        self.drop_where(|p| p.alt == alt && rect(p).is_some_and(|(l, r, _, _)| l <= col && col <= r));
                    if upper {
                        self.free_unused(hit);
                    }
                }
            }
            b'y' => {
                if let Some((_, line)) = cell(1, y) {
                    let hit =
                        self.drop_where(|p| p.alt == alt && rect(p).is_some_and(|(_, _, t, b)| t <= line && line <= b));
                    if upper {
                        self.free_unused(hit);
                    }
                }
            }
            b'z' => {
                // 가상 배치는 z로 지우지 못한다(명세). 상대 배치는 자기 z로 맞는다
                let z = signed(c, b'z');
                let hit = self.drop_where(|p| p.alt == alt && p.z == z && !matches!(p.loc, Loc::Virtual));
                if upper {
                    self.free_unused(hit);
                }
            }
            // 그림 번호 범위 [x, y]. 위쪽 값이 0이거나 거꾸로면 아무것도 고르지 않는다
            b'r' => {
                if y == 0 || x > y {
                    return;
                }
                self.drop_where(|p| p.alt == alt && (x..=y).contains(&p.image));
                if upper {
                    let ids: Vec<u32> = self.images.keys().copied().filter(|id| (x..=y).contains(id)).collect();
                    self.free_unused(ids);
                }
            }
            b'f' => {
                let (id_in, number) = (num(c, b'i'), num(c, b'I'));
                let id = if id_in != 0 {
                    Some(id_in)
                } else {
                    self.id_by_number(number)
                };
                if let Some(id) = id.filter(|id| upper && self.images.contains_key(id)) {
                    self.remove_image(id);
                }
            }
            // 모르는 값은 무시한다
            _ => {}
        }
    }

    /// 화면 지우기와 초기화에 맞춰 놓인 그림을 지운다.
    /// ED 2(보이는 화면)는 여기서 다루지 않는다: 엔진이 올렸는지(프롬프트에서만, #329)에 달렸으므로 `erase_screen`이 한다.
    /// ED 3(스크롤백): 줄 번호의 기준이 바뀌므로 주 화면의 그림을 모두 지운다. 초기화(RIS)는 모두 지운다
    pub fn erase(&mut self, visible: bool, scrollback: bool) {
        match (visible, scrollback) {
            (true, true) => self.placements.clear(),
            // 가상 배치는 줄에 붙지 않아 남고, 줄에 붙은 자리가 지워져 부모를 잃은 상대 배치만 함께 간다
            (false, true) => {
                self.placements.retain(|p| p.alt || !p.is_pin());
                self.reap_orphans(false);
            }
            _ => {}
        }
    }

    /// ED 2를 처리한 뒤(#329, Ghostty `kitty_images.clearScreen`, Terminal.zig 4527~4535줄): 보이는 자리에 걸친 그림을 지우고
    /// 스크롤백에 다 들어간 것만 남긴다. `active_top`은 지금 활성 화면 맨 위의 절대 줄(엔진이 올렸으면 이미 늘어난 스크롤백 크기)이다.
    /// 대체 화면이면 대체 화면 그림만, 주 화면이면 주 화면 그림만 본다
    pub fn erase_screen(&mut self, active_top: i64, in_alt: bool) {
        // 가상 배치와 상대 배치는 보이는 자리로 지우지 않는다. 지워진 부모의 자식은 뒤따라 간다
        self.placements
            .retain(|p| p.alt != in_alt || !p.is_pin() || (!in_alt && p.line + i64::from(p.rows) <= active_top));
        self.reap_orphans(false);
    }

    /// 대체 화면을 나왔다: 그 화면에 놓인 그림을 지운다
    pub fn left_alt_screen(&mut self) {
        self.placements.retain(|p| !p.alt);
        self.reap_orphans(false);
    }

    fn quiet(c: &HashMap<u8, String>) -> u32 {
        num(c, b'q')
    }

    /// 답을 만든다(Ghostty `Response.encode`): `i=`(번호), `I=`(이미지 번호), `p=`(자리 번호) 가운데 0이 아닌 것만 이 차례로 쓴다.
    /// 번호도 이미지 번호도 0이면 답할 곳이 없어 답하지 않는다. `q=1`은 OK 답을, `q=2`는 오류 답도 하지 않는다.
    /// 답에는 ASCII만(설명은 앱 로그로). 터미널에 되돌아가는 글이라 제어 문자를 넣지 않는다
    fn answer(c: &HashMap<u8, String>, id: u32, message: &str, ok: bool) -> Outcome {
        let number = num(c, b'I');
        let quiet = Self::quiet(c);
        if (id == 0 && number == 0) || (ok && quiet >= 1) || (!ok && quiet >= 2) {
            return Outcome::default();
        }
        let mut keys = Vec::new();
        for (k, v) in [('i', id), ('I', number), ('p', num(c, b'p'))] {
            if v != 0 {
                keys.push(format!("{k}={v}"));
            }
        }
        Outcome {
            reply: Some(format!("\x1b_G{};{message}\x1b\\", keys.join(",")).into_bytes()),
            advance: None,
        }
    }

    fn ok(&self, c: &HashMap<u8, String>) -> Outcome {
        Self::answer(c, num(c, b'i'), "OK", true)
    }

    /// 정해진 번호(`id`)로 OK를 답한다(이미지 번호로 번호를 받았을 때)
    fn ok_id(&self, c: &HashMap<u8, String>, id: u32) -> Outcome {
        Self::answer(c, id, "OK", true)
    }

    fn error(&self, c: &HashMap<u8, String>, code: &str, why: &str) -> Outcome {
        let _ = why;
        Self::answer(c, num(c, b'i'), code, false)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const CUR: Cursor = Cursor {
        line: 100,
        col: 4,
        cell_w: 10,
        cell_h: 20,
        alt: false,
        top: 90,
        rows: 24,
        cols: 80,
    };

    fn b64(data: &[u8]) -> String {
        const T: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
        let mut s = String::new();
        for ch in data.chunks(3) {
            let n = (ch[0] as u32) << 16 | (*ch.get(1).unwrap_or(&0) as u32) << 8 | *ch.get(2).unwrap_or(&0) as u32;
            for i in 0..4 {
                if i <= ch.len() {
                    s.push(T[(n >> (18 - 6 * i) & 63) as usize] as char);
                } else {
                    s.push('=');
                }
            }
        }
        s
    }

    #[test]
    fn base64_round_trip() {
        for data in [&b""[..], b"a", b"ab", b"abc", "한글과 그림".as_bytes()] {
            assert_eq!(base64_decode(b64(data).as_bytes()).unwrap(), data);
        }
        assert!(base64_decode(b"ab$c").is_none());
    }

    #[test]
    fn query_answers_ok_without_storing() {
        let mut g = Graphics::default();
        // icat이 지원을 알아보는 질의: 1×1 RGB
        let out = g.handle(format!("i=31,s=1,v=1,a=q,t=d,f=24;{}", b64(&[0, 0, 0])).as_bytes(), CUR);
        assert_eq!(out.reply.unwrap(), b"\x1b_Gi=31;OK\x1b\\");
        assert!(g.image(31).is_none());
    }

    #[test]
    fn transmit_and_display_rgb_places_at_cursor_and_moves_it() {
        let mut g = Graphics::default();
        // 25×30px 그림: 칸 10×20이면 3칸 × 2줄
        let px = vec![200u8; 25 * 30 * 3];
        let out = g.handle(format!("a=T,f=24,s=25,v=30,i=7;{}", b64(&px)).as_bytes(), CUR);
        assert_eq!(out.reply.unwrap(), b"\x1b_Gi=7;OK\x1b\\");
        assert_eq!(out.advance, Some((1, 3)));
        let p = &g.placements[0];
        assert_eq!((p.line, p.col, p.cols, p.rows), (100, 4, 3, 2));
        assert_eq!(
            (p.geom.want_cols, p.geom.want_rows),
            (0, 0),
            "c·r이 없으면 그림 원래 크기"
        );
        let img = g.image(7).unwrap();
        assert_eq!((img.width, img.height, img.rgba.len()), (25, 30, 25 * 30 * 4));
        assert_eq!(&img.rgba[..4], &[200, 200, 200, 255]);
    }

    #[test]
    fn chunked_png_with_zlib_and_quiet_mode() {
        // 2×1 RGBA PNG를 만들어 zlib으로 한 번 더 감싸 두 조각으로 보낸다
        let mut png_bytes = Vec::new();
        {
            let mut enc = png::Encoder::new(&mut png_bytes, 2, 1);
            enc.set_color(png::ColorType::Rgba);
            enc.set_depth(png::BitDepth::Eight);
            let mut w = enc.write_header().unwrap();
            w.write_image_data(&[255, 0, 0, 255, 0, 0, 255, 128]).unwrap();
        }
        let z = miniz_oxide::deflate::compress_to_vec_zlib(&png_bytes, 6);
        let all = b64(&z);
        let (a, b) = all.split_at(all.len() / 2 / 4 * 4);
        let mut g = Graphics::default();
        let first = g.handle(format!("a=T,f=100,o=z,i=9,q=1,c=4,r=1,m=1;{a}").as_bytes(), CUR);
        assert_eq!(first, Outcome::default(), "조각 중에는 아무것도 안 함");
        let out = g.handle(format!("m=0;{b}").as_bytes(), CUR);
        assert_eq!(out.reply, None, "q=1이면 OK를 답하지 않는다");
        assert_eq!(out.advance, Some((0, 4)));
        let img = g.image(9).unwrap();
        assert_eq!((img.width, img.height), (2, 1));
        assert_eq!(&img.rgba[4..8], &[0, 0, 255, 128]);
        assert_eq!((g.placements[0].geom.want_cols, g.placements[0].geom.want_rows), (4, 1));
    }

    #[test]
    fn file_and_shared_memory_transmission_are_refused() {
        let mut g = Graphics::default();
        for t in ["f", "t", "s"] {
            let out = g.handle(format!("a=T,t={t},f=100,i=3;{}", b64(b"/etc/passwd")).as_bytes(), CUR);
            assert_eq!(out.reply.unwrap(), b"\x1b_Gi=3;EINVAL\x1b\\", "t={t}");
        }
        assert!(g.image(3).is_none() && g.placements.is_empty());
    }

    #[test]
    fn limits_refuse_huge_images_without_allocating() {
        let mut g = Graphics::default();
        let out = g.handle(
            format!("a=T,f=32,s=100000,v=100000,i=5;{}", b64(b"abcd")).as_bytes(),
            CUR,
        );
        assert_eq!(out.reply.unwrap(), b"\x1b_Gi=5;EINVAL\x1b\\");
        let out = g.handle(format!("a=T,f=32,s=4,v=4,i=5;{}", b64(b"short")).as_bytes(), CUR);
        assert_eq!(out.reply.unwrap(), b"\x1b_Gi=5;ENODATA\x1b\\");
        assert!(g.image(5).is_none());
    }

    #[test]
    fn storage_limit_evicts_old_images_and_refuses_oversized_ones() {
        // 1×1 RGB 한 장의 몫은 4 + 4096 = 4100바이트. 상한 3장 몫(12300)
        let mut g = Graphics::with_limit(3 * 4100);
        let px = b64(&[1, 2, 3]);
        for i in 1..=5u32 {
            g.handle(format!("a=t,f=24,s=1,v=1,i={i},q=2;{px}").as_bytes(), CUR);
        }
        assert!(g.images.len() <= 3, "{}개", g.images.len());
        assert!(g.image(5).is_some() && g.image(1).is_none());
        // 한 장이 상한보다 크면 거절한다(ENOSPC)
        let mut g = Graphics::with_limit(4000);
        let out = g.handle(format!("a=t,f=24,s=1,v=1,i=9;{px}").as_bytes(), CUR);
        assert_eq!(out.reply.unwrap(), b"\x1b_Gi=9;ENOSPC\x1b\\");
        assert!(g.image(9).is_none());
    }

    #[test]
    fn zero_limit_disables_the_protocol_without_replying() {
        let mut g = Graphics::with_limit(0);
        let px = b64(&[1, 2, 3]);
        // 질의(a=q)에도 답하지 않는다: 지원하지 않는 터미널처럼 보인다
        for cmd in ["a=q,i=1,s=1,v=1,f=24", "a=T,i=2,s=1,v=1,f=24", "a=t,i=3,s=1,v=1,f=24"] {
            let out = g.handle(format!("{cmd};{px}").as_bytes(), CUR);
            assert!(out.reply.is_none() && out.advance.is_none(), "{cmd}");
        }
        assert!(g.images.is_empty() && g.placements.is_empty());
    }

    #[test]
    fn default_limit_is_ghostty_320mb() {
        assert_eq!(DEFAULT_STORE, 320_000_000);
    }

    /// 보안 검토: 작은 그림을 아주 많이 보내도 그림 수가 상한에서 멈춘다
    #[test]
    fn image_count_is_capped() {
        let mut g = Graphics::default();
        let px = b64(&[1, 2, 3]);
        for i in 1..=5000u32 {
            g.handle(format!("a=t,f=24,s=1,v=1,i={i},q=2;{px}").as_bytes(), CUR);
        }
        assert!(g.images.len() <= 4096, "{}개", g.images.len());
        assert!(g.image(5000).is_some(), "최근 것은 남는다");
        assert!(g.image(1).is_none(), "오래된 것부터 버린다");
    }

    #[test]
    fn delete_and_erase_and_alt_screen() {
        let mut g = Graphics::default();
        let px = b64(&[1, 2, 3]);
        g.handle(format!("a=T,f=24,s=1,v=1,i=1,q=2;{px}").as_bytes(), CUR);
        g.handle(
            format!("a=T,f=24,s=1,v=1,i=2,q=2;{px}").as_bytes(),
            Cursor { line: 10, ..CUR },
        );
        assert_eq!(g.placements.len(), 2);
        g.handle(b"a=d,d=i,i=1", CUR);
        assert_eq!(g.placements.len(), 1);
        assert!(g.image(1).is_some(), "d=i는 놓인 것만 지운다");
        g.handle(b"a=d,d=I,i=1", CUR);
        assert!(g.image(1).is_none(), "d=I는 그림도 지운다");
        // ED 2는 보이는 자리의 그림을 지우고 스크롤백에 다 들어간 것만 남긴다(#329). ED 3은 주 화면의 그림을 지운다(대체 화면 것은 남김)
        g.handle(b"a=p,i=2,q=2", Cursor { alt: true, ..CUR });
        g.erase(true, false);
        assert_eq!(g.placements.len(), 2, "erase(true,false)는 아무것도 하지 않는다");
        g.erase_screen(5, false);
        assert_eq!(
            g.placements.len(),
            1,
            "줄 10은 활성 화면(5~)에 걸쳐 지우고 대체 화면 것은 남는다"
        );
        g.handle(b"a=p,i=2,q=2", Cursor { line: 10, ..CUR });
        g.erase_screen(50, false);
        assert_eq!(g.placements.len(), 2, "스크롤백에 다 들어간 줄 10의 그림은 남는다");
        g.erase_screen(50, true);
        assert_eq!(g.placements.len(), 1, "대체 화면이면 대체 화면 그림만 지운다");
        g.handle(b"a=p,i=2,q=2", Cursor { alt: true, ..CUR });
        g.erase(false, true);
        assert_eq!(g.placements.len(), 1);
        assert!(g.placements[0].alt);
        g.erase(true, true);
        assert!(g.placements.is_empty(), "초기화는 모두 지운다");
        g.handle(b"a=p,i=2,q=2", Cursor { alt: true, ..CUR });
        g.left_alt_screen();
        assert!(g.placements.is_empty(), "대체 화면의 그림은 나올 때 지운다");
    }

    /// 1×1 RGB 그림을 보내는 명령(자료 첫 바이트로 새 그림인지 가린다)
    fn rgb1(action: &str, id: u32, px: [u8; 3]) -> String {
        format!("a={action},f=24,s=1,v=1,i={id};{}", b64(&px))
    }

    #[test]
    fn retransmit_with_display_replaces_old_placement() {
        let mut g = Graphics::default();
        g.handle(rgb1("T", 5, [255, 0, 0]).as_bytes(), CUR);
        let out = g.handle(rgb1("T", 5, [0, 0, 255]).as_bytes(), Cursor { line: 10, ..CUR });
        assert_eq!(out.reply.unwrap(), b"\x1b_Gi=5;OK\x1b\\");
        assert_eq!(g.placements.len(), 1, "옛 자리가 남았다");
        assert_eq!(g.placements[0].line, 10);
        assert_eq!(&g.image(5).unwrap().rgba[..3], &[0, 0, 255]);
    }

    #[test]
    fn retransmit_without_display_leaves_no_placement() {
        let mut g = Graphics::default();
        g.handle(rgb1("T", 5, [255, 0, 0]).as_bytes(), CUR);
        let out = g.handle(rgb1("t", 5, [0, 0, 255]).as_bytes(), Cursor { line: 10, ..CUR });
        assert_eq!(out.reply.unwrap(), b"\x1b_Gi=5;OK\x1b\\");
        assert!(g.placements.is_empty(), "옛 자리가 남았다");
        assert_eq!(&g.image(5).unwrap().rgba[..3], &[0, 0, 255]);
    }

    #[test]
    fn retransmit_keeps_other_images_and_placements() {
        let mut g = Graphics::default();
        g.handle(rgb1("T", 5, [255, 0, 0]).as_bytes(), CUR);
        g.handle(rgb1("T", 6, [0, 255, 0]).as_bytes(), Cursor { line: 20, ..CUR });
        g.handle(rgb1("t", 5, [0, 0, 255]).as_bytes(), CUR);
        assert_eq!(g.placements.len(), 1);
        assert_eq!((g.placements[0].image, g.placements[0].line), (6, 20));
        assert_eq!(&g.image(6).unwrap().rgba[..3], &[0, 255, 0]);
    }

    #[test]
    fn failed_retransmit_removes_old_image_and_placement() {
        let bad = [
            ("a=T,f=24,s=1,v=1,i=5;!!!!".to_string(), "EINVAL"),
            (format!("a=T,f=24,s=2,v=2,i=5;{}", b64(&[1, 2, 3])), "ENODATA"),
            (format!("a=t,t=f,f=24,s=1,v=1,i=5;{}", b64(b"/etc/passwd")), "EINVAL"),
        ];
        for (cmd, code) in bad {
            let mut g = Graphics::default();
            g.handle(rgb1("T", 5, [255, 0, 0]).as_bytes(), CUR);
            g.handle(rgb1("T", 6, [0, 255, 0]).as_bytes(), Cursor { line: 20, ..CUR });
            let out = g.handle(cmd.as_bytes(), CUR);
            assert_eq!(
                out.reply.unwrap(),
                format!("\x1b_Gi=5;{code}\x1b\\").into_bytes(),
                "{cmd}"
            );
            assert!(g.image(5).is_none(), "옛 그림이 남았다: {cmd}");
            assert!(g.placements.iter().all(|p| p.image != 5), "옛 자리가 남았다: {cmd}");
            assert!(
                g.image(6).is_some() && g.placements.len() == 1,
                "다른 번호가 지워졌다: {cmd}"
            );
        }
    }

    #[test]
    fn failed_retransmit_over_limit_removes_old_image() {
        let mut g = Graphics::default();
        g.handle(rgb1("T", 5, [255, 0, 0]).as_bytes(), CUR);
        g.limit = 3; // 새 그림(1×1 RGBA 4바이트)이 상한을 넘는다
        let out = g.handle(rgb1("T", 5, [0, 0, 255]).as_bytes(), CUR);
        assert_eq!(out.reply.unwrap(), b"\x1b_Gi=5;ENOSPC\x1b\\");
        assert!(g.image(5).is_none() && g.placements.is_empty());
    }

    #[test]
    fn failed_transmit_without_id_touches_nothing() {
        let mut g = Graphics::default();
        g.handle(rgb1("T", 5, [255, 0, 0]).as_bytes(), CUR);
        let out = g.handle(b"a=T,f=24,s=1,v=1;!!!!", CUR);
        assert_eq!(out, Outcome::default());
        assert!(g.image(5).is_some() && g.placements.len() == 1);
    }

    // ---- 조각 전송 중 온 명령(Ghostty graphics_exec.zig, #15184) ----

    /// 5번 그림(빨강)을 놓아 두고, 5번을 파랑으로 바꾸려는 조각 전송(a=T)의 첫 조각까지 보낸다
    fn with_pending_replace() -> Graphics {
        let mut g = Graphics::default();
        g.handle(rgb1("T", 5, [255, 0, 0]).as_bytes(), CUR);
        g.handle(rgb1("T", 6, [9, 9, 9]).as_bytes(), CUR);
        let all = b64(&[0, 0, 255]); // 4글자
        let out = g.handle(format!("a=T,f=24,s=1,v=1,i=5,m=1;{}", &all[..2]).as_bytes(), CUR);
        assert_eq!(out, Outcome::default());
        assert!(g.pending.is_some());
        g
    }

    #[test]
    fn delete_during_chunked_transfer_aborts_it_and_removes_old_image() {
        let mut g = with_pending_replace();
        // 다른 그림(6)을 지우는 명령이 왔다
        let out = g.handle(b"a=d,d=I,i=6", CUR);
        assert_eq!(out, Outcome::default());
        assert!(g.pending.is_none(), "받던 전송이 남았다");
        assert!(g.image(5).is_none(), "바꾸려던 옛 그림이 남았다");
        assert!(g.placements.iter().all(|p| p.image != 5), "옛 자리가 남았다");
        assert!(g.image(6).is_none(), "지우기가 실행되지 않았다");
        // 뒤늦게 온 마지막 조각은 새 명령이라 그림을 만들지 않는다
        let all = b64(&[0, 0, 255]);
        g.handle(format!("m=0;{}", &all[2..]).as_bytes(), CUR);
        assert!(g.image(5).is_none() && g.placements.is_empty());
    }

    #[test]
    fn non_direct_transmit_during_chunked_transfer_aborts_it() {
        for cmd in ["a=t,t=f,i=9;L3RtcC94", "a=T,t=t,i=9;L3RtcC94", "t=s,i=9;L3RtcC94"] {
            let mut g = with_pending_replace();
            let out = g.handle(cmd.as_bytes(), CUR);
            assert_eq!(out.reply.unwrap(), b"\x1b_Gi=9;EINVAL\x1b\\", "{cmd}");
            assert!(g.pending.is_none(), "{cmd}");
            assert!(g.image(5).is_none(), "옛 그림이 남았다: {cmd}");
            assert!(g.image(6).is_some(), "다른 그림이 지워졌다: {cmd}");
        }
    }

    #[test]
    fn query_and_put_during_chunked_transfer_run_without_touching_it() {
        let mut g = with_pending_replace();
        let out = g.handle(format!("a=q,i=31,s=1,v=1,f=24;{}", b64(&[0, 0, 0])).as_bytes(), CUR);
        assert_eq!(out.reply.unwrap(), b"\x1b_Gi=31;OK\x1b\\");
        // m=1이 붙은 질의도 한 번에 실행하고 받던 전송은 그대로다
        let out = g.handle(format!("a=q,i=32,s=1,v=1,f=24,m=1;{}", b64(&[0, 0, 0])).as_bytes(), CUR);
        assert_eq!(out.reply.unwrap(), b"\x1b_Gi=32;OK\x1b\\");
        let out = g.handle(b"a=p,i=6", Cursor { line: 7, ..CUR });
        assert_eq!(out.reply.unwrap(), b"\x1b_Gi=6;OK\x1b\\");
        assert!(g.placements.iter().any(|p| p.image == 6 && p.line == 7));
        assert!(g.pending.is_some());
        assert!(g.image(5).is_some(), "받는 동안 옛 그림은 그대로다");
        // 마지막 조각으로 그림이 완성된다
        let all = b64(&[0, 0, 255]);
        let out = g.handle(format!("m=0;{}", &all[2..]).as_bytes(), CUR);
        assert_eq!(out.reply.unwrap(), b"\x1b_Gi=5;OK\x1b\\");
        assert_eq!(g.image(5).unwrap().rgba.as_slice(), &[0, 0, 255, 255]);
        assert!(g.pending.is_none());
    }

    #[test]
    fn next_chunk_with_only_m_and_q_still_appends() {
        let mut g = with_pending_replace();
        let all = b64(&[0, 0, 255]);
        let out = g.handle(format!("m=0,q=1;{}", &all[2..]).as_bytes(), CUR);
        // 첫 조각의 제어(q 없음)를 쓰므로 답한다
        assert_eq!(out.reply.unwrap(), b"\x1b_Gi=5;OK\x1b\\");
        assert_eq!(g.image(5).unwrap().rgba.as_slice(), &[0, 0, 255, 255]);
    }

    #[test]
    fn oversize_chunk_removes_old_image() {
        let mut g = with_pending_replace();
        let big = vec![b'A'; MAX_PAYLOAD / 2 + 1];
        let mut cmd = b"m=1;".to_vec();
        cmd.extend_from_slice(&big);
        g.handle(&cmd, CUR);
        let out = g.handle(&cmd, CUR);
        assert_eq!(out.reply.unwrap(), b"\x1b_Gi=5;EFBIG\x1b\\");
        assert!(g.pending.is_none());
        assert!(g.image(5).is_none() && g.placements.iter().all(|p| p.image != 5));
        assert!(g.image(6).is_some());
    }

    #[test]
    fn chunked_transfer_without_numeric_id_deletes_nothing_on_abort() {
        let mut g = Graphics::default();
        g.handle(rgb1("T", 5, [255, 0, 0]).as_bytes(), CUR);
        g.handle(b"a=T,f=24,s=1,v=1,I=5,m=1;AA", CUR);
        g.handle(b"a=d,d=a", CUR);
        assert!(g.pending.is_none() && g.image(5).is_some());
    }

    // ---- 기능 질의(a=q, e=) ----

    fn query(g: &mut Graphics, cmd: &str) -> Option<String> {
        g.handle(cmd.as_bytes(), CUR)
            .reply
            .map(|r| String::from_utf8(r).unwrap())
    }

    #[test]
    fn feature_query_answers_only_supported_keys_in_order() {
        let mut g = Graphics::default();
        // 유니코드 자리표시(U=1, 0이 아닌 값은 모두 1로 다시 쓴다)와 상대 배치 키(P Q H V)도 답한다(#553)
        assert_eq!(
            query(&mut g, "a=q,i=31,e=T,U=2").unwrap(),
            "\x1b_Gi=31,e=T,U=1;OK\x1b\\"
        );
        // 소문자 a~z, 대문자 A~Z 차례. 모르는 값(f=99, t=f)과 하지 않는 키(x, z)는 빠진다
        assert_eq!(
            query(&mut g, "a=q,i=1,e=t,t=d,f=100,o=z,s=10,v=10,x=3,z=1,m=0").unwrap(),
            "\x1b_Gi=1,e=t,f=100,o=z,s=10,t=d,v=10;OK\x1b\\"
        );
        assert_eq!(
            query(&mut g, "a=q,i=1,e=t,t=f,f=99,s=0,v=10001,o=x").unwrap(),
            "\x1b_Gi=1,e=t;OK\x1b\\"
        );
        assert_eq!(
            query(&mut g, "a=q,i=1,e=q,f=24").unwrap(),
            "\x1b_Gi=1,e=q,f=24;OK\x1b\\"
        );
        // T는 놓기 키(c r C)도 답한다
        assert_eq!(
            query(&mut g, "a=q,i=2,e=T,c=4,r=0,C=1,f=32,X=1").unwrap(),
            "\x1b_Gi=2,e=T,c=4,f=32,C=1,X=1;OK\x1b\\"
        );
        assert_eq!(query(&mut g, "a=q,i=2,e=T,C=2").unwrap(), "\x1b_Gi=2,e=T;OK\x1b\\");
        // p는 놓기 키만
        assert_eq!(
            query(&mut g, "a=q,i=2,e=p,c=4,r=5,C=1,f=32,s=3").unwrap(),
            "\x1b_Gi=2,e=p,c=4,r=5,C=1;OK\x1b\\"
        );
        // d는 처리하는 d 값만
        assert_eq!(
            query(&mut g, "a=q,i=2,e=d,d=I,x=1").unwrap(),
            "\x1b_Gi=2,e=d,d=I;OK\x1b\\"
        );
        assert_eq!(query(&mut g, "a=q,i=2,e=d,d=z").unwrap(), "\x1b_Gi=2,e=d,d=z;OK\x1b\\");
        assert_eq!(
            query(&mut g, "a=q,i=2,e=d,d=f").unwrap(),
            "\x1b_Gi=2,e=d;OK\x1b\\",
            "애니메이션 지우기는 답에서 뺀다"
        );
        assert_eq!(query(&mut g, "a=q,i=2,e=d").unwrap(), "\x1b_Gi=2,e=d;OK\x1b\\");
    }

    #[test]
    fn feature_query_unknown_action_answers_plain_ok() {
        let mut g = Graphics::default();
        for e in ["f", "a", "c", "x", "Z"] {
            assert_eq!(
                query(&mut g, &format!("a=q,i=7,e={e},f=24")).unwrap(),
                "\x1b_Gi=7;OK\x1b\\",
                "{e}"
            );
        }
    }

    #[test]
    fn feature_query_rejects_bad_action_and_missing_id() {
        let mut g = Graphics::default();
        for e in ["5", "tt", "", "\u{d55c}"] {
            assert_eq!(
                query(&mut g, &format!("a=q,i=7,e={e}")).unwrap(),
                "\x1b_Gi=7;EINVAL\x1b\\",
                "{e:?}"
            );
        }
        // 번호가 없으면 오류 답 규칙대로 답이 없다
        assert_eq!(query(&mut g, "a=q,e=T"), None);
        assert_eq!(query(&mut g, "a=q,i=0,e=T"), None);
        // 질의가 아닌 동작의 e=는 기능 질의가 아니다(자료를 푸는 보통 동작)
        assert_eq!(
            query(&mut g, "a=t,i=3,e=T,f=24,s=1,v=1;!!!!").unwrap(),
            "\x1b_Gi=3;EINVAL\x1b\\"
        );
    }

    #[test]
    fn feature_query_ignores_payload_pending_and_quiet() {
        let mut g = with_pending_replace();
        // 잘못된 base64여도 풀지 않는다
        assert_eq!(
            query(&mut g, "a=q,i=7,e=t,f=24;$$$$").unwrap(),
            "\x1b_Gi=7,e=t,f=24;OK\x1b\\"
        );
        assert!(g.pending.is_some() && g.image(7).is_none());
        // m=1이어도 받는 전송을 만들지 않는다
        let mut g = Graphics::default();
        assert!(query(&mut g, "a=q,i=7,e=t,m=1;AA").is_some());
        assert!(g.pending.is_none());
        assert_eq!(query(&mut g, "a=q,i=7,e=T,q=1"), None);
        assert_eq!(query(&mut g, "a=q,i=7,e=5,q=1"), Some("\x1b_Gi=7;EINVAL\x1b\\".into()));
        assert_eq!(query(&mut g, "a=q,i=7,e=5,q=2"), None);
    }

    #[test]
    fn feature_query_reply_never_echoes_request_bytes() {
        let mut g = Graphics::default();
        // 글자 값은 허용 목록의 글자만, 숫자는 파싱한 값을 다시 쓴다
        let r = query(&mut g, "a=q,i=+07,e=t,t=d\x1b,o=z\x07,f=0100,s=00010,v=\x1b[1m").unwrap();
        assert_eq!(r, "\x1b_Gi=7,e=t,f=100,s=10;OK\x1b\\");
    }

    // ---- 자리 번호·잘라내기·오프셋·z·그림 번호(#547, Ghostty graphics_exec.zig·graphics_storage.zig) ----

    /// 1×1 RGB 그림을 저장만 한다(놓지 않는다)
    fn store1(g: &mut Graphics, ctl: &str) -> Option<String> {
        let out = g.handle(format!("a=t,f=24,s=1,v=1,{ctl};{}", b64(&[1, 2, 3])).as_bytes(), CUR);
        out.reply.map(|r| String::from_utf8(r).unwrap())
    }

    fn put(g: &mut Graphics, ctl: &str, cur: Cursor) -> Option<String> {
        let out = g.handle(format!("a=p,{ctl}").as_bytes(), cur);
        out.reply.map(|r| String::from_utf8(r).unwrap())
    }

    /// 남은 자리 (그림, 자리 번호)를 정렬해서
    fn left(g: &Graphics) -> Vec<(u32, u32)> {
        let mut v: Vec<_> = g.placements.iter().map(|p| (p.image, p.pid)).collect();
        v.sort();
        v
    }

    fn at(line: i64, col: usize) -> Cursor {
        Cursor { line, col, ..CUR }
    }

    #[test]
    fn same_image_and_placement_id_replaces_the_old_placement() {
        let mut g = Graphics::default();
        store1(&mut g, "i=1");
        assert_eq!(put(&mut g, "i=1,p=5", at(100, 4)).unwrap(), "\x1b_Gi=1,p=5;OK\x1b\\");
        assert_eq!(
            put(&mut g, "i=1,p=5,c=3", at(110, 8)).unwrap(),
            "\x1b_Gi=1,p=5;OK\x1b\\"
        );
        assert_eq!(left(&g), vec![(1, 5)], "같은 (그림, 자리) 쌍은 하나만");
        assert_eq!((g.placements[0].line, g.placements[0].col), (110, 8));
        put(&mut g, "i=1,p=6", at(100, 4));
        assert_eq!(left(&g), vec![(1, 5), (1, 6)], "다른 자리 번호는 따로");
    }

    #[test]
    fn same_placement_id_on_another_image_is_a_different_placement() {
        let mut g = Graphics::default();
        store1(&mut g, "i=1");
        store1(&mut g, "i=2");
        put(&mut g, "i=1,p=5", CUR);
        put(&mut g, "i=2,p=5", CUR);
        assert_eq!(left(&g), vec![(1, 5), (2, 5)]);
    }

    #[test]
    fn placements_without_a_placement_id_never_replace_each_other() {
        let mut g = Graphics::default();
        store1(&mut g, "i=1");
        for line in [100, 101, 102] {
            put(&mut g, "i=1", at(line, 0));
        }
        assert_eq!(g.placements.len(), 3);
        // 자리 번호 0은 응답에도 p를 붙이지 않는다
        assert_eq!(put(&mut g, "i=1", CUR).unwrap(), "\x1b_Gi=1;OK\x1b\\");
        // 자리 번호가 있는 것은 그것만 바꾼다
        put(&mut g, "i=1,p=9", at(100, 0));
        put(&mut g, "i=1,p=9", at(100, 1));
        assert_eq!(g.placements.len(), 5);
    }

    #[test]
    fn place_keeps_source_rect_offset_and_z() {
        let mut g = Graphics::default();
        let px = vec![0u8; 25 * 30 * 3];
        let out = g.handle(
            format!("a=T,f=24,s=25,v=30,i=7,x=5,y=6,w=10,h=12,X=3,Y=4,z=-3;{}", b64(&px)).as_bytes(),
            CUR,
        );
        let p = &g.placements[0];
        assert_eq!(p.geom.src, [5, 6, 10, 12]);
        assert_eq!(p.geom.off, [3, 4]);
        assert_eq!(p.z, -3);
        assert_eq!((p.geom.img_w, p.geom.img_h), (25, 30));
        // 잘라낸 10×12 + 오프셋 (3, 4): 13px -> 2칸, 16px -> 1줄
        assert_eq!((p.cols, p.rows), (2, 1));
        assert_eq!(out.advance, Some((0, 2)));
    }

    #[test]
    fn offset_is_clamped_below_the_cell_size_and_z_may_be_positive() {
        let mut g = Graphics::default();
        store1(&mut g, "i=1");
        put(&mut g, "i=1,X=99,Y=99,z=2147483647", CUR);
        put(&mut g, "i=1,z=-2147483648", CUR);
        assert_eq!(g.placements[0].geom.off, [9, 19], "칸 10×20이면 9, 19까지");
        assert_eq!(g.placements[0].z, i32::MAX);
        assert_eq!(g.placements[1].z, i32::MIN);
    }

    #[test]
    fn columns_and_rows_follow_the_cropped_source_and_aspect_ratio() {
        let mut g = Graphics::default();
        let px = vec![0u8; 100 * 50 * 3];
        // 40×20만 잘라 c=8만 주면: 너비 80px, 높이 40px -> 8칸 2줄
        g.handle(
            format!("a=T,f=24,s=100,v=50,i=1,w=40,h=20,c=8;{}", b64(&px)).as_bytes(),
            CUR,
        );
        let p = &g.placements[0];
        assert_eq!((p.cols, p.rows), (8, 2));
        assert_eq!((p.geom.want_cols, p.geom.want_rows), (8, 0));
    }

    #[test]
    fn transmit_with_an_image_number_assigns_the_lowest_free_id() {
        let mut g = Graphics::default();
        assert_eq!(store1(&mut g, "I=13").unwrap(), "\x1b_Gi=1,I=13;OK\x1b\\");
        assert_eq!(store1(&mut g, "I=13").unwrap(), "\x1b_Gi=2,I=13;OK\x1b\\");
        assert_eq!(store1(&mut g, "i=3").unwrap(), "\x1b_Gi=3;OK\x1b\\");
        assert_eq!(store1(&mut g, "I=14,p=4").unwrap(), "\x1b_Gi=4,I=14,p=4;OK\x1b\\");
        assert_eq!(g.image(1).unwrap().number, 13);
        assert_eq!(
            g.image(2).unwrap().number,
            13,
            "같은 번호를 다시 보내도 옛 그림은 남는다"
        );
        assert_eq!(g.image(3).unwrap().number, 0);
    }

    #[test]
    fn transmit_without_id_or_number_gets_an_implicit_id_and_no_reply() {
        let mut g = Graphics::default();
        assert_eq!(store1(&mut g, "q=0"), None);
        assert_eq!(store1(&mut g, "q=0"), None);
        let mut ids: Vec<u32> = g.images.keys().copied().collect();
        ids.sort();
        assert_eq!(
            ids,
            vec![2_147_483_647, 2_147_483_648],
            "낮은 번호와 겹치지 않는 내부 번호"
        );
        // 놓기(T)에도 답하지 않지만 그림은 놓인다
        let out = g.handle(rgb1("T", 0, [1, 2, 3]).as_bytes(), CUR);
        assert_eq!(out.reply, None);
        assert_eq!(g.placements.len(), 1);
    }

    #[test]
    fn display_by_image_number_uses_the_newest_image_with_that_number() {
        let mut g = Graphics::default();
        store1(&mut g, "I=13");
        store1(&mut g, "I=13");
        assert_eq!(put(&mut g, "I=13,p=2", CUR).unwrap(), "\x1b_Gi=2,I=13,p=2;OK\x1b\\");
        assert_eq!(left(&g), vec![(2, 2)], "가장 나중에 보낸 그림");
        let out = g.handle(format!("a=T,f=24,s=1,v=1,I=20;{}", b64(&[1, 2, 3])).as_bytes(), CUR);
        assert_eq!(out.reply.unwrap(), b"\x1b_Gi=3,I=20;OK\x1b\\");
        assert_eq!(g.placements.len(), 2);
        assert_eq!(put(&mut g, "I=99", CUR).unwrap(), "\x1b_GI=99;ENOENT\x1b\\");
    }

    #[test]
    fn image_id_and_number_together_are_refused_for_every_action() {
        let mut g = Graphics::default();
        store1(&mut g, "i=1");
        for ctl in ["a=p,i=1,I=2,p=3", "a=t,f=24,s=1,v=1,i=1,I=2,p=3", "a=d,d=i,i=1,I=2,p=3"] {
            let out = g.handle(format!("{ctl};{}", b64(&[1, 2, 3])).as_bytes(), CUR);
            assert_eq!(out.reply.unwrap(), b"\x1b_Gi=1,I=2,p=3;EINVAL\x1b\\", "{ctl}");
        }
        assert!(
            g.image(1).is_some() && g.placements.is_empty(),
            "아무것도 바뀌지 않는다"
        );
        // q=2면 오류 답도 없다
        assert_eq!(put(&mut g, "i=1,I=2,q=2", CUR), None);
    }

    #[test]
    fn display_needs_an_id_or_number_and_an_existing_image() {
        let mut g = Graphics::default();
        assert_eq!(put(&mut g, "p=3", CUR), None, "번호가 없으면 답할 곳이 없다");
        assert_eq!(put(&mut g, "i=8,p=3", CUR).unwrap(), "\x1b_Gi=8,p=3;ENOENT\x1b\\");
        assert!(g.placements.is_empty());
    }

    /// 지우기 시험용 장면. 활성 화면은 줄 90~113(맨 위 90), 칸 80
    ///  A: 그림 1, 자리 1, 줄 100~101 칸 4~6
    ///  B: 그림 1, 자리 2, 줄 105 칸 20~21, z=-5
    ///  C: 그림 2(번호 7), 자리 없음, 줄 50~51 칸 0~1 (스크롤백)
    ///  D: 그림 3, 줄 100 칸 5, z=7 (A와 한 칸이 겹친다)
    ///  그림 4는 놓지 않았다. 대체 화면에도 그림 1을 놓았다(E)
    fn scene() -> Graphics {
        let mut g = Graphics::default();
        for ctl in ["i=1", "I=7", "i=3", "i=4"] {
            store1(&mut g, ctl);
        }
        put(&mut g, "i=1,p=1,c=3,r=2", at(100, 4));
        put(&mut g, "i=1,p=2,c=2,r=1,z=-5", at(105, 20));
        put(&mut g, "i=2,c=2,r=2", at(50, 0));
        put(&mut g, "i=3,c=1,r=1,z=7", at(100, 5));
        put(
            &mut g,
            "i=1,p=3,c=1,r=1",
            Cursor {
                alt: true,
                ..at(100, 40)
            },
        );
        assert_eq!(left(&g), vec![(1, 1), (1, 2), (1, 3), (2, 0), (3, 0)]);
        g
    }

    fn del(g: &mut Graphics, ctl: &str) -> Outcome {
        let out = g.handle(format!("a=d,{ctl}").as_bytes(), CUR);
        assert_eq!(out, Outcome::default(), "지우기는 답하지 않는다: {ctl}");
        out
    }

    #[test]
    fn delete_a_removes_only_visible_placements_of_the_active_screen() {
        let mut g = scene();
        del(&mut g, "d=a");
        assert_eq!(left(&g), vec![(1, 3), (2, 0)], "스크롤백의 C와 대체 화면의 E는 남는다");
        assert!(
            g.image(1).is_some() && g.image(3).is_some(),
            "소문자는 그림 자료를 남긴다"
        );
    }

    #[test]
    fn delete_capital_a_frees_only_images_that_lost_their_placements() {
        let mut g = scene();
        del(&mut g, "d=A");
        assert_eq!(left(&g), vec![(1, 3), (2, 0)]);
        assert!(g.image(3).is_none(), "D만 쓰던 그림 3은 지운다");
        assert!(g.image(1).is_some(), "대체 화면 자리가 남은 그림 1은 남는다");
        assert!(g.image(4).is_some(), "놓지 않은 그림은 건드리지 않는다");
        assert!(g.image(2).is_some());
    }

    #[test]
    fn delete_by_id_and_placement_id() {
        let mut g = scene();
        del(&mut g, "d=i,i=1,p=2");
        assert_eq!(
            left(&g),
            vec![(1, 1), (1, 3), (2, 0), (3, 0)],
            "자리 번호를 주면 그 자리만"
        );
        del(&mut g, "d=i,i=1");
        assert_eq!(
            left(&g),
            vec![(1, 3), (2, 0), (3, 0)],
            "화면의 자리는 모두. 대체 화면 자리는 남는다"
        );
        assert!(g.image(1).is_some());
    }

    #[test]
    fn delete_capital_i_frees_the_image_only_when_it_matched_and_is_unused() {
        let mut g = scene();
        del(&mut g, "d=I,i=1,p=99");
        assert!(
            g.image(1).is_some() && g.placements.len() == 5,
            "맞는 자리가 없으면 아무것도 지우지 않는다"
        );
        del(&mut g, "d=I,i=3");
        assert!(g.image(3).is_none());
        del(&mut g, "d=I,i=4");
        assert!(g.image(4).is_none(), "자리가 없던 그림도 지운다");
        del(&mut g, "d=I,i=1,p=1");
        assert!(g.image(1).is_some(), "다른 자리(B)가 남아 있으면 그림도 남는다");
        del(&mut g, "d=I,i=1");
        assert!(g.image(1).is_some(), "대체 화면 자리 E가 남아 있다");
    }

    #[test]
    fn delete_by_image_number_picks_the_newest() {
        let mut g = scene();
        // 번호 7을 한 번 더: 쓰지 않은 가장 낮은 번호 5가 된다
        assert_eq!(store1(&mut g, "I=7").unwrap(), "\x1b_Gi=5,I=7;OK\x1b\\");
        put(&mut g, "i=5,p=4,c=1,r=1", at(110, 0));
        del(&mut g, "d=n,I=7");
        assert!(
            g.placements.iter().all(|p| p.image != 5),
            "번호 7의 가장 나중 그림(5)의 자리를 지운다"
        );
        assert!(g.placements.iter().any(|p| p.image == 2));
        assert!(g.image(5).is_some());
        del(&mut g, "d=N,I=7");
        assert!(g.image(5).is_none(), "N은 그림도 지운다");
        assert!(g.image(2).is_some());
        del(&mut g, "d=n,I=555");
    }

    #[test]
    fn delete_at_cursor_removes_everything_covering_the_cursor_cell() {
        let mut g = scene();
        // 커서는 줄 100, 칸 4: A만 덮는다(D는 칸 5)
        del(&mut g, "d=c");
        assert_eq!(left(&g), vec![(1, 2), (1, 3), (2, 0), (3, 0)]);
        assert!(g.image(1).is_some());
        let mut g = scene();
        del(&mut g, "d=C");
        assert_eq!(left(&g), vec![(1, 2), (1, 3), (2, 0), (3, 0)]);
        assert!(g.image(1).is_some(), "B와 E가 남았다");
        let mut g = scene();
        g.handle(b"a=d,d=C", Cursor { col: 5, ..CUR });
        assert_eq!(left(&g), vec![(1, 2), (1, 3), (2, 0)], "칸 5는 A와 D를 덮는다");
        assert!(g.image(3).is_none(), "D만 쓰던 그림 3은 대문자라 지운다");
    }

    #[test]
    fn delete_at_cell_uses_one_based_active_coordinates() {
        let mut g = scene();
        // 활성 화면 맨 위가 줄 90이니 줄 100은 y=11, 칸 5는 x=6
        del(&mut g, "d=p,x=6,y=11");
        assert_eq!(left(&g), vec![(1, 2), (1, 3), (2, 0)], "A와 D");
        assert!(g.image(3).is_some());
        let mut g = scene();
        del(&mut g, "d=P,x=6,y=11");
        assert!(g.image(3).is_none() && g.image(1).is_some());
        // 0이거나 화면 밖이면 아무것도 지우지 않는다
        for ctl in [
            "d=p,x=0,y=11",
            "d=p,x=6,y=0",
            "d=p,x=6",
            "d=p,x=81,y=11",
            "d=p,x=6,y=25",
        ] {
            let mut g = scene();
            del(&mut g, ctl);
            assert_eq!(g.placements.len(), 5, "{ctl}");
        }
    }

    #[test]
    fn delete_at_cell_with_z_only_removes_matching_z() {
        let mut g = scene();
        del(&mut g, "d=q,x=6,y=11,z=7");
        assert_eq!(left(&g), vec![(1, 1), (1, 2), (1, 3), (2, 0)], "D만");
        let mut g = scene();
        del(&mut g, "d=Q,x=6,y=11,z=0");
        assert_eq!(left(&g), vec![(1, 2), (1, 3), (2, 0), (3, 0)], "z가 0인 A만");
        assert!(g.image(1).is_some());
        let mut g = scene();
        del(&mut g, "d=q,x=6,y=11,z=-5");
        assert_eq!(g.placements.len(), 5);
    }

    #[test]
    fn delete_by_column_matches_any_row_including_scrollback() {
        let mut g = scene();
        // 칸 5(x=6): A, D. 칸 0(x=1): C(스크롤백)
        del(&mut g, "d=x,x=6");
        assert_eq!(left(&g), vec![(1, 2), (1, 3), (2, 0)]);
        let mut g = scene();
        del(&mut g, "d=X,x=1");
        assert_eq!(left(&g), vec![(1, 1), (1, 2), (1, 3), (3, 0)]);
        assert!(g.image(2).is_none(), "C의 그림 2는 대문자라 지운다");
        let mut g = scene();
        del(&mut g, "d=x,x=0");
        del(&mut g, "d=x");
        assert_eq!(g.placements.len(), 5);
    }

    #[test]
    fn delete_by_row_uses_one_based_active_rows() {
        let mut g = scene();
        // 줄 100은 y=11: A(100~101)와 D. 줄 105는 y=16: B
        del(&mut g, "d=y,y=11");
        assert_eq!(left(&g), vec![(1, 2), (1, 3), (2, 0)]);
        let mut g = scene();
        del(&mut g, "d=Y,y=16");
        assert_eq!(left(&g), vec![(1, 1), (1, 3), (2, 0), (3, 0)]);
        assert!(g.image(1).is_some());
        let mut g = scene();
        del(&mut g, "d=y,y=12");
        assert_eq!(left(&g), vec![(1, 2), (1, 3), (2, 0), (3, 0)], "A는 줄 101에도 걸친다");
        let mut g = scene();
        del(&mut g, "d=y,y=0");
        del(&mut g, "d=y,y=99");
        assert_eq!(g.placements.len(), 5);
    }

    #[test]
    fn delete_by_z_matches_every_placement_of_the_screen() {
        let mut g = scene();
        del(&mut g, "d=z,z=-5");
        assert_eq!(left(&g), vec![(1, 1), (1, 3), (2, 0), (3, 0)]);
        assert!(g.image(1).is_some());
        del(&mut g, "d=Z,z=7");
        assert_eq!(left(&g), vec![(1, 1), (1, 3), (2, 0)]);
        assert!(g.image(3).is_none());
        // z 기본값 0: 스크롤백의 C와 A
        del(&mut g, "d=z");
        assert_eq!(left(&g), vec![(1, 3)]);
    }

    #[test]
    fn delete_by_id_range() {
        let mut g = scene();
        del(&mut g, "d=r,x=2,y=3");
        assert_eq!(left(&g), vec![(1, 1), (1, 2), (1, 3)]);
        assert!(g.image(2).is_some() && g.image(3).is_some(), "소문자는 그림을 남긴다");
        let mut g = scene();
        del(&mut g, "d=R,x=2,y=4");
        assert!(
            g.image(2).is_none() && g.image(3).is_none() && g.image(4).is_none(),
            "놓지 않은 그림 4도 지운다"
        );
        assert!(g.image(1).is_some());
        // 거꾸로 된 범위, 상한 0, 값 없음은 아무것도 고르지 않는다
        for ctl in ["d=R,x=4,y=2", "d=R,x=1", "d=R", "d=R,x=0,y=0"] {
            let mut g = scene();
            del(&mut g, ctl);
            assert_eq!(g.placements.len(), 5, "{ctl}");
            assert_eq!(g.images.len(), 4, "{ctl}");
        }
    }

    #[test]
    fn delete_never_touches_the_other_screen() {
        let mut g = scene();
        g.handle(b"a=d,d=a", Cursor { alt: true, ..CUR });
        assert_eq!(left(&g), vec![(1, 1), (1, 2), (2, 0), (3, 0)], "대체 화면의 E만 지웠다");
    }

    #[test]
    fn animation_frame_delete_without_frames_only_drops_the_image_for_capital_f() {
        let mut g = scene();
        del(&mut g, "d=f,i=1");
        assert!(
            g.image(1).is_some() && g.placements.len() == 5,
            "프레임이 없으면 소문자는 아무것도 안 한다"
        );
        del(&mut g, "d=F,i=3");
        assert!(g.image(3).is_none() && g.placements.iter().all(|p| p.image != 3));
        del(&mut g, "d=F,I=7");
        assert!(g.image(2).is_none());
        del(&mut g, "d=F");
    }

    #[test]
    fn placement_cap_still_evicts_the_oldest() {
        let mut g = Graphics::default();
        store1(&mut g, "i=1");
        for p in 1..=(MAX_PLACEMENTS as u32 + 5) {
            put(&mut g, &format!("i=1,p={p},q=1"), CUR);
        }
        assert_eq!(g.placements.len(), MAX_PLACEMENTS);
        assert_eq!(g.placements[0].pid, 6, "가장 오래된 것부터 버린다");
    }

    #[test]
    fn feature_query_answers_the_new_placement_and_delete_keys() {
        let mut g = Graphics::default();
        assert_eq!(
            query(&mut g, "a=q,i=2,e=p,x=1,y=2,w=3,h=4,X=5,Y=6,z=-7,p=8,c=4").unwrap(),
            "\x1b_Gi=2,e=p,c=4,h=4,p=8,w=3,x=1,y=2,z=-7,X=5,Y=6;OK\x1b\\"
        );
        for d in [
            "a", "A", "i", "I", "p", "P", "c", "C", "x", "X", "y", "Y", "z", "Z", "r", "R", "n", "N", "q", "Q",
        ] {
            assert_eq!(
                query(&mut g, &format!("a=q,i=2,e=d,d={d}")).unwrap(),
                format!("\x1b_Gi=2,e=d,d={d};OK\x1b\\"),
                "{d}"
            );
        }
        // 애니메이션 지우기(f·F)와 모르는 값은 답에서 뺀다
        for d in ["f", "F", "w", "!"] {
            assert_eq!(
                query(&mut g, &format!("a=q,i=2,e=d,d={d}")).unwrap(),
                "\x1b_Gi=2,e=d;OK\x1b\\",
                "{d}"
            );
        }
    }

    #[test]
    fn feature_query_answers_unicode_placeholder_and_relative_keys_in_order() {
        let mut g = Graphics::default();
        // 키 차례: 소문자 a~z, 대문자 A~Z(H P Q U V X Y). 0인 값은 빠지고 H·V는 부호를 지킨다. 전송 동작(t)에는 놓기 키가 없다
        assert_eq!(
            query(&mut g, "a=q,i=2,e=p,P=7,Q=3,H=-4,V=5,U=1,c=2").unwrap(),
            "\x1b_Gi=2,e=p,c=2,H=-4,P=7,Q=3,U=1,V=5;OK\x1b\\"
        );
        assert_eq!(
            query(&mut g, "a=q,i=2,e=T,P=0,Q=0,H=0,V=0,U=0").unwrap(),
            "\x1b_Gi=2,e=T;OK\x1b\\"
        );
        assert_eq!(query(&mut g, "a=q,i=2,e=t,P=7,U=1").unwrap(), "\x1b_Gi=2,e=t;OK\x1b\\");
    }

    // ---- 가상 배치(U=1)와 상대 배치(P Q H V, #553). 시험 이름은 Ghostty graphics_exec.zig의 "kittygfx ..."를 따른다 ----

    /// 1x1 RGB 그림을 번호 `i`로 보낸다(답은 버린다)
    fn send1(g: &mut Graphics, i: u32) {
        g.handle(format!("a=t,f=24,s=1,v=1,i={i};AAAA").as_bytes(), CUR);
    }

    fn reply(g: &mut Graphics, cmd: &str) -> String {
        let out = g.handle(cmd.as_bytes(), CUR);
        out.reply.map(|r| String::from_utf8(r).unwrap()).unwrap_or_default()
    }

    fn find(g: &Graphics, image: u32, pid: u32) -> Option<&Placement> {
        g.placements.iter().find(|p| p.image == image && p.pid == pid)
    }

    #[test]
    fn virtual_placement_is_stored_without_a_position_and_never_moves_the_cursor() {
        let mut g = Graphics::default();
        send1(&mut g, 1);
        // U=2도 가상 배치다(kitty는 bool로 저장한다). C와 상관없이 커서를 옮기지 않는다
        for cmd in ["a=p,i=1,p=1,U=1", "a=p,i=1,p=2,U=2,C=2", "a=p,i=1,U=1,c=4,r=2"] {
            let out = g.handle(cmd.as_bytes(), CUR);
            assert!(out.advance.is_none(), "{cmd}");
        }
        assert_eq!(g.placements.len(), 3);
        assert!(g.placements.iter().all(|p| p.loc == Loc::Virtual));
        // 자리 번호 없는 것은 내부 번호가 다르다(서로 바꾸지 않는다)
        assert_eq!(g.placements.iter().filter(|p| p.pid == 0).count(), 1);
        assert!(g.placements.iter().filter(|p| p.pid == 0).all(|p| p.internal != 0));
        // 같은 (그림, 자리 번호)로 다시 만들면 바꾼다
        g.handle(b"a=p,i=1,p=1,U=1,c=9", CUR);
        assert_eq!(g.placements.len(), 3);
        assert_eq!(find(&g, 1, 1).unwrap().geom.want_cols, 9);
        // a=T로도 만든다. 답은 보통 놓기와 같다
        let out = g.handle(b"a=T,f=24,s=1,v=1,i=5,U=1;AAAA", CUR);
        assert_eq!(out.reply.unwrap(), b"\x1b_Gi=5;OK\x1b\\");
        assert!(out.advance.is_none());
        assert_eq!(find(&g, 5, 0).unwrap().loc, Loc::Virtual);
    }

    #[test]
    fn virtual_placement_with_parent_is_rejected_before_the_image_lookup() {
        let mut g = Graphics::default();
        // 그림 42가 없어도 ENOENT가 아니라 EINVAL이다(kitty와 같은 차례)
        assert_eq!(reply(&mut g, "a=p,i=42,U=1,P=1"), "\x1b_Gi=42;EINVAL\x1b\\");
        send1(&mut g, 1);
        assert_eq!(reply(&mut g, "a=p,i=1,U=1,P=1"), "\x1b_Gi=1;EINVAL\x1b\\");
        // a=T에서는 그림은 저장되고 놓기만 거절한다
        assert_eq!(
            reply(&mut g, "a=T,f=24,s=1,v=1,i=2,U=1,P=1;AAAA"),
            "\x1b_Gi=2;EINVAL\x1b\\"
        );
        assert!(g.image(2).is_some());
        assert!(g.placements.is_empty());
    }

    #[test]
    fn relative_placement_with_missing_parent_image() {
        let mut g = Graphics::default();
        send1(&mut g, 1);
        let out = g.handle(b"a=p,i=1,p=1,P=42,Q=1,H=2,V=2", CUR);
        assert_eq!(out.reply.unwrap(), b"\x1b_Gi=1,p=1;ENOPARENT\x1b\\");
        assert!(out.advance.is_none(), "실패한 놓기는 커서를 옮기지 않는다");
        assert!(g.placements.is_empty());
    }

    #[test]
    fn relative_placement_with_missing_parent_placement() {
        let mut g = Graphics::default();
        send1(&mut g, 1);
        send1(&mut g, 2);
        // 부모 그림에 자리가 하나도 없다
        assert_eq!(reply(&mut g, "a=p,i=1,P=2"), "\x1b_Gi=1;ENOPARENT\x1b\\");
        // 자리는 있는데 Q가 가리키는 자리가 없다
        g.handle(b"a=p,i=2,p=1,C=1", CUR);
        assert_eq!(reply(&mut g, "a=p,i=1,P=2,Q=9"), "\x1b_Gi=1;ENOPARENT\x1b\\");
        assert_eq!(g.placements.len(), 1);
    }

    #[test]
    fn relative_placement_cannot_parent_itself() {
        let mut g = Graphics::default();
        send1(&mut g, 1);
        g.handle(b"a=p,i=1,p=1,C=1", CUR);
        // Q로 직접, 그리고 Q 없이 부모 자리를 고르면 바꾸려는 자리가 뽑히는 경우
        for cmd in ["a=p,i=1,p=1,P=1,Q=1", "a=p,i=1,p=1,P=1"] {
            assert_eq!(reply(&mut g, cmd), "\x1b_Gi=1,p=1;EINVAL\x1b\\", "{cmd}");
        }
        assert_eq!(find(&g, 1, 1).unwrap().loc, Loc::Pin, "처음 자리는 그대로");
    }

    #[test]
    fn relative_placement_cycle() {
        let mut g = Graphics::default();
        send1(&mut g, 1);
        for cmd in ["a=p,i=1,p=1,C=1", "a=p,i=1,p=2,P=1,Q=1"] {
            assert!(g.handle(cmd.as_bytes(), CUR).reply.unwrap().ends_with(b";OK\x1b\\"));
        }
        // 1을 2의 자식으로 바꾸면 1 -> 2 -> 1
        assert_eq!(reply(&mut g, "a=p,i=1,p=1,P=1,Q=2"), "\x1b_Gi=1,p=1;ECYCLE\x1b\\");
        assert_eq!(find(&g, 1, 1).unwrap().loc, Loc::Pin);
    }

    #[test]
    fn relative_placement_chain_depth_limit_is_eight_links() {
        let mut g = Graphics::default();
        send1(&mut g, 1);
        g.handle(b"a=p,i=1,p=1,C=1", CUR);
        // 9 -> 8 -> ... -> 2 -> 1: 연결 8개까지는 된다
        for id in 2..10 {
            let r = reply(&mut g, &format!("a=p,i=1,p={id},P=1,Q={}", id - 1));
            assert!(r.ends_with(";OK\x1b\\"), "{id}: {r}");
        }
        assert_eq!(reply(&mut g, "a=p,i=1,p=10,P=1,Q=9"), "\x1b_Gi=1,p=10;ETOODEEP\x1b\\");
        assert_eq!(g.placements.len(), 9);
    }

    #[test]
    fn relative_placement_does_not_move_the_cursor_and_keeps_its_link() {
        let mut g = Graphics::default();
        send1(&mut g, 1);
        g.handle(b"a=p,i=1,p=1,C=1", CUR);
        // C를 주지 않아도(기본은 옮김) 상대 배치는 옮기지 않는다
        let out = g.handle(b"a=p,i=1,p=2,P=1,Q=1,H=3,V=-2,c=2,r=2", CUR);
        assert!(out.advance.is_none());
        let Loc::Relative(rel) = find(&g, 1, 2).unwrap().loc else {
            panic!("상대 배치가 아니다")
        };
        assert_eq!((rel.h, rel.v), (3, -2));
        assert_eq!((rel.parent.image, rel.parent.pid, rel.parent.alt), (1, 1, false));
    }

    #[test]
    fn relative_parent_fallback_picks_the_lowest_external_id() {
        let mut g = Graphics::default();
        send1(&mut g, 1);
        send1(&mut g, 2);
        // 내부 번호 자리(p 없음)보다 자리 번호가 있는 쪽이, 그 가운데 낮은 번호가 부모가 된다
        for cmd in ["a=p,i=1,C=1", "a=p,i=1,p=7,C=1", "a=p,i=1,p=3,C=1", "a=p,i=2,P=1"] {
            g.handle(cmd.as_bytes(), CUR);
        }
        let Loc::Relative(rel) = g.placements.iter().find(|p| p.image == 2).unwrap().loc else {
            panic!()
        };
        assert_eq!((rel.parent.image, rel.parent.pid), (1, 3));
    }

    #[test]
    fn deleting_a_parent_deletes_relative_placements_transitively() {
        let mut g = Graphics::default();
        send1(&mut g, 1);
        // 뿌리 <- 자식 <- 손자
        for cmd in ["a=p,i=1,p=1,C=1", "a=p,i=1,p=2,P=1,Q=1", "a=p,i=1,p=3,P=1,Q=2"] {
            g.handle(cmd.as_bytes(), CUR);
        }
        assert_eq!(g.placements.len(), 3);
        // 가운데를 지우면 손자도 가고 뿌리는 남는다
        g.handle(b"a=d,d=i,i=1,p=2", CUR);
        assert_eq!(g.placements.len(), 1);
        assert!(find(&g, 1, 1).is_some());
        // 뿌리를 지우면 모두 가고 그림 자료는 남는다(소문자)
        g.handle(b"a=d,d=i,i=1,p=1", CUR);
        assert!(g.placements.is_empty());
        assert!(g.image(1).is_some());
    }

    #[test]
    fn uppercase_delete_frees_images_left_unplaced_by_the_cascade() {
        let mut g = Graphics::default();
        send1(&mut g, 1);
        send1(&mut g, 2);
        for cmd in ["a=p,i=1,p=1,C=1", "a=p,i=2,p=1,P=1,Q=1"] {
            g.handle(cmd.as_bytes(), CUR);
        }
        g.handle(b"a=d,d=I,i=1", CUR);
        assert!(g.placements.is_empty());
        assert!(g.image(1).is_none());
        assert!(g.image(2).is_none(), "자식의 그림도 자리가 없어져 지워진다");
    }

    #[test]
    fn retransmitting_the_parent_image_deletes_its_children_and_frees_their_image() {
        let mut g = Graphics::default();
        send1(&mut g, 1);
        send1(&mut g, 2);
        for cmd in ["a=p,i=1,p=1,C=1", "a=p,i=2,p=1,P=1,Q=1"] {
            g.handle(cmd.as_bytes(), CUR);
        }
        assert_eq!(g.placements.len(), 2);
        send1(&mut g, 1);
        assert!(g.placements.is_empty());
        assert!(
            g.image(2).is_none(),
            "Ghostty는 같은 번호로 다시 보내는 것을 d=I로 다룬다"
        );
    }

    #[test]
    fn visible_deletes_skip_virtual_placements_and_z_delete_skips_them_too() {
        let mut g = Graphics::default();
        send1(&mut g, 1);
        send1(&mut g, 2);
        g.handle(b"a=p,i=1,p=1,U=1,z=-1", CUR);
        g.handle(b"a=p,i=2,p=1,C=1,z=-1", CUR);
        // 커서에 놓인 자리(그림 2, p=1)의 자식
        g.handle(b"a=p,i=2,p=2,P=2,Q=1,z=-1", CUR);
        assert_eq!(g.placements.len(), 3);
        // d=a·A는 보이는 자리만: 가상 배치는 남고, 놓은 자리가 가면 자식도 간다. 가상 배치의 그림 자료도 남는다
        g.handle(b"a=d,d=A", CUR);
        assert_eq!(g.placements.len(), 1);
        assert_eq!(g.placements[0].loc, Loc::Virtual);
        assert!(g.image(1).is_some());
        assert!(g.image(2).is_none(), "대문자는 자리가 없어진 그림 자료를 지운다");
        // d=z는 가상 배치를 고르지 않는다
        g.handle(b"a=d,d=z,z=-1", CUR);
        assert_eq!(g.placements.len(), 1);
        // 칸으로 지우기도 가상 배치에는 닿지 않는다
        g.handle(b"a=d,d=c", CUR);
        g.handle(b"a=d,d=p,x=5,y=11", CUR);
        assert_eq!(g.placements.len(), 1);
        // 그림으로 지우면 간다(자리 번호를 줘도)
        g.handle(b"a=d,d=i,i=1,p=1", CUR);
        assert!(g.placements.is_empty());
    }

    #[test]
    fn z_delete_matches_relative_placements_by_their_own_z() {
        let mut g = Graphics::default();
        send1(&mut g, 1);
        g.handle(b"a=p,i=1,p=1,C=1,z=3", CUR);
        g.handle(b"a=p,i=1,p=2,P=1,Q=1,z=9", CUR);
        g.handle(b"a=d,d=z,z=9", CUR);
        assert_eq!(g.placements.len(), 1);
        assert!(find(&g, 1, 1).is_some());
    }

    #[test]
    fn relative_placements_follow_a_virtual_parent_through_screen_clears_and_die_with_it() {
        let mut g = Graphics::default();
        send1(&mut g, 1);
        g.handle(b"a=p,i=1,p=1,U=1", CUR);
        g.handle(b"a=p,i=1,p=2,P=1,Q=1", CUR);
        // ED 2·ED 3는 가상 배치를 건드리지 않아 자식도 남는다
        g.erase_screen(CUR.top, false);
        g.erase(false, true);
        assert_eq!(g.placements.len(), 2);
        // 가상 배치를 지우면 자식도 간다
        g.handle(b"a=d,d=i,i=1,p=1", CUR);
        assert!(g.placements.is_empty());
    }

    #[test]
    fn alt_screen_placements_are_separate_and_leave_with_the_screen() {
        let mut g = Graphics::default();
        send1(&mut g, 1);
        g.handle(b"a=p,i=1,p=1,C=1", CUR);
        let alt = Cursor { alt: true, ..CUR };
        // 대체 화면에서는 주 화면의 자리를 부모로 삼을 수 없다(저장소가 화면마다 따로다)
        assert_eq!(
            g.handle(b"a=p,i=1,p=2,P=1,Q=1", alt).reply.unwrap(),
            b"\x1b_Gi=1,p=2;ENOPARENT\x1b\\"
        );
        g.handle(b"a=p,i=1,p=1,U=1", alt);
        g.handle(b"a=p,i=1,p=2,P=1,Q=1", alt);
        assert_eq!(g.placements.len(), 3);
        g.left_alt_screen();
        assert_eq!(g.placements.len(), 1, "대체 화면의 가상 배치와 자식이 함께 간다");
    }

    #[test]
    fn replacing_a_parent_keeps_its_children() {
        let mut g = Graphics::default();
        send1(&mut g, 1);
        g.handle(b"a=p,i=1,p=1,C=1", CUR);
        g.handle(b"a=p,i=1,p=2,P=1,Q=1,H=1", CUR);
        // 같은 열쇠로 다시 놓아도 자식은 이어진다(Ghostty: 같은 키에 덮어쓴다)
        g.handle(b"a=p,i=1,p=1,C=1,x=0", CUR);
        assert_eq!(g.placements.len(), 2);
    }

    #[test]
    fn limit_zero_storage_ignores_virtual_and_relative_commands() {
        let mut g = Graphics::with_limit(0);
        assert!(g.handle(b"a=p,i=1,U=1", CUR).reply.is_none());
        assert!(g.placements.is_empty());
    }
}
