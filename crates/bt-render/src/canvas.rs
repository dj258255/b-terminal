//! 0RGB 픽셀 버퍼에 그리는 기본 동작

use bt_core::Rgb;

use crate::font::Glyph;

/// 창의 픽셀 버퍼. 줄 우선, 곱한 알파 ARGB(맨 앞 바이트가 알파). 불투명 창은 알파를 무시하고,
/// 투명 창(#86)은 그대로 넘겨 기본 바탕이 비치고 글자 가장자리가 바르게 섞인다
pub struct Canvas<'a> {
    pub buf: &'a mut [u32],
    pub width: u32,
    pub height: u32,
    /// 세로로 그릴 수 있는 범위 `[위, 아래)`. 바뀐 줄만 다시 그릴 때 이웃 줄을 건드리지 않게 좁힌다
    clip: (i32, i32),
    /// 투명 창: (기본 바탕색 RGB, 그 색으로 칠할 반투명 픽셀). 그 색을 칠하면 반투명으로 칠한다
    see_through: Option<(u32, u32)>,
    /// 배경 그림 층(#134): (기본 바탕색 RGB, 화면 크기의 미리 합성한 픽셀). 그 색을 칠하면 층에서 베낀다
    backdrop: Option<(u32, &'a [u32])>,
    /// `background-opacity-cells`(#177): 색이 있는 바탕 칸을 칠할 알파(0~255)
    cells_alpha: Option<u8>,
}

/// `background-opacity-cells`가 켜져 있고 창이 투명하면 색 바탕 칸을 칠할 알파(0~255). 아니면 없다(#177)
pub fn cells_alpha(opacity: f32, cells: bool) -> Option<u8> {
    (cells && opacity < 1.0).then(|| (opacity.clamp(0.0, 1.0) * 255.0).round() as u8)
}

impl<'a> Canvas<'a> {
    pub fn new(buf: &'a mut [u32], width: u32, height: u32) -> Self {
        assert!(buf.len() >= (width * height) as usize, "버퍼가 화면보다 작다");
        Self {
            buf,
            width,
            height,
            clip: (0, height as i32),
            see_through: None,
            backdrop: None,
            cells_alpha: None,
        }
    }

    /// `background-opacity-cells`(#177): 앞으로 `fill_rect_cell`로 칠하는 색 바탕 칸을 `alpha`만큼만 덮는 반투명으로 칠한다
    pub fn set_cells_alpha(&mut self, alpha: Option<u8>) {
        self.cells_alpha = alpha;
    }

    /// 칸 하나의 바탕을 칠한다. `set_cells_alpha`가 있으면 색 바탕 칸도 반투명으로, 기본 바탕색은 `fill_rect`와 같다
    pub fn fill_rect_cell(&mut self, x: i32, y: i32, w: u32, h: u32, color: Rgb) {
        match self.cells_alpha {
            Some(a) if !self.is_default_bg(color) => self.fill_rect_translucent(x, y, w, h, color, a),
            _ => self.fill_rect(x, y, w, h, color),
        }
    }

    /// 투명 창·배경 그림이 기본 바탕색으로 다루는 색인가
    fn is_default_bg(&self, color: Rgb) -> bool {
        let rgb = color.to_u32();
        self.see_through.is_some_and(|(bg, _)| bg == rgb) || self.backdrop.is_some_and(|(bg, _)| bg == rgb)
    }

    /// 사각형을 곱한 알파 `alpha`의 반투명 단색으로 칠한다(아래 픽셀은 덮어쓴다)
    fn fill_rect_translucent(&mut self, x: i32, y: i32, w: u32, h: u32, color: Rgb, alpha: u8) {
        let x0 = x.max(0) as u32;
        let y0 = y.max(self.clip.0) as u32;
        let x1 = (x + w as i32).clamp(0, self.width as i32) as u32;
        let y1 = (y + h as i32).clamp(0, self.clip.1) as u32;
        if x0 >= x1 || y0 >= y1 {
            return;
        }
        let a = alpha as u32;
        let mul = |c: u8| (c as u32 * a + 127) / 255;
        let px = (a << 24) | (mul(color.r) << 16) | (mul(color.g) << 8) | mul(color.b);
        for row in y0..y1 {
            let start = (row * self.width + x0) as usize;
            let end = (row * self.width + x1) as usize;
            self.buf[start..end].fill(px);
        }
    }

