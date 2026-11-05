//! 세로 사이드바: 탭마다 상태 점, 이름, 한 줄 설명, 안 읽음 표시

use bt_core::{AgentState, Rgb, Theme, WaitReason};
use bt_render::{Canvas, Fonts, draw_text, truncate};

/// 사이드바 한 줄에 보일 것
#[derive(Debug, Clone)]
pub struct Item {
    pub title: String,
    pub detail: String,
    pub state: AgentState,
    pub unread: bool,
    pub active: bool,
}

/// 아래쪽 사용량 한 줄
#[derive(Debug, Clone, PartialEq)]
pub struct UsageRow {
    pub label: String,
    pub percent: f64,
    /// 다시 차기까지(짧은 말: "2시간")
    pub resets_in: Option<String>,
}

/// 남은 초를 짧은 말로
pub fn short_duration(secs: i64) -> String {
    match secs {
        s if s < 60 => "1분".into(),
        s if s < 3600 => format!("{}분", s / 60),
        s if s < 86_400 => format!("{}시간", s / 3600),
        s => format!("{}일", s / 86_400),
    }
}

/// 사용량 막대 색: 60% 아래 초록, 85% 아래 노랑, 그 위 빨강
pub fn usage_color(percent: f64, t: &Theme) -> Rgb {
    if percent < 60.0 {
        t.palette[2]
    } else if percent < 85.0 {
        t.palette[3]
    } else {
        t.palette[1]
    }
}

/// 사이드바 폭(셀 수)
pub const WIDTH_CELLS: u32 = 28;
/// 한 항목이 차지하는 줄 수(이름, 설명, 틈)
const ITEM_LINES: i32 = 2;
const HEADER_LINES: i32 = 2;

pub struct Palette {
    pub bg: Rgb,
    pub active_bg: Rgb,
    pub text: Rgb,
    pub dim: Rgb,
    pub border: Rgb,
}

impl Palette {
    pub fn from_theme(t: &Theme) -> Self {
        Self {
            bg: t.background.mix(t.foreground, 0.04),
            active_bg: t.background.mix(t.foreground, 0.13),
            text: t.foreground,
            dim: t.foreground.mix(t.background, 0.45),
            border: t.background.mix(t.foreground, 0.12),
        }
    }
}

/// 상태 점의 색
pub fn state_color(state: AgentState, t: &Theme) -> Rgb {
    match state {
        AgentState::Idle => t.foreground.mix(t.background, 0.6),
        AgentState::Working => t.palette[4],
        AgentState::Waiting(WaitReason::Permission) | AgentState::Waiting(WaitReason::Question) => t.palette[3],
        AgentState::Waiting(WaitReason::Input) | AgentState::Done => t.palette[2],
        AgentState::Error => t.palette[1],
    }
}

/// 사이드바 폭(픽셀)
pub fn width_px(fonts: &Fonts, pad: i32) -> u32 {
    fonts.metrics().width * WIDTH_CELLS + pad as u32 * 2
}

