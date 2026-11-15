//! kitty 그래픽 애니메이션(#554): <https://sw.kovidgoyal.net/kitty/graphics-protocol/#animation>
//!
//! 프레임 1(뿌리 프레임)은 그림 자신의 자료(`Image.rgba`)이고, 프레임 2~N을 여기서 든다. Ghostty와 같이 프레임을 받을 때
//! 그림 크기의 RGBA로 미리 합성해 둔다(kitty는 차이 사각형만 두고 그릴 때 합성한다). 메모리를 더 쓰는 대신 지금 프레임은
//! 늘 바로 그릴 수 있고, 프레임 사이에 참조 사슬이 없다. 프레임 메모리는 그림 저장량에 함께 센다.
//!
//! 이 모듈은 상태 기계와 픽셀 연산만 든다(시계는 인자로 받는다). 저장소·답·저장량 계산은 `graphics.rs`가 한다.
//! 규칙의 출처는 Ghostty `src/terminal/kitty/graphics_animation.zig`(상태·간격), `graphics_pixel.zig`(픽셀),
//! `graphics_exec.zig`(프레임 번호·검사), `graphics_storage.zig`(`animationTick`, `deleteAnimationFrame`)다(main 7fa26e8b675f)

use std::sync::Arc;

/// 새 프레임에 간격을 안 줬을 때(z 없음 또는 0)의 간격(ms). kitty `DEFAULT_GAP`, Ghostty `default_gap_ms`
pub const DEFAULT_GAP_MS: u32 = 40;

/// 프레임마다 저장량에 더하는 고정 몫(바이트). 1×1 그림에 프레임을 수백만 개 보내 상한을 피하지 못하게 한다(Ghostty에는 없다)
pub const FRAME_OVERHEAD: usize = 128;

/// 재생 상태
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum State {
    /// 멈춤: 프레임은 `a=a,c=N`으로만 바뀐다. 모든 그림의 처음 상태
    #[default]
    Stopped,
    /// `a=a,s=2`: 흐르지만 끝에서 돌지 않고 마지막 프레임에서 새 프레임을 기다린다
    Loading,
    /// `a=a,s=3`: 흐르고 돈다
    Running,
}

/// 뿌리 다음의 프레임 하나
#[derive(Debug, Clone)]
pub struct Frame {
    /// 그림 크기의 RGBA(곱하지 않은 알파)
    pub data: Arc<Vec<u8>>,
    /// 이 프레임을 보이는 시간(ms). 0이면 건너뛴다
    pub gap_ms: u32,
}

/// 그림 하나의 애니메이션 상태
#[derive(Debug, Clone, Default)]
pub struct Animation {
    /// 뿌리 다음 프레임: `frames[i]`는 프로토콜 프레임 번호 `i + 2`
    pub frames: Vec<Frame>,
    /// 뿌리 프레임의 간격(ms). 0이면 건너뛴다(뿌리는 간격 0으로 생기고 `a=a,r=1,z=N`으로 준다)
    pub root_gap_ms: u32,
    /// 지금 보이는 프레임의 0부터 센 번호: 0은 뿌리, i는 `frames[i - 1]`
    pub current_index: u32,
    pub state: State,
    /// 반복 횟수 상한(0이면 끝없이). `a=a,v=`의 값에서 1을 뺀 것
    pub max_loops: u32,
    /// 재생을 시작한 뒤 끝낸 반복 수
    pub current_loop: u32,
    /// 지금 프레임을 보인 때(시계의 ms). 없으면 다음 `tick`이 지금으로 찍는다
    pub frame_shown_at_ms: Option<u64>,
    /// `frames`가 든 바이트(고정 몫 포함). 프레임을 넣고 뺄 때 고친다: 받을 때마다 처음부터 세면
    /// 작은 그림에 프레임을 많이 보낸 출력 하나가 프레임 수의 제곱만큼 일을 시킨다
    bytes: usize,
}

impl Animation {
    /// 뿌리를 포함한 프레임 수
    pub fn frame_count(&self) -> u32 {
        (self.frames.len() as u32).saturating_add(1)
    }

