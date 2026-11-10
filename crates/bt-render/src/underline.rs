//! 밑줄 다섯 모양(#276). 픽셀 규칙은 Ghostty `font/sprite/draw/special.zig`의 `underline`·`underline_double`·
//! `underline_dotted`·`underline_dashed`·`underline_curly`를 따른다.
//!
//! Ghostty는 칸 아래로 칸 높이의 1/4까지 넘치는 캔버스에 그리고 그 안에서 위치를 자른다.
//! 여기서는 줄이 칸 안(0..높이)에 들어가게 자른다(font.rs의 `underline_offset`도 같다).
//! 점과 물결의 가장자리는 8x8 표본으로 덮임(coverage)을 재서 부드럽게 한다(Ghostty는 z2d의 안티에일리어싱).

use std::cell::RefCell;
use std::collections::HashMap;
use std::f64::consts::{PI, SQRT_2};
use std::rc::Rc;

use bt_core::Rgb;

use crate::canvas::Canvas;

/// 밑줄 모양(SGR 4, 4:2~4:5)
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Underline {
    #[default]
    None,
    Single,
    Double,
    Curly,
    Dotted,
    Dashed,
}

/// 한 칸(너비 `w`, 높이 `h`)의 왼쪽 위 `(x, y)`에 밑줄을 긋는다. `pos`는 밑줄 윗변의 칸 안 위치, `thick`은 두께(픽셀)
#[allow(clippy::too_many_arguments)]
pub fn draw(canvas: &mut Canvas, kind: Underline, x: i32, y: i32, w: u32, h: u32, pos: u32, thick: u32, color: Rgb) {
    match kind {
        Underline::None => {}
        Underline::Single => {
            let top = pos.min(h.saturating_sub(thick));
            canvas.fill_rect(x, y + top as i32, w, thick, color);
        }
        Underline::Double => {
            // 단일 밑줄 자리의 위아래로 한 두께씩 비워 "음각" 하나를 만든다
            let top = pos.min(h.saturating_sub(2 * thick));
            canvas.fill_rect(x, y + top.saturating_sub(thick) as i32, w, thick, color);
            canvas.fill_rect(x, y + (top + thick) as i32, w, thick, color);
        }
        Underline::Dashed => {
            let top = pos.min(h.saturating_sub(thick));
            let dash = w / 3 + 1;
            let count = w / dash + 1;
            for i in (0..count).step_by(2) {
                let dx = i * dash;
                if dx >= w {
                    break;
                }
                canvas.fill_rect(x + dx as i32, y + top as i32, dash.min(w - dx), thick, color);
            }
        }
        Underline::Dotted | Underline::Curly => {
            let key = (kind as u8, w, h, pos, thick);
            let mask = mask_for(key, || match kind {
                Underline::Dotted => dotted_mask(w, h, pos, thick),
                _ => curly_mask(w, h, pos, thick),
            });
            for row in 0..h {
                for col in 0..w {
                    let a = mask[(row * w + col) as usize];
                    if a != 0 {
                        canvas.blend_px(x + col as i32, y + row as i32, color, a);
                    }
                }
            }
        }
    }
}

type Key = (u8, u32, u32, u32, u32);

thread_local! {
    static MASKS: RefCell<HashMap<Key, Rc<Vec<u8>>>> = RefCell::new(HashMap::new());
}

/// 같은 칸 크기·위치·두께의 가장자리 덮임은 한 번만 잰다(칸마다 다시 재면 화면이 가득 찰 때 느리다)
fn mask_for(key: Key, make: impl FnOnce() -> Vec<u8>) -> Rc<Vec<u8>> {
    MASKS.with(|m| {
        let mut m = m.borrow_mut();
        if let Some(v) = m.get(&key) {
            return v.clone();
        }
        // 글꼴 크기를 자주 바꿔도 메모리가 늘지 않게
        if m.len() >= 16 {
            m.clear();
        }
        let v = Rc::new(make());
        m.insert(key, v.clone());
        v
    })
}

