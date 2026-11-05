//! 0RGB 픽셀 버퍼에 그리는 기본 동작

use bt_core::Rgb;

use crate::font::Glyph;

/// 창의 픽셀 버퍼(softbuffer와 같은 0RGB). 줄 우선
pub struct Canvas<'a> {
    pub buf: &'a mut [u32],
    pub width: u32,
    pub height: u32,
    /// 세로로 그릴 수 있는 범위 `[위, 아래)`. 바뀐 줄만 다시 그릴 때 이웃 줄을 건드리지 않게 좁힌다
    clip: (i32, i32),
}

impl<'a> Canvas<'a> {
    pub fn new(buf: &'a mut [u32], width: u32, height: u32) -> Self {
        assert!(buf.len() >= (width * height) as usize, "버퍼가 화면보다 작다");
        Self {
            buf,
            width,
            height,
            clip: (0, height as i32),
        }
    }

    /// 세로 그리기 범위를 `[top, bottom)`으로 좁힌다(화면 안으로 묶는다)
    pub fn set_clip_y(&mut self, top: i32, bottom: i32) {
        self.clip = (top.clamp(0, self.height as i32), bottom.clamp(0, self.height as i32));
    }

    pub fn reset_clip(&mut self) {
        self.clip = (0, self.height as i32);
    }

    pub fn clear(&mut self, color: Rgb) {
        let n = (self.width * self.height) as usize;
        self.buf[..n].fill(color.to_u32());
    }

    /// 사각형을 칠한다. 화면 밖은 자른다
    pub fn fill_rect(&mut self, x: i32, y: i32, w: u32, h: u32, color: Rgb) {
        let x0 = x.max(0) as u32;
        let y0 = y.max(self.clip.0) as u32;
        let x1 = (x + w as i32).clamp(0, self.width as i32) as u32;
        let y1 = (y + h as i32).clamp(0, self.clip.1) as u32;
        if x0 >= x1 || y0 >= y1 {
            return;
        }
        let c = color.to_u32();
        for row in y0..y1 {
            let start = (row * self.width + x0) as usize;
            let end = (row * self.width + x1) as usize;
            self.buf[start..end].fill(c);
        }
    }

    /// 테두리만 그린다
    pub fn stroke_rect(&mut self, x: i32, y: i32, w: u32, h: u32, t: u32, color: Rgb) {
        self.fill_rect(x, y, w, t, color);
        self.fill_rect(x, y + h as i32 - t as i32, w, t, color);
        self.fill_rect(x, y, t, h, color);
        self.fill_rect(x + w as i32 - t as i32, y, t, h, color);
    }

    /// 사각형 안을 반투명으로 덮는다(alpha 0~255)
    pub fn blend_rect(&mut self, x: i32, y: i32, w: u32, h: u32, color: Rgb, alpha: u8) {
        let x0 = x.max(0) as u32;
        let y0 = y.max(self.clip.0) as u32;
        let x1 = (x + w as i32).clamp(0, self.width as i32) as u32;
        let y1 = (y + h as i32).clamp(0, self.clip.1) as u32;
        for row in y0..y1 {
            for col in x0..x1 {
                let i = (row * self.width + col) as usize;
                self.buf[i] = blend(self.buf[i], color, alpha as u32);
            }
        }
    }

    /// 글리프를 기준선 자리(x, baseline_y)에 덮는다
    pub fn draw_glyph(&mut self, x: i32, baseline_y: i32, glyph: &Glyph, color: Rgb) {
        let gx = x + glyph.left;
        let gy = baseline_y - glyph.top;
        for row in 0..glyph.height {
            let py = gy + row as i32;
            if py < self.clip.0 || py >= self.clip.1 {
                continue;
            }
            for col in 0..glyph.width {
                let px = gx + col as i32;
                if px < 0 || px >= self.width as i32 {
                    continue;
                }
                let a = glyph.bitmap[row * glyph.width + col] as u32;
                if a == 0 {
                    continue;
                }
                let i = (py as u32 * self.width + px as u32) as usize;
                self.buf[i] = blend(self.buf[i], color, a);
            }
        }
    }

    pub fn pixel(&self, x: u32, y: u32) -> Rgb {
        Rgb::from_u32(self.buf[(y * self.width + x) as usize])
    }
}

fn blend(dst: u32, src: Rgb, a: u32) -> u32 {
    if a >= 255 {
        return src.to_u32();
    }
    let inv = 255 - a;
    let ch = |shift: u32, s: u8| -> u32 {
        let d = (dst >> shift) & 0xff;
        ((s as u32 * a + d * inv + 127) / 255) << shift
    };
    ch(16, src.r) | ch(8, src.g) | ch(0, src.b)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fill_rect_clips_to_bounds() {
        let mut buf = vec![0u32; 16];
        let mut c = Canvas::new(&mut buf, 4, 4);
        c.fill_rect(-2, -2, 4, 4, Rgb::new(255, 0, 0));
        assert_eq!(c.pixel(0, 0), Rgb::new(255, 0, 0));
        assert_eq!(c.pixel(1, 1), Rgb::new(255, 0, 0));
        assert_eq!(c.pixel(2, 2), Rgb::new(0, 0, 0));
        c.fill_rect(3, 3, 10, 10, Rgb::new(0, 255, 0));
        assert_eq!(c.pixel(3, 3), Rgb::new(0, 255, 0));
    }

    #[test]
    fn blend_half() {
        assert_eq!(blend(0x000000, Rgb::new(255, 255, 255), 128), 0x808080);
        assert_eq!(blend(0x123456, Rgb::new(1, 2, 3), 255), 0x010203);
    }
}
