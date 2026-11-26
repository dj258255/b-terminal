//! 커서 흔적(kitty `cursor_trail`, #891): 커서가 멀리 뛰면 네 꼭짓점이 서로 다른 속도로 따라오는 사각형을 커서 뒤에 그린다.
//!
//! 이 모듈은 그리지 않는다. 상태 갱신(`CursorTrail::update`)과, 사각형을 칸 격자 픽셀 사각형들로 쪼개는 일(`trail_rects`)만
//! 한다. CPU 길(`draw_grid`)과 GPU 길(`bt-gpu`의 장면)이 같은 `trail_rects`를 쓰므로 두 길이 같은 픽셀을 칠한다.
//! 시간은 값(`Duration`)으로 받는다(시계를 읽지 않아서 시험할 수 있다).
//!
//! 규칙의 출처는 kitty 저장소 `kitty/cursor_trail.c`(`update_cursor_trail*`)와 `kitty/shaders/trail.slang`이다.
//! 칸(분할 하나) 격자의 픽셀 좌표(왼쪽 위가 원점, y는 아래로)에서 계산한다. kitty와 다른 점 둘: kitty는 창 전체의
//! NDC에서 계산하므로 가로·세로 비율이 다르면 감쇠 속도를 정하는 내적이 조금 달라지고, 다른 칸으로 옮길 때 칸을 넘는
//! 흔적도 그린다. 여기는 칸마다 따로 계산하고 칸 안에만 그린다.

use std::time::Duration;

use bt_core::Rgb;

use crate::font::CellMetrics;
use crate::grid::CursorStyle;

/// 움직이는 동안 다시 그리는 간격(Ghostty 사용자 셰이더의 `draw_interval_ms` 8ms와 같다)
pub const FRAME: Duration = Duration::from_millis(8);

/// 흔적 설정(설정 `cursor-trail*`)
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct TrailParams {
    /// 커서가 이만큼 가만히 있은 뒤의 이동만 따라간다(`cursor-trail`, ms). 0이면 흔적을 쓰지 않으니 이 값은 늘 양수다
    pub delay: Duration,
    /// 가장 빠른·가장 느린 꼭짓점이 남은 거리의 1/1024로 줄어드는 데 걸리는 초
    pub decay_fast: f32,
    pub decay_slow: f32,
    /// 이동이 가로·세로로 이 칸 수 이하면 흔적을 시작하지 않는다
    pub threshold: (u32, u32),
}

/// 칸 격자 픽셀의 사각형(실수)
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct RectF {
    pub left: f32,
    pub top: f32,
    pub right: f32,
    pub bottom: f32,
}

impl RectF {
    pub fn new(left: f32, top: f32, right: f32, bottom: f32) -> Self {
        Self {
            left,
            top,
            right,
            bottom,
        }
    }
}

/// 흔적 좌표가 기대는 칸 크기와 격자 크기. 바뀌면(글자 크기, 창 크기) 흔적 없이 커서를 따라간다
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct TrailLayout {
    pub cell: (u32, u32),
    pub grid: (usize, usize),
}

/// 한 프레임에 앱이 넘기는 커서 상태
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct TrailInput {
    /// 커서 칸 (열, 줄)
    pub cell: (usize, usize),
    /// 커서가 차지하는 사각형(모양별: `cursor_target`)
    pub rect: RectF,
    /// 커서가 보이는가(DECTCEM). 깜빡임으로 꺼진 때는 보이는 것이다(kitty는 깜빡임에 흔적을 멈추지 않는다)
    pub visible: bool,
    pub layout: TrailLayout,
}

/// 흔적 사각형의 네 꼭짓점. 순서는 kitty 셰이더의 꼭짓점 번호와 같다(둘레를 따라 돈다): 오른쪽 위, 오른쪽 아래, 왼쪽 아래, 왼쪽 위
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct TrailQuad {
    pub x: [f32; 4],
    pub y: [f32; 4],
}

/// `CursorTrail::update`의 결과
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct TrailFrame {
    /// 이번 프레임에 그릴 흔적(없으면 그리지 않는다)
    pub quad: Option<TrailQuad>,
    /// 흔적의 불투명도(0~1)
    pub opacity: f32,
    /// 칸을 다시 그려야 한다: 흔적이 있거나, 방금 끝나 지워야 한다
    pub repaint: bool,
    /// 이만큼 뒤에 다시 부르라는 요청. 없으면 깨우지 않아도 된다(가만히 있을 때는 늘 없음)
    pub wake: Option<Duration>,
}

/// 모서리 번호별로 쓰는 사각형 변: x는 (오른, 오른, 왼, 왼), y는 (위, 아래, 아래, 위)
const CORNER_X: [usize; 4] = [1, 1, 0, 0];
const CORNER_Y: [usize; 4] = [0, 1, 1, 0];

