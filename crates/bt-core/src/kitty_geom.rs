//! kitty 그래픽 배치의 기하(Ghostty `graphics_storage.zig`의 `Placement`: `sourceRect`·`cellOffset`·`pixelSize`·`gridSize`와
//! `render.zig`의 `imageLayerFromZ`). 그림 저장소(bt-term)와 그리는 쪽(bt-render)이 같은 규칙을 쓰려고 여기에 한 벌만 둔다.
//! 창도 VT 엔진도 모른다

/// 놓인 그림 하나의 기하 입력: 프로그램이 준 값(원본 그대로)과 그림 크기
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub struct Geom {
    /// 그림의 원래 픽셀 크기
    pub img_w: u32,
    pub img_h: u32,
    /// 잘라낼 직사각형 `x, y, w, h`(원본 그림 픽셀). 너비·높이 0은 그림 끝까지
    pub src: [u32; 4],
    /// 요청한 칸 수(`c`, `r`). 0이면 정하지 않음
    pub want_cols: u32,
    pub want_rows: u32,
    /// 첫 칸 안에서 그림이 시작하는 자리(`X`, `Y`, 픽셀). 칸 크기 안으로 자른다
    pub off: [u32; 2],
    /// 유니코드 자리표시 조각(#553). 있으면 `src`·`off`·`want_*`는 보지 않고 가상 배치의 칸 격자에서 이 조각이 보여 줄 부분을 푼다
    pub piece: Option<Piece>,
}

/// 가상 배치(`U=1`)의 칸 격자 안에서 자리표시 칸 이어진 한 줄이 보여 주는 조각(Ghostty `graphics_unicode.zig`의 `Placement`).
/// 칸 크기에 달린 값(그림을 칸에 맞춘 크기·여백)은 그리는 쪽이 자기 칸 크기로 `Geom::resolve`해서 정한다
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub struct Piece {
    /// 가상 배치가 요청한 칸 수(`c`, `r`). 0이면 그림 크기로 정한다
    pub grid_cols: u32,
    pub grid_rows: u32,
    /// 조각 첫 칸이 보여 주는 격자 안의 칸(0부터)
    pub col: u32,
    pub row: u32,
    /// 조각이 이어진 칸 수(한 줄)
    pub width: u32,
}

/// 칸 크기에 맞춰 푼 값
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Resolved {
    /// 그림에서 실제로 쓰는 부분 `x, y, w, h`(그림 안으로 자른 값)
    pub src: [u32; 4],
    /// 화면에 그릴 크기(픽셀)
    pub dest_w: u32,
    pub dest_h: u32,
    /// 첫 칸 안 오프셋(칸 크기보다 작게 자른 값)
    pub off_x: u32,
    pub off_y: u32,
    /// 차지하는 칸 수. 칸 크기를 모르면(0) 0
    pub cols: u32,
    pub rows: u32,
}

/// 그리는 층: 칸 바탕색 아래, 칸 바탕 위·글자 아래, 글자 위
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Layer {
    BelowBackground,
    BelowText,
    AboveText,
}

/// z 순서가 들어가는 층(Ghostty `imageLayerFromZ`, kitty와 같다): `i32::MIN / 2`보다 작으면 칸 바탕 아래, 0보다 작으면 글자 아래, 아니면 글자 위
pub fn layer_of_z(z: i32) -> Layer {
    if z < i32::MIN / 2 {
        Layer::BelowBackground
    } else if z < 0 {
        Layer::BelowText
    } else {
        Layer::AboveText
    }
}

/// `value * num / den`을 가장 가까운 정수로(Ghostty `scaleDimension`). 넘치면 `u32::MAX`
fn scale(value: u32, num: u32, den: u32) -> u32 {
    if den == 0 {
        return 0;
    }
    let r = (value as u64 * num as u64 + den as u64 / 2) / den as u64;
    u32::try_from(r).unwrap_or(u32::MAX)
}

fn div_ceil(a: u32, b: u32) -> u32 {
    if b == 0 { 0 } else { a.div_ceil(b) }
}

