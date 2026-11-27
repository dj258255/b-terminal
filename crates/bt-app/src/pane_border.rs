//! 칸 테두리(kitty `window_border_width` 따위, #915). 칸 경계선과 고른 칸 색, 전체 테두리, 둥근 모서리를 한 곳에서 계획하고 그린다.
//!
//! 원본: `kitty/borders.py`(`Borders.__call__`·`add_borders`), `kitty/layout/splits.py`(`layout_pair`·`minimal_borders`),
//! `kitty/window.py`(`effective_border`), `kitty/shaders/rounded_rect.slang`.
//!
//! - 설정이 하나도 없으면(`PaneBorderConfig::is_default`) 계획은 칸 경계선 사각형을 `split-divider-color`로 칠하는 것 그대로다.
//! - 최소 테두리(`draw-minimal-borders = yes`): 이웃 칸과 맞닿은 쪽만 그린다. 칸 사이 틈은 `2 × 두께`이고 앞쪽 절반은 앞 칸의 색,
//!   뒤쪽 절반은 뒤 칸의 색이다(kitty `layout_pair`는 칸마다 자기 쪽 두께를 그리고 `minimal_borders`가 칸마다 색을 정한다).
//! - 전체 테두리(`no`): 칸마다 바깥 사각형 안쪽 둘레에 두께만큼 두른다. 칸 사이 틈은 없고(이웃 칸 테두리가 맞붙는다) 글자 자리는
//!   두께만큼 줄어든다. 칸이 하나여도 `draw-window-borders-for-single-window = yes`면 같다
//! - 둥근 모서리는 전체 테두리에서만이다(kitty `window_border_radius`: "applies only when full window borders are drawn")
//! - 칸 테두리 색을 정하지 않은 쪽은 `split-divider-color`(없으면 지금 기본 경계선 색)를 쓴다. kitty의 기본 색은 쓰지 않는다

use bt_core::Rgb;
use bt_core::paneborder::{ActiveColor, PaneBorderConfig};
use bt_render::canvas::Canvas;

use crate::split::Rect;

/// 지금까지의 칸 경계선 두께(픽셀)
pub fn legacy_px(scale: f64) -> u32 {
    (scale.round() as u32).max(1)
}

/// 테두리 한 줄의 두께(픽셀). 두께를 정하지 않았으면 지금 경계선 두께
pub fn width_px(cfg: &PaneBorderConfig, scale: f64) -> u32 {
    cfg.width.map_or_else(|| legacy_px(scale), |w| w.to_px(scale, true))
}

/// 칸과 칸 사이 틈(픽셀): 칸 배치가 경계선 자리로 비워 두는 두께.
/// 최소 테두리는 양쪽 두께를 더한 `2 × 두께`(두께를 정하지 않았으면 지금 경계선 두께 그대로), 전체 테두리는 0(이웃 칸 테두리가 맞붙는다)
pub fn gap_px(cfg: &PaneBorderConfig, scale: f64) -> u32 {
    match (cfg.minimal, cfg.width) {
        (false, _) => 0,
        (true, None) => legacy_px(scale),
        (true, Some(w)) => 2 * w.to_px(scale, true),
    }
}

/// 전체 테두리가 이 칸에 그려지는가. `single`은 탭에 보이는 칸이 하나뿐이라는 뜻(크게 보기 포함)
fn draws_full(cfg: &PaneBorderConfig, single: bool) -> bool {
    if single { cfg.single } else { !cfg.minimal }
}

/// 칸의 바깥 사각형에서 글자 자리를 위해 네 변에서 뺄 픽셀. 전체 테두리가 아니면 0.
/// 둥근 모서리(`radius > 0`)는 모서리 안쪽 곡선이 글자 칸을 덮지 않도록 두께보다 더 뺀다(kitty는 덮는다: 패딩으로 비키라고 안내한다)
pub fn text_inset(cfg: &PaneBorderConfig, scale: f64, single: bool) -> u32 {
    if !draws_full(cfg, single) {
        return 0;
    }
    let bw = width_px(cfg, scale);
    let r = cfg.radius.to_px(scale, false);
    if bw == 0 || r == 0 {
        return bw;
    }
    // 안쪽 곡선(반지름 r - bw, 가장자리 번짐 1픽셀)이 글자 자리 모서리 픽셀 중심을 덮지 않을 만큼
    let (r, b) = (r as f64, bw as f64);
    let t = (r - 0.5 - (r - b - 1.0).max(0.0) / std::f64::consts::SQRT_2)
        .ceil()
        .max(0.0) as u32;
    t.max(bw)
}