/// 칸 하나의 커서 흔적 상태
#[derive(Debug, Clone)]
pub struct CursorTrail {
    corner_x: [f32; 4],
    corner_y: [f32; 4],
    edge_x: [f32; 2],
    edge_y: [f32; 2],
    has_edge: bool,
    opacity: f32,
    needs_render: bool,
    updated_at: Option<Duration>,
    layout: Option<TrailLayout>,
    /// 마지막으로 본 커서 칸과 그것을 처음 본 때(`None`은 아주 오래전)
    cell: Option<(usize, usize)>,
    cell_since: Option<Duration>,
}

impl Default for CursorTrail {
    fn default() -> Self {
        Self::new()
    }
}

impl CursorTrail {
    pub fn new() -> Self {
        Self {
            corner_x: [0.0; 4],
            corner_y: [0.0; 4],
            edge_x: [0.0; 2],
            edge_y: [0.0; 2],
            has_edge: false,
            // kitty도 0에서 시작해 `decay_slow`초에 걸쳐 1이 된다
            opacity: 0.0,
            needs_render: false,
            updated_at: None,
            layout: None,
            cell: None,
            cell_since: None,
        }
    }

    /// 프레임마다 부른다. kitty `update_cursor_trail`과 같은 차례: 목표(커서 사각형) 갱신 → 꼭짓점 이동 → 불투명도 → 그릴지
    pub fn update(&mut self, now: Duration, p: &TrailParams, inp: &TrailInput) -> TrailFrame {
        if self.layout != Some(inp.layout) {
            // 글자 크기·창 크기가 바뀌면 좌표계가 달라졌다. 흔적 없이 커서에 붙는다(kitty는 창 크기를 끄는 동안 흔적을 건너뛴다)
            self.layout = Some(inp.layout);
            self.has_edge = false;
            self.needs_render = false;
            self.cell = None;
        }
        if self.cell != Some(inp.cell) {
            // 처음 본 커서는 바로 따라가고, 옮긴 것은 본 때부터 `delay`를 센다
            self.cell_since = self.cell.map(|_| now);
            self.cell = Some(inp.cell);
        }
        let rested = self.cell_since.is_none_or(|t| now.saturating_sub(t) >= p.delay);
        if rested {
            self.update_target(inp.rect);
        }
        let dt = self.updated_at.map_or(0.0, |t| now.saturating_sub(t).as_secs_f32());
        self.update_corners(dt, p, inp);
        self.update_opacity(dt, p, inp.visible);
        let was_rendering = self.needs_render;
        self.update_needs_render();
        self.updated_at = Some(now);

        let quad = self.needs_render.then_some(TrailQuad {
            x: self.corner_x,
            y: self.corner_y,
        });
        let wake = if self.needs_render {
            Some(FRAME)
        } else {
            // 옮긴 지 `delay`가 안 됐으면 그때 다시 본다(kitty `set_maximum_wait`)
            self.cell_since
                .map(|t| (t + p.delay).saturating_sub(now))
                .filter(|d| !d.is_zero())
        };
        TrailFrame {
            quad,
            opacity: self.opacity,
            repaint: self.needs_render || was_rendering,
            wake,
        }
    }

    fn update_target(&mut self, r: RectF) {
        let (ex, ey) = ([r.left, r.right], [r.top, r.bottom]);
        if !self.has_edge {
            // 처음(또는 좌표계가 바뀐 뒤)에는 흔적 없이 커서 자리에 붙는다
            self.has_edge = true;
            self.edge_x = ex;
            self.edge_y = ey;
            for i in 0..4 {
                self.corner_x[i] = ex[CORNER_X[i]];
                self.corner_y[i] = ey[CORNER_Y[i]];
            }
            return;
        }
        self.edge_x = ex;
        self.edge_y = ey;
    }

