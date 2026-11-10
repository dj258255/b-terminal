//! b-terminal(#346): macOS IOSurface 올리기의 순수 논리. 창과 시스템 호출 없이 시험할 수 있게 따로 뺐다.
//! 묶음에서 쓸 것 고르기, 크기·색 공간이 바뀌면 비우기, 줄 복사(`bytesPerRow`가 폭×4보다 클 때)와 알파 채우기.

/// 묶음에 두는 IOSurface 수 상한
pub const MAX_SLOTS: usize = 3;

/// 묶음 한 칸의 겉모습(IOSurface 자체는 시스템 것이라 값만 본다)
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SlotInfo {
    /// 가로 픽셀
    pub width: usize,
    /// 세로 픽셀
    pub height: usize,
    /// 색 공간 열쇠(`color_space_key`)
    pub space: u64,
    /// `IOSurfaceIsInUse`: Core Animation이나 화면 합성이 아직 쥐고 있다
    pub in_use: bool,
}

/// `pick_slot`의 답
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Pick {
    /// 묶음의 이 칸을 다시 쓴다
    Reuse(usize),
    /// 다시 쓸 것이 없다. 새로 만든다
    New,
}

/// 색 공간 열쇠. 화면 색 공간이면 화면 번호, 아니면(`window-colorspace = srgb`) 화면과 무관한 값
pub fn color_space_key(native: bool, display: u32) -> u64 {
    if native {
        u64::from(display)
    } else {
        1 << 32
    }
}

/// 같은 크기·같은 색 공간이고 사용 중이 아닌 첫 칸을 고른다. 없으면 새로 만든다
pub fn pick_slot(slots: &[SlotInfo], width: usize, height: usize, space: u64) -> Pick {
    slots
        .iter()
        .position(|s| s.width == width && s.height == height && s.space == space && !s.in_use)
        .map_or(Pick::New, Pick::Reuse)
}

/// 크기나 색 공간이 바뀌었으면 옛 칸을 비운다. 남길 칸에 true
pub fn retain_mask(slots: &[SlotInfo], width: usize, height: usize, space: u64) -> Vec<bool> {
    slots
        .iter()
        .map(|s| s.width == width && s.height == height && s.space == space)
        .collect()
}

/// 새 칸을 하나 더하기 전에 앞(가장 오래된 쪽)에서 버릴 칸 수
pub fn evict_for_new(len: usize) -> usize {
    (len + 1).saturating_sub(MAX_SLOTS)
}

/// `src`(폭×높이 픽셀, 0RGB 또는 곱한 알파 ARGB)를 줄마다 `bytes_per_row` 간격으로 `dst`에 옮긴다.
/// 한 픽셀은 BGRA 바이트 순서(little-endian u32). `fill_alpha`면 알파 바이트를 0xff로 채운다.
/// 줄 끝 덧칸은 건드리지 않는다. 크기가 모자라거나 `bytes_per_row`가 폭×4보다 작으면 쓰지 않고 false
pub fn copy_frame(
    dst: &mut [u8],
    bytes_per_row: usize,
    src: &[u32],
    width: usize,
    height: usize,
    fill_alpha: bool,
) -> bool {
    let row_bytes = width * 4;
    if width == 0 || height == 0 || bytes_per_row < row_bytes || src.len() < width * height {
        return false;
    }
    // 마지막 줄은 덧칸 없이 폭×4바이트만 있으면 된다
    if dst.len() < bytes_per_row * (height - 1) + row_bytes {
        return false;
    }
    let mask = if fill_alpha { 0xff00_0000 } else { 0 };
    for (y, src_row) in src.chunks_exact(width).take(height).enumerate() {
        let start = y * bytes_per_row;
        for (out, px) in dst[start..start + row_bytes].chunks_exact_mut(4).zip(src_row) {
            out.copy_from_slice(&(px | mask).to_le_bytes());
        }
    }
    true
}
