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
}