/// 사각형의 네 변을 `t`픽셀씩 줄인다(남는 것이 없으면 크기 0)
pub fn inset_rect(r: Rect, t: u32) -> Rect {
    let w = r.w.saturating_sub(2 * t);
    let h = r.h.saturating_sub(2 * t);
    Rect::new(r.x + t.min(r.w / 2) as i32, r.y + t.min(r.h / 2) as i32, w, h)
}

/// 칸 하나의 상태
#[derive(Debug, Clone, Copy)]
pub struct PaneState {
    pub outer: Rect,
    pub focused: bool,
    /// 보지 않은 동안 벨이 울렸다(kitty `needs_attention`)
    pub attention: bool,
}

/// 계획을 세울 때 필요한 바깥 값
#[derive(Debug, Clone, Copy)]
pub struct Env {
    /// 색을 정하지 않은 쪽이 쓰는 색(`split-divider-color`, 없으면 바탕색과 글자색 사이)
    pub fallback: Rgb,
    /// 둥근 테두리 바깥 모서리를 메우는 색
    pub background: Rgb,
    pub scale: f64,
    /// 창이 앞에 있는가(전체 테두리에서 고른 칸 색은 창이 앞일 때만, kitty `os_window_focused`)
    pub window_focused: bool,
}

/// 둥근 테두리 하나
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Ring {
    pub rect: Rect,
    pub thickness: u32,
    pub radius: u32,
    pub color: Rgb,
    pub outside: Rgb,
}