impl Geom {
    /// 그림 안으로 자른 직사각형(Ghostty `sourceRect`): 너비·높이 0은 그림 끝까지, 그림 밖으로 나가는 부분은 자른다
    pub fn source_rect(&self) -> [u32; 4] {
        let x = self.src[0].min(self.img_w);
        let y = self.src[1].min(self.img_h);
        let w = if self.src[2] > 0 { self.src[2] } else { self.img_w };
        let h = if self.src[3] > 0 { self.src[3] } else { self.img_h };
        [x, y, w.min(self.img_w - x), h.min(self.img_h - y)]
    }

    /// 칸 크기(픽셀)로 푼다(Ghostty `cellOffset`·`pixelSize`·`gridSize`). 칸 크기를 모르면(0) 칸 수와 오프셋은 0이다
    pub fn resolve(&self, cell_w: u32, cell_h: u32) -> Resolved {
        if let Some(piece) = &self.piece {
            return piece.resolve(self.img_w, self.img_h, cell_w, cell_h);
        }
        let src = self.source_rect();
        let off_x = if cell_w > 0 { self.off[0].min(cell_w - 1) } else { 0 };
        let off_y = if cell_h > 0 { self.off[1].min(cell_h - 1) } else { 0 };
        let (sw, sh) = (src[2], src[3]);
        let (want_c, want_r) = (self.want_cols, self.want_rows);
        let (dest_w, dest_h) = if want_c == 0 && want_r == 0 {
            (sw, sh)
        } else if want_c > 0 && want_r > 0 {
            (
                cell_w.saturating_mul(want_c).saturating_sub(off_x),
                cell_h.saturating_mul(want_r).saturating_sub(off_y),
            )
        } else if want_c > 0 {
            let w = cell_w.saturating_mul(want_c).saturating_sub(off_x);
            (w, scale(w, sh, sw))
        } else {
            let h = cell_h.saturating_mul(want_r).saturating_sub(off_y);
            (scale(h, sw, sh), h)
        };
        let (cols, rows) = if want_c > 0 && want_r > 0 {
            (want_c, want_r)
        } else {
            (
                div_ceil(dest_w.saturating_add(off_x), cell_w),
                div_ceil(dest_h.saturating_add(off_y), cell_h),
            )
        };
        Resolved {
            src,
            dest_w,
            dest_h,
            off_x,
            off_y,
            cols,
            rows,
        }
    }
}

