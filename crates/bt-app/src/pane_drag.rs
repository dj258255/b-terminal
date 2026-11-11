//! 분할 칸을 끌어 옮기는 손잡이(`drag-handle`, Ghostty 1.4). 창 없이 시험하려고 판단·상태·그리기를 이 파일에 모은다.
//!
//! 모양과 자리는 Ghostty macOS 소스(`SurfaceGrabHandle.swift`)를 따른다: 칸 위쪽 가운데 80×12pt 손잡이, 그 안에 점 셋(`…`),
//! 칸 위 20%(최소 12pt)의 가로 띠에 마우스가 있으면 점이 보이고(손잡이 위면 더 진하게), 놓을 자리는 칸의 그쪽 절반을
//! 강조색 30%로 덮는다. 본 것은 소스 글뿐이고 Ghostty 창은 띄워 보지 않았다

use bt_core::Rgb;
use bt_core::config::DragHandle;
use bt_render::canvas::Canvas;

use crate::split::{Direction, Rect, drop_direction, drop_preview};

/// 손잡이 크기(점). Ghostty `handleSize`
const HANDLE_W_PT: f64 = 80.0;
const HANDLE_H_PT: f64 = 12.0;
/// 손잡이를 드러내는 띠의 높이: 칸 높이의 이 비율(최소 손잡이 높이). Ghostty `hoverHeightFactor`
const BAND_FRACTION: f64 = 0.2;
/// 점 하나의 한 변과 점 사이 간격(점)
const DOT_PT: f64 = 2.5;
const DOT_GAP_PT: f64 = 3.0;
/// 점의 덮음 알파: 평소 0.3, 손잡이 위(또는 끄는 중) 0.8. Ghostty와 같다
const DIM_ALPHA: u8 = 77;
const BRIGHT_ALPHA: u8 = 204;
/// 놓을 자리 미리 보기의 덮음 알파(0.3)
const DROP_ALPHA: u8 = 77;

/// 칸 바깥 사각형 위쪽 가운데의 손잡이 자리(칸이 작으면 칸 안으로 줄인다)
pub fn handle_rect(outer: Rect, scale: f64) -> Rect {
    let w = ((HANDLE_W_PT * scale).round() as u32).min(outer.w);
    let h = ((HANDLE_H_PT * scale).round() as u32).min(outer.h);
    Rect::new(outer.x + ((outer.w - w) / 2) as i32, outer.y, w, h)
}

/// 손잡이를 드러내는 위쪽 가로 띠(칸 너비 전체)
pub fn hover_band(outer: Rect, scale: f64) -> Rect {
    let handle_h = (HANDLE_H_PT * scale).round();
    let h = (outer.h as f64 * BAND_FRACTION).max(handle_h).min(outer.h as f64) as u32;
    Rect::new(outer.x, outer.y, outer.w, h)
}

/// 손잡이를 어떻게 보일까
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Look {
    Hidden,
    /// 점을 흐리게(마우스가 띠 안, 또는 `always`)
    Dim,
    /// 점을 진하게(손잡이 위, 끄는 중)
    Bright,
}

/// 손잡이를 정하는 입력
#[derive(Debug, Clone, Copy)]
pub struct HandleInput {
    pub mode: DragHandle,
    /// 이 탭의 칸 수
    pub panes: usize,
    /// 칸 하나를 크게 보는 중
    pub zoomed: bool,
    /// 마우스가 이 칸의 위쪽 띠 안에 있다
    pub in_band: bool,
    /// 마우스가 이 칸의 손잡이 위에 있다
    pub over_handle: bool,
    /// 이 칸을 끄는 중이다
    pub dragging: bool,
}

/// 손잡이를 보일지, 얼마나 진하게 보일지. `auto`는 칸이 둘 이상일 때만 보인다(끌어서 새 창 만들기가 없어서).
/// 크게 보는 중에는 끌 수 없으므로 `always`만 보인다(Ghostty는 그때도 그리지만 끌 수 없는 손잡이는 혼란만 준다)
pub fn look(i: HandleInput) -> Look {
    let strong = i.over_handle || i.dragging;
    match i.mode {
        DragHandle::Never => Look::Hidden,
        DragHandle::Always => {
            if strong {
                Look::Bright
            } else {
                Look::Dim
            }
        }
        DragHandle::Auto => {
            // Ghostty macOS는 칸 하나여도 보인다(끌어서 새 창을 만든다). b-terminal은 그 기능이 없어
            // 칸 하나의 손잡이는 눌러도 하는 일이 없으므로 모든 OS에서 칸이 둘 이상일 때만 보인다
            let allowed = i.panes > 1;
            if !allowed || i.zoomed || !(i.in_band || strong) {
                Look::Hidden
            } else if strong {
                Look::Bright
            } else {
                Look::Dim
            }
        }
    }
}