/// 그릴 것: 칠할 사각형들과 둥근 테두리들
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Plan {
    pub fills: Vec<(Rect, Rgb)>,
    pub rings: Vec<Ring>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Role {
    Active,
    Bell,
    Inactive,
}

fn color_of(cfg: &PaneBorderConfig, env: &Env, role: Role) -> Rgb {
    let inactive = cfg.inactive.unwrap_or(env.fallback);
    match role {
        Role::Inactive => inactive,
        Role::Bell => cfg.bell.unwrap_or(inactive),
        Role::Active => match cfg.active {
            ActiveColor::Color(c) => c,
            _ => env.fallback,
        },
    }
}

/// 최소 테두리의 칸 역할(kitty `minimal_borders`: 고른 칸이거나 주의가 필요한 칸이 색을 받고, 벨이 이긴다)
fn role_minimal(cfg: &PaneBorderConfig, p: &PaneState) -> Role {
    if p.attention {
        Role::Bell
    } else if p.focused && cfg.active != ActiveColor::Off {
        Role::Active
    } else {
        Role::Inactive
    }
}

/// 전체 테두리의 칸 역할(kitty `Borders.__call__`: 고른 칸이고 창이 앞이면 고른 색, 아니면 벨 또는 안 고른 색)
fn role_full(cfg: &PaneBorderConfig, env: &Env, p: &PaneState) -> Role {
    if p.focused && cfg.active != ActiveColor::Off && env.window_focused {
        Role::Active
    } else if p.attention {
        Role::Bell
    } else {
        Role::Inactive
    }
}

/// 칸들과 경계선 사각형으로 그릴 것을 정한다. `dividers`는 칸 배치가 낸 경계선 자리(`Node::dividers`)
pub fn plan(cfg: &PaneBorderConfig, env: &Env, panes: &[PaneState], dividers: &[Rect]) -> Plan {
    if cfg.is_default() {
        return Plan {
            fills: dividers.iter().map(|d| (*d, env.fallback)).collect(),
            rings: Vec::new(),
        };
    }
    let single = panes.len() < 2;
    if draws_full(cfg, single) {
        return full_plan(cfg, env, panes);
    }
    if single {
        return Plan::default();
    }
    minimal_plan(cfg, env, panes, dividers)
}

/// 한 쪽 가장자리에 맞닿은 칸들이 경계선의 어느 구간을 차지하는가: (시작, 끝, 역할)
fn side_segments(
    cfg: &PaneBorderConfig,
    panes: &[PaneState],
    vertical: bool,
    edge: i32,
    span: (i32, i32),
    after: bool,
) -> Vec<(i32, i32, Role)> {
    let mut v: Vec<(i32, i32, Role)> = panes
        .iter()
        .filter(|p| {
            let o = p.outer;
            match (vertical, after) {
                (true, false) => o.right() == edge,
                (true, true) => o.x == edge,
                (false, false) => o.bottom() == edge,
                (false, true) => o.y == edge,
            }
        })
        .filter_map(|p| {
            let o = p.outer;
            let (lo, hi) = if vertical { (o.y, o.bottom()) } else { (o.x, o.right()) };
            let (lo, hi) = (lo.max(span.0), hi.min(span.1));
            (lo < hi).then(|| (lo, hi, role_minimal(cfg, p)))
        })
        .collect();
    v.sort_by_key(|s| s.0);
    v
}

fn minimal_plan(cfg: &PaneBorderConfig, env: &Env, panes: &[PaneState], dividers: &[Rect]) -> Plan {
    let mut fills = Vec::new();
    for d in dividers.iter().filter(|d| d.w > 0 && d.h > 0) {
        // 칸이 좌우로 나란하면 경계선은 세로선(폭 = 틈)
        let vertical = d.w <= d.h;
        let (gap, start, span) = if vertical {
            (d.w, d.x, (d.y, d.bottom()))
        } else {
            (d.h, d.y, (d.x, d.right()))
        };
        let before = side_segments(cfg, panes, vertical, start, span, false);
        let after = side_segments(cfg, panes, vertical, start + gap as i32, span, true);
        // 어느 칸에도 닿지 않는 틈이 비지 않게 먼저 기본 색을 깐다
        fills.push((*d, env.fallback));
        let rect = |from: i32, to: i32, off: i32, thick: u32| {
            if vertical {
                Rect::new(start + off, from, thick, (to - from) as u32)
            } else {
                Rect::new(from, start + off, (to - from) as u32, thick)
            }
        };
        if gap >= 2 {
            let first = gap / 2;
            for (lo, hi, role) in &before {
                fills.push((rect(*lo, *hi, 0, first), color_of(cfg, env, *role)));
            }
            for (lo, hi, role) in &after {
                fills.push((rect(*lo, *hi, first as i32, gap - first), color_of(cfg, env, *role)));
            }
        } else {
            // 틈이 한 픽셀이라 나눌 수 없다: 양쪽 가운데 더 눈에 띄는 역할의 색을 쓴다(고른 칸 > 벨 > 안 고른 칸)
            let mut cuts: Vec<i32> = before.iter().chain(&after).flat_map(|s| [s.0, s.1]).collect();
            cuts.sort_unstable();
            cuts.dedup();
            let at = |segs: &[(i32, i32, Role)], p: i32| segs.iter().find(|s| s.0 <= p && p < s.1).map(|s| s.2);
            let rank = |r: Role| match r {
                Role::Active => 2,
                Role::Bell => 1,
                Role::Inactive => 0,
            };
            for w in cuts.windows(2) {
                let role = [at(&before, w[0]), at(&after, w[0])]
                    .into_iter()
                    .flatten()
                    .max_by_key(|r| rank(*r));
                if let Some(role) = role {
                    fills.push((rect(w[0], w[1], 0, gap), color_of(cfg, env, role)));
                }
            }
        }
    }
    Plan {
        fills,
        rings: Vec::new(),
    }
}

fn full_plan(cfg: &PaneBorderConfig, env: &Env, panes: &[PaneState]) -> Plan {
    let mut plan = Plan::default();
    let bw = width_px(cfg, env.scale);
    let radius = cfg.radius.to_px(env.scale, false);
    for p in panes {
        let o = p.outer;
        // 칸이 얇아도 가운데가 남게 절반을 넘기지 않는다
        let t = bw.min(o.w / 2).min(o.h / 2);
        if t == 0 {
            continue;
        }
        let color = color_of(cfg, env, role_full(cfg, env, p));
        if radius == 0 {
            let side_h = o.h - 2 * t;
            for r in [
                Rect::new(o.x, o.y, o.w, t),
                Rect::new(o.x, o.y + (o.h - t) as i32, o.w, t),
                Rect::new(o.x, o.y + t as i32, t, side_h),
                Rect::new(o.x + (o.w - t) as i32, o.y + t as i32, t, side_h),
            ] {
                plan.fills.push((r, color));
            }
        } else {
            plan.rings.push(Ring {
                rect: o,
                thickness: t,
                radius: radius.min(o.w / 2).min(o.h / 2),
                color,
                outside: env.background,
            });
        }
    }
    plan
}

impl Plan {
    pub fn is_empty(&self) -> bool {
        self.fills.is_empty() && self.rings.is_empty()
    }

    pub fn has_rings(&self) -> bool {
        !self.rings.is_empty()
    }

    /// 캔버스에 그린다. CPU 길과 GPU 길(위층 캔버스)이 이 함수 하나를 부른다
    pub fn draw(&self, canvas: &mut Canvas) {
        for (r, c) in &self.fills {
            canvas.fill_rect(r.x, r.y, r.w, r.h, *c);
        }
        for ring in &self.rings {
            draw_ring(canvas, ring);
        }
    }
}

fn smoothstep(lo: f64, hi: f64, x: f64) -> f64 {
    let t = ((x - lo) / (hi - lo)).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

/// kitty `rounded_rect.slang`와 같은 식: 둥근 사각형까지의 부호 있는 거리로 바깥 가장자리와 안쪽 가장자리의 덮음을 구한다.
/// 바깥 모서리(둥근 선 밖)는 `outside` 색으로 메운다
fn draw_ring(canvas: &mut Canvas, ring: &Ring) {
    let Ring {
        rect,
        thickness,
        radius,
        color,
        outside,
    } = *ring;
    let (w, h) = (rect.w as f64, rect.h as f64);
    let r = radius as f64;
    let band = (thickness + 1).max(radius);
    let (cx, cy) = (rect.x as f64 + w / 2.0, rect.y as f64 + h / 2.0);
    let mut px = |x: i32, y: i32| {
        let (dx, dy) = (
            (x as f64 + 0.5 - cx).abs() - (w / 2.0 - r),
            (y as f64 + 0.5 - cy).abs() - (h / 2.0 - r),
        );
        let dist = dx.max(0.0).hypot(dy.max(0.0)) + dx.max(dy).min(0.0) - r;
        let outer_edge = -dist;
        let inner_edge = outer_edge - thickness as f64;
        let outer_cov = smoothstep(-1.0, 1.0, outer_edge);
        let alpha = outer_cov - smoothstep(-1.0, 1.0, inner_edge);
        let out_a = ((1.0 - outer_cov) * 255.0).round() as u8;
        if out_a > 0 {
            canvas.blend_px(x, y, outside, out_a);
        }
        let a = (alpha * 255.0).round() as u8;
        if a > 0 {
            canvas.blend_px(x, y, color, a);
        }
    };
    for row in 0..rect.h {
        let y = rect.y + row as i32;
        let edge_row = row < band || row + band >= rect.h;
        for col in 0..rect.w {
            if edge_row || col < band || col + band >= rect.w {
                px(rect.x + col as i32, y);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use bt_core::paneborder::Len;

    const FALLBACK: Rgb = Rgb { r: 60, g: 60, b: 60 };
    const BG: Rgb = Rgb { r: 10, g: 10, b: 10 };
    const GREEN: Rgb = Rgb { r: 0, g: 255, b: 0 };
    const GREY: Rgb = Rgb { r: 204, g: 204, b: 204 };
    const ORANGE: Rgb = Rgb { r: 255, g: 90, b: 0 };

    fn env() -> Env {
        Env {
            fallback: FALLBACK,
            background: BG,
            scale: 1.0,
            window_focused: true,
        }
    }

    fn pane(x: i32, y: i32, w: u32, h: u32, focused: bool) -> PaneState {
        PaneState {
            outer: Rect::new(x, y, w, h),
            focused,
            attention: false,
        }
    }

    /// 100x50 창을 가운데 틈 `g`로 나눈 두 칸과 경계선
    fn two(g: u32, left_focused: bool) -> (Vec<PaneState>, Vec<Rect>) {
        let w = (100 - g) / 2;
        (
            vec![
                pane(0, 0, w, 50, left_focused),
                pane((w + g) as i32, 0, w, 50, !left_focused),
            ],
            vec![Rect::new(w as i32, 0, g, 50)],
        )
    }

    fn paint(plan: &Plan, w: u32, h: u32) -> Vec<u32> {
        let mut buf = vec![0xff00_0000 | 0x0000_0001; (w * h) as usize];
        let mut c = Canvas::new(&mut buf, w, h);
        plan.draw(&mut c);
        buf
    }

    fn px(buf: &[u32], w: u32, x: u32, y: u32) -> Rgb {
        Rgb::from_u32(buf[(y * w + x) as usize])
    }

    fn cfg(f: impl FnOnce(&mut PaneBorderConfig)) -> PaneBorderConfig {
        let mut c = PaneBorderConfig::default();
        f(&mut c);
        c
    }

    #[test]
    fn without_settings_the_divider_pixels_are_what_they_were() {
        for g in [1, 2, 3] {
            let (panes, dividers) = two(g, true);
            let p = plan(&PaneBorderConfig::default(), &env(), &panes, &dividers);
            // 지금 코드: 경계선 사각형을 `split-divider-color`로 칠한다
            let mut want = vec![0xff00_0000 | 0x0000_0001; 100 * 50];
            let mut c = Canvas::new(&mut want, 100, 50);
            for d in &dividers {
                c.fill_rect(d.x, d.y, d.w, d.h, FALLBACK);
            }
            assert_eq!(paint(&p, 100, 50), want, "틈 {g}");
            assert!(p.rings.is_empty());
        }
        // 칸이 하나면 그릴 것이 없다
        assert!(plan(&PaneBorderConfig::default(), &env(), &[pane(0, 0, 100, 50, true)], &[]).is_empty());
    }

    #[test]
    fn gap_and_inset_follow_kitty_width_rules() {
        let d = PaneBorderConfig::default();
        assert_eq!(gap_px(&d, 1.0), 1);
        assert_eq!(gap_px(&d, 2.0), 2);
        assert_eq!(text_inset(&d, 2.0, false), 0);
        assert_eq!(text_inset(&d, 2.0, true), 0);
        // 두께를 정하면 최소 테두리의 틈은 양쪽 두께 합
        let w = cfg(|c| c.width = Some(Len { value: 3.0, px: true }));
        assert_eq!(gap_px(&w, 2.0), 6);
        assert_eq!(text_inset(&w, 2.0, false), 0);
        // kitty 기본 0.5pt는 배율 2에서 1픽셀, 틈 2
        let k = cfg(|c| c.width = Some(Len { value: 0.5, px: false }));
        assert_eq!((width_px(&k, 2.0), gap_px(&k, 2.0)), (1, 2));
        // 두께 0: 선도 틈도 없다
        let z = cfg(|c| c.width = Some(Len { value: 0.0, px: true }));
        assert_eq!(gap_px(&z, 2.0), 0);
        // 전체 테두리: 틈 0, 글자 자리는 두께만큼, 칸이 하나면 단일 칸 설정이 켜져야
        let full = cfg(|c| {
            c.minimal = false;
            c.width = Some(Len { value: 3.0, px: true });
        });
        assert_eq!(gap_px(&full, 2.0), 0);
        assert_eq!(text_inset(&full, 2.0, false), 3);
        assert_eq!(text_inset(&full, 2.0, true), 0);
        let single = cfg(|c| {
            c.single = true;
            c.width = Some(Len { value: 3.0, px: true });
        });
        assert_eq!(text_inset(&single, 2.0, true), 3);
        assert_eq!(text_inset(&single, 2.0, false), 0);
        // 둥근 모서리는 두께보다 더 비킨다
        let round = cfg(|c| {
            c.minimal = false;
            c.width = Some(Len { value: 1.0, px: true });
            c.radius = Len { value: 6.0, px: true };
        });
        assert_eq!(text_inset(&round, 1.0, false), 3);
        // 최소 테두리에서는 반지름이 아무 일도 하지 않는다
        let minimal_round = cfg(|c| c.radius = Len { value: 6.0, px: true });
        assert_eq!(text_inset(&minimal_round, 1.0, false), 0);
        assert_eq!(inset_rect(Rect::new(10, 20, 100, 50), 3), Rect::new(13, 23, 94, 44));
        assert_eq!(inset_rect(Rect::new(0, 0, 4, 4), 3), Rect::new(2, 2, 0, 0));
    }

    #[test]
    fn minimal_border_paints_each_half_with_its_own_panes_color() {
        let c = cfg(|c| {
            c.width = Some(Len { value: 2.0, px: true });
            c.active = ActiveColor::Color(GREEN);
            c.inactive = Some(GREY);
        });
        let g = gap_px(&c, 1.0);
        assert_eq!(g, 4);
        // 왼쪽 칸이 고른 칸: 틈의 왼쪽 절반은 고른 색, 오른쪽 절반은 안 고른 색
        let (panes, dividers) = two(g, true);
        let buf = paint(&plan(&c, &env(), &panes, &dividers), 100, 50);
        let x0 = dividers[0].x as u32;
        assert_eq!(px(&buf, 100, x0, 10), GREEN);
        assert_eq!(px(&buf, 100, x0 + 1, 10), GREEN);
        assert_eq!(px(&buf, 100, x0 + 2, 10), GREY);
        assert_eq!(px(&buf, 100, x0 + 3, 10), GREY);
        // 칸 안쪽은 건드리지 않는다
        assert_eq!(px(&buf, 100, x0 - 1, 10), Rgb::from_u32(1));
        assert_eq!(px(&buf, 100, x0 + 4, 10), Rgb::from_u32(1));
        // 오른쪽 칸을 고르면 거꾸로
        let (panes, dividers) = two(g, false);
        let buf = paint(&plan(&c, &env(), &panes, &dividers), 100, 50);
        assert_eq!(px(&buf, 100, x0, 10), GREY);
        assert_eq!(px(&buf, 100, x0 + 3, 10), GREEN);
    }

    #[test]
    fn unset_roles_use_the_split_divider_color_not_kitty_defaults() {
        // 고른 색만 정했다: 안 고른 쪽은 `split-divider-color` 그대로
        let c = cfg(|c| c.active = ActiveColor::Color(GREEN));
        let (panes, dividers) = two(2, true);
        let buf = paint(&plan(&c, &env(), &panes, &dividers), 100, 50);
        let x0 = dividers[0].x as u32;
        assert_eq!(px(&buf, 100, x0, 10), GREEN);
        assert_eq!(px(&buf, 100, x0 + 1, 10), FALLBACK);
        // 안 고른 색만 정했다: 고른 쪽은 그대로
        let c = cfg(|c| c.inactive = Some(GREY));
        let buf = paint(&plan(&c, &env(), &panes, &dividers), 100, 50);
        assert_eq!(px(&buf, 100, x0, 10), FALLBACK);
        assert_eq!(px(&buf, 100, x0 + 1, 10), GREY);
    }

    #[test]
    fn one_pixel_gap_takes_the_more_prominent_side() {
        let c = cfg(|c| c.active = ActiveColor::Color(GREEN));
        let (panes, dividers) = two(1, false);
        let buf = paint(&plan(&c, &env(), &panes, &dividers), 100, 50);
        assert_eq!(px(&buf, 100, dividers[0].x as u32, 10), GREEN);
        let (panes, dividers) = two(1, true);
        let buf = paint(&plan(&c, &env(), &panes, &dividers), 100, 50);
        assert_eq!(px(&buf, 100, dividers[0].x as u32, 10), GREEN);
    }

    #[test]
    fn bell_color_marks_the_pane_that_needs_attention_and_active_none_hides_the_active_color() {
        let c = cfg(|c| {
            c.width = Some(Len { value: 1.0, px: true });
            c.active = ActiveColor::Color(GREEN);
            c.inactive = Some(GREY);
            c.bell = Some(ORANGE);
        });
        let (mut panes, dividers) = two(2, true);
        panes[1].attention = true;
        let buf = paint(&plan(&c, &env(), &panes, &dividers), 100, 50);
        let x0 = dividers[0].x as u32;
        assert_eq!(px(&buf, 100, x0, 5), GREEN);
        assert_eq!(px(&buf, 100, x0 + 1, 5), ORANGE);
        // 벨 색을 안 정했으면 안 고른 색
        let c2 = cfg(|c| {
            c.width = Some(Len { value: 1.0, px: true });
            c.inactive = Some(GREY);
        });
        let buf = paint(&plan(&c2, &env(), &panes, &dividers), 100, 50);
        assert_eq!(px(&buf, 100, x0 + 1, 5), GREY);
        // active-border-color = none: 고른 칸도 안 고른 색(kitty `draw_active_borders`가 거짓)
        let c3 = cfg(|c| {
            c.width = Some(Len { value: 1.0, px: true });
            c.active = ActiveColor::Off;
            c.inactive = Some(GREY);
        });
        panes[1].attention = false;
        let buf = paint(&plan(&c3, &env(), &panes, &dividers), 100, 50);
        assert_eq!((px(&buf, 100, x0, 5), px(&buf, 100, x0 + 1, 5)), (GREY, GREY));
    }

    #[test]
    fn nested_split_cuts_the_neighbors_halves_where_the_pane_edges_are() {
        // [1 | (2 / 3)]: 왼쪽 칸 하나, 오른쪽에 위아래 두 칸. 오른쪽 칸 둘 가운데 위 칸을 골랐다
        let c = cfg(|c| {
            c.width = Some(Len { value: 1.0, px: true });
            c.active = ActiveColor::Color(GREEN);
            c.inactive = Some(GREY);
        });
        let panes = [
            pane(0, 0, 49, 51, false),
            pane(51, 0, 49, 24, true),
            pane(51, 27, 49, 24, false),
        ];
        let dividers = [Rect::new(49, 0, 2, 51), Rect::new(51, 24, 49, 3)];
        let buf = paint(&plan(&c, &env(), &panes, &dividers), 100, 51);
        // 세로 틈: 왼쪽 절반은 칸 1(안 고름), 오른쪽 절반은 위 칸 구간만 고른 색, 아래 칸 구간은 안 고른 색
        assert_eq!(px(&buf, 100, 49, 10), GREY);
        assert_eq!(px(&buf, 100, 50, 10), GREEN);
        assert_eq!(px(&buf, 100, 50, 40), GREY);
        // 가로 틈(3픽셀): 위쪽 1픽셀은 위 칸 색, 나머지 2픽셀은 아래 칸 색
        assert_eq!(px(&buf, 100, 70, 24), GREEN);
        assert_eq!(px(&buf, 100, 70, 25), GREY);
        assert_eq!(px(&buf, 100, 70, 26), GREY);
    }

    #[test]
    fn full_border_frames_every_pane_and_leaves_the_text_area_alone() {
        let c = cfg(|c| {
            c.minimal = false;
            c.width = Some(Len { value: 2.0, px: true });
            c.active = ActiveColor::Color(GREEN);
            c.inactive = Some(GREY);
        });
        assert_eq!(gap_px(&c, 1.0), 0);
        let panes = [pane(0, 0, 50, 50, true), pane(50, 0, 50, 50, false)];
        let p = plan(&c, &env(), &panes, &[Rect::new(50, 0, 0, 50)]);
        let buf = paint(&p, 100, 50);
        // 고른 칸은 네 변이 모두 고른 색
        for (x, y) in [
            (0, 20),
            (1, 20),
            (49, 20),
            (20, 0),
            (20, 1),
            (20, 48),
            (20, 49),
            (48, 20),
        ] {
            assert_eq!(px(&buf, 100, x, y), GREEN, "({x},{y})");
        }
        // 이웃 칸은 안 고른 색, 두 칸 테두리가 맞붙는다
        assert_eq!(px(&buf, 100, 50, 20), GREY);
        assert_eq!(px(&buf, 100, 99, 20), GREY);
        // 글자 자리(두께만큼 안쪽)는 칠하지 않는다
        let inner = inset_rect(panes[0].outer, text_inset(&c, 1.0, false));
        for y in inner.y..inner.bottom() {
            for x in inner.x..inner.right() {
                assert_eq!(px(&buf, 100, x as u32, y as u32), Rgb::from_u32(1), "({x},{y})");
            }
        }
        // 창이 뒤에 있으면 고른 칸도 안 고른 색(kitty `os_window_focused`)
        let mut e = env();
        e.window_focused = false;
        let buf = paint(&plan(&c, &e, &panes, &[]), 100, 50);
        assert_eq!(px(&buf, 100, 0, 20), GREY);
    }

    #[test]
    fn rounded_full_border_rounds_the_corners_and_keeps_clear_of_the_text_area() {
        let c = cfg(|c| {
            c.minimal = false;
            c.width = Some(Len { value: 1.0, px: true });
            c.radius = Len { value: 6.0, px: true };
            c.active = ActiveColor::Color(GREEN);
            c.inactive = Some(GREY);
        });
        let panes = [pane(0, 0, 40, 30, true), pane(40, 0, 40, 30, false)];
        let p = plan(&c, &env(), &panes, &[]);
        assert_eq!(p.rings.len(), 2);
        assert!(p.fills.is_empty());
        let buf = paint(&p, 80, 30);
        // 맨 모서리 픽셀은 바깥이라 바탕색으로 메워진다
        assert_eq!(px(&buf, 80, 0, 0), BG);
        // 위 변 한가운데는 고른 색에 가깝다(가장자리 번짐으로 완전한 초록은 아니다: kitty 셰이더도 1픽셀 두께에서 알파 0.69 안팎이다)
        let top = px(&buf, 80, 20, 0);
        assert!(top.g > 150 && top.r < 80, "{top:?}");
        // 둥근 선 위(모서리 호의 가운데 근처)에도 테두리 색이 얹힌다
        let arc = px(&buf, 80, 2, 2);
        assert!(arc.g > 100 && arc.r < 80, "{arc:?}");
        // 글자 자리는 한 픽셀도 건드리지 않는다
        for (i, pane) in panes.iter().enumerate() {
            let inner = inset_rect(pane.outer, text_inset(&c, 1.0, false));
            assert!(inner.w > 0 && inner.h > 0);
            for y in inner.y..inner.bottom() {
                for x in inner.x..inner.right() {
                    assert_eq!(px(&buf, 80, x as u32, y as u32), Rgb::from_u32(1), "칸 {i} ({x},{y})");
                }
            }
        }
        // 칸 오른쪽 위 모서리도 같은 둥글기
        assert_eq!(px(&buf, 80, 39, 0), BG);
        assert_eq!(px(&buf, 80, 40, 0), BG);
    }

    #[test]
    fn minimal_borders_ignore_the_radius() {
        let c = cfg(|c| {
            c.width = Some(Len { value: 1.0, px: true });
            c.radius = Len { value: 8.0, px: true };
        });
        let (panes, dividers) = two(2, true);
        assert!(plan(&c, &env(), &panes, &dividers).rings.is_empty());
    }

    #[test]
    fn single_window_border_needs_the_option() {
        let one = [pane(0, 0, 60, 40, true)];
        let off = cfg(|c| c.width = Some(Len { value: 2.0, px: true }));
        assert!(plan(&off, &env(), &one, &[]).is_empty());
        let on = cfg(|c| {
            c.single = true;
            c.width = Some(Len { value: 2.0, px: true });
            c.active = ActiveColor::Color(GREEN);
            c.inactive = Some(GREY);
        });
        let buf = paint(&plan(&on, &env(), &one, &[]), 60, 40);
        assert_eq!(px(&buf, 60, 0, 10), GREEN);
        assert_eq!(px(&buf, 60, 59, 10), GREEN);
        assert_eq!(px(&buf, 60, 30, 1), GREEN);
        assert_eq!(px(&buf, 60, 30, 2), Rgb::from_u32(1));
        // 창이 뒤로 가면 안 고른 색(kitty 문서)
        let mut e = env();
        e.window_focused = false;
        let buf = paint(&plan(&on, &e, &one, &[]), 60, 40);
        assert_eq!(px(&buf, 60, 0, 10), GREY);
        // 칸이 둘이고 최소 테두리면 단일 칸 설정은 아무것도 바꾸지 않는다
        let (panes, dividers) = two(4, true);
        let p = plan(&on, &env(), &panes, &dividers);
        assert!(p.rings.is_empty() && !p.fills.is_empty());
    }

    #[test]
    fn zero_width_draws_nothing() {
        let c = cfg(|c| c.width = Some(Len { value: 0.0, px: true }));
        assert_eq!(gap_px(&c, 2.0), 0);
        let panes = [pane(0, 0, 50, 50, true), pane(50, 0, 50, 50, false)];
        assert!(plan(&c, &env(), &panes, &[Rect::new(50, 0, 0, 50)]).is_empty());
        let full = cfg(|c| {
            c.minimal = false;
            c.width = Some(Len { value: 0.0, px: true });
        });
        assert!(plan(&full, &env(), &panes, &[]).is_empty());
    }

    /// CPU 길은 창 버퍼에 곧장, GPU 길은 투명한 위층(곱한 알파)에 그려 위에 얹는다. 같은 `Plan::draw`가 그리므로
    /// 위층에 그려 얹은 것과 곧장 그린 것이 같은 픽셀이어야 한다(반올림 한 단계 안쪽)
    #[test]
    fn upper_layer_composite_matches_drawing_straight_on_the_window() {
        let base: u32 = 0xff00_0000 | 0x20_40_60;
        for c in [
            cfg(|c| {
                c.width = Some(Len { value: 2.0, px: true });
                c.active = ActiveColor::Color(GREEN);
            }),
            cfg(|c| {
                c.minimal = false;
                c.width = Some(Len { value: 2.0, px: true });
                c.radius = Len { value: 7.0, px: true };
                c.active = ActiveColor::Color(GREEN);
                c.inactive = Some(GREY);
            }),
        ] {
            let g = gap_px(&c, 1.0);
            let (panes, dividers) = two(g, true);
            let p = plan(&c, &env(), &panes, &dividers);
            let mut direct = vec![base; 100 * 50];
            p.draw(&mut Canvas::new(&mut direct, 100, 50));
            let mut layer = vec![0u32; 100 * 50];
            p.draw(&mut Canvas::new(&mut layer, 100, 50));
            for (i, (d, l)) in direct.iter().zip(&layer).enumerate() {
                let a = l >> 24;
                for shift in [16, 8, 0] {
                    let (lc, bc) = ((l >> shift) & 0xff, (base >> shift) & 0xff);
                    let over = lc + (bc * (255 - a) + 127) / 255;
                    let want = (d >> shift) & 0xff;
                    assert!(
                        over.abs_diff(want) <= 2,
                        "픽셀 {i} 채널 {shift}: 얹음 {over} 곧장 {want}"
                    );
                }
            }
        }
    }
}
