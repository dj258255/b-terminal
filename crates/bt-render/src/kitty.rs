//! kitty 그림을 칸 격자 위에 그린다(#76, #547). 어디에 얼마나 그릴지는 `bt_core::kitty_geom`이 정하고(저장소와 같은 규칙),
//! 여기서는 그 값대로 캔버스에 찍는다. CPU 길(`redraw`)과 GPU 길의 층(아래·가운데·위)이 모두 이 함수를 쓰므로 같은 픽셀이 나온다.
//! bt-term을 모른다: 앱이 `ImageView`를 `ImagePaint`로 옮겨 담는다

use bt_core::kitty_geom::{Geom, Layer, layer_of_z};

use crate::canvas::Canvas;

/// 그릴 그림 하나
#[derive(Debug, Clone, Copy)]
pub struct ImagePaint<'a> {
    /// 곱하지 않은 알파 RGBA(`geom.img_w × geom.img_h`)
    pub rgba: &'a [u8],
    pub geom: Geom,
    /// 왼쪽 위 칸(칸 격자 기준. 위나 왼쪽으로 일부 가려졌으면 음수)
    pub row: i32,
    pub col: i32,
    pub z: i32,
}

/// `layer`에 그릴 그림이 있는가
pub fn has_layer(images: &[ImagePaint<'_>], layer: Layer) -> bool {
    images.iter().any(|p| layer_of_z(p.z) == layer)
}

/// `layer`의 그림을 z가 낮은 것부터 그린다(z가 같으면 놓인 차례). `origin`은 칸 격자 왼쪽 위의 픽셀 자리, `cell`은 칸 크기(px),
/// `clip`(x0, y0, x1, y1) 밖은 그리지 않는다(칸 격자 안으로 자른다)
pub fn paint_layer(
    canvas: &mut Canvas,
    images: &[ImagePaint<'_>],
    layer: Layer,
    origin: (i32, i32),
    cell: (u32, u32),
    clip: (i32, i32, i32, i32),
) {
    let mut order: Vec<&ImagePaint<'_>> = images.iter().filter(|p| layer_of_z(p.z) == layer).collect();
    order.sort_by_key(|p| p.z);
    for p in order {
        let r = p.geom.resolve(cell.0, cell.1);
        if r.dest_w == 0 || r.dest_h == 0 {
            continue;
        }
        let x = origin.0 as i64 + (i64::from(p.col) * cell.0 as i64) + r.off_x as i64;
        let y = origin.1 as i64 + (p.row as i64 * cell.1 as i64) + r.off_y as i64;
        let (Ok(x), Ok(y)) = (i32::try_from(x), i32::try_from(y)) else {
            continue;
        };
        canvas.draw_rgba_src(
            x,
            y,
            r.dest_w,
            r.dest_h,
            p.rgba,
            p.geom.img_w,
            p.geom.img_h,
            r.src,
            clip,
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use bt_core::Rgb;

    /// `w × h` 그림: 픽셀 (x, y)는 (x*10, y*10, 7, 255)
    fn pattern(w: u32, h: u32) -> Vec<u8> {
        let mut v = Vec::new();
        for y in 0..h {
            for x in 0..w {
                v.extend_from_slice(&[(x * 10) as u8, (y * 10) as u8, 7, 255]);
            }
        }
        v
    }

    fn geom(w: u32, h: u32) -> Geom {
        Geom {
            img_w: w,
            img_h: h,
            ..Geom::default()
        }
    }

    fn px(buf: &[u32], w: u32, x: u32, y: u32) -> u32 {
        buf[(y * w + x) as usize] & 0x00ff_ffff
    }

    const ALL: (i32, i32, i32, i32) = (0, 0, 1000, 1000);

    #[test]
    fn crop_draws_only_the_source_rect_at_native_size() {
        let rgba = pattern(8, 8);
        let mut buf = vec![0u32; 40 * 40];
        let mut c = Canvas::new(&mut buf, 40, 40);
        let g = Geom {
            src: [2, 3, 3, 2],
            ..geom(8, 8)
        };
        let p = ImagePaint {
            rgba: &rgba,
            geom: g,
            row: 1,
            col: 2,
            z: 0,
        };
        // 칸 10×10: 칸 (2, 1) = 픽셀 (20, 10)
        paint_layer(&mut c, &[p], Layer::AboveText, (0, 0), (10, 10), ALL);
        // 그림의 (2, 3)이 (20, 10)에, 3×2px만
        assert_eq!(px(&buf, 40, 20, 10), 0x0014_1e07, "첫 픽셀은 원본 (2, 3)");
        assert_eq!(px(&buf, 40, 22, 11), 0x0028_2807, "원본 (4, 4)");
        assert_eq!(px(&buf, 40, 23, 10), 0, "잘라낸 너비 밖");
        assert_eq!(px(&buf, 40, 20, 12), 0, "잘라낸 높이 밖");
    }

    #[test]
    fn offset_moves_the_image_inside_the_first_cell() {
        let rgba = pattern(4, 4);
        let mut buf = vec![0u32; 40 * 40];
        let mut c = Canvas::new(&mut buf, 40, 40);
        let g = Geom {
            off: [3, 4],
            ..geom(4, 4)
        };
        let p = ImagePaint {
            rgba: &rgba,
            geom: g,
            row: 0,
            col: 1,
            z: 0,
        };
        paint_layer(&mut c, &[p], Layer::AboveText, (5, 5), (10, 10), ALL);
        // 원점 (5, 5) + 칸 (1, 0) -> (15, 5), 오프셋 (3, 4) -> (18, 9)
        assert_eq!(px(&buf, 40, 17, 9), 0);
        assert_eq!(px(&buf, 40, 18, 8), 0);
        assert_eq!(px(&buf, 40, 18, 9), 0x0000_0007);
        assert_eq!(px(&buf, 40, 21, 12), 0x001e_1e07, "마지막 픽셀 (3, 3)");
        assert_eq!(px(&buf, 40, 22, 12), 0);
    }

    #[test]
    fn scaled_crop_stretches_the_source_to_the_cells() {
        let rgba = pattern(8, 8);
        let mut buf = vec![0u32; 40 * 40];
        let mut c = Canvas::new(&mut buf, 40, 40);
        // 원본 (4, 4)부터 2×2를 2칸×2줄(20×20px)에 늘린다
        let g = Geom {
            src: [4, 4, 2, 2],
            want_cols: 2,
            want_rows: 2,
            ..geom(8, 8)
        };
        let p = ImagePaint {
            rgba: &rgba,
            geom: g,
            row: 0,
            col: 0,
            z: 0,
        };
        paint_layer(&mut c, &[p], Layer::AboveText, (0, 0), (10, 10), ALL);
        assert_eq!(px(&buf, 40, 0, 0), 0x0028_2807, "원본 (4, 4)");
        assert_eq!(px(&buf, 40, 9, 9), 0x0028_2807);
        assert_eq!(px(&buf, 40, 10, 0), 0x0032_2807, "원본 (5, 4)");
        assert_eq!(px(&buf, 40, 0, 10), 0x0028_3207, "원본 (4, 5)");
        assert_eq!(px(&buf, 40, 20, 0), 0);
    }

    fn one<'a>(rgba: &'a [u8], z: i32) -> ImagePaint<'a> {
        ImagePaint {
            rgba,
            geom: Geom {
                want_cols: 1,
                want_rows: 1,
                ..geom(1, 1)
            },
            row: 0,
            col: 0,
            z,
        }
    }

    #[test]
    fn layers_draw_only_their_own_z_range_and_lower_z_first() {
        let red = vec![255, 0, 0, 255];
        let blue = vec![0, 0, 255, 255];
        // 파랑(z=3)을 뒤에 넘겨도 빨강(z=5)이 위에 온다
        let imgs = [one(&red, 5), one(&blue, 3)];
        let mut buf = vec![0u32; 100];
        let mut c = Canvas::new(&mut buf, 10, 10);
        paint_layer(&mut c, &imgs, Layer::AboveText, (0, 0), (10, 10), ALL);
        assert_eq!(c.pixel(5, 5), Rgb::new(255, 0, 0));
        let imgs = [one(&red, -1), one(&blue, i32::MIN)];
        assert!(has_layer(&imgs, Layer::BelowText) && has_layer(&imgs, Layer::BelowBackground));
        assert!(!has_layer(&imgs, Layer::AboveText));
        let mut buf = vec![0u32; 100];
        let mut c = Canvas::new(&mut buf, 10, 10);
        paint_layer(&mut c, &imgs, Layer::AboveText, (0, 0), (10, 10), ALL);
        assert_eq!(c.pixel(5, 5), Rgb::new(0, 0, 0), "다른 층은 그리지 않는다");
        paint_layer(&mut c, &imgs, Layer::BelowText, (0, 0), (10, 10), ALL);
        assert_eq!(c.pixel(5, 5), Rgb::new(255, 0, 0));
        paint_layer(&mut c, &imgs, Layer::BelowBackground, (0, 0), (10, 10), ALL);
        assert_eq!(c.pixel(5, 5), Rgb::new(0, 0, 255));
    }

    #[test]
    fn empty_crop_and_clip_draw_nothing() {
        let rgba = pattern(4, 4);
        let mut buf = vec![0u32; 100];
        let mut c = Canvas::new(&mut buf, 10, 10);
        let g = Geom {
            src: [9, 9, 1, 1],
            ..geom(4, 4)
        };
        let p = ImagePaint {
            rgba: &rgba,
            geom: g,
            row: 0,
            col: 0,
            z: 0,
        };
        paint_layer(&mut c, &[p], Layer::AboveText, (0, 0), (10, 10), ALL);
        let p = ImagePaint { geom: geom(4, 4), ..p };
        paint_layer(&mut c, &[p], Layer::AboveText, (0, 0), (10, 10), (0, 0, 0, 0));
        assert!(buf.iter().all(|&v| v == 0));
    }

    /// yazi가 96×96 그림을 `U=1`(칸 수 없음)로 놓고 자리표시 6칸×3줄을 찍은 경우(#747). 칸 16×35px이면 격자 96×105px 가운데에
    /// 위·아래 여백 4.5px로 그림이 놓인다. 끝 줄 조각은 Ghostty와 같은 반올림으로 원본 y 66, 높이 31이 되어 그림 끝(96)을 1px 넘는데,
    /// Ghostty는 가장자리 픽셀로 잡아 그린다. 세 줄 모두 그려져 96px 높이가 다 보여야 한다
    #[test]
    fn unicode_placeholder_rows_draw_even_when_rounding_overshoots_the_image_edge() {
        use bt_core::kitty_geom::Piece;
        let (cw, ch) = (16u32, 35u32);
        let rgba: Vec<u8> = [230u8, 30, 30, 255].repeat(96 * 96);
        let (w, h) = (6 * cw, 3 * ch);
        let mut buf = vec![0u32; (w * h) as usize];
        let mut c = Canvas::new(&mut buf, w, h);
        let rows: Vec<ImagePaint<'_>> = (0..3)
            .map(|row| ImagePaint {
                rgba: &rgba,
                geom: Geom {
                    piece: Some(Piece {
                        grid_cols: 0,
                        grid_rows: 0,
                        col: 0,
                        row,
                        width: 6,
                    }),
                    ..geom(96, 96)
                },
                row: row as i32,
                col: 0,
                z: 0,
            })
            .collect();
        let last = rows[2].geom.resolve(cw, ch);
        assert_eq!(
            last.src,
            [0, 66, 96, 31],
            "Ghostty와 같은 반올림: 끝 줄 조각이 그림을 1px 넘는다"
        );
        paint_layer(
            &mut c,
            &rows,
            Layer::AboveText,
            (0, 0),
            (cw, ch),
            (0, 0, w as i32, h as i32),
        );
        let red: Vec<u32> = (0..h).filter(|&y| c.pixel(48, y) == Rgb::new(230, 30, 30)).collect();
        assert_eq!(
            (red.first().copied(), red.last().copied(), red.len()),
            (Some(5), Some(100), 96),
            "가운데 세로줄: 여백 4.5px 아래로 96px 모두 그려진다(끝 줄 칸은 y 70~104)"
        );
    }
}