/// 손잡이나 놓을 자리 미리 보기를 위층에 그려야 하는가. 덧그림 표시(`chrome.overlay`)에 들어가 위층을 쓰는 판단에 빠지지 않게 한다
pub fn needs_upper(looks: &[Look], has_drop: bool) -> bool {
    has_drop || looks.iter().any(|l| *l != Look::Hidden)
}

/// 손잡이를 잡을 수 있는가. 옮길 칸이 둘 이상이고 크게 보는 중이 아니어야 한다. 아니면 누름은 평소처럼 칸이 받는다
pub fn grabbable(mode: DragHandle, panes: usize, zoomed: bool) -> bool {
    mode != DragHandle::Never && panes > 1 && !zoomed
}

/// 놓을 자리: 이 칸의 이 쪽
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Drop {
    pub pane: u64,
    pub dir: Direction,
}

/// 마우스 자리가 가리키는 놓을 자리. 끄는 칸 자신과 칸 밖(경계선·탭 줄 등)은 `None`
pub fn drop_at(rects: &[(u64, Rect)], from: u64, x: f64, y: f64) -> Option<Drop> {
    if !rects.iter().any(|(id, _)| *id == from) {
        return None;
    }
    let (pane, rect) = rects.iter().find(|(_, r)| r.contains(x, y))?;
    if *pane == from {
        return None;
    }
    Some(Drop {
        pane: *pane,
        dir: drop_direction(*rect, x, y)?,
    })
}

/// 끌기를 마친 결과
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Outcome {
    /// `from` 칸을 `to` 칸의 `dir` 쪽으로 옮긴다
    Move { from: u64, to: u64, dir: Direction },
    /// 아무것도 하지 않는다(Esc, 칸 밖, 자기 자신 위)
    Cancelled,
}

/// 끌기 상태 기계. 누름 → 움직임 → 놓음(또는 취소)만 안다. 창·좌표는 모르고 앱이 계산한 놓을 자리를 받는다
#[derive(Debug, Default)]
pub struct PaneDrag {
    from: Option<u64>,
    target: Option<Drop>,
}

impl PaneDrag {
    pub fn is_active(&self) -> bool {
        self.from.is_some()
    }

    /// 끌고 있는 칸
    pub fn from(&self) -> Option<u64> {
        self.from
    }

    /// 지금 가리키는 놓을 자리(미리 보기)
    pub fn target(&self) -> Option<Drop> {
        self.target
    }

    /// 손잡이를 눌렀다. 손잡이 위(`Some(칸)`)이고 끌고 있지 않을 때만 시작해 참
    pub fn press(&mut self, handle_pane: Option<u64>) -> bool {
        match (self.from, handle_pane) {
            (None, Some(pane)) => {
                self.from = Some(pane);
                self.target = None;
                true
            }
            _ => false,
        }
    }

    /// 마우스가 움직였다. 놓을 자리가 바뀌었으면 참(다시 그린다)
    pub fn moved(&mut self, target: Option<Drop>) -> bool {
        if self.from.is_none() || self.target == target {
            return false;
        }
        self.target = target;
        true
    }

    /// 단추를 놓았다. 끌고 있지 않았으면 `None`. 놓은 자리가 칸이 아니거나 끌던 칸 자신이면 취소다
    pub fn release(&mut self, target: Option<Drop>) -> Option<Outcome> {
        let from = self.from.take()?;
        self.target = None;
        Some(match target {
            Some(d) if d.pane != from => Outcome::Move {
                from,
                to: d.pane,
                dir: d.dir,
            },
            _ => Outcome::Cancelled,
        })
    }

    /// Esc 등으로 취소했다. 끌고 있었으면 `Cancelled`
    pub fn cancel(&mut self) -> Option<Outcome> {
        self.from.take()?;
        self.target = None;
        Some(Outcome::Cancelled)
    }
}