    /// 0부터 센 번호의 간격. 범위 밖이면 0
    pub fn gap_at(&self, index: u32) -> u32 {
        if index == 0 {
            self.root_gap_ms
        } else {
            self.frames.get(index as usize - 1).map_or(0, |f| f.gap_ms)
        }
    }

    pub fn set_gap_at(&mut self, index: u32, gap_ms: u32) {
        if index == 0 {
            self.root_gap_ms = gap_ms;
        } else if let Some(f) = self.frames.get_mut(index as usize - 1) {
            f.gap_ms = gap_ms;
        }
    }

    /// 간격의 합. 0이면(모든 프레임이 간격 0) 흐르지 않는다. 아래 `tick`의 건너뛰기 고리가 끝나게 하는 조건이기도 하다
    pub fn duration_ms(&self) -> u64 {
        self.frames
            .iter()
            .fold(u64::from(self.root_gap_ms), |t, f| t + u64::from(f.gap_ms))
    }

    /// 프레임(뿌리 제외)이 든 바이트(고정 몫 포함). 저장량에 센다
    pub fn frame_bytes(&self) -> usize {
        self.bytes
    }

    /// 프레임을 끝에 더한다. `frames`에 바로 넣지 말고 이것을 쓴다(저장량을 함께 고친다)
    pub fn push_frame(&mut self, f: Frame) {
        self.bytes = self.bytes.saturating_add(f.data.len() + FRAME_OVERHEAD);
        self.frames.push(f);
    }

    /// `frames[i]`를 뺀다(저장량을 함께 고친다)
    fn remove_frame(&mut self, i: usize) -> Frame {
        let f = self.frames.remove(i);
        self.bytes = self.bytes.saturating_sub(f.data.len() + FRAME_OVERHEAD);
        f
    }

    /// 지금 시각 `now_ms`에 보일 프레임으로 옮기고, 다음 프레임이 바뀔 때까지 남은 ms를 돌려준다(Ghostty `animationTick`의 한 그림 몫).
    /// 시계는 호출하는 쪽이 정한다(같은 시계를 계속 써야 한다). 흐르지 않거나 더 바뀔 것이 없으면 `None`.
    ///
    /// 한 번에 한 프레임만 나아가고 따라잡지 않는다(kitty와 같다): 부르는 때가 늦으면 건너뛰지 않고 느려진다.
    /// 간격 0인 프레임은 보이지 않고 건너뛴다. 반복 횟수를 다 쓰면 마지막 프레임에 선다
    pub fn tick(&mut self, now_ms: u64) -> Option<u64> {
        // kitty `image_is_animatable`에 맞춘 문들
        if self.state == State::Stopped || self.frames.is_empty() || self.duration_ms() == 0 {
            return None;
        }
        if self.max_loops > 0 && self.current_loop >= self.max_loops {
            return None;
        }
        // 처음(또는 다른 길로 지금 프레임이 바뀐 뒤)이면 지금 보인 것으로 치고 그때부터 센다.
        // 시계가 거꾸로 갔으면(미래의 도장) 지금으로 다시 잡는다
        let shown_at = match self.frame_shown_at_ms {
            Some(at) if at <= now_ms => at,
            _ => now_ms,
        };
        self.frame_shown_at_ms = Some(shown_at);
        let mut next_at = shown_at.saturating_add(u64::from(self.gap_at(self.current_index)));
        if now_ms >= next_at {
            'advance: {
                // 다음으로 보일 프레임까지 걷는다. 간격 0인 프레임을 건너뛰려고 고리다(`duration_ms() > 0`이라 끝난다)
                let count = self.frame_count();
                let mut idx = self.current_index;
                loop {
                    let next = (idx + 1) % count;
                    if next == 0 {
                        // 마지막을 지나 뿌리로 돌아간다. 기다리는 상태(s=2)는 돌지 않고 마지막에서 새 프레임을 기다린다
                        if self.state == State::Loading {
                            break 'advance;
                        }
                        // 돌 때마다 반복 하나를 끝낸다. 횟수를 다 썼으면 마지막에 선다
                        self.current_loop += 1;
                        if self.max_loops > 0 && self.current_loop >= self.max_loops {
                            break 'advance;
                        }
                    }
                    idx = next;
                    if self.gap_at(idx) != 0 {
                        break;
                    }
                }
                self.current_index = idx;
                self.frame_shown_at_ms = Some(now_ms);
                next_at = now_ms.saturating_add(u64::from(self.gap_at(idx)));
            }
        }
        // 서 버린 그림(기다리는 상태의 끝, 횟수 소진)은 next_at이 지나 있어 다음을 정하지 않는다.
        // 새 프레임이 오거나 `a=a`가 상태를 바꾸면 그쪽에서 다시 깬다
        (next_at > now_ms).then(|| next_at - now_ms)
    }
}