/// 사이드바를 그린다. `height`는 창 높이
pub fn draw(canvas: &mut Canvas, fonts: &mut Fonts, theme: &Theme, items: &[Item], pad: i32, usage: &[UsageRow]) {
    let m = fonts.metrics();
    let w = width_px(fonts, pad);
    let p = Palette::from_theme(theme);
    canvas.fill_rect(0, 0, w, canvas.height, p.bg);
    canvas.fill_rect(w as i32 - 1, 0, 1, canvas.height, p.border);
    let lh = m.height as i32;
    let text_x = pad + m.width as i32 * 2;
    let max_cells = WIDTH_CELLS as usize - 3;

    draw_text(canvas, fonts, pad, pad, "b-terminal", p.dim, true);
    let count = format!("탭 {}", items.len());
    let cw = m.width as i32;
    let cx = w as i32 - pad - cw * bt_render::text_cells(&count) as i32;
    draw_text(canvas, fonts, cx, pad, &count, p.dim, false);

    for (i, item) in items.iter().enumerate() {
        let y = pad + (HEADER_LINES + i as i32 * (ITEM_LINES + 1)) * lh - lh / 2;
        if item.active {
            canvas.fill_rect(0, y - lh / 4, w - 1, (lh * ITEM_LINES + lh / 2) as u32, p.active_bg);
            canvas.fill_rect(
                0,
                y - lh / 4,
                m.stroke * 3,
                (lh * ITEM_LINES + lh / 2) as u32,
                state_color(item.state, theme).mix(p.text, 0.2),
            );
        }
        // 상태 점
        let dot = state_color(item.state, theme);
        let r = (m.width as i32 / 2).max(3);
        draw_dot(canvas, pad + cw / 2, y + lh / 2, r, dot);
        let title = truncate(&item.title, max_cells);
        draw_text(
            canvas,
            fonts,
            text_x,
            y,
            &title,
            if item.active { p.text } else { p.text.mix(p.bg, 0.15) },
            item.active,
        );
        if item.unread {
            draw_dot(
                canvas,
                w as i32 - pad - cw / 2,
                y + lh / 2,
                (r * 2 / 3).max(2),
                theme.palette[3],
            );
        }
        let detail = truncate(&item.detail, max_cells);
        let detail_color = match item.state {
            AgentState::Waiting(_) | AgentState::Error => dot,
            _ => p.dim,
        };
        draw_text(canvas, fonts, text_x, y + lh, &detail, detail_color, false);
    }

    // 아래쪽: 구독 한도 사용량(Claude 상태줄이 보낸 것)
    if !usage.is_empty() {
        let lines = usage.len() as i32 + 1;
        let top = canvas.height as i32 - pad - lh * lines;
        canvas.fill_rect(pad, top - lh / 2, w - pad as u32 * 2, 1, p.border);
        draw_text(canvas, fonts, pad, top, "Claude 사용량", p.dim, false);
        let label_cells = 6;
        let bar_cells = 11;
        for (i, row) in usage.iter().enumerate() {
            let y = top + lh * (i as i32 + 1);
            draw_text(
                canvas,
                fonts,
                pad,
                y,
                &truncate(&row.label, label_cells),
                p.text.mix(p.bg, 0.2),
                false,
            );
            let bx = pad + cw * label_cells as i32;
            let bw = (cw * bar_cells) as u32;
            let bh = (lh / 3).max(3) as u32;
            let by = y + (lh - bh as i32) / 2;
            canvas.fill_rect(bx, by, bw, bh, p.active_bg);
            let fill = ((bw as f64) * row.percent.clamp(0.0, 100.0) / 100.0).round() as u32;
            canvas.fill_rect(bx, by, fill, bh, usage_color(row.percent, theme));
            let pct = format!("{:>3.0}%", row.percent);
            let px = bx + bw as i32 + cw;
            let used = draw_text(canvas, fonts, px, y, &pct, p.text, false);
            if let Some(r) = &row.resets_in {
                draw_text(canvas, fonts, px + used + cw, y, &truncate(r, 5), p.dim, false);
            }
        }
    }
}

/// 창 좌표 y가 몇 번째 항목인가
pub fn hit(fonts: &Fonts, pad: i32, y: f64, count: usize) -> Option<usize> {
    let lh = fonts.metrics().height as f64;
    let top = pad as f64 + HEADER_LINES as f64 * lh - lh / 2.0 - lh / 4.0;
    if y < top {
        return None;
    }
    let idx = ((y - top) / (lh * (ITEM_LINES + 1) as f64)).floor() as usize;
    (idx < count).then_some(idx)
}

fn draw_dot(canvas: &mut Canvas, cx: i32, cy: i32, r: i32, color: Rgb) {
    for dy in -r..=r {
        for dx in -r..=r {
            let d2 = dx * dx + dy * dy;
            if d2 <= r * r {
                // 가장자리 한 픽셀은 반투명으로
                if d2 > (r - 1) * (r - 1) {
                    canvas.blend_rect(cx + dx, cy + dy, 1, 1, color, 140);
                } else {
                    canvas.fill_rect(cx + dx, cy + dy, 1, 1, color);
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn durations_and_colors() {
        assert_eq!(short_duration(30), "1분");
        assert_eq!(short_duration(125), "2분");
        assert_eq!(short_duration(7200), "2시간");
        assert_eq!(short_duration(3 * 86_400 + 5), "3일");
        let t = Theme::default();
        assert_eq!(usage_color(10.0, &t), t.palette[2]);
        assert_eq!(usage_color(70.0, &t), t.palette[3]);
        assert_eq!(usage_color(95.0, &t), t.palette[1]);
    }
}