/// 손잡이의 점 셋을 칸 위쪽 가운데에 그린다. `Hidden`이면 아무 픽셀도 건드리지 않는다
pub fn draw_handle(canvas: &mut Canvas, outer: Rect, look: Look, color: Rgb, scale: f64) {
    let alpha = match look {
        Look::Hidden => return,
        Look::Dim => DIM_ALPHA,
        Look::Bright => BRIGHT_ALPHA,
    };
    let handle = handle_rect(outer, scale);
    let dot = ((DOT_PT * scale).round() as u32).max(1);
    let gap = ((DOT_GAP_PT * scale).round() as u32).max(1);
    let total = dot * 3 + gap * 2;
    if handle.w < total || handle.h < dot {
        return;
    }
    let x0 = handle.x + ((handle.w - total) / 2) as i32;
    // Ghostty는 손잡이 가운데에서 3pt 위(오프셋 -3)에 점을 놓는다
    let center = handle.y + (handle.h / 2) as i32 - (3.0 * scale).round() as i32;
    let y = (center - (dot / 2) as i32).max(handle.y);
    for k in 0..3 {
        canvas.blend_rect(x0 + (k * (dot + gap)) as i32, y, dot, dot, color, alpha);
    }
}

/// 놓을 자리 미리 보기: 칸의 그쪽 절반을 강조색으로 덮는다
pub fn draw_drop(canvas: &mut Canvas, target_rect: Rect, dir: Direction, accent: Rgb) {
    let r = drop_preview(target_rect, dir);
    canvas.blend_rect(r.x, r.y, r.w, r.h, accent, DROP_ALPHA);
}

#[cfg(test)]
mod tests {
    use super::*;

    fn input() -> HandleInput {
        HandleInput {
            mode: DragHandle::Auto,
            panes: 2,
            zoomed: false,
            in_band: false,
            over_handle: false,
            dragging: false,
        }
    }

    #[test]
    fn never_is_always_hidden() {
        for (in_band, over_handle, dragging) in [(false, false, false), (true, true, false), (true, true, true)] {
            let i = HandleInput {
                mode: DragHandle::Never,
                in_band,
                over_handle,
                dragging,
                ..input()
            };
            assert_eq!(look(i), Look::Hidden);
        }
    }

    #[test]
    fn always_shows_even_for_one_pane() {
        let one = HandleInput {
            mode: DragHandle::Always,
            panes: 1,
            ..input()
        };
        assert_eq!(look(one), Look::Dim);
        assert_eq!(
            look(HandleInput {
                over_handle: true,
                ..one
            }),
            Look::Bright
        );
        assert_eq!(look(HandleInput { dragging: true, ..one }), Look::Bright);
    }

    #[test]
    fn auto_shows_in_the_band_only() {
        assert_eq!(look(input()), Look::Hidden, "마우스가 띠 밖이면 숨는다");
        assert_eq!(
            look(HandleInput {
                in_band: true,
                ..input()
            }),
            Look::Dim
        );
        assert_eq!(
            look(HandleInput {
                in_band: true,
                over_handle: true,
                ..input()
            }),
            Look::Bright
        );
        assert_eq!(
            look(HandleInput {
                dragging: true,
                ..input()
            }),
            Look::Bright,
            "끄는 중에는 마우스가 띠를 벗어나도 보인다"
        );
    }

    #[test]
    fn auto_needs_two_panes() {
        // Ghostty macOS는 칸 하나여도 보이지만(끌어서 새 창), 그 기능이 없어 숨긴다
        let band = HandleInput {
            in_band: true,
            ..input()
        };
        assert_eq!(look(HandleInput { panes: 1, ..band }), Look::Hidden);
        assert_eq!(look(HandleInput { panes: 2, ..band }), Look::Dim);
    }

    #[test]
    fn zoomed_hides_auto_and_blocks_grabbing() {
        let z = HandleInput {
            zoomed: true,
            in_band: true,
            ..input()
        };
        assert_eq!(look(z), Look::Hidden);
        assert_eq!(
            look(HandleInput {
                mode: DragHandle::Always,
                ..z
            }),
            Look::Dim
        );
        assert!(!grabbable(DragHandle::Always, 3, true));
    }