/// 칸 `w x h`의 픽셀마다 `inside`인 표본 비율(0~255). `rows`만 훑는다
fn coverage(w: u32, h: u32, rows: std::ops::Range<i32>, inside: impl Fn(f64, f64) -> bool) -> Vec<u8> {
    const N: u32 = 8;
    let mut out = vec![0u8; (w * h) as usize];
    for row in rows.start.max(0)..rows.end.min(h as i32) {
        for col in 0..w {
            let mut hit = 0;
            for sy in 0..N {
                for sx in 0..N {
                    let px = col as f64 + (sx as f64 + 0.5) / N as f64;
                    let py = row as f64 + (sy as f64 + 0.5) / N as f64;
                    if inside(px, py) {
                        hit += 1;
                    }
                }
            }
            out[(row as u32 * w + col) as usize] = ((hit * 255 + N * N / 2) / (N * N)) as u8;
        }
    }
    out
}

/// 점 밑줄: 지름이 두께의 √2배인 점을 칸 안에 고르게(special.zig `underline_dotted`)
fn dotted_mask(w: u32, h: u32, pos: u32, thick: u32) -> Vec<u8> {
    let (fw, fh, fpos, ft) = (w as f64, h as f64, pos as f64, thick as f64);
    let radius = ft * SQRT_2 / 2.0;
    // 점 줄기의 가운데. 칸 아래로 잘리지 않는 가장 낮은 자리까지만
    let yc = (fpos + 0.5 * ft).min(fh - radius.ceil());
    let n = (fw / (4.0 * radius))
        .ceil()
        .min((fw / (3.0 * radius)).floor())
        .min((fw / (2.0 * radius + 1.0)).floor())
        .max(1.0);
    let step = fw / n;
    let centers: Vec<f64> = (0..n as usize).map(|i| step / 2.0 + step * i as f64).collect();
    let r2 = radius * radius;
    coverage(
        w,
        h,
        (yc - radius).floor() as i32..(yc + radius).ceil() as i32 + 1,
        |px, py| centers.iter().any(|cx| (px - cx).powi(2) + (py - yc).powi(2) <= r2),
    )
}

/// 물결 밑줄: 칸마다 한 주기. 진폭은 칸 너비/π, 곡률 0.4, 둥근 끝(special.zig `underline_curly`)
fn curly_mask(w: u32, h: u32, pos: u32, thick: u32) -> Vec<u8> {
    let (fw, fh, ft) = (w as f64, h as f64, thick as f64);
    let amp = fw / PI;
    // 칸 아래로 잘리지 않는 가장 낮은 자리까지만
    let top = (pos as f64).min(fh - amp - ft);
    let bottom = top + amp;
    let (r, c) = (0.4, 0.5 * fw);
    let cubic = |p: [(f64, f64); 4], t: f64| {
        let u = 1.0 - t;
        let f =
            |a: f64, b: f64, c: f64, d: f64| u * u * u * a + 3.0 * u * u * t * b + 3.0 * u * t * t * c + t * t * t * d;
        (f(p[0].0, p[1].0, p[2].0, p[3].0), f(p[0].1, p[1].1, p[2].1, p[3].1))
    };
    let a = [(0.0, bottom), (c * r, bottom), (c - c * r, top), (c, top)];
    let b = [(c, top), (c + c * r, top), (fw - c * r, bottom), (fw, bottom)];
    const SEG: usize = 48;
    let mut pts: Vec<(f64, f64)> = (0..=SEG).map(|i| cubic(a, i as f64 / SEG as f64)).collect();
    pts.extend((1..=SEG).map(|i| cubic(b, i as f64 / SEG as f64)));
    let half2 = (ft / 2.0) * (ft / 2.0);
    let dist2 = |px: f64, py: f64| {
        pts.windows(2)
            .map(|s| {
                let ((x0, y0), (x1, y1)) = (s[0], s[1]);
                let (dx, dy) = (x1 - x0, y1 - y0);
                let l2 = dx * dx + dy * dy;
                let t = if l2 == 0.0 {
                    0.0
                } else {
                    (((px - x0) * dx + (py - y0) * dy) / l2).clamp(0.0, 1.0)
                };
                (px - (x0 + t * dx)).powi(2) + (py - (y0 + t * dy)).powi(2)
            })
            .fold(f64::MAX, f64::min)
    };
    coverage(
        w,
        h,
        (top - ft).floor() as i32..(bottom + ft).ceil() as i32 + 1,
        |px, py| dist2(px, py) <= half2,
    )
}
