//! 배경 그림(Ghostty `background-image`, #134): PNG·JPEG를 풀고, 맞추기·자리·반복대로
//! 창 크기의 배경 층을 바탕색 위에 미리 합성한다. 캔버스는 기본 바탕색을 칠할 때 이 층을 베낀다

use bt_core::config::{BgFit, BgPosition};

/// 풀어 둔 그림(RGBA, 곱하지 않은 알파)
#[derive(Debug, Clone)]
pub struct BgImage {
    pub width: u32,
    pub height: u32,
    pub rgba: Vec<u8>,
}

/// 그림 한 변의 상한(px)과 파일 크기 상한
pub const MAX_SIDE: u32 = 10_000;
pub const MAX_FILE: u64 = 64 * 1024 * 1024;

/// PNG·JPEG 바이트를 푼다(첫 바이트로 형식을 가림). 상한을 넘거나 다른 형식이면 오류
pub fn decode(bytes: &[u8]) -> Result<BgImage, String> {
    if bytes.starts_with(b"\x89PNG\r\n\x1a\n") {
        decode_png(bytes)
    } else if bytes.starts_with(&[0xFF, 0xD8, 0xFF]) {
        decode_jpeg(bytes)
    } else {
        Err("PNG나 JPEG가 아닙니다".into())
    }
}

fn too_big(w: u32, h: u32) -> Result<(), String> {
    if w == 0 || h == 0 || w > MAX_SIDE || h > MAX_SIDE {
        return Err(format!("그림 크기 {w}×{h}는 받지 않습니다(한 변 {MAX_SIDE}px까지)"));
    }
    Ok(())
}

fn decode_png(bytes: &[u8]) -> Result<BgImage, String> {
    let mut dec = png::Decoder::new(std::io::Cursor::new(bytes));
    dec.set_transformations(png::Transformations::EXPAND | png::Transformations::STRIP_16);
    let mut reader = dec.read_info().map_err(|e| format!("PNG: {e}"))?;
    let (w, h) = (reader.info().width, reader.info().height);
    // 크기를 먼저 보고 버퍼를 잡는다
    too_big(w, h)?;
    let mut buf = vec![0u8; reader.output_buffer_size()];
    let frame = reader.next_frame(&mut buf).map_err(|e| format!("PNG: {e}"))?;
    let px = &buf[..frame.buffer_size()];
    let rgba: Vec<u8> = match frame.color_type {
        png::ColorType::Rgba => px.to_vec(),
        png::ColorType::Rgb => px.chunks_exact(3).flat_map(|c| [c[0], c[1], c[2], 255]).collect(),
        png::ColorType::GrayscaleAlpha => px.chunks_exact(2).flat_map(|c| [c[0], c[0], c[0], c[1]]).collect(),
        png::ColorType::Grayscale => px.iter().flat_map(|&g| [g, g, g, 255]).collect(),
        png::ColorType::Indexed => return Err("PNG: 팔레트를 풀지 못했습니다".into()),
    };
    Ok(BgImage {
        width: w,
        height: h,
        rgba,
    })
}

fn decode_jpeg(bytes: &[u8]) -> Result<BgImage, String> {
    let opts = zune_core::options::DecoderOptions::default()
        .jpeg_set_out_colorspace(zune_core::colorspace::ColorSpace::RGBA)
        .set_max_width(MAX_SIDE as usize)
        .set_max_height(MAX_SIDE as usize);
    let mut d = zune_jpeg::JpegDecoder::new_with_options(bytes, opts);
    let rgba = d.decode().map_err(|e| format!("JPEG: {e:?}"))?;
    let (w, h) = d.dimensions().ok_or("JPEG: 크기를 읽지 못했습니다")?;
    too_big(w as u32, h as u32)?;
    if rgba.len() != w * h * 4 {
        return Err("JPEG: 픽셀 수가 맞지 않습니다".into());
    }
    Ok(BgImage {
        width: w as u32,
        height: h as u32,
        rgba,
    })
}

/// 파일을 읽어 푼다(크기 상한을 먼저 본다)
pub fn load(path: &std::path::Path) -> Result<BgImage, String> {
    let meta = std::fs::metadata(path).map_err(|e| format!("{}: {e}", path.display()))?;
    if meta.len() > MAX_FILE {
        return Err(format!("{}: 파일이 너무 큽니다({}바이트)", path.display(), meta.len()));
    }
    let bytes = std::fs::read(path).map_err(|e| format!("{}: {e}", path.display()))?;
    decode(&bytes).map_err(|e| format!("{}: {e}", path.display()))
}