    fn update_corners(&mut self, dt: f32, p: &TrailParams, inp: &TrailInput) {
        let (cw, ch) = (inp.layout.cell.0.max(1) as f32, inp.layout.cell.1.max(1) as f32);
        let skip = (!inp.visible && self.opacity <= 0.0)
            || (!self.needs_render && (p.threshold.0 > 0 || p.threshold.1 > 0) && {
                // 지금 흔적 꼭짓점 0(오른쪽 위)과 커서 오른쪽 위의 거리가 문턱 이하면 흔적을 시작하지 않는다
                let dx = ((self.corner_x[0] - self.edge_x[1]) / cw).round().abs();
                let dy = ((self.corner_y[0] - self.edge_y[0]) / ch).round().abs();
                dx <= p.threshold.0 as f32 && dy <= p.threshold.1 as f32
            });
        if skip {
            for i in 0..4 {
                self.corner_x[i] = self.edge_x[CORNER_X[i]];
                self.corner_y[i] = self.edge_y[CORNER_Y[i]];
            }
            return;
        }
        if dt <= 0.0 {
            return;
        }
        // 꼭짓점은 커서 꼭짓점까지 남은 거리에 비례한 속도로 다가간다(지수 감쇠). 커서 쪽으로 향한 꼭짓점일수록 빠르다
        let cx = (self.edge_x[0] + self.edge_x[1]) * 0.5;
        let cy = (self.edge_y[0] + self.edge_y[1]) * 0.5;
        let diag_2 = (self.edge_x[1] - self.edge_x[0]).hypot(self.edge_y[1] - self.edge_y[0]) * 0.5;
        let mut dx = [0.0f32; 4];
        let mut dy = [0.0f32; 4];
        let mut dot = [0.0f32; 4];
        for i in 0..4 {
            let (tx, ty) = (self.edge_x[CORNER_X[i]], self.edge_y[CORNER_Y[i]]);
            dx[i] = tx - self.corner_x[i];
            dy[i] = ty - self.corner_y[i];
            if dx[i].abs() < 1e-6 && dy[i].abs() < 1e-6 {
                dx[i] = 0.0;
                dy[i] = 0.0;
                continue;
            }
            dot[i] = (dx[i] * (tx - cx) + dy[i] * (ty - cy)) / diag_2 / dx[i].hypot(dy[i]);
        }
        let min = dot.iter().copied().fold(f32::MAX, f32::min);
        let max = dot.iter().copied().fold(f32::MIN, f32::max);
        for i in 0..4 {
            if dx[i] == 0.0 && dy[i] == 0.0 {
                continue;
            }
            let decay = if min == max {
                p.decay_slow
            } else {
                p.decay_slow + (p.decay_fast - p.decay_slow) * (dot[i] - min) / (max - min)
            };
            let step = if decay > 0.0 {
                1.0 - (-10.0 * dt / decay).exp2()
            } else {
                1.0
            };
            self.corner_x[i] += dx[i] * step;
            self.corner_y[i] += dy[i] * step;
        }
    }

    /// 커서가 보이면 `decay_slow`초에 걸쳐 1로, 숨으면 0으로(kitty `update_cursor_trail_opacity`)
    fn update_opacity(&mut self, dt: f32, p: &TrailParams, visible: bool) {
        let d = if p.decay_slow > 0.0 { dt / p.decay_slow } else { 1.0 };
        self.opacity = if visible {
            (self.opacity + d).min(1.0)
        } else {
            (self.opacity - d).max(0.0)
        };
    }

    /// 꼭짓점 하나라도 커서 꼭짓점에서 반 픽셀 이상 떨어져 있으면 그린다(kitty `update_cursor_trail_needs_render`)
    fn update_needs_render(&mut self) {
        self.needs_render = (0..4).any(|i| {
            (self.edge_x[CORNER_X[i]] - self.corner_x[i]).abs() >= 0.5
                || (self.edge_y[CORNER_Y[i]] - self.corner_y[i]).abs() >= 0.5
        });
    }
}

/// 커서가 차지하는 사각형(칸 격자 픽셀): `draw_cursor`가 칠하는 자리와 같다(칸 가운데에 둔 커서 높이, 막대 두께, 밑줄 두께).
/// 흔적의 목표이면서 흔적이 칠하지 않는 자리다(kitty 셰이더 `cursor_edge`)
pub fn cursor_target(style: CursorStyle, m: &CellMetrics, col: usize, row: usize, wide: bool) -> RectF {
    let w = m.width * if wide { 2 } else { 1 };
    let ch = m.cursor_height.min(m.height);
    let left = (col as u32 * m.width) as f32;
    let top = (row as u32 * m.height + (m.height - ch) / 2) as f32;
    let (width, top, height) = match style {
        CursorStyle::Beam => (m.cursor_beam.min(w), top, ch),
        CursorStyle::Underline => {
            let t = (m.stroke * 2).min(ch);
            (w, top + (ch - t) as f32, t)
        }
        CursorStyle::Block | CursorStyle::Hollow | CursorStyle::Lock => (w, top, ch),
    };
    RectF::new(left, top, left + width as f32, top + height as f32)
}

/// 그릴 흔적 하나: 꼭짓점, 칠하지 않을 커서 사각형, 색과 알파(0~255)
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct TrailPaint {
    pub quad: TrailQuad,
    pub mask: RectF,
    pub color: Rgb,
    pub alpha: u8,
}

/// 픽셀 사각형(칸 격자 왼쪽 위가 원점)
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PxRect {
    pub x: i32,
    pub y: i32,
    pub w: u32,
    pub h: u32,
}

