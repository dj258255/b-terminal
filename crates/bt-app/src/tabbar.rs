//! 가로 탭 줄(Ghostty처럼). 탭이 둘 이상일 때만 창 위쪽에 그린다

use bt_core::{Rgb, Theme};
use bt_render::{Canvas, Fonts, draw_text, text_cells, truncate};

/// 탭 줄 한 칸
#[derive(Debug, Clone)]
pub struct TabLabel {
    pub title: String,
    pub active: bool,
    /// 보지 않은 알림(OSC 9·99·777, 벨)이 있다
    pub unread: bool,
    /// 진행 표시(OSC 9;4): (상태 1 보통·2 오류·3 끝 모름·4 멈춤, 퍼센트)
    pub progress: Option<(u8, u8)>,
}

/// 진행 막대를 (x, y)에서 폭 w로 그린다. 끝을 모르면(3) 가운데 3분의 1을 칠한다
pub fn draw_progress(canvas: &mut Canvas, theme: &Theme, x: i32, y: i32, w: u32, p: (u8, u8), h: u32) {
    let (state, percent) = p;
    let color = match state {
        2 => theme.palette[1],
        4 => theme.palette[3],
        _ => theme.palette[4],
    };
    let h = h.max(2);
    canvas.fill_rect(x, y, w, h, theme.background.mix(theme.foreground, 0.12));
    match state {
        3 => canvas.fill_rect(x + w as i32 / 3, y, w / 3, h, color),
        _ => canvas.fill_rect(
            x,
            y,
            (w as f32 * percent.min(100) as f32 / 100.0).round() as u32,
            h,
            color,
        ),
    }
}

/// 탭 하나의 최대 폭(셀 수)
const MAX_TAB_CELLS: u32 = 28;

pub fn height_px(fonts: &Fonts) -> u32 {
    let m = fonts.metrics();
    m.height + m.height / 2
}

/// 탭마다 (x, 폭). 창 폭을 고르게 나누되 한 칸은 MAX_TAB_CELLS를 넘지 않는다
fn slots(fonts: &Fonts, width: u32, count: usize) -> Vec<(i32, u32)> {
    if count == 0 {
        return Vec::new();
    }
    let cap = fonts.metrics().width * MAX_TAB_CELLS;
    let w = (width / count as u32).min(cap).max(1);
    (0..count).map(|i| (i as i32 * w as i32, w)).collect()
}

pub fn draw(canvas: &mut Canvas, fonts: &mut Fonts, theme: &Theme, tabs: &[TabLabel]) {
    let m = fonts.metrics();
    let h = height_px(fonts);
    let bar = theme.background.mix(theme.foreground, 0.06);
    let active_bg = theme.background;
    let border = theme.background.mix(theme.foreground, 0.16);
    let text = theme.foreground;
    let dim = theme.foreground.mix(theme.background, 0.45);
    canvas.fill_rect(0, 0, canvas.width, h, bar);
    canvas.fill_rect(0, h as i32 - 1, canvas.width, 1, border);
    let cw = m.width as i32;
    for (tab, (x, w)) in tabs.iter().zip(slots(fonts, canvas.width, tabs.len())) {
        if tab.active {
            canvas.fill_rect(x, 0, w, h, active_bg);
        }
        canvas.fill_rect(x + w as i32 - 1, m.height as i32 / 4, 1, h - m.height / 2, border);
        // 가운데 맞춘 제목. 안 읽음 점 자리를 남긴다
        let max_cells = ((w as i32 / cw) - 4).max(1) as usize;
        let title = truncate(&tab.title, max_cells);
        let tw = cw * text_cells(&title) as i32;
        let tx = x + (w as i32 - tw) / 2;
        let ty = (h as i32 - m.height as i32) / 2;
        draw_text(
            canvas,
            fonts,
            tx,
            ty,
            &title,
            if tab.active { text } else { dim },
            false,
        );
        if let Some(p) = tab.progress {
            draw_progress(
                canvas,
                theme,
                x,
                h as i32 - (m.stroke * 3) as i32 - 1,
                w,
                p,
                m.stroke * 3,
            );
        }
        if tab.unread {
            dot(canvas, x + cw, h as i32 / 2, (cw / 3).max(2), theme.palette[3]);
        }
    }
}

/// 창 좌표가 몇 번째 탭인가
pub fn hit(fonts: &Fonts, width: u32, x: f64, count: usize) -> Option<usize> {
    slots(fonts, width, count)
        .iter()
        .position(|&(sx, w)| x >= sx as f64 && x < (sx + w as i32) as f64)
}

fn dot(canvas: &mut Canvas, cx: i32, cy: i32, r: i32, color: Rgb) {
    for dy in -r..=r {
        for dx in -r..=r {
            if dx * dx + dy * dy <= r * r {
                canvas.fill_rect(cx + dx, cy + dy, 1, 1, color);
            }
        }
    }
}