/// 그림(`iw`×`ih`)을 `w`×`h` 영역에 놓을 사각형(x, y, 폭, 높이)
pub fn place(fit: BgFit, pos: BgPosition, img: (u32, u32), area: (u32, u32)) -> (i32, i32, u32, u32) {
    let (iw, ih) = (img.0.max(1) as f64, img.1.max(1) as f64);
    let (w, h) = (area.0 as f64, area.1 as f64);
    let (dw, dh) = match fit {
        BgFit::Contain => {
            let s = (w / iw).min(h / ih);
            (iw * s, ih * s)
        }
        BgFit::Cover => {
            let s = (w / iw).max(h / ih);
            (iw * s, ih * s)
        }
        BgFit::Stretch => (w, h),
        BgFit::None => (iw, ih),
    };
    use BgPosition::*;
    let (ax, ay) = match pos {
        TopLeft => (0.0, 0.0),
        TopCenter => (0.5, 0.0),
        TopRight => (1.0, 0.0),
        CenterLeft => (0.0, 0.5),
        Center => (0.5, 0.5),
        CenterRight => (1.0, 0.5),
        BottomLeft => (0.0, 1.0),
        BottomCenter => (0.5, 1.0),
        BottomRight => (1.0, 1.0),
    };
    let (dw, dh) = (dw.round(), dh.round());
    (
        ((w - dw) * ax).round() as i32,
        ((h - dh) * ay).round() as i32,
        dw as u32,
        dh as u32,
    )
}