/// 흔적을 `0..width × 0..height` 안의 서로 겹치지 않는 사각형들로 쪼갠다. 칠할 픽셀은 kitty가 삼각형 부채(꼭짓점 0·1·2, 0·2·3, `GL_TRIANGLE_FAN`)를
/// 래스터할 때와 같이 픽셀 중심이 삼각형 안에 드는 것이고(kitty 셰이더는 안티앨리어싱이 없다), 커서 사각형 안은 뺀다.
/// 같은 가로 범위로 이어지는 줄은 한 사각형으로 합친다(옆으로·위아래로 뛴 흔적이 몇 개 안 되게)
pub fn trail_rects(p: &TrailPaint, width: i32, height: i32) -> Vec<PxRect> {
    let q = &p.quad;
    if p.alpha == 0 || width <= 0 || height <= 0 || q.x.iter().chain(&q.y).any(|v| !v.is_finite()) {
        return Vec::new();
    }
    let min_y = q.y.iter().copied().fold(f32::MAX, f32::min);
    let max_y = q.y.iter().copied().fold(f32::MIN, f32::max);
    let y0 = ((min_y - 0.5).ceil().max(0.0)) as i32;
    let y1 = ((max_y - 0.5).floor().min(height as f32 - 1.0)) as i32;
    let tris = [[0, 1, 2], [0, 2, 3]];
    let m = &p.mask;
    let mut out: Vec<PxRect> = Vec::new();
    // 앞 줄에서 이어질 수 있는 사각형(`out`의 번호)
    let mut open: Vec<usize> = Vec::new();
    for y in y0..=y1 {
        let yc = y as f32 + 0.5;
        let mut spans: Vec<(i32, i32)> = Vec::with_capacity(3);
        for t in tris {
            if let Some((a, b)) = scan_triangle(q, t, yc) {
                spans.push((pixel_start(a), pixel_end(b)));
            }
        }
        spans.retain(|(a, b)| a <= b);
        spans.sort_unstable();
        let mut merged: Vec<(i32, i32)> = Vec::with_capacity(2);
        for (a, b) in spans {
            match merged.last_mut() {
                Some(last) if a <= last.1 + 1 => last.1 = last.1.max(b),
                _ => merged.push((a, b)),
            }
        }
        // 커서 사각형 안을 뺀다
        if yc >= m.top && yc <= m.bottom && m.right >= m.left {
            let (ma, mb) = (pixel_start(m.left), pixel_end(m.right));
            let mut cut = Vec::with_capacity(3);
            for (a, b) in merged {
                if mb < a || ma > b {
                    cut.push((a, b));
                    continue;
                }
                if a < ma {
                    cut.push((a, ma - 1));
                }
                if b > mb {
                    cut.push((mb + 1, b));
                }
            }
            merged = cut;
        }
        let mut next_open = Vec::with_capacity(merged.len());
        for (a, b) in merged {
            let (a, b) = (a.max(0), b.min(width - 1));
            if a > b {
                continue;
            }
            let found = open
                .iter()
                .copied()
                .find(|&i| out[i].x == a && out[i].x + out[i].w as i32 - 1 == b && out[i].y + out[i].h as i32 == y);
            match found {
                Some(i) => {
                    out[i].h += 1;
                    next_open.push(i);
                }
                None => {
                    out.push(PxRect {
                        x: a,
                        y,
                        w: (b - a + 1) as u32,
                        h: 1,
                    });
                    next_open.push(out.len() - 1);
                }
            }
        }
        open = next_open;
    }
    out
}

/// 픽셀 `i`의 중심(`i + 0.5`)이 `[a, b]`에 드는 가장 작은 `i`와 가장 큰 `i`
fn pixel_start(a: f32) -> i32 {
    (a - 0.5).ceil() as i32
}
fn pixel_end(b: f32) -> i32 {
    (b - 0.5).floor() as i32
}

/// 삼각형이 가로선 `y = yc`와 만나는 구간 `[왼, 오른]`
fn scan_triangle(q: &TrailQuad, t: [usize; 3], yc: f32) -> Option<(f32, f32)> {
    let mut lo = f32::MAX;
    let mut hi = f32::MIN;
    for k in 0..3 {
        let (i, j) = (t[k], t[(k + 1) % 3]);
        let (xa, ya, xb, yb) = (q.x[i], q.y[i], q.x[j], q.y[j]);
        if ya == yb {
            if ya == yc {
                lo = lo.min(xa.min(xb));
                hi = hi.max(xa.max(xb));
            }
        } else if (ya <= yc && yc <= yb) || (yb <= yc && yc <= ya) {
            let x = xa + (yc - ya) * (xb - xa) / (yb - ya);
            lo = lo.min(x);
            hi = hi.max(x);
        }
    }
    (lo <= hi).then_some((lo, hi))
}