    #[test]
    fn upper_layer_is_needed_exactly_when_something_is_drawn() {
        assert!(!needs_upper(&[], false));
        assert!(!needs_upper(&[Look::Hidden, Look::Hidden], false));
        assert!(needs_upper(&[Look::Hidden, Look::Dim], false));
        assert!(needs_upper(&[Look::Bright], false));
        assert!(needs_upper(&[Look::Hidden], true), "놓을 자리만 있어도");
    }

    #[test]
    fn whatever_needs_no_upper_layer_draws_nothing() {
        // 위층 단언의 짝: 쓰지 않는다고 본 입력은 실제로 아무 픽셀도 칠하지 않는다
        let outer = Rect::new(0, 0, 400, 300);
        let looks = [Look::Hidden];
        assert!(!needs_upper(&looks, false));
        let base = pixels(|_| {});
        let img = pixels(|c| {
            for l in looks {
                draw_handle(c, outer, l, Rgb { r: 255, g: 255, b: 255 }, 1.0);
            }
        });
        assert_eq!(img, base);
    }

    #[test]
    fn grabbable_needs_two_panes_and_not_never() {
        assert!(grabbable(DragHandle::Auto, 2, false));
        assert!(grabbable(DragHandle::Always, 2, false));
        assert!(!grabbable(DragHandle::Never, 2, false));
        assert!(!grabbable(DragHandle::Always, 1, false), "칸이 하나면 끌 것이 없다");
    }

    #[test]
    fn handle_is_top_center_and_clamped() {
        let outer = Rect::new(100, 40, 801, 600);
        let h = handle_rect(outer, 2.0);
        assert_eq!((h.w, h.h), (160, 24));
        assert_eq!(h.y, 40);
        assert_eq!(h.x, 100 + (801 - 160) / 2);
        let tiny = handle_rect(Rect::new(0, 0, 50, 8), 1.0);
        assert_eq!((tiny.w, tiny.h), (50, 8), "칸보다 크지 않다");
    }

    #[test]
    fn hover_band_is_a_fifth_of_the_height_with_a_floor() {
        let band = hover_band(Rect::new(0, 0, 400, 500), 1.0);
        assert_eq!((band.w, band.h), (400, 100));
        let short = hover_band(Rect::new(0, 0, 400, 30), 1.0);
        assert_eq!(short.h, 12, "칸이 낮아도 손잡이 높이는 된다");
        assert_eq!(hover_band(Rect::new(0, 0, 400, 8), 1.0).h, 8, "칸보다 높지 않다");
    }

    fn rects() -> Vec<(u64, Rect)> {
        vec![(1, Rect::new(0, 0, 400, 300)), (2, Rect::new(401, 0, 400, 300))]
    }

    #[test]
    fn drop_at_names_the_pane_under_the_pointer_and_the_nearest_edge() {
        assert_eq!(
            drop_at(&rects(), 1, 410.0, 150.0),
            Some(Drop {
                pane: 2,
                dir: Direction::Left
            })
        );
        assert_eq!(
            drop_at(&rects(), 1, 600.0, 5.0),
            Some(Drop {
                pane: 2,
                dir: Direction::Up
            })
        );
    }

    #[test]
    fn drop_at_ignores_self_gaps_and_outside() {
        assert_eq!(drop_at(&rects(), 1, 200.0, 150.0), None, "자기 자신 위");
        assert_eq!(drop_at(&rects(), 1, 400.5, 150.0), None, "경계선 위");
        assert_eq!(drop_at(&rects(), 1, 900.0, 150.0), None, "칸 밖");
        assert_eq!(drop_at(&rects(), 9, 600.0, 150.0), None, "끌던 칸이 이 탭에 없다");
    }

    fn left_of_2() -> Option<Drop> {
        Some(Drop {
            pane: 2,
            dir: Direction::Left,
        })
    }

    #[test]
    fn press_move_release_yields_a_move() {
        let mut d = PaneDrag::default();
        assert!(!d.is_active());
        assert!(d.press(Some(1)));
        assert!(d.is_active());
        assert_eq!(d.from(), Some(1));
        assert!(d.moved(left_of_2()), "놓을 자리가 생겼다");
        assert_eq!(d.target(), left_of_2());
        assert!(!d.moved(left_of_2()), "같으면 다시 그리지 않는다");
        let down = Some(Drop {
            pane: 2,
            dir: Direction::Down,
        });
        assert!(d.moved(down));
        assert_eq!(
            d.release(down),
            Some(Outcome::Move {
                from: 1,
                to: 2,
                dir: Direction::Down
            })
        );
        assert!(!d.is_active());
        assert_eq!(d.target(), None);
    }