/// `a=f`의 프레임 번호: r이 1..=count면 그 프레임을 고치고, 아니면(없음·0·count+1 넘음) 끝에 새로 만든다
pub fn resolve_frame_number(edit: u32, count: u32) -> u32 {
    if edit == 0 || edit > count.saturating_add(1) {
        count.saturating_add(1)
    } else {
        edit
    }
}

/// 새 프레임의 간격: 양수는 그대로, 0은 기본 40ms, 음수는 간격 0(보이지 않는 프레임)
pub fn new_frame_gap(z: i32) -> u32 {
    match z {
        1.. => z as u32,
        0 => DEFAULT_GAP_MS,
        _ => 0,
    }
}

/// 이미 있는 프레임의 간격을 고칠 값: 0이면 바꾸지 않고(`None`), 양수는 그대로, 음수는 0
pub fn edit_frame_gap(z: i32) -> Option<u32> {
    match z {
        0 => None,
        1.. => Some(z as u32),
        _ => Some(0),
    }
}

/// `a=d,d=f`·`d=F`: 프레임 하나를 지운다. 번호 0은 뿌리, 끝을 넘으면 마지막이다. 뿌리를 지우면 프레임 2가 뿌리가 된다
/// (Ghostty `deleteAnimationFrame`). 지금 프레임 번호를 맞춘다. 프레임이 하나도 없는 그림은 부르는 쪽이 먼저 가른다
///
/// 지금 프레임 맞춤은 Ghostty를 그대로 옮겼다. 지운 프레임의 위치로 `number - 2`(뿌리 다음 목록의 칸)를 쓰므로 0부터 센 프레임 번호
/// `number - 1`과 한 칸 다르다. 지금 프레임을 지울 때 앞 프레임으로 물러나는 효과가 있다
pub fn delete_frame(root: &mut Arc<Vec<u8>>, anim: &mut Animation, number: u32) {
    if anim.frames.is_empty() {
        return;
    }
    let count = anim.frame_count();
    let number = number.min(count).max(1);
    if number == 1 {
        let promoted = anim.remove_frame(0);
        *root = promoted.data;
        anim.root_gap_ms = promoted.gap_ms;
    } else {
        anim.remove_frame(number as usize - 2);
    }
    let removed_idx = if number == 1 { 0 } else { number - 2 };
    let remaining = anim.frames.len() as u32;
    if anim.current_index > remaining {
        anim.current_index = remaining;
        anim.frame_shown_at_ms = None;
        return;
    }
    if removed_idx == anim.current_index {
        anim.frame_shown_at_ms = None;
    } else if removed_idx < anim.current_index {
        anim.current_index -= 1;
    }
}

/// `a=c`의 사각형을 검사한다(`w`·`h`가 0이면 그림 전체). 범위를 벗어나면 `Err`(`a=f`는 자르지만 `a=c`는 오류다).
/// 같은 프레임이면 원본과 대상이 겹쳐서는 안 된다. 값은 u64로 세어 믿을 수 없는 u32가 넘치지 않게 한다.
/// 돌려주는 값: 실제 (너비, 높이)
pub fn check_compose_rect(
    img: (u32, u32),
    dst: (u32, u32),
    src: (u32, u32),
    size: (u32, u32),
    same_frame: bool,
) -> Result<(u32, u32), &'static str> {
    let w = if size.0 > 0 { size.0 } else { img.0 };
    let h = if size.1 > 0 { size.1 } else { img.1 };
    let (w64, h64) = (u64::from(w), u64::from(h));
    if u64::from(dst.0) + w64 > u64::from(img.0) || u64::from(dst.1) + h64 > u64::from(img.1) {
        return Err("대상 사각형이 그림을 벗어납니다");
    }
    if u64::from(src.0) + w64 > u64::from(img.0) || u64::from(src.1) + h64 > u64::from(img.1) {
        return Err("원본 사각형이 그림을 벗어납니다");
    }
    if same_frame {
        let (dx, dy, sx, sy) = (u64::from(dst.0), u64::from(dst.1), u64::from(src.0), u64::from(src.1));
        let x_overlaps = sx.max(dx) < sx.min(dx) + w64;
        let y_overlaps = sy.max(dy) < sy.min(dy) + h64;
        if x_overlaps && y_overlaps {
            return Err("원본과 대상 사각형이 겹칩니다");
        }
    }
    Ok((w, h))
}

