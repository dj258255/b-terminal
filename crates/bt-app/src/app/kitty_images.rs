//! kitty 그림을 칸 위에 그리는 층 나누기(#547). CPU 길(`redraw`)과 GPU 길(`redraw_gpu`)이 같은 함수를 쓴다:
//! 칸 바탕 아래(`z < i32::MIN / 2`) → 칸 바탕 → 글자 아래(`z < 0`) → 글자 → 글자 위(`z >= 0`)(Ghostty `ImageLayer`).
//! 그림의 자리와 크기는 `bt_core::kitty_geom`이 이 칸의 칸 크기로 정한다

use std::time::{Duration, Instant};

use bt_core::kitty_geom::Layer;
use bt_render::kitty::{ImagePaint, has_layer, paint_layer};
use bt_render::{Canvas, GridView, Under};

/// 칸 하나의 그림들과 그리는 자리
pub(super) struct PaneImages<'a> {
    paints: Vec<ImagePaint<'a>>,
    /// 칸 격자 왼쪽 위(창 픽셀)
    origin: (i32, i32),
    /// 칸 크기(px)
    cell: (u32, u32),
    /// 격자 사각형 `(x0, y0, x1, y1)`: 이 밖은 자른다
    clip: (i32, i32, i32, i32),
}

impl<'a> PaneImages<'a> {
    pub(super) fn new(
        images: &'a [bt_term::ImageView],
        origin: (i32, i32),
        cell: (u32, u32),
        grid: (usize, usize),
    ) -> Self {
        let paints = images
            .iter()
            .map(|im| ImagePaint {
                rgba: &im.rgba,
                geom: im.geom,
                row: im.row,
                col: im.col,
                z: im.z,
            })
            .collect();
        let clip = (
            origin.0,
            origin.1,
            origin.0 + (grid.0 as u32 * cell.0) as i32,
            origin.1 + (grid.1 as u32 * cell.1) as i32,
        );
        Self {
            paints,
            origin,
            cell,
            clip,
        }
    }

    pub(super) fn has(&self, layer: Layer) -> bool {
        has_layer(&self.paints, layer)
    }

    /// `layer`의 그림을 그린다(z가 낮은 것부터)
    pub(super) fn paint(&self, canvas: &mut Canvas, layer: Layer) {
        paint_layer(canvas, &self.paints, layer, self.origin, self.cell, self.clip);
    }

    /// 격자를 그리면서 칸 바탕 아래·글자 아래 그림을 끼운다. 글자 위 그림은 부른 쪽이 `paint(AboveText)`로 뒤에 그린다.
    /// `default_bg`는 화면의 기본 바탕색: 칸 바탕 아래 그림이 있으면 그 색의 칸은 칠하지 않아 그림이 비친다
    pub(super) fn draw_grid(
        &self,
        canvas: &mut Canvas,
        fonts: &mut bt_render::Fonts,
        view: &GridView,
        default_bg: bt_core::Rgb,
    ) {
        let below_bg = |c: &mut Canvas| self.paint(c, Layer::BelowBackground);
        let below_text = |c: &mut Canvas| self.paint(c, Layer::BelowText);
        let under = Under {
            below_bg: self
                .has(Layer::BelowBackground)
                .then_some(&below_bg as &dyn Fn(&mut Canvas)),
            below_text: self
                .has(Layer::BelowText)
                .then_some(&below_text as &dyn Fn(&mut Canvas)),
            default_bg: Some(default_bg),
        };
        bt_render::draw_grid_under(canvas, fonts, self.origin.0, self.origin.1, view, &under);
    }
}

/// kitty 애니메이션(#554)의 다음 프레임 때에 창을 깨울 시각. `next`는 보이는 그림 가운데 가장 이른 다음 프레임 시각(없으면 깨우지 않는다).
/// Ghostty처럼 그리기 간격(`DRAW_INTERVAL_MS`, 8ms)보다 빨리 깨우지 않는다(`generic.zig`의 `animationWake`가 `@max(next -| now, draw_interval_ms)`).
/// 아주 짧은 간격(1ms)을 보낸 프로그램이 있어도 다시 그리기는 8ms에 한 번을 넘지 않는다
pub(super) fn frame_wake(next: Option<Instant>, now: Instant) -> Option<Instant> {
    next.map(|t| t.max(now + Duration::from_millis(super::shader_inputs::DRAW_INTERVAL_MS)))
}

/// `about_to_wait`에서 창 하나의 애니메이션 깨우기를 본다. 돌려주는 값: (지금 다시 그리기를 요청할지, 남길 깨울 때).
/// 가려진 창은 깨우지 않고 남기지도 않는다(다시 보이면 `Occluded(false)`가 바로 그린다). 깨울 때가 되면 요청하고 지운다:
/// 그린 프레임이 다음 때를 새로 정한다
pub(super) fn wake_step(wake: Option<Instant>, now: Instant, occluded: bool) -> (bool, Option<Instant>) {
    if occluded {
        return (false, None);
    }
    match wake {
        Some(t) if t <= now => (true, None),
        other => (false, other),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ms(n: u64) -> Duration {
        Duration::from_millis(n)
    }

    #[test]
    fn no_animation_means_no_wake() {
        let t0 = Instant::now();
        assert_eq!(frame_wake(None, t0), None);
    }

    #[test]
    fn wake_follows_the_next_frame_time() {
        let t0 = Instant::now();
        assert_eq!(frame_wake(Some(t0 + ms(100)), t0), Some(t0 + ms(100)));
    }

    #[test]
    fn very_short_gaps_never_redraw_faster_than_the_draw_interval() {
        let t0 = Instant::now();
        // 1ms 간격을 보낸 프로그램: 8ms 아래로 내려가지 않는다
        assert_eq!(frame_wake(Some(t0 + ms(1)), t0), Some(t0 + ms(8)));
        // 이미 지난 때(늦었다)도 바로 그리지 않고 8ms 뒤로 잡는다(Ghostty: 늦은 프레임은 다음 깨어남에서 집는다)
        assert_eq!(frame_wake(Some(t0), t0 + ms(50)), Some(t0 + ms(58)));
        assert_eq!(frame_wake(Some(t0 + ms(8)), t0), Some(t0 + ms(8)));
    }

    #[test]
    fn wake_step_fires_when_due_and_clears() {
        let t0 = Instant::now();
        assert_eq!(wake_step(None, t0, false), (false, None));
        assert_eq!(wake_step(Some(t0 + ms(10)), t0, false), (false, Some(t0 + ms(10))));
        assert_eq!(wake_step(Some(t0 + ms(10)), t0 + ms(10), false), (true, None));
        assert_eq!(wake_step(Some(t0 + ms(10)), t0 + ms(99), false), (true, None));
    }

    #[test]
    fn an_occluded_window_is_never_woken() {
        let t0 = Instant::now();
        assert_eq!(wake_step(Some(t0), t0 + ms(5), true), (false, None));
        assert_eq!(wake_step(Some(t0 + ms(500)), t0, true), (false, None));
    }
}