/// `w`×`h` 배경 층: `base`(바탕색 픽셀, 곱한 알파 ARGB) 위에 그림을 `alpha`(0~1)로 덮는다.
/// `repeat`면 놓은 자리에서 사방으로 이어 깐다
#[allow(clippy::too_many_arguments)]
pub fn compose(
    img: &BgImage,
    w: u32,
    h: u32,
    fit: BgFit,
    pos: BgPosition,
    repeat: bool,
    alpha: f32,
    base: u32,
) -> Vec<u32> {
    let mut out = vec![base; (w * h) as usize];
    let (x, y, dw, dh) = place(fit, pos, (img.width, img.height), (w, h));
    let alpha = alpha.clamp(0.0, 1.0);
    if dw == 0 || dh == 0 || alpha == 0.0 || img.rgba.len() < (img.width * img.height * 4) as usize {
        return out;
    }
    let (dw, dh) = (dw as i64, dh as i64);
    // 놓은 자리 기준 좌표(반복이면 감싼다). 그림 밖이면 없다
    let local = |p: i64, origin: i64, len: i64| {
        let r = p - origin;
        if repeat {
            Some(r.rem_euclid(len))
        } else {
            (0..len).contains(&r).then_some(r)
        }
    };
    let (ba, br, bg, bb) = (base >> 24, (base >> 16) & 0xff, (base >> 8) & 0xff, base & 0xff);
    for py in 0..h as i64 {
        let Some(ry) = local(py, y as i64, dh) else { continue };
        let sy = (ry * img.height as i64 / dh) as usize;
        for px in 0..w as i64 {
            let Some(rx) = local(px, x as i64, dw) else { continue };
            let sx = (rx * img.width as i64 / dw) as usize;
            let s = (sy * img.width as usize + sx) * 4;
            let ia = img.rgba[s + 3] as f32 / 255.0 * alpha;
            if ia <= 0.0 {
                continue;
            }
            // 곱한 알파로 바탕 위에 덮는다
            let mix = |c: u8, b: u32| (c as f32 * ia + b as f32 * (1.0 - ia)).round() as u32;
            let a = (255.0 * ia + ba as f32 * (1.0 - ia)).round() as u32;
            out[(py as u32 * w + px as u32) as usize] =
                (a << 24) | (mix(img.rgba[s], br) << 16) | (mix(img.rgba[s + 1], bg) << 8) | mix(img.rgba[s + 2], bb);
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn png(w: u32, h: u32, rgba: &[u8]) -> Vec<u8> {
        let mut out = Vec::new();
        {
            let mut enc = png::Encoder::new(&mut out, w, h);
            enc.set_color(png::ColorType::Rgba);
            enc.set_depth(png::BitDepth::Eight);
            let mut wr = enc.write_header().unwrap();
            wr.write_image_data(rgba).unwrap();
        }
        out
    }

    #[test]
    fn decodes_png_and_jpeg_and_rejects_others() {
        let img = decode(&png(2, 1, &[255, 0, 0, 255, 0, 0, 255, 128])).unwrap();
        assert_eq!((img.width, img.height), (2, 1));
        assert_eq!(img.rgba, vec![255, 0, 0, 255, 0, 0, 255, 128]);
        let jpg = decode(include_bytes!("../tests/fixtures/red-blue-4x2.jpg")).unwrap();
        assert_eq!((jpg.width, jpg.height, jpg.rgba.len()), (4, 2, 32));
        assert!(
            jpg.rgba[0] > 200 && jpg.rgba[2] < 60,
            "왼쪽은 빨강: {:?}",
            &jpg.rgba[..4]
        );
        assert!(
            jpg.rgba[14] > 200 && jpg.rgba[12] < 60,
            "오른쪽은 파랑: {:?}",
            &jpg.rgba[12..16]
        );
        assert_eq!(jpg.rgba[3], 255, "JPEG는 불투명");
        assert!(decode(b"GIF89a....").is_err());
        assert!(decode(b"").is_err());
        // 상한보다 넓은 그림
        let wide = png(MAX_SIDE + 1, 1, &vec![0u8; ((MAX_SIDE + 1) * 4) as usize]);
        assert!(decode(&wide).is_err());
    }

    #[test]
    fn place_fits_and_aligns_like_ghostty() {
        use BgFit::*;
        use BgPosition::*;
        // 200×100 그림을 400×400 영역에
        assert_eq!(place(Contain, Center, (200, 100), (400, 400)), (0, 100, 400, 200));
        assert_eq!(place(Cover, Center, (200, 100), (400, 400)), (-200, 0, 800, 400));
        assert_eq!(place(Stretch, Center, (200, 100), (400, 400)), (0, 0, 400, 400));
        assert_eq!(place(None, Center, (200, 100), (400, 400)), (100, 150, 200, 100));
        assert_eq!(place(None, TopLeft, (200, 100), (400, 400)), (0, 0, 200, 100));
        assert_eq!(place(None, BottomRight, (200, 100), (400, 400)), (200, 300, 200, 100));
        assert_eq!(place(None, CenterLeft, (200, 100), (400, 400)), (0, 150, 200, 100));
        assert_eq!(place(None, TopCenter, (200, 100), (400, 400)), (100, 0, 200, 100));
    }

    /// 큰 창(3200×2100px)의 층 합성 시간. `cargo test --release -- --ignored --nocapture`로 잰다
    #[test]
    #[ignore]
    fn compose_time_large_window() {
        let img = BgImage {
            width: 1920,
            height: 1080,
            rgba: (0..1920 * 1080 * 4).map(|i| (i % 251) as u8).collect(),
        };
        let mut times = Vec::new();
        for _ in 0..5 {
            let t = std::time::Instant::now();
            let layer = compose(
                &img,
                3200,
                2100,
                BgFit::Cover,
                BgPosition::Center,
                false,
                0.35,
                0xff20_2020,
            );
            times.push(t.elapsed().as_secs_f64() * 1000.0);
            assert_eq!(layer.len(), 3200 * 2100);
        }
        times.sort_by(|a, b| a.partial_cmp(b).unwrap());
        println!(
            "층 합성 3200x2100, 5회: 최소 {:.1}ms 중앙값 {:.1}ms 최대 {:.1}ms",
            times[0], times[2], times[4]
        );
    }

    #[test]
    fn compose_blends_over_the_background_and_tiles() {
        let red = BgImage {
            width: 1,
            height: 1,
            rgba: vec![255, 0, 0, 255],
        };
        let base = 0xff00_0000; // 검정, 불투명
        // 반만 덮으면 반쯤 빨강
        let layer = compose(&red, 2, 2, BgFit::None, BgPosition::TopLeft, false, 0.5, base);
        assert_eq!(layer[0] & 0x00ff_ffff, 0x0080_0000, "{:08x}", layer[0]);
        assert_eq!(layer[1], base, "그림 밖은 바탕 그대로");
        assert_eq!(layer[0] >> 24, 0xff, "불투명한 바탕 위면 불투명");
        // 반복하면 모든 칸에
        let tiled = compose(&red, 3, 2, BgFit::None, BgPosition::Center, true, 1.0, base);
        assert!(tiled.iter().all(|&p| p == 0xffff_0000), "{tiled:08x?}");
    }
}