/// RGBA 바탕을 한 색으로 채운다(`Y=`, 0xRRGGBBAA)
pub fn fill_background(data: &mut [u8], bg: [u8; 4]) {
    if bg == [0; 4] {
        data.fill(0);
        return;
    }
    for px in data.chunks_exact_mut(4) {
        px.copy_from_slice(&bg);
    }
}

/// `Y=`의 값(R이 가장 높은 바이트인 32비트)을 RGBA 바이트로
pub fn background_bytes(y: u32) -> [u8; 4] {
    y.to_be_bytes()
}

/// 곱하지 않은 RGBA의 source-over 한 픽셀(wuffs `composite_nonpremul_nonpremul_src_over`와 같은 16비트 정수 식).
/// 원본이 비었으면 대상을 그대로, 불투명하면 원본을 그대로, 대상이 비었으면 원본을 그대로 둔다
fn src_over(dst: &mut [u8], src: &[u8]) {
    let sa8 = src[3];
    if sa8 == 0 {
        return;
    }
    if sa8 == 255 || dst[3] == 0 {
        dst.copy_from_slice(src);
        return;
    }
    let wide = |b: u8| u32::from(b) * 0x101;
    let (sa, da) = (wide(sa8), wide(dst[3]));
    let ia = 0xFFFF - sa;
    // 대상을 곱한 알파로, 원본도 곱한 뒤 합치고, 다시 곱하지 않은 알파로 돌린다
    let out_a = sa + da * ia / 0xFFFF;
    for i in 0..3 {
        let s = wide(src[i]) * sa / 0xFFFF;
        let d = wide(dst[i]) * da / 0xFFFF;
        let c = s + d * ia / 0xFFFF;
        dst[i] = ((c * 0xFFFF / out_a.max(1)) >> 8).min(255) as u8;
    }
    dst[3] = (out_a >> 8).min(255) as u8;
}

fn compose_row(dst: &mut [u8], src: &[u8], overwrite: bool) {
    if overwrite {
        dst.copy_from_slice(src);
    } else {
        for (d, s) in dst.chunks_exact_mut(4).zip(src.chunks_exact(4)) {
            src_over(d, s);
        }
    }
}

/// `src`(`src_w`×`src_h` RGBA)를 `dst`(`dst_w`×`dst_h` RGBA) 위 `(x, y)`에 합성한다. 밖으로 나간 부분은 조용히 자른다(kitty `a=f`와 같다).
/// 버퍼가 크기보다 짧으면 아무것도 하지 않는다
#[allow(clippy::too_many_arguments)]
pub fn compose_rect(
    dst: &mut [u8],
    dst_w: u32,
    dst_h: u32,
    src: &[u8],
    src_w: u32,
    src_h: u32,
    x: u32,
    y: u32,
    overwrite: bool,
) {
    if x >= dst_w || y >= dst_h {
        return;
    }
    if dst.len() < dst_w as usize * dst_h as usize * 4 || src.len() < src_w as usize * src_h as usize * 4 {
        return;
    }
    let width = src_w.min(dst_w - x) as usize;
    let height = src_h.min(dst_h - y) as usize;
    for row in 0..height {
        let d = ((y as usize + row) * dst_w as usize + x as usize) * 4;
        let s = row * src_w as usize * 4;
        compose_row(&mut dst[d..d + width * 4], &src[s..s + width * 4], overwrite);
    }
}