    /// 투명 창(#86): 앞으로 `bg`를 칠하면 `alpha`만큼만 덮는 반투명(곱한 알파)으로 칠한다
    pub fn set_see_through(&mut self, bg: Rgb, alpha: u8) {
        let a = alpha as u32;
        let mul = |c: u8| (c as u32 * a + 127) / 255;
        let px = (a << 24) | (mul(bg.r) << 16) | (mul(bg.g) << 8) | mul(bg.b);
        self.see_through = Some((bg.to_u32(), px));
    }

    /// 배경 그림 층(#134): 앞으로 `bg`를 칠하면 `layer`(화면 크기)의 같은 자리 픽셀을 베낀다. 크기가 맞지 않으면 쓰지 않는다
    pub fn set_backdrop(&mut self, bg: Rgb, layer: &'a [u32]) {
        if layer.len() >= (self.width * self.height) as usize {
            self.backdrop = Some((bg.to_u32(), layer));
        }
    }

    /// 이 색이 배경 그림 층으로 칠할 기본 바탕색이면 그 층
    fn backdrop_for(&self, color: Rgb) -> Option<&'a [u32]> {
        self.backdrop.filter(|(bg, _)| *bg == color.to_u32()).map(|(_, l)| l)
    }

    /// 칠할 픽셀 값: 보통은 불투명, 투명 창의 기본 바탕색이면 반투명
    fn solid(&self, color: Rgb) -> u32 {
        let rgb = color.to_u32();
        match self.see_through {
            Some((bg, px)) if bg == rgb => px,
            _ => 0xff00_0000 | rgb,
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
        if let Some(layer) = self.backdrop_for(color) {
            self.buf[..n].copy_from_slice(&layer[..n]);
            return;
        }
        let c = self.solid(color);
        self.buf[..n].fill(c);
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
        if let Some(layer) = self.backdrop_for(color) {
            for row in y0..y1 {
                let start = (row * self.width + x0) as usize;
                let end = (row * self.width + x1) as usize;
                self.buf[start..end].copy_from_slice(&layer[start..end]);
            }
            return;
        }
        let c = self.solid(color);
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

    /// 픽셀 하나를 덮음 `alpha`(0~255)로 얹는다. 화면 밖과 그리기 범위 밖은 무시한다(밑줄의 부드러운 가장자리, #276)
    pub fn blend_px(&mut self, x: i32, y: i32, color: Rgb, alpha: u8) {
        if x < 0 || x >= self.width as i32 || y < self.clip.0 || y >= self.clip.1 {
            return;
        }
        let i = (y as u32 * self.width + x as u32) as usize;
        self.buf[i] = blend(self.buf[i], color, alpha as u32);
    }

    /// 글리프를 기준선 자리(x, baseline_y)에 덮는다
    pub fn draw_glyph(&mut self, x: i32, baseline_y: i32, glyph: &Glyph, color: Rgb) {
        // 색 글리프는 bitmap이 비어 있다(draw_color_glyph로 그린다). 잘못 들어와도 죽지 않게(#112)
        if glyph.bitmap.len() < glyph.width * glyph.height {
            return;
        }
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

    /// 색 글리프(RGBA)를 기준선 자리(x, baseline_y)에 덮는다
    pub fn draw_color_glyph(&mut self, x: i32, baseline_y: i32, glyph: &Glyph) {
        let Some(rgba) = &glyph.color else { return };
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
                let s = (row * glyph.width + col) * 4;
                let a = rgba[s + 3] as u32;
                if a == 0 {
                    continue;
                }
                let i = (py as u32 * self.width + px as u32) as usize;
                self.buf[i] = blend(self.buf[i], Rgb::new(rgba[s], rgba[s + 1], rgba[s + 2]), a);
            }
        }
    }

    /// RGBA 그림(`iw × ih`)을 (x, y)에서 `w × h`로 늘려(가장 가까운 픽셀) 덮는다. `clip`(x0, y0, x1, y1) 밖은 그리지 않는다(#76)
    #[allow(clippy::too_many_arguments)]
    pub fn draw_rgba(
        &mut self,
        x: i32,
        y: i32,
        w: u32,
        h: u32,
        rgba: &[u8],
        iw: u32,
        ih: u32,
        clip: (i32, i32, i32, i32),
    ) {
        self.draw_rgba_src(x, y, w, h, rgba, iw, ih, [0, 0, iw, ih], clip);
    }

    /// `draw_rgba`에서 그림의 일부 `src`(`x, y, w, h`, 그림 픽셀)만 `w × h`로 늘려 덮는다(kitty 그림 잘라내기, #547).
    /// `src`가 비었거나 시작점이 그림 밖이면 아무것도 그리지 않는다. 끝이 그림을 넘는 부분은 가장자리 픽셀을 쓴다:
    /// 자리표시 조각(#553)은 Ghostty와 같은 반올림으로 그림 끝을 1px 넘을 수 있고, Ghostty는 이 값을 텍스처 좌표로 써서
    /// 샘플러가 가장자리로 잡아 그린다(#747)
    #[allow(clippy::too_many_arguments)]
    pub fn draw_rgba_src(
        &mut self,
        x: i32,
        y: i32,
        w: u32,
        h: u32,
        rgba: &[u8],
        iw: u32,
        ih: u32,
        src: [u32; 4],
        clip: (i32, i32, i32, i32),
    ) {
        let [sx0, sy0, sw, sh] = src;
        if w == 0
            || h == 0
            || sw == 0
            || sh == 0
            || iw == 0
            || ih == 0
            || sx0 >= iw
            || sy0 >= ih
            || rgba.len() < iw as usize * ih as usize * 4
        {
            return;
        }
        let x0 = x.max(clip.0).max(0);
        let y0 = y.max(clip.1).max(self.clip.0);
        let x1 = (x as i64 + w as i64).min(clip.2 as i64).min(self.width as i64) as i32;
        let y1 = (y as i64 + h as i64).min(clip.3 as i64).min(self.clip.1 as i64) as i32;
        for py in y0..y1 {
            let sy = (sy0 as u64 + (py - y) as u64 * sh as u64 / h as u64).min(ih as u64 - 1) as u32;
            for px in x0..x1 {
                let sx = (sx0 as u64 + (px - x) as u64 * sw as u64 / w as u64).min(iw as u64 - 1) as u32;
                let s = (sy as usize * iw as usize + sx as usize) * 4;
                let a = rgba[s + 3] as u32;
                if a == 0 {
                    continue;
                }
                let i = (py as u32 * self.width + px as u32) as usize;
                self.buf[i] = blend(self.buf[i], Rgb::new(rgba[s], rgba[s + 1], rgba[s + 2]), a);
            }
        }
    }

    /// 픽셀의 색(알파는 버린다)
    pub fn pixel(&self, x: u32, y: u32) -> Rgb {
        Rgb::from_u32(self.buf[(y * self.width + x) as usize])
    }
}