    #[test]
    fn release_uses_the_final_position_not_the_last_preview() {
        let mut d = PaneDrag::default();
        d.press(Some(1));
        d.moved(left_of_2());
        assert_eq!(d.release(None), Some(Outcome::Cancelled), "칸 밖에 놓으면 취소");
    }

    #[test]
    fn releasing_on_itself_cancels() {
        let mut d = PaneDrag::default();
        d.press(Some(1));
        let own = Some(Drop {
            pane: 1,
            dir: Direction::Left,
        });
        assert_eq!(d.release(own), Some(Outcome::Cancelled));
    }

    #[test]
    fn escape_cancels_and_later_release_is_nothing() {
        let mut d = PaneDrag::default();
        d.press(Some(1));
        d.moved(left_of_2());
        assert_eq!(d.cancel(), Some(Outcome::Cancelled));
        assert!(!d.is_active());
        assert_eq!(d.target(), None);
        assert_eq!(d.release(left_of_2()), None, "취소한 뒤 놓아도 옮기지 않는다");
        assert_eq!(d.cancel(), None);
    }

    #[test]
    fn press_off_the_handle_or_while_dragging_does_not_start() {
        let mut d = PaneDrag::default();
        assert!(!d.press(None));
        assert!(!d.is_active());
        assert!(!d.moved(left_of_2()), "끌지 않을 때 움직임은 상태를 바꾸지 않는다");
        assert_eq!(d.target(), None);
        assert!(d.press(Some(1)));
        assert!(!d.press(Some(2)), "끄는 중에 다시 누르지 않는다");
        assert_eq!(d.from(), Some(1));
    }

    fn pixels(f: impl FnOnce(&mut Canvas)) -> Vec<u32> {
        let (w, h) = (400u32, 300u32);
        let mut buf = vec![0x0010_2030u32; (w * h) as usize];
        let mut c = Canvas::new(&mut buf, w, h);
        f(&mut c);
        buf
    }

    #[test]
    fn hidden_handle_changes_no_pixel() {
        let outer = Rect::new(0, 0, 400, 300);
        let base = pixels(|_| {});
        let hidden = pixels(|c| draw_handle(c, outer, Look::Hidden, Rgb { r: 255, g: 255, b: 255 }, 1.0));
        assert_eq!(base, hidden);
    }

    #[test]
    fn shown_handle_paints_only_inside_the_handle_and_bright_is_stronger() {
        let outer = Rect::new(0, 0, 400, 300);
        let white = Rgb { r: 255, g: 255, b: 255 };
        let base = pixels(|_| {});
        let dim = pixels(|c| draw_handle(c, outer, Look::Dim, white, 1.0));
        let bright = pixels(|c| draw_handle(c, outer, Look::Bright, white, 1.0));
        let handle = handle_rect(outer, 1.0);
        let changed = |img: &[u32]| -> Vec<usize> { (0..img.len()).filter(|&i| img[i] != base[i]).collect() };
        let dim_px = changed(&dim);
        assert_eq!(dim_px.len(), 3 * 3 * 3, "점 셋, 한 변 3픽셀(2.5 반올림)");
        for i in &dim_px {
            let (x, y) = ((*i % 400) as i32, (*i / 400) as i32);
            assert!(handle.contains(x as f64, y as f64), "{x},{y}");
        }
        assert_eq!(changed(&bright), dim_px, "같은 자리");
        let lum = |p: u32| (p >> 8) & 0xff;
        assert!(lum(bright[dim_px[0]]) > lum(dim[dim_px[0]]));
    }

    #[test]
    fn drop_preview_paints_exactly_the_half() {
        let rect = Rect::new(0, 0, 400, 300);
        let base = pixels(|_| {});
        let img = pixels(|c| draw_drop(c, rect, Direction::Right, Rgb { r: 0, g: 120, b: 255 }));
        let changed = (0..img.len()).filter(|&i| img[i] != base[i]).count();
        assert_eq!(changed, 200 * 300);
        assert_eq!(img[100], base[100], "왼쪽 절반은 그대로");
        assert_ne!(img[300], base[300]);
    }
}