/// 같은 너비(`canvas_w`)의 두 캔버스 사이에서 `w`×`h` 사각형을 `src`의 `(sx, sy)`에서 `dst`의 `(dx, dy)`로 합성한다.
/// 범위는 `check_compose_rect`로 미리 검사해야 한다. 어긋나면 아무것도 하지 않는다
#[allow(clippy::too_many_arguments)]
pub fn compose_canvas_rect(
    dst: &mut [u8],
    src: &[u8],
    canvas_w: u32,
    w: u32,
    h: u32,
    (sx, sy): (u32, u32),
    (dx, dy): (u32, u32),
    overwrite: bool,
) {
    let stride = canvas_w as usize * 4;
    let in_bounds = |buf_len: usize, x: u32, y: u32| {
        (y as usize + h as usize).saturating_mul(stride) <= buf_len && x as usize + w as usize <= canvas_w as usize
    };
    if !in_bounds(dst.len(), dx, dy) || !in_bounds(src.len(), sx, sy) {
        return;
    }
    for row in 0..h as usize {
        let d = (dy as usize + row) * stride + dx as usize * 4;
        let s = (sy as usize + row) * stride + sx as usize * 4;
        compose_row(&mut dst[d..d + w as usize * 4], &src[s..s + w as usize * 4], overwrite);
    }
}

