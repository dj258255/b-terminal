//! kitty 그림을 칸 위에 그리는 층 나누기(#547). CPU 길(`redraw`)과 GPU 길(`redraw_gpu`)이 같은 함수를 쓴다:
//! 칸 바탕 아래(`z < i32::MIN / 2`) → 칸 바탕 → 글자 아래(`z < 0`) → 글자 → 글자 위(`z >= 0`)(Ghostty `ImageLayer`).
//! 그림의 자리와 크기는 `bt_core::kitty_geom`이 이 칸의 칸 크기로 정한다

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