impl Piece {
    /// 조각이 그릴 곳을 푼다(Ghostty `graphics_unicode.zig`의 `renderPlacement`, 같은 부동소수점 식과 반올림).
    /// 그림을 격자 칸 수에 비율을 지켜 맞추고(남는 쪽은 가운데), 조각에 해당하는 부분만 잘라 낸다.
    /// `Resolved.off_*`는 조각 첫 칸 안의 여백(픽셀), `dest_*`는 그릴 크기, `src`는 그림에서 쓰는 부분이다.
    /// 칸 크기를 모르거나 격자가 u16 칸을 넘거나 보이는 부분이 없으면 모두 0이다(그리지 않는다)
    fn resolve(&self, img_w: u32, img_h: u32, cell_w: u32, cell_h: u32) -> Resolved {
        const NONE: Resolved = Resolved {
            src: [0; 4],
            dest_w: 0,
            dest_h: 0,
            off_x: 0,
            off_y: 0,
            cols: 0,
            rows: 0,
        };
        if cell_w == 0 || cell_h == 0 || img_w == 0 || img_h == 0 || self.width == 0 {
            return NONE;
        }
        // 격자 크기: 요청이 없으면 그림을 원래 크기로 담을 칸 수. Ghostty는 칸 수를 u16으로 담는다
        let rows = if self.grid_rows == 0 {
            div_ceil(img_h, cell_h)
        } else {
            self.grid_rows
        };
        let cols = if self.grid_cols == 0 {
            div_ceil(img_w, cell_w)
        } else {
            self.grid_cols
        };
        if rows == 0 || cols == 0 || rows > u32::from(u16::MAX) || cols > u32::from(u16::MAX) {
            return NONE;
        }
        let (iw, ih) = (f64::from(img_w), f64::from(img_h));
        let (cw, ch) = (f64::from(cell_w), f64::from(cell_h));
        let (g_cols, g_rows) = (f64::from(cols), f64::from(rows));

        // 그림을 격자에 비율을 지켜 맞춘다: 넓으면 너비에 맞추고 높이를 가운데로, 아니면 반대
        let (p_rows_px, p_cols_px) = (g_rows * ch, g_cols * cw);
        let (x_scale, y_scale, sx_off, sy_off) = if iw * p_rows_px > ih * p_cols_px {
            let s = p_cols_px / iw.max(1.0);
            (s, s, 0.0, (p_rows_px - ih * s) / 2.0)
        } else {
            let s = p_rows_px / ih.max(1.0);
            (s, s, (p_cols_px - iw * s) / 2.0, 0.0)
        };
        // 맞춘 그림(여백 포함)을 원본 픽셀 공간으로 되돌린다
        let img_x_off = sx_off / x_scale;
        let img_y_off = sy_off / y_scale;
        let sc_w = iw + img_x_off * 2.0;
        let sc_h = ih + img_y_off * 2.0;

        // 조각이 차지하는 부분(여백 포함 공간)
        let (vp_w, vp_h) = (f64::from(self.width), 1.0);
        let mut src_w = sc_w * (vp_w / g_cols);
        let mut src_h = sc_h * (vp_h / g_rows);
        let mut src_x = sc_w * (f64::from(self.col) / g_cols);
        let mut src_y = sc_h * (f64::from(self.row) / g_rows);

        let (mut x_offset, mut y_offset) = (0.0f64, 0.0f64);
        let mut width = f64::from(self.width) * cw;
        let mut height = ch;

        if src_y < img_y_off {
            // 조각의 위쪽이 여백이다: 원본 텍스처에는 여백이 없으니 잘라 내고 그릴 자리를 아래로 민다
            let offset = img_y_off - src_y;
            src_h -= offset;
            y_offset = offset;
            height -= offset * y_scale;
            src_y = 0.0;
            // 위·아래 여백을 함께 쓰면 높이가 원본보다 클 수 있다
            if src_h > ih {
                src_h = ih;
                height = ih * y_scale;
            }
        } else if src_y + src_h > sc_h - img_y_off {
            // 조각의 아래쪽이 여백이다: 원본 끝까지만
            src_y -= img_y_off;
            src_h = sc_h - img_y_off - src_y;
            src_h -= img_y_off;
            height = src_h * y_scale;
        } else {
            src_y -= img_y_off;
        }

        if src_x < img_x_off {
            let offset = img_x_off - src_x;
            src_w -= offset;
            x_offset = offset;
            width -= offset * x_scale;
            src_x = 0.0;
            if src_w > iw {
                src_w = iw;
                width = iw * x_scale;
            }
        } else if src_x + src_w > sc_w - img_x_off {
            src_x -= img_x_off;
            src_w = sc_w - img_x_off - src_x;
            src_w -= img_x_off;
            width = src_w * x_scale;
        } else {
            src_x -= img_x_off;
        }

        // 보이는 부분이 없으면(그림 밖의 칸) 그리지 않는다
        if src_w <= 0.0 || src_h <= 0.0 {
            return NONE;
        }
        let round = |v: f64| v.max(0.0).round() as u32;
        let src = [round(src_x), round(src_y), round(src_w), round(src_h)];
        let (dest_w, dest_h) = (round(width), round(height));
        // 자른 부분의 너비·높이 0은 `Geom`에서 "끝까지"라는 뜻이라 그리지 않는 것으로 막는다
        if src[2] == 0 || src[3] == 0 || dest_w == 0 || dest_h == 0 {
            return NONE;
        }
        Resolved {
            src,
            dest_w,
            dest_h,
            off_x: round(x_offset * x_scale),
            off_y: round(y_offset * y_scale),
            cols: self.width,
            rows: 1,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn g(img: (u32, u32), src: [u32; 4], want: (u32, u32), off: [u32; 2]) -> Geom {
        Geom {
            img_w: img.0,
            img_h: img.1,
            src,
            want_cols: want.0,
            want_rows: want.1,
            off,
            piece: None,
        }
    }

    #[test]
    fn layer_ranges_follow_ghostty() {
        assert_eq!(layer_of_z(0), Layer::AboveText);
        assert_eq!(layer_of_z(5), Layer::AboveText);
        assert_eq!(layer_of_z(-1), Layer::BelowText);
        assert_eq!(layer_of_z(-1_073_741_824), Layer::BelowText);
        assert_eq!(layer_of_z(-1_073_741_825), Layer::BelowBackground);
        assert_eq!(layer_of_z(i32::MIN), Layer::BelowBackground);
        assert_eq!(layer_of_z(i32::MAX), Layer::AboveText);
    }

    #[test]
    fn native_size_uses_the_whole_image_and_rounds_cells_up() {
        let r = g((25, 30), [0; 4], (0, 0), [0; 2]).resolve(10, 20);
        assert_eq!(r.src, [0, 0, 25, 30]);
        assert_eq!((r.dest_w, r.dest_h), (25, 30));
        assert_eq!((r.cols, r.rows), (3, 2));
    }

    #[test]
    fn source_rect_is_clamped_into_the_image_and_zero_means_to_the_edge() {
        let r = g((100, 50), [10, 20, 0, 0], (0, 0), [0; 2]).resolve(10, 10);
        assert_eq!(r.src, [10, 20, 90, 30], "w·h 0은 그림 끝까지");
        let r = g((100, 50), [90, 40, 50, 50], (0, 0), [0; 2]).resolve(10, 10);
        assert_eq!(r.src, [90, 40, 10, 10], "그림 밖으로 나가는 부분은 자른다");
        let r = g((100, 50), [500, 500, 5, 5], (0, 0), [0; 2]).resolve(10, 10);
        assert_eq!(r.src, [100, 50, 0, 0], "시작이 그림 밖이면 빈 직사각형");
        assert_eq!((r.dest_w, r.dest_h), (0, 0));
    }

    #[test]
    fn cropped_native_size_follows_the_cropped_source() {
        // 100x50 그림에서 40x20만: 칸 10x10이면 4칸 x 2줄
        let r = g((100, 50), [10, 10, 40, 20], (0, 0), [0; 2]).resolve(10, 10);
        assert_eq!((r.dest_w, r.dest_h, r.cols, r.rows), (40, 20, 4, 2));
    }

    #[test]
    fn offset_is_clamped_below_one_cell_and_adds_to_the_grid_size() {
        let r = g((25, 30), [0; 4], (0, 0), [99, 7]).resolve(10, 20);
        assert_eq!((r.off_x, r.off_y), (9, 7), "칸 크기 - 1까지");
        // 25+9=34 -> 4칸, 30+7=37 -> 2줄
        assert_eq!((r.cols, r.rows), (4, 2));
        assert_eq!((r.dest_w, r.dest_h), (25, 30));
    }

    #[test]
    fn columns_and_rows_given_stretch_to_the_cells_minus_the_offset() {
        let r = g((25, 30), [0; 4], (4, 2), [3, 5]).resolve(10, 20);
        assert_eq!((r.cols, r.rows), (4, 2));
        assert_eq!((r.dest_w, r.dest_h), (4 * 10 - 3, 2 * 20 - 5));
    }

    #[test]
    fn only_columns_given_keeps_the_aspect_ratio_of_the_cropped_source() {
        // 잘라낸 40x20, 4칸(40px) -> 높이 20px -> 칸 높이 10이면 2줄
        let r = g((100, 50), [0, 0, 40, 20], (4, 0), [0; 2]).resolve(10, 10);
        assert_eq!((r.dest_w, r.dest_h, r.cols, r.rows), (40, 20, 4, 2));
        // 80px로 늘어나면 높이 40
        let r = g((100, 50), [0, 0, 40, 20], (8, 0), [0; 2]).resolve(10, 10);
        assert_eq!((r.dest_w, r.dest_h, r.cols, r.rows), (80, 40, 8, 4));
    }

    #[test]
    fn only_rows_given_keeps_the_aspect_ratio() {
        let r = g((100, 50), [0; 4], (0, 3), [0; 2]).resolve(10, 10);
        assert_eq!((r.dest_w, r.dest_h, r.cols, r.rows), (60, 30, 6, 3));
    }

    #[test]
    fn unknown_cell_size_gives_no_cells() {
        let r = g((25, 30), [0; 4], (0, 0), [3, 3]).resolve(0, 0);
        assert_eq!((r.cols, r.rows, r.off_x, r.off_y), (0, 0, 0, 0));
    }

    #[test]
    fn huge_values_saturate_instead_of_overflowing() {
        let r = g((25, 30), [u32::MAX; 4], (u32::MAX, u32::MAX), [u32::MAX; 2]).resolve(u32::MAX, u32::MAX);
        let _ = r;
        let r = g((25, 30), [0; 4], (u32::MAX, 0), [0; 2]).resolve(1000, 1000);
        assert!(r.dest_w >= 1000);
    }

    /// 조각 하나의 Geom: 그림 `img`, 격자 `grid`(칸), 조각의 칸 `(col, row)`와 너비
    fn piece(img: (u32, u32), grid: (u32, u32), at: (u32, u32), width: u32) -> Geom {
        Geom {
            img_w: img.0,
            img_h: img.1,
            piece: Some(Piece {
                grid_cols: grid.0,
                grid_rows: grid.1,
                col: at.0,
                row: at.1,
                width,
            }),
            ..Geom::default()
        }
    }

    /// Ghostty `graphics_unicode.zig` "unicode render placement: dog 4x2"(그림 500x306, 칸 36x80, 4칸 2줄 격자)
    #[test]
    fn piece_dog_4x2_matches_ghostty() {
        let r = piece((500, 306), (4, 2), (0, 0), 4).resolve(36, 80);
        assert_eq!((r.off_x, r.off_y), (0, 36));
        assert_eq!(r.src, [0, 0, 500, 153]);
        assert_eq!((r.dest_w, r.dest_h), (144, 44));
        assert_eq!((r.cols, r.rows), (4, 1));
        let r = piece((500, 306), (4, 2), (0, 1), 4).resolve(36, 80);
        assert_eq!((r.off_x, r.off_y), (0, 0));
        assert_eq!(r.src, [0, 153, 500, 153]);
        assert_eq!((r.dest_w, r.dest_h), (144, 44));
    }

    /// "dog 2x2 with blank cells": 격자는 2x2인데 조각 너비가 4(칸이 격자보다 많이 이어졌다)
    #[test]
    fn piece_dog_2x2_with_blank_cells_matches_ghostty() {
        let r = piece((500, 306), (2, 2), (0, 0), 4).resolve(36, 80);
        assert_eq!((r.off_x, r.off_y), (0, 58));
        assert_eq!(r.src, [0, 0, 500, 153]);
        assert_eq!((r.dest_w, r.dest_h), (72, 22));
        let r = piece((500, 306), (2, 2), (0, 1), 4).resolve(36, 80);
        assert_eq!((r.off_x, r.off_y), (0, 0));
        assert_eq!(r.src, [0, 153, 500, 153]);
        assert_eq!((r.dest_w, r.dest_h), (72, 22));
    }

    /// "dog 1x1"
    #[test]
    fn piece_dog_1x1_matches_ghostty() {
        let r = piece((500, 306), (1, 1), (0, 0), 4).resolve(36, 80);
        assert_eq!((r.off_x, r.off_y), (0, 29));
        assert_eq!(r.src, [0, 0, 500, 306]);
        assert_eq!((r.dest_w, r.dest_h), (36, 22));
    }

    #[test]
    fn piece_without_requested_grid_uses_the_image_size_in_cells() {
        // 20x40 그림, 칸 10x20 -> 격자 2x2, 비율이 맞아 여백이 없다
        let r = piece((20, 40), (0, 0), (1, 1), 1).resolve(10, 20);
        assert_eq!(r.src, [10, 20, 10, 20]);
        assert_eq!((r.dest_w, r.dest_h, r.off_x, r.off_y), (10, 20, 0, 0));
    }

    #[test]
    fn piece_that_cannot_be_drawn_resolves_to_nothing() {
        // 칸 크기를 모른다, 격자가 u16을 넘는다, 조각이 그림 밖의 칸이다
        assert_eq!(piece((10, 10), (1, 1), (0, 0), 1).resolve(0, 0).dest_w, 0);
        assert_eq!(piece((10, 10), (70_000, 1), (0, 0), 1).resolve(10, 10).dest_w, 0);
        // 격자가 4칸인데 칸 10이 그림 밖이다(조각이 5번째 칸)
        let r = piece((40, 10), (4, 1), (9, 0), 1).resolve(10, 10);
        assert_eq!((r.dest_w, r.dest_h), (0, 0));
        // 큰 값도 넘치지 않는다
        let _ =
            piece((u32::MAX, u32::MAX), (65_535, 65_535), (u32::MAX, u32::MAX), u32::MAX).resolve(u32::MAX, u32::MAX);
    }
}