/// 한 캔버스 안에서 서로 겹치지 않는 두 사각형 사이를 합성한다(`a=c`에서 원본·대상 프레임이 같을 때).
/// 줄마다 원본을 작은 버퍼에 먼저 옮겨 두므로 같은 줄을 쓰는 사각형도 안전하다
pub fn compose_within(
    canvas: &mut [u8],
    canvas_w: u32,
    w: u32,
    h: u32,
    (sx, sy): (u32, u32),
    (dx, dy): (u32, u32),
    overwrite: bool,
) {
    let stride = canvas_w as usize * 4;
    if (sy.max(dy) as usize + h as usize).saturating_mul(stride) > canvas.len()
        || sx.max(dx) as usize + w as usize > canvas_w as usize
    {
        return;
    }
    let mut row_buf = vec![0u8; w as usize * 4];
    for row in 0..h as usize {
        let s = (sy as usize + row) * stride + sx as usize * 4;
        row_buf.copy_from_slice(&canvas[s..s + w as usize * 4]);
        let d = (dy as usize + row) * stride + dx as usize * 4;
        compose_row(&mut canvas[d..d + w as usize * 4], &row_buf, overwrite);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn anim(gaps: &[u32]) -> Animation {
        // gaps[0]은 뿌리의 간격
        let mut a = Animation {
            root_gap_ms: gaps[0],
            state: State::Running,
            ..Default::default()
        };
        for &g in &gaps[1..] {
            a.push_frame(Frame {
                data: Arc::new(vec![0; 4]),
                gap_ms: g,
            });
        }
        a
    }

    #[test]
    fn gap_helpers_and_duration() {
        let mut a = anim(&[0, 100]);
        assert_eq!(a.frame_count(), 2);
        assert_eq!(a.gap_at(0), 0);
        assert_eq!(a.gap_at(1), 100);
        assert_eq!(a.gap_at(9), 0, "범위 밖은 0");
        assert_eq!(a.duration_ms(), 100);
        assert_eq!(a.frame_bytes(), 4 + FRAME_OVERHEAD);
        a.set_gap_at(0, 40);
        a.set_gap_at(1, 60);
        a.set_gap_at(9, 5);
        assert_eq!((a.root_gap_ms, a.duration_ms()), (40, 100));
    }

    #[test]
    fn gap_rules_for_new_and_edited_frames() {
        assert_eq!(new_frame_gap(100), 100);
        assert_eq!(new_frame_gap(0), DEFAULT_GAP_MS);
        assert_eq!(new_frame_gap(-5), 0);
        assert_eq!(new_frame_gap(i32::MIN), 0);
        assert_eq!(edit_frame_gap(0), None, "고칠 때 0은 간격을 바꾸지 않는다");
        assert_eq!(edit_frame_gap(77), Some(77));
        assert_eq!(edit_frame_gap(-1), Some(0));
    }

    #[test]
    fn frame_number_resolution() {
        // r이 없거나 0이거나 count+1을 넘으면 끝에 새로 만든다
        assert_eq!(resolve_frame_number(0, 3), 4);
        assert_eq!(resolve_frame_number(9, 3), 4);
        assert_eq!(resolve_frame_number(4, 3), 4);
        assert_eq!(resolve_frame_number(1, 3), 1);
        assert_eq!(resolve_frame_number(3, 3), 3);
        assert_eq!(resolve_frame_number(u32::MAX, u32::MAX), u32::MAX);
    }

    #[test]
    fn tick_waits_for_the_gap_then_advances_one_frame() {
        let mut a = anim(&[40, 40]);
        // 처음 부르면 지금 보인 것으로 치고 40ms를 센다
        assert_eq!(a.tick(0), Some(40));
        assert_eq!(a.current_index, 0);
        assert_eq!(a.tick(10), Some(30));
        assert_eq!(a.current_index, 0);
        // 간격이 지나면 다음 프레임으로, 그 프레임의 간격을 센다
        assert_eq!(a.tick(40), Some(40));
        assert_eq!(a.current_index, 1);
        // 돌아서 뿌리로
        assert_eq!(a.tick(80), Some(40));
        assert_eq!(a.current_index, 0);
        assert_eq!(a.current_loop, 1);
    }

    #[test]
    fn tick_does_not_catch_up_when_late() {
        let mut a = anim(&[10, 10, 10]);
        assert_eq!(a.tick(0), Some(10));
        // 1초 늦게 불렀다: 한 프레임만 나아가고 건너뛰지 않는다
        assert_eq!(a.tick(1000), Some(10));
        assert_eq!(a.current_index, 1);
        assert_eq!(a.frame_shown_at_ms, Some(1000));
    }

    #[test]
    fn tick_skips_gapless_frames() {
        // 뿌리 40, 프레임2 간격 0, 프레임3 40: 뿌리 다음은 프레임3
        let mut a = anim(&[40, 0, 40]);
        a.tick(0);
        assert_eq!(a.tick(40), Some(40));
        assert_eq!(a.current_index, 2, "간격 0인 프레임 2는 보이지 않는다");
        // 프레임3 다음은 뿌리(프레임 2는 또 건너뜀)
        assert_eq!(a.tick(80), Some(40));
        assert_eq!(a.current_index, 0);
    }

    #[test]
    fn tick_never_advances_when_every_gap_is_zero_or_stopped_or_single() {
        let mut a = anim(&[0, 0]);
        assert_eq!(a.tick(0), None);
        assert_eq!(a.tick(10_000), None);
        assert_eq!(a.current_index, 0);
        let mut stopped = anim(&[40, 40]);
        stopped.state = State::Stopped;
        assert_eq!(stopped.tick(0), None);
        assert_eq!(stopped.frame_shown_at_ms, None, "멈춘 그림은 시계를 건드리지 않는다");
        let mut single = anim(&[40]);
        assert_eq!(single.tick(0), None, "뿌리뿐이면 갈 곳이 없다");
    }

    #[test]
    fn tick_loading_state_parks_on_the_last_frame() {
        let mut a = anim(&[40, 40]);
        a.state = State::Loading;
        a.tick(0);
        assert_eq!(a.tick(40), Some(40));
        assert_eq!(a.current_index, 1);
        // 마지막에서 돌지 않고 선다: 다음을 정하지 않는다
        assert_eq!(a.tick(100), None);
        assert_eq!(a.current_index, 1);
        assert_eq!(a.current_loop, 0, "도는 것을 거절했으니 반복도 세지 않는다");
        // 새 프레임이 오면(프레임 수가 늘면) 이어서 나아간다
        a.push_frame(Frame {
            data: Arc::new(vec![0; 4]),
            gap_ms: 25,
        });
        assert_eq!(a.tick(150), Some(25));
        assert_eq!(a.current_index, 2);
    }

    #[test]
    fn tick_loop_budget_runs_out_on_the_last_frame() {
        // v=3은 max_loops=2: 두 바퀴를 돌고 마지막 프레임에 선다
        let mut a = anim(&[10, 40]);
        a.max_loops = 2;
        assert_eq!(a.tick(0), Some(10));
        assert_eq!(a.tick(10), Some(40));
        assert_eq!(a.current_index, 1);
        assert_eq!(a.tick(50), Some(10), "첫 바퀴를 끝내고 뿌리로");
        assert_eq!((a.current_index, a.current_loop), (0, 1));
        assert_eq!(a.tick(60), Some(40));
        assert_eq!(a.current_index, 1);
        // 두 번째 바퀴 끝: 횟수를 다 써서 마지막 프레임에 선다
        assert_eq!(a.tick(100), None);
        assert_eq!((a.current_index, a.current_loop), (1, 2));
        assert_eq!(a.tick(500), None);
        assert_eq!(a.current_index, 1);
    }

    #[test]
    fn tick_re_anchors_when_the_clock_goes_backwards() {
        let mut a = anim(&[40, 40]);
        a.frame_shown_at_ms = Some(10_000);
        assert_eq!(a.tick(5), Some(40), "미래의 도장은 지금으로 다시 잡는다");
        assert_eq!(a.frame_shown_at_ms, Some(5));
    }

    #[test]
    fn delete_frame_promotes_the_root_and_fixes_the_current_index() {
        let mut root = Arc::new(vec![1; 4]);
        let mut a = anim(&[5, 10, 20]);
        a.frames[0].data = Arc::new(vec![2; 4]);
        a.frames[1].data = Arc::new(vec![3; 4]);
        // 뿌리(번호 0)를 지우면 프레임 2가 뿌리가 되고 간격도 따라온다
        delete_frame(&mut root, &mut a, 0);
        assert_eq!(*root, vec![2; 4]);
        assert_eq!((a.frame_count(), a.root_gap_ms), (2, 10));
        // 끝을 넘는 번호는 마지막 프레임
        delete_frame(&mut root, &mut a, 9);
        assert_eq!(a.frame_count(), 1);
        // 프레임이 하나뿐이면 아무것도 하지 않는다
        delete_frame(&mut root, &mut a, 1);
        assert_eq!(a.frame_count(), 1);
        assert_eq!(*root, vec![2; 4]);
    }

    #[test]
    fn delete_frame_clamps_the_current_index() {
        let mut root = Arc::new(vec![1; 4]);
        let mut a = anim(&[5, 10, 20]);
        a.current_index = 2;
        a.frame_shown_at_ms = Some(7);
        // 마지막(지금) 프레임을 지운다: 번호를 끝으로 맞추고 시계를 다시 시작한다
        delete_frame(&mut root, &mut a, 3);
        assert_eq!(a.current_index, 1);
        assert_eq!(a.frame_shown_at_ms, None);
    }

    #[test]
    fn compose_rect_overwrite_clips_at_the_edge() {
        // 2×2 캔버스에 2×1 사각형을 (1, 1)에: 첫 픽셀만 들어가고 나머지는 오른쪽으로 잘린다
        let mut dst = [0u8; 16];
        compose_rect(&mut dst, 2, 2, &[1, 2, 3, 4, 5, 6, 7, 8], 2, 1, 1, 1, true);
        let mut want = [0u8; 16];
        want[12..16].copy_from_slice(&[1, 2, 3, 4]);
        assert_eq!(dst, want);
    }

    #[test]
    fn compose_rect_entirely_outside_does_nothing() {
        let mut dst = [9u8; 16];
        compose_rect(&mut dst, 2, 2, &[1; 4], 1, 1, 2, 0, true);
        compose_rect(&mut dst, 2, 2, &[1; 4], 1, 1, 0, 2, true);
        assert_eq!(dst, [9u8; 16]);
        // 버퍼가 크기보다 짧아도 panic하지 않는다
        compose_rect(&mut dst[..8], 2, 2, &[1; 4], 1, 1, 0, 0, true);
        assert_eq!(dst, [9u8; 16]);
    }

    #[test]
    fn alpha_blend_matches_the_reference_vectors() {
        let blend = |d: [u8; 4], s: [u8; 4]| {
            let mut d = d;
            compose_rect(&mut d, 1, 1, &s, 1, 1, 0, 0, false);
            d
        };
        // 불투명 원본은 복사
        assert_eq!(blend([10, 20, 30, 40], [100, 110, 120, 255]), [100, 110, 120, 255]);
        // 투명 원본은 대상을 그대로
        assert_eq!(blend([10, 20, 30, 40], [100, 110, 120, 0]), [10, 20, 30, 40]);
        // 불투명 검정 위의 50% 흰색
        let o = blend([0, 0, 0, 255], [255, 255, 255, 128]);
        assert_eq!((o[3], o[0]), (255, 128));
        // 투명한 대상 위에서는 원본이 그대로
        assert_eq!(blend([0, 0, 0, 0], [200, 100, 50, 128]), [200, 100, 50, 128]);
        // 반투명 둘이 겹치면 알파가 늘고 값은 0..=255 안
        let o = blend([200, 0, 0, 128], [0, 0, 200, 128]);
        assert!(o[3] > 128 && o[3] < 255, "{o:?}");
        assert!(o[0] > 0 && o[2] > 0, "{o:?}");
    }

    #[test]
    fn fill_background_and_byte_order() {
        let mut b = [7u8; 8];
        fill_background(&mut b, [1, 2, 3, 4]);
        assert_eq!(b, [1, 2, 3, 4, 1, 2, 3, 4]);
        fill_background(&mut b, [0; 4]);
        assert_eq!(b, [0; 8]);
        // Y=4278190335는 0xff0000ff: 불투명 빨강(R이 가장 높은 바이트)
        assert_eq!(background_bytes(4_278_190_335), [255, 0, 0, 255]);
    }

    #[test]
    fn compose_canvas_rect_copies_between_frames_and_within_one() {
        // 3×1 캔버스: 0번 픽셀을 다른 캔버스의 2번에 복사
        let src = [1, 2, 3, 4, 0, 0, 0, 0, 9, 9, 9, 9];
        let mut dst = [0u8; 12];
        compose_canvas_rect(&mut dst, &src, 3, 1, 1, (0, 0), (2, 0), true);
        assert_eq!(dst, [0, 0, 0, 0, 0, 0, 0, 0, 1, 2, 3, 4]);
        // 같은 캔버스 안: 0번 픽셀을 2번으로
        let mut c = src;
        compose_within(&mut c, 3, 1, 1, (0, 0), (2, 0), true);
        assert_eq!(c, [1, 2, 3, 4, 0, 0, 0, 0, 1, 2, 3, 4]);
        // 범위가 어긋나면 아무것도 하지 않는다
        compose_within(&mut c, 3, 5, 1, (0, 0), (2, 0), true);
        compose_canvas_rect(&mut dst, &src[..4], 3, 1, 1, (0, 0), (0, 0), true);
        assert_eq!(c, [1, 2, 3, 4, 0, 0, 0, 0, 1, 2, 3, 4]);
    }

    #[test]
    fn compose_rect_check_validates_in_u64() {
        let img = (4, 3);
        assert_eq!(
            check_compose_rect(img, (0, 0), (0, 0), (0, 0), false),
            Ok((4, 3)),
            "0은 그림 전체"
        );
        assert!(
            check_compose_rect(img, (1, 0), (0, 0), (0, 0), false).is_err(),
            "대상이 벗어남"
        );
        assert!(
            check_compose_rect(img, (0, 0), (0, 1), (0, 0), false).is_err(),
            "원본이 벗어남"
        );
        assert_eq!(check_compose_rect(img, (2, 0), (0, 0), (2, 3), false), Ok((2, 3)));
        // 같은 프레임: 겹치면 오류, 안 겹치면 통과
        assert!(check_compose_rect(img, (0, 0), (0, 0), (1, 1), true).is_err());
        assert!(check_compose_rect(img, (2, 0), (0, 0), (2, 1), true).is_ok());
        assert!(
            check_compose_rect(img, (1, 0), (0, 0), (2, 1), true).is_err(),
            "한 칸 겹침"
        );
        // 믿을 수 없는 u32가 넘치지 않는다
        assert!(check_compose_rect(img, (u32::MAX, u32::MAX), (0, 0), (u32::MAX, u32::MAX), false).is_err());
        assert!(check_compose_rect(img, (0, 0), (u32::MAX, 0), (1, 1), true).is_err());
    }
}