/// 불투명한 `src`를 덮음 `a`(0~255)만큼 곱한 알파 픽셀 `dst` 위에 얹는다(over). 바탕이 불투명이면 알파는 ff 그대로
fn blend(dst: u32, src: Rgb, a: u32) -> u32 {
    if a >= 255 {
        return 0xff00_0000 | src.to_u32();
    }
    let inv = 255 - a;
    let ch = |shift: u32, s: u32| -> u32 {
        let d = (dst >> shift) & 0xff;
        ((s * a + d * inv + 127) / 255) << shift
    };
    ch(24, 255) | ch(16, src.r as u32) | ch(8, src.g as u32) | ch(0, src.b as u32)
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
        // 불투명 바탕(알파 ff) 위에 섞으면 알파는 ff 그대로
        assert_eq!(blend(0xff00_0000, Rgb::new(255, 255, 255), 128), 0xff80_8080);
        assert_eq!(blend(0xff12_3456, Rgb::new(1, 2, 3), 255), 0xff01_0203);
    }

    /// #134: 배경 그림 층이 있으면 기본 바탕색 칠하기만 층에서 베낀다
    #[test]
    fn backdrop_replaces_only_the_default_background() {
        let (w, h) = (4u32, 2u32);
        let layer: Vec<u32> = (0..w * h).map(|i| 0xff00_0000 | i).collect();
        let mut buf = vec![0u32; (w * h) as usize];
        let mut c = Canvas::new(&mut buf, w, h);
        let bg = Rgb::new(10, 20, 30);
        c.set_backdrop(bg, &layer);
        c.clear(bg);
        assert_eq!(c.buf, &layer[..], "기본 바탕은 층 그대로");
        c.fill_rect(1, 1, 2, 1, Rgb::new(255, 0, 0));
        assert_eq!(c.buf[5], 0xffff_0000, "다른 색은 그대로 칠한다");
        c.fill_rect(0, 1, 4, 1, bg);
        assert_eq!(&c.buf[4..8], &layer[4..8], "기본 바탕으로 다시 칠하면 층이 돌아온다");
        // 크기가 맞지 않는 층은 쓰지 않는다
        let small = vec![0xff12_3456u32; 3];
        let mut buf2 = vec![0u32; (w * h) as usize];
        let mut c2 = Canvas::new(&mut buf2, w, h);
        c2.set_backdrop(bg, &small);
        c2.clear(bg);
        assert_eq!(c2.buf[0], 0xff00_0000 | bg.to_u32());
    }

    /// #86: 투명 창이면 기본 바탕색은 반투명(곱한 알파)으로 칠하고, 그 위의 글자 가장자리는 덮음 정도만큼 알파가 오른다
    #[test]
    fn see_through_background_blends_alpha() {
        let bg = Rgb::new(40, 44, 52);
        let mut buf = vec![0u32; 4];
        let mut c = Canvas::new(&mut buf, 2, 2);
        c.set_see_through(bg, 128);
        c.clear(bg);
        // 반투명 바탕: 알파 128, 색은 곱한 값
        assert_eq!(c.buf[0], 0x8014_161a);
        // 다른 색은 불투명
        c.fill_rect(1, 0, 1, 1, Rgb::new(255, 0, 0));
        assert_eq!(c.buf[1] >> 24, 0xff);
        // 흰 글자를 덮음 50%로 섞은 가장자리: 알파는 128과 255 사이, 곱한 색은 알파를 넘지 않는다
        c.buf[2] = blend(c.buf[2], Rgb::new(255, 255, 255), 128);
        let a = c.buf[2] >> 24;
        assert!(a > 128 && a < 255, "알파 {a}");
        assert!((c.buf[2] >> 16 & 0xff) <= a);
        // 덮음 0이면 그대로, 255면 불투명 흰색
        let before = c.buf[3];
        c.buf[3] = blend(c.buf[3], Rgb::new(255, 255, 255), 0);
        assert_eq!(c.buf[3], before);
        assert_eq!(blend(before, Rgb::new(255, 255, 255), 255), 0xffff_ffff);
    }

    /// #177: 색 바탕 칸의 알파는 투명 창에서 옵션을 켰을 때만 있다
    #[test]
    fn cells_alpha_needs_transparent_window_and_option() {
        assert_eq!(cells_alpha(0.5, true), Some(128));
        assert_eq!(cells_alpha(0.0, true), Some(0));
        assert_eq!(cells_alpha(0.5, false), None);
        assert_eq!(cells_alpha(1.0, true), None, "불투명 창이면 할 일이 없다");
        assert_eq!(cells_alpha(7.0, true), None);
    }

    /// #177: 기본은 색 바탕 칸이 불투명이고, 켜면 곱한 알파로 비친다. 기본 바탕색 칸은 둘 다 같다
    #[test]
    fn colored_cells_become_translucent_only_with_cells_alpha() {
        let bg = Rgb::new(40, 44, 52);
        let red = Rgb::new(200, 100, 50);
        let mut buf = vec![0u32; 4];
        let mut c = Canvas::new(&mut buf, 4, 1);
        c.set_see_through(bg, 128);
        c.fill_rect_cell(0, 0, 1, 1, bg);
        c.fill_rect_cell(1, 0, 1, 1, red);
        assert_eq!(c.buf[0], 0x8014_161a, "기본 바탕은 반투명");
        assert_eq!(c.buf[1], 0xffc8_6432, "옵션이 없으면 색 칸은 불투명");
        c.set_cells_alpha(Some(128));
        c.fill_rect_cell(2, 0, 1, 1, red);
        // 알파 128, 곱한 색: 200*128/255=100(0x64), 100->50(0x32), 50->25(0x19)
        assert_eq!(c.buf[2], 0x8064_3219);
        c.fill_rect_cell(3, 0, 1, 1, bg);
        assert_eq!(c.buf[3], 0x8014_161a, "기본 바탕색은 그대로 반투명");
        // fill_rect(UI 등)는 옵션의 영향을 받지 않는다
        c.fill_rect(0, 0, 1, 1, red);
        assert_eq!(c.buf[0], 0xffc8_6432);
        // 끄면 다시 불투명
        c.set_cells_alpha(None);
        c.fill_rect_cell(2, 0, 1, 1, red);
        assert_eq!(c.buf[2], 0xffc8_6432);
    }
}
