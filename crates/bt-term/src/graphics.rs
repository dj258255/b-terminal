//! kitty 그래픽 프로토콜(#76): <https://sw.kovidgoyal.net/kitty/graphics-protocol/>
//!
//! 터미널에 찍힌 글이 보내는 그림이다. 믿지 않는다: 파일·공유 메모리 전송(t=f·t·s)은 받지 않고(터미널이
//! 임의 파일을 읽게 되는 길), 받는 바이트·그림 크기·저장 총량에 상한을 둔다. 여기서는 VT 엔진과 상관없이
//! 명령을 읽고 그림을 저장하며, 무엇을 답하고 커서를 얼마나 옮길지만 정한다. 줄은 스크롤백 맨 위를 0으로 센다

use std::collections::HashMap;
use std::sync::Arc;

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
}

/// 화면에 놓인 그림
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Placement {
    pub image: u32,
    /// 왼쪽 위 칸: 스크롤백 맨 위를 0으로 센 줄, 칸
    pub line: i64,
    pub col: usize,
    /// 차지하는 칸 수
    pub cols: u32,
    pub rows: u32,
    /// 칸 크기에 맞춰 늘리는가(c·r을 줬을 때). 아니면 그림 원래 픽셀 크기
    pub scaled: bool,
    /// 대체 화면(vim 등)에 놓였다. 대체 화면을 나오면 지운다
    pub alt: bool,
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
        // 조각 모으기: 첫 조각의 제어를 쓰고 자료를 잇는다
        let (control, payload) = match self.pending.take() {
            Some(mut p) => {
                if p.payload.len() + payload.len() > MAX_PAYLOAD {
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
        let action = control.get(&b'a').map(String::as_str).unwrap_or("t");
        match action {
            "q" => match self.decode(&control, &payload) {
                // 질의: 그림을 저장하지 않고 받을 수 있는지만 답한다
                Ok(_) => self.ok(&control),
                Err((code, why)) => self.error(&control, code, why),
            },
            "t" | "T" => {
                let image = match self.decode(&control, &payload) {
                    Ok(img) => img,
                    Err((code, why)) => return self.error(&control, code, why),
                };
                let id = num(&control, b'i');
                if id == 0 {
                    return self.error(&control, "EINVAL", "그림 번호(i)가 필요합니다");
                }
                // 한 장이 저장 상한보다 크면 받지 않는다: 지금 넣은 것은 남기므로 상한을 넘겨 두게 된다
                if cost(&image) > self.limit {
                    return self.error(&control, "ENOSPC", "그림이 저장 상한(image-storage-limit)을 넘습니다");
                }
                self.store(id, image);
                if action == "T" {
                    self.place(id, &control, cur)
                } else {
                    self.ok(&control)
                }
            }
            "p" => {
                let id = num(&control, b'i');
                if !self.images.contains_key(&id) {
                    return self.error(&control, "ENOENT", "그런 그림이 없습니다");
                }
                self.place(id, &control, cur)
            }
            "d" => {
                self.delete(&control);
                Outcome::default()
            }
            _ => self.error(&control, "EINVAL", "모르는 동작입니다"),
        }
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
        if let Some(img) = self.images.remove(&id) {
            self.stored -= cost(&img);
        }
        self.placements.retain(|p| p.image != id);
    }

    /// 커서 자리에 그림을 놓는다. c·r이 없으면 그림 픽셀 크기만큼의 칸
    fn place(&mut self, id: u32, c: &HashMap<u8, String>, cur: Cursor) -> Outcome {
        let Some(img) = self.images.get(&id) else {
            return self.error(c, "ENOENT", "그런 그림이 없습니다");
        };
        let (want_c, want_r) = (num(c, b'c'), num(c, b'r'));
        let cols = if want_c > 0 {
            want_c
        } else {
            img.width.div_ceil(cur.cell_w.max(1))
        }
        .clamp(1, 1000);
        let rows = if want_r > 0 {
            want_r
        } else {
            img.height.div_ceil(cur.cell_h.max(1))
        }
        .clamp(1, 1000);
        if self.placements.len() >= MAX_PLACEMENTS {
            self.placements.remove(0);
        }
        self.placements.push(Placement {
            image: id,
            line: cur.line,
            col: cur.col,
            cols,
            rows,
            scaled: want_c > 0 || want_r > 0,
            alt: cur.alt,
        });
        let mut out = self.ok(c);
        // C=1이면 커서를 옮기지 않는다. 아니면 그림의 마지막 줄, 그림 오른쪽 칸으로(kitty와 같다)
        if num(c, b'C') != 1 {
            out.advance = Some((rows - 1, cols));
        }
        out
    }

    fn delete(&mut self, c: &HashMap<u8, String>) {
        let what = c.get(&b'd').map(String::as_str).unwrap_or("a");
        let id = num(c, b'i');
        match what {
            "a" => self.placements.clear(),
            "A" => {
                self.placements.clear();
                self.images.clear();
                self.stored = 0;
            }
            "i" => self.placements.retain(|p| p.image != id),
            "I" => self.remove_image(id),
            // 다른 대상(자리·z 등)은 아직 받지 않는다: 아무것도 지우지 않는다
            _ => {}
        }
    }

    /// 화면 지우기와 초기화에 맞춰 놓인 그림을 지운다.
    /// ED 2(보이는 화면)는 여기서 다루지 않는다: 엔진이 올렸는지(프롬프트에서만, #329)에 달렸으므로 `erase_screen`이 한다.
    /// ED 3(스크롤백): 줄 번호의 기준이 바뀌므로 주 화면의 그림을 모두 지운다. 초기화(RIS)는 모두 지운다
    pub fn erase(&mut self, visible: bool, scrollback: bool) {
        match (visible, scrollback) {
            (true, true) => self.placements.clear(),
            (false, true) => self.placements.retain(|p| p.alt),
            _ => {}
        }
    }

    /// ED 2를 처리한 뒤(#329, Ghostty `kitty_images.clearScreen`, Terminal.zig 4527~4535줄): 보이는 자리에 걸친 그림을 지우고
    /// 스크롤백에 다 들어간 것만 남긴다. `active_top`은 지금 활성 화면 맨 위의 절대 줄(엔진이 올렸으면 이미 늘어난 스크롤백 크기)이다.
    /// 대체 화면이면 대체 화면 그림만, 주 화면이면 주 화면 그림만 본다
    pub fn erase_screen(&mut self, active_top: i64, in_alt: bool) {
        self.placements
            .retain(|p| p.alt != in_alt || (!in_alt && p.line + i64::from(p.rows) <= active_top));
    }

    /// 대체 화면을 나왔다: 그 화면에 놓인 그림을 지운다
    pub fn left_alt_screen(&mut self) {
        self.placements.retain(|p| !p.alt);
    }

    fn quiet(c: &HashMap<u8, String>) -> u32 {
        num(c, b'q')
    }

    fn ok(&self, c: &HashMap<u8, String>) -> Outcome {
        let id = num(c, b'i');
        if id == 0 || Self::quiet(c) >= 1 {
            return Outcome::default();
        }
        Outcome {
            reply: Some(format!("\x1b_Gi={id};OK\x1b\\").into_bytes()),
            advance: None,
        }
    }

    fn error(&self, c: &HashMap<u8, String>, code: &str, why: &str) -> Outcome {
        let id = num(c, b'i');
        if id == 0 || Self::quiet(c) >= 2 {
            return Outcome::default();
        }
        // 답에는 ASCII만(설명은 앱 로그로). 터미널에 되돌아가는 글이라 제어 문자를 넣지 않는다
        let _ = why;
        Outcome {
            reply: Some(format!("\x1b_Gi={id};{code}\x1b\\").into_bytes()),
            advance: None,
        }
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
        assert_eq!((p.line, p.col, p.cols, p.rows, p.scaled), (100, 4, 3, 2, false));
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
        assert!(g.placements[0].scaled);
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
}