/// 창 오른쪽 아래의 알림 띠(클립보드 복사 같은 일을 사람에게 보인다)
pub fn draw_toast(canvas: &mut Canvas, fonts: &mut Fonts, theme: &Theme, text: &str) {
    let m = fonts.metrics();
    let text = truncate(text, 48);
    let tw = m.width * text_cells(&text) as u32;
    let (w, h) = (tw + m.width * 2, m.height + m.height / 2);
    let x = canvas.width as i32 - w as i32 - m.width as i32;
    let y = canvas.height as i32 - h as i32 - m.height as i32 / 2;
    let bg = theme.background.mix(theme.foreground, 0.85);
    canvas.fill_rect(x, y, w, h, bg);
    draw_text(
        canvas,
        fonts,
        x + m.width as i32,
        y + (h as i32 - m.height as i32) / 2,
        &text,
        theme.background,
        false,
    );
}

/// 링크 주소를 창 왼쪽 아래에 띄운다(Cmd를 누른 채 링크 위에 있을 때)
pub fn draw_link_hint(canvas: &mut Canvas, fonts: &mut Fonts, theme: &Theme, url: &str) {
    let m = fonts.metrics();
    // 창 폭의 대부분까지 쓴다
    let max_cells = ((canvas.width / m.width.max(1)) as usize).saturating_sub(6).max(8);
    // 호스트는 늘 다 보이고 가운데를 줄인다(끝만 자르면 `github.com@다른곳`처럼 속일 수 있다, #54)
    let text = bt_term::links::hint_text(url, max_cells);
    let tw = m.width * text_cells(&text) as u32;
    let (w, h) = (tw + m.width * 2, m.height + m.height / 2);
    let x = m.width as i32 / 2;
    let y = canvas.height as i32 - h as i32 - m.height as i32 / 2;
    canvas.fill_rect(x, y, w, h, theme.background.mix(theme.foreground, 0.12));
    canvas.stroke_rect(x, y, w, h, 1, theme.background.mix(theme.foreground, 0.3));
    draw_text(
        canvas,
        fonts,
        x + m.width as i32,
        y + (h as i32 - m.height as i32) / 2,
        &text,
        theme.foreground,
        false,
    );
}

/// 찾기 막대를 칸(`pane`)의 오른쪽 위에 띄운다(#39). 결과가 없으면 붉게 보인다
pub fn draw_search_bar(
    canvas: &mut Canvas,
    fonts: &mut Fonts,
    theme: &Theme,
    pane: crate::split::Rect,
    query: &str,
    status: &str,
    missing: bool,
) {
    let m = fonts.metrics();
    let cw = m.width as i32;
    let label = "찾기 ";
    // 칸 폭의 절반 남짓까지. 글이 길면 앞을 줄여 끝(지금 치는 자리)을 보인다
    let max_cells = ((pane.w as i32 / cw) - 4).clamp(12, 60) as usize;
    let status_cells = text_cells(status);
    let room = max_cells.saturating_sub(text_cells(label) + status_cells + 3).max(4);
    let q: String = if text_cells(query) > room {
        let tail: String = query
            .chars()
            .rev()
            .take(room - 1)
            .collect::<Vec<_>>()
            .into_iter()
            .rev()
            .collect();
        format!("…{tail}")
    } else {
        query.to_string()
    };
    let body_cells = text_cells(label) + text_cells(&q).max(room.min(16)) + status_cells + 3;
    let (w, h) = (body_cells as u32 * m.width, m.height + m.height / 2);
    let x = pane.x + pane.w as i32 - w as i32 - cw;
    let y = pane.y + m.height as i32 / 4;
    let bg = theme.background.mix(theme.foreground, 0.1);
    let border = if missing {
        theme.palette[1]
    } else {
        theme.background.mix(theme.foreground, 0.35)
    };
    canvas.fill_rect(x, y, w, h, bg);
    canvas.stroke_rect(x, y, w, h, 1, border);
    let ty = y + (h as i32 - m.height as i32) / 2;
    let dim = theme.foreground.mix(theme.background, 0.45);
    let mut tx = x + cw;
    draw_text(canvas, fonts, tx, ty, label, dim, false);
    tx += cw * text_cells(label) as i32;
    draw_text(canvas, fonts, tx, ty, &q, theme.foreground, false);
    tx += cw * text_cells(&q) as i32;
    // 치는 자리
    canvas.fill_rect(tx, ty, (m.stroke * 2).max(2), m.height, theme.cursor);
    let sx = x + w as i32 - cw * (status_cells as i32 + 1);
    draw_text(
        canvas,
        fonts,
        sx,
        ty,
        status,
        if missing { theme.palette[1] } else { dim },
        false,
    );
}
