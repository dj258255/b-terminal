//! 가로 탭 줄(Ghostty처럼). 탭이 둘 이상일 때만 창 위쪽에 그린다

use bt_core::{Rgb, Theme};
use bt_render::proportional::draw_text_proportional;
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

/// 프로그램이 보낸 진행 표시(OSC 9;4)를 받는가: `progress-style`이 꺼져 있으면 조용히 버린다.
/// 상태 0(지움)도 표시를 남기지 않는다. 받으면 (상태, 퍼센트)
pub fn accept_progress(style: bool, state: u8, percent: u8) -> Option<(u8, u8)> {
    (style && state != 0).then_some((state, percent))
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

/// 탭 줄의 높이·구분선 같은 자리를 셀 글꼴: 제목 글꼴이 있으면 그것(터미널 글꼴 크기를 따르지 않는다, #474),
/// 없으면 터미널 글꼴. 그리는 쪽(`draw_with_title_font`)과 높이를 세는 쪽이 이것을 거친다. 탭 자리(`slots`·`hit`)는 글꼴을 보지 않는다(#478)
pub fn bar_fonts<'a>(terminal: &'a Fonts, title: Option<&'a Fonts>) -> &'a Fonts {
    title.unwrap_or(terminal)
}

/// 터미널 글꼴로 셀 때의 탭 줄 높이(px): 셀 높이의 1.5배
fn height_px(fonts: &Fonts) -> u32 {
    let m = fonts.metrics();
    m.height + m.height / 2
}

/// 탭 줄 높이(px). 그리는 쪽·격자 자리·새 탭 줄 자리가 모두 이것을 쓴다.
/// 제목 글꼴이 있으면 그 셀 높이의 5/3이다(#474). 시스템 UI 글꼴(SF) 13pt는 셀 높이가 터미널 글꼴보다 낮아(배율 2에서 31px)
/// 1.5배면 46px로 0.50.0의 52px보다 눈에 띄게 낮아진다. 5/3이면 51px다. 제목 글꼴이 없으면 터미널 셀 높이의 1.5배 그대로다
pub fn bar_height(terminal: &Fonts, title: Option<&Fonts>) -> u32 {
    match title {
        Some(t) => {
            let h = t.metrics().height;
            h + h * 2 / 3
        }
        None => height_px(terminal),
    }
}

/// 탭마다 (x, 폭). 상한 없이 창 폭을 탭 수로 고르게 나누고, 나머지 픽셀은 마지막 탭이 가져 오른쪽 끝까지 채운다(#478).
/// 폭이 탭 수보다 작으면 탭마다 1px로 하고 창 밖으로 넘어간다(패닉은 없다)
fn slots(width: u32, count: usize) -> Vec<(i32, u32)> {
    if count == 0 {
        return Vec::new();
    }
    let w = (width / count as u32).max(1);
    let last = width.saturating_sub(w * (count as u32 - 1)).max(1);
    (0..count)
        .map(|i| (i as i32 * w as i32, if i + 1 == count { last } else { w }))
        .collect()
}

/// 탭 줄을 `top`(창 위에서 내려온 픽셀)부터 그린다. 제목 막대 밑으로 콘텐츠가 들어가는 창에서는 `top`이 제목 막대 높이다(#192).
/// 제목 글꼴을 따로 쓰는 탭 줄(#241 `window-title-font-family`). `title_fonts`가 없으면 터미널 글꼴로 셀 격자에 맞춰 그린다.
/// 있으면 글자마다 advance로 자리를 잡고(비례폭도 겹치지 않게) 잘림도 그 폭으로 센다. 탭 높이·구분선·안 읽음 점·진행 막대도 제목 글꼴 지표로 센다(#474)
pub fn draw_with_title_font(
    canvas: &mut Canvas,
    fonts: &mut Fonts,
    mut title_fonts: Option<&mut Fonts>,
    theme: &Theme,
    tabs: &[TabLabel],
    top: i32,
) {
    // 높이·구분선·점·진행 막대는 모두 제목 글꼴이 있으면 그 지표로 센다(#474). 탭 자리는 누른 자리 판정(`hit`)과 같은 `slots`다
    let m = bar_fonts(fonts, title_fonts.as_deref()).metrics();
    let h = bar_height(fonts, title_fonts.as_deref());
    let bar = theme.background.mix(theme.foreground, 0.06);
    let active_bg = theme.background;
    let border = theme.background.mix(theme.foreground, 0.16);
    let text = theme.foreground;
    let dim = theme.foreground.mix(theme.background, 0.45);
    canvas.fill_rect(0, top, canvas.width, h, bar);
    canvas.fill_rect(0, top + h as i32 - 1, canvas.width, 1, border);
    let cw = m.width as i32;
    for (tab, (x, w)) in tabs.iter().zip(slots(canvas.width, tabs.len())) {
        if tab.active {
            canvas.fill_rect(x, top, w, h, active_bg);
        }
        canvas.fill_rect(x + w as i32 - 1, top + m.height as i32 / 4, 1, h - m.height / 2, border);
        // 가운데 맞춘 제목. 안 읽음 점 자리를 남긴다
        let color = if tab.active { text } else { dim };
        if let Some(tf) = title_fonts.as_deref_mut() {
            let room_px = (w as i32 - 4 * cw).max(cw) as f32;
            let (title, tw) = fit_title(&tab.title, room_px, |c| tf.advance(c));
            let tx = x + (w as i32 - tw.round() as i32) / 2;
            let ty = top + (h as i32 - tf.metrics().height as i32) / 2;
            draw_text_proportional(canvas, tf, tx, ty, &title, color);
        } else {
            let max_cells = ((w as i32 / cw) - 4).max(1) as usize;
            let title = truncate(&tab.title, max_cells);
            let tw = cw * text_cells(&title) as i32;
            let tx = x + (w as i32 - tw) / 2;
            let ty = top + (h as i32 - m.height as i32) / 2;
            draw_text(canvas, fonts, tx, ty, &title, color, false);
        }
        if let Some(p) = tab.progress {
            draw_progress(
                canvas,
                theme,
                x,
                top + h as i32 - (m.stroke * 3) as i32 - 1,
                w,
                p,
                m.stroke * 3,
            );
        }
        if tab.unread {
            dot(canvas, x + cw, top + h as i32 / 2, (cw / 3).max(2), theme.palette[3]);
        }
    }
}

/// 제목 글꼴을 따로 쓸 때(#241) 탭 하나의 제목을 방 `room_px` 안에 맞춘다: (잘린 제목, 그린 폭 px).
/// 폭은 글자마다의 advance 합이라 비례폭 글꼴에서도 글자가 겹치거나 탭 밖으로 나가지 않는다
pub fn fit_title(title: &str, room_px: f32, mut advance: impl FnMut(char) -> f32) -> (String, f32) {
    let fitted = bt_render::proportional::truncate_width(title, room_px, &mut advance);
    let width = bt_render::proportional::text_width(&fitted, &mut advance);
    (fitted, width)
}

/// 창 좌표가 몇 번째 탭인가
pub fn hit(width: u32, x: f64, count: usize) -> Option<usize> {
    slots(width, count)
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

/// 보안 키보드 입력이 켜져 있는 동안 창 오른쪽 위에 자물쇠를 띄운다(Ghostty `macos-secure-input-indication`, #258)
pub fn draw_secure_badge(canvas: &mut Canvas, fonts: &mut Fonts, theme: &Theme, top: u32) {
    let m = fonts.metrics();
    let side = m.height + m.height / 2;
    let margin = (m.height / 2).max(4);
    let x = canvas.width as i32 - side as i32 - margin as i32;
    let y = top as i32 + margin as i32;
    let edge = theme.background.mix(theme.foreground, 0.5);
    canvas.fill_rect(x, y, side, side, edge);
    canvas.fill_rect(x + 1, y + 1, side - 2, side - 2, theme.background);
    let pad = side / 5;
    for (rx, ry, rw, rh) in bt_render::lock_rects(x + pad as i32, y + pad as i32, side - 2 * pad, side - 2 * pad) {
        canvas.fill_rect(rx, ry, rw, rh, theme.foreground);
    }
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

/// 명령 팔레트(#77)를 창 위쪽 가운데에 그린다: 친 글 한 줄, 그 아래 맞는 동작들(이름 왼쪽, 단축키 오른쪽)
pub fn draw_palette(
    canvas: &mut Canvas,
    fonts: &mut Fonts,
    theme: &Theme,
    query: &str,
    rows: &[(String, String)],
    selected: usize,
) {
    let m = fonts.metrics();
    let cw = m.width as i32;
    let line_h = m.height + m.height / 3;
    let max_rows = 10usize;
    let width_cells = ((canvas.width / m.width.max(1)) as usize)
        .saturating_sub(4)
        .clamp(20, 60);
    let w = width_cells as u32 * m.width;
    let shown = rows.len().min(max_rows);
    // 고른 줄이 보이게 목록을 민다
    let first = selected
        .saturating_sub(max_rows - 1)
        .min(rows.len().saturating_sub(shown));
    let h = line_h * (1 + shown.max(1) as u32) + m.height / 2;
    let x = (canvas.width as i32 - w as i32) / 2;
    let y = m.height as i32;
    canvas.fill_rect(x, y, w, h, theme.background.mix(theme.foreground, 0.08));
    canvas.stroke_rect(x, y, w, h, 1, theme.background.mix(theme.foreground, 0.35));
    let pad = (line_h as i32 - m.height as i32) / 2;
    let dim = theme.foreground.mix(theme.background, 0.45);
    // 친 글
    let prompt = "› ";
    draw_text(canvas, fonts, x + cw, y + pad, prompt, dim, false);
    let qx = x + cw * (1 + text_cells(prompt) as i32);
    let q = truncate(query, width_cells.saturating_sub(4));
    draw_text(canvas, fonts, qx, y + pad, &q, theme.foreground, false);
    canvas.fill_rect(
        qx + cw * text_cells(&q) as i32,
        y + pad,
        (m.stroke * 2).max(2),
        m.height,
        theme.cursor,
    );
    canvas.fill_rect(x, y + line_h as i32, w, 1, theme.background.mix(theme.foreground, 0.2));
    if rows.is_empty() {
        draw_text(
            canvas,
            fonts,
            x + cw,
            y + line_h as i32 + pad,
            "맞는 명령이 없습니다",
            dim,
            false,
        );
        return;
    }
    for (n, (label, keys)) in rows.iter().enumerate().skip(first).take(shown) {
        let ry = y + line_h as i32 * (1 + (n - first) as i32) + m.height as i32 / 4;
        if n == selected {
            canvas.fill_rect(x + 1, ry, w - 2, line_h, theme.selection_background);
        }
        let fg = if n == selected {
            theme.selection_foreground.unwrap_or(theme.foreground)
        } else {
            theme.foreground
        };
        let keys_cells = text_cells(keys);
        let label = truncate(label, width_cells.saturating_sub(keys_cells + 4));
        draw_text(canvas, fonts, x + cw, ry + pad, &label, fg, false);
        draw_text(
            canvas,
            fonts,
            x + w as i32 - cw * (keys_cells as i32 + 1),
            ry + pad,
            keys,
            dim,
            false,
        );
    }
}

/// 닫기·끝내기 확인 막대(#108)를 창 위쪽 가운데에 그린다: 묻는 글 한 줄, 그 아래 키 안내
pub fn draw_confirm(canvas: &mut Canvas, fonts: &mut Fonts, theme: &Theme, message: &str, hint: &str) {
    let m = fonts.metrics();
    let cw = m.width as i32;
    let line_h = m.height + m.height / 3;
    let max_cells = ((canvas.width / m.width.max(1)) as usize).saturating_sub(4).max(10);
    let message = truncate(message, max_cells.saturating_sub(4));
    let cells = text_cells(&message).max(text_cells(hint)) + 4;
    let w = cells.min(max_cells) as u32 * m.width;
    let h = line_h * 2 + m.height / 2;
    let x = (canvas.width as i32 - w as i32) / 2;
    let y = m.height as i32;
    canvas.fill_rect(x, y, w, h, theme.background.mix(theme.foreground, 0.08));
    // 눈에 띄게 노란 테두리(경고)
    canvas.stroke_rect(x, y, w, h, (m.stroke).max(1), theme.palette[3]);
    let pad = (line_h as i32 - m.height as i32) / 2 + m.height as i32 / 4;
    draw_text(canvas, fonts, x + cw * 2, y + pad, &message, theme.foreground, false);
    let dim = theme.foreground.mix(theme.background, 0.4);
    draw_text(canvas, fonts, x + cw * 2, y + pad + line_h as i32, hint, dim, false);
}

/// 글 입력 막대(#124, 칸 제목 바꾸기)를 창 위쪽 가운데에: 앞말과 친 글·커서 한 줄, 그 아래 키 안내
pub fn draw_prompt(canvas: &mut Canvas, fonts: &mut Fonts, theme: &Theme, label: &str, text: &str, hint: &str) {
    let m = fonts.metrics();
    let cw = m.width as i32;
    let line_h = m.height + m.height / 3;
    let width_cells = ((canvas.width / m.width.max(1)) as usize)
        .saturating_sub(4)
        .clamp(20, 60);
    let w = width_cells as u32 * m.width;
    let h = line_h * 2 + m.height / 2;
    let x = (canvas.width as i32 - w as i32) / 2;
    let y = m.height as i32;
    canvas.fill_rect(x, y, w, h, theme.background.mix(theme.foreground, 0.08));
    canvas.stroke_rect(x, y, w, h, 1, theme.background.mix(theme.foreground, 0.35));
    let pad = (line_h as i32 - m.height as i32) / 2 + m.height as i32 / 4;
    let dim = theme.foreground.mix(theme.background, 0.45);
    draw_text(canvas, fonts, x + cw, y + pad, label, dim, false);
    let tx = x + cw * (1 + text_cells(label) as i32);
    let room = width_cells.saturating_sub(text_cells(label) + 3);
    // 긴 글은 끝(친 자리)이 보이게 앞을 자른다
    let shown: String = {
        let mut cells = 0;
        let mut out: Vec<char> = Vec::new();
        for c in text.chars().rev() {
            cells += bt_render::char_cells(c);
            if cells > room {
                break;
            }
            out.push(c);
        }
        out.into_iter().rev().collect()
    };
    draw_text(canvas, fonts, tx, y + pad, &shown, theme.foreground, false);
    canvas.fill_rect(
        tx + cw * text_cells(&shown) as i32,
        y + pad,
        (m.stroke * 2).max(2),
        m.height,
        theme.cursor,
    );
    let hint = truncate(hint, width_cells.saturating_sub(2));
    draw_text(canvas, fonts, x + cw, y + pad + line_h as i32, &hint, dim, false);
}

/// 크기 표시 띠(`w`×`h`)를 칸 안 어디에 둘지(#122). 가장자리에서 `margin`만큼 띄운다
pub fn overlay_origin(
    pos: bt_core::config::OverlayPosition,
    pane: crate::split::Rect,
    w: u32,
    h: u32,
    margin: i32,
) -> (i32, i32) {
    use bt_core::config::OverlayPosition::*;
    let (left, right) = (pane.x + margin, pane.x + pane.w as i32 - w as i32 - margin);
    let (top, bottom) = (pane.y + margin, pane.y + pane.h as i32 - h as i32 - margin);
    let cx = pane.x + (pane.w as i32 - w as i32) / 2;
    let cy = pane.y + (pane.h as i32 - h as i32) / 2;
    match pos {
        Center => (cx, cy),
        TopLeft => (left, top),
        TopCenter => (cx, top),
        TopRight => (right, top),
        BottomLeft => (left, bottom),
        BottomCenter => (cx, bottom),
        BottomRight => (right, bottom),
    }
}

/// 크기 표시 띠(#122): 칸 안 `pos` 자리에 `80 × 24`
pub fn draw_resize_overlay(
    canvas: &mut Canvas,
    fonts: &mut Fonts,
    theme: &Theme,
    pane: crate::split::Rect,
    pos: bt_core::config::OverlayPosition,
    text: &str,
) {
    let m = fonts.metrics();
    let (w, h) = (m.width * (text_cells(text) as u32 + 2), m.height + m.height / 2);
    if w > pane.w || h > pane.h {
        return;
    }
    let (x, y) = overlay_origin(pos, pane, w, h, m.width as i32);
    canvas.fill_rect(x, y, w, h, theme.background.mix(theme.foreground, 0.85));
    draw_text(
        canvas,
        fonts,
        x + m.width as i32,
        y + (h as i32 - m.height as i32) / 2,
        text,
        theme.background,
        false,
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 가짜 비례폭: i·l 3, W·M 12, 한글 16, `…` 10, 나머지 8
    fn adv(c: char) -> f32 {
        match c {
            'i' | 'l' => 3.0,
            'W' | 'M' => 12.0,
            '…' => 10.0,
            c if (c as u32) >= 0xAC00 => 16.0,
            _ => 8.0,
        }
    }

    #[test]
    fn fit_title_measures_real_width() {
        // 들어가면 그대로, 폭은 advance 합
        assert_eq!(fit_title("iiW", 100.0, adv), ("iiW".to_string(), 18.0));
        // 좁은 글자가 많은 제목은 같은 글자 수의 넓은 제목보다 더 남는다
        let (narrow, nw) = fit_title("iiiiiiiiii", 25.0, adv);
        let (wide, ww) = fit_title("WWWWWWWWWW", 25.0, adv);
        assert_eq!((narrow.as_str(), nw), ("iiiii…", 25.0));
        assert_eq!((wide.as_str(), ww), ("W…", 22.0));
        // 한글은 16px씩
        assert_eq!(fit_title("안녕하세요", 42.0, adv), ("안녕…".to_string(), 42.0));
    }

    #[test]
    fn fit_title_width_never_exceeds_room() {
        for t in ["iiiiiiiiiiii", "WWWWWWWWWWWW", "안녕하세요 abc", "Mixed Wiki 한글"] {
            for room in [20.0, 33.0, 60.0, 101.0] {
                let (out, w) = fit_title(t, room, adv);
                assert!(w <= room, "{t:?} {room}: {out:?} {w}");
                assert_eq!(w, out.chars().map(adv).sum::<f32>(), "{out:?}");
            }
        }
    }

    #[test]
    fn title_font_text_stays_inside_its_tab() {
        let Ok(set) = Fonts::new_with_title(&bt_render::FontConfig::default(), Some("JetBrains Mono")) else {
            return;
        };
        let (mut fonts, mut title) = (set.terminal, set.title);
        assert!(title.is_some());
        let theme = Theme::ghostty_default();
        let (w, h) = (300u32, 60u32);
        let mut buf = vec![0u32; (w * h) as usize];
        let mut c = Canvas::new(&mut buf, w, h);
        c.clear(theme.background);
        let mk = |t: &str| TabLabel {
            title: t.into(),
            active: false,
            unread: false,
            progress: None,
        };
        let tabs = [mk(&"안녕 W".repeat(30)), mk("")];
        draw_with_title_font(&mut c, &mut fonts, title.as_mut(), &theme, &tabs, 0);
        // 둘째 탭(x 150..299)은 제목이 없으니 바탕·구분선·밑줄 말고는 아무것도 없어야 한다
        let bar = theme.background.mix(theme.foreground, 0.06);
        let bh = bar_height(&fonts, title.as_ref());
        for y in 0..bh - 1 {
            for x in 150..299 {
                assert_eq!(c.pixel(x, y), bar, "({x},{y})에 첫 탭 제목이 넘어 나왔다");
            }
        }
    }

    /// #460: 키를 비우면(macOS) 시스템 UI 글꼴로 그린다. 그 글꼴의 키가 탭 줄 높이 안에 들어가고 아래 구분선을 덮지 않는다.
    /// 시스템 UI 글꼴이 없으면(Linux 등) 건너뛴다
    #[test]
    fn system_ui_title_font_fits_the_tab_bar() {
        let Ok(set) = Fonts::new_with_title(&bt_render::FontConfig::default(), None) else {
            return;
        };
        let (mut fonts, mut title) = (set.terminal, set.title);
        let Some(tf) = title.as_ref() else {
            eprintln!("건너뜀: 시스템 UI 글꼴이 없어 터미널 글꼴로 그린다");
            return;
        };
        let bh = bar_height(&fonts, Some(tf));
        assert!(
            tf.metrics().height <= bh,
            "제목 글꼴 키 {} > 탭 줄 {bh}",
            tf.metrics().height
        );
        let theme = Theme::ghostty_default();
        let (w, h) = (300u32, 60u32);
        let mut buf = vec![0u32; (w * h) as usize];
        let mut c = Canvas::new(&mut buf, w, h);
        c.clear(theme.background);
        let mk = |t: &str| TabLabel {
            title: t.into(),
            active: false,
            unread: false,
            progress: None,
        };
        let tabs = [mk(&"Hgjpq안녕 Ég ".repeat(10)), mk("")];
        draw_with_title_font(&mut c, &mut fonts, title.as_mut(), &theme, &tabs, 0);
        let border = theme.background.mix(theme.foreground, 0.16);
        for x in 0..149 {
            assert_eq!(c.pixel(x, bh - 1), border, "({x})에서 구분선이 덮였다");
        }
        let bar = theme.background.mix(theme.foreground, 0.06);
        let inked = (0..149).any(|x| (0..bh - 1).any(|y| c.pixel(x, y) != bar));
        assert!(inked, "제목이 그려지지 않았다");
        for y in 0..bh - 1 {
            for x in 150..299 {
                assert_eq!(c.pixel(x, y), bar, "({x},{y})에 첫 탭 제목이 넘어 나왔다");
            }
        }
    }

    /// #474: 기본 설정(시스템 UI 제목 글꼴 13pt, 배율 2)의 탭 줄 높이는 0.50.0의 52px에서 크게 바뀌지 않는다(48~56px)
    #[test]
    fn default_tab_bar_height_stays_near_0_50() {
        let cfg = bt_render::FontConfig {
            scale: 2.0,
            ..bt_render::FontConfig::default()
        };
        let Ok(set) = Fonts::new_with_title(&cfg, None) else {
            return;
        };
        let Some(tf) = set.title.as_ref() else {
            eprintln!("건너뜀: 시스템 UI 글꼴이 없다");
            return;
        };
        let h = bar_height(&set.terminal, Some(tf));
        assert!(
            (48..=56).contains(&h),
            "탭 줄 {h}px, 제목 셀 높이 {}",
            tf.metrics().height
        );
        assert!(tf.metrics().height <= h);
    }

    /// #474: 제목 글꼴이 있으면 탭 줄의 높이와 탭 자리는 터미널 글꼴 크기(13과 20)와 상관없이 같다
    #[test]
    fn tab_bar_ignores_terminal_font_size_with_title_font() {
        let build = |size: f32| {
            let cfg = bt_render::FontConfig {
                size,
                scale: 2.0,
                ..bt_render::FontConfig::default()
            };
            Fonts::new_with_title(&cfg, Some("JetBrains Mono")).ok()
        };
        let (Some(a), Some(b)) = (build(13.0), build(20.0)) else {
            return;
        };
        let (ta, tb) = (a.title.as_ref().unwrap(), b.title.as_ref().unwrap());
        assert_ne!(
            height_px(&a.terminal),
            height_px(&b.terminal),
            "터미널 글꼴 높이는 크기를 따른다"
        );
        assert_eq!(bar_height(&a.terminal, Some(ta)), bar_height(&b.terminal, Some(tb)));
        // 탭 자리(`slots`·`hit`)는 글꼴이 아니라 창 폭과 탭 수만 보므로(#478) 글꼴 크기와 상관없다
        // 제목 글꼴이 없으면 터미널 글꼴로 센다
        assert_eq!(bar_height(&b.terminal, None), height_px(&b.terminal));
    }

    /// #478: 탭 폭에 상한이 없다. 폭 합이 창 폭과 같고 이웃 탭이 빈틈·겹침 없이 이어진다
    #[test]
    fn slots_fill_the_whole_width() {
        for count in [1usize, 2, 3, 7] {
            let s = slots(2000, count);
            assert_eq!(s.len(), count);
            assert_eq!(s.iter().map(|&(_, w)| w).sum::<u32>(), 2000, "탭 {count}: 폭 합");
            assert_eq!(s[0].0, 0);
            for pair in s.windows(2) {
                assert_eq!(pair[0].0 + pair[0].1 as i32, pair[1].0, "탭 {count}: 이웃이 이어짐");
            }
            let last = s[count - 1];
            assert_eq!(last.0 + last.1 as i32, 2000, "탭 {count}: 오른쪽 끝");
            let first_w = s[0].1;
            assert!(
                s[..count - 1].iter().all(|&(_, w)| w == first_w),
                "나머지는 마지막 탭만 가진다"
            );
        }
    }

    /// #478: 누른 자리는 그린 자리와 같다. x=0, 경계 양쪽, 오른쪽 끝(width-1)
    #[test]
    fn hit_matches_the_slots() {
        for count in [1usize, 2, 3, 7] {
            let s = slots(2000, count);
            assert_eq!(hit(2000, 0.0, count), Some(0));
            assert_eq!(hit(2000, 1999.0, count), Some(count - 1), "탭 {count}: 오른쪽 끝");
            assert_eq!(hit(2000, 2000.0, count), None);
            for (i, &(x, _)) in s.iter().enumerate().skip(1) {
                assert_eq!(hit(2000, x as f64 - 1.0, count), Some(i - 1));
                assert_eq!(hit(2000, x as f64, count), Some(i));
            }
        }
        assert_eq!(hit(2000, 5.0, 0), None);
    }

    /// #478: 폭이 0이거나 탭 수보다 작아도 패닉이 나지 않는다
    #[test]
    fn slots_do_not_panic_on_tiny_widths() {
        assert!(slots(2000, 0).is_empty());
        for (width, count) in [(0u32, 3usize), (1, 3), (3, 7), (5, 5)] {
            let s = slots(width, count);
            assert_eq!(s.len(), count);
            assert!(s.iter().all(|&(_, w)| w >= 1));
            let _ = hit(width, 0.0, count);
        }
    }

    #[test]
    fn progress_is_ignored_when_style_is_off() {
        assert_eq!(accept_progress(true, 1, 40), Some((1, 40)));
        assert_eq!(accept_progress(true, 0, 0), None, "상태 0은 지움");
        assert_eq!(accept_progress(false, 1, 40), None);
        assert_eq!(accept_progress(false, 3, 0), None);
    }

    #[test]
    fn tab_bar_starts_below_the_top_offset() {
        let Ok(mut fonts) = Fonts::new(&bt_render::FontConfig::default()) else {
            return;
        };
        let theme = Theme::ghostty_default();
        let (w, h) = (300u32, 200u32);
        let mut buf = vec![0u32; (w * h) as usize];
        let mut c = Canvas::new(&mut buf, w, h);
        c.clear(theme.background);
        let tabs = [TabLabel {
            title: "a".into(),
            active: false,
            unread: false,
            progress: None,
        }];
        let top = 40;
        draw_with_title_font(&mut c, &mut fonts, None, &theme, &tabs, top);
        let bar = theme.background.mix(theme.foreground, 0.06).to_u32();
        let at = |y: u32| buf[(y * w + 5) as usize] & 0xFF_FFFF;
        // 제목 막대 자리는 그대로, 그 밑부터 탭 줄 색
        assert_ne!(at(top as u32 - 1), bar);
        assert_eq!(at(top as u32 + 1), bar);
    }

    #[test]
    fn prompt_bar_keeps_the_end_of_long_text_visible() {
        let Ok(mut fonts) = Fonts::new(&bt_render::FontConfig::default()) else {
            return;
        };
        let theme = Theme::ghostty_default();
        let (w, h) = (600u32, 200u32);
        let mut buf = vec![0u32; (w * h) as usize];
        let mut c = Canvas::new(&mut buf, w, h);
        c.clear(theme.background);
        let long = "가".repeat(200) + "끝";
        draw_prompt(&mut c, &mut fonts, &theme, "제목 › ", &long, "↩ 정하기");
        // 커서(theme.cursor 색 막대)가 상자 안에 있다
        let m = fonts.metrics();
        let row = m.height + m.height / 2;
        let cursor: Vec<u32> = (0..w).filter(|&x| c.pixel(x, row) == theme.cursor).collect();
        assert!(!cursor.is_empty(), "커서가 보인다");
        assert!(*cursor.last().unwrap() < w - m.width, "커서가 상자 안");
    }

    #[test]
    fn resize_overlay_sits_at_the_chosen_spot_inside_the_pane() {
        use bt_core::config::OverlayPosition::*;
        let pane = crate::split::Rect {
            x: 100,
            y: 50,
            w: 400,
            h: 300,
        };
        let at = |p| overlay_origin(p, pane, 80, 20, 10);
        assert_eq!(at(Center), (260, 190));
        assert_eq!(at(TopLeft), (110, 60));
        assert_eq!(at(TopCenter), (260, 60));
        assert_eq!(at(TopRight), (410, 60));
        assert_eq!(at(BottomLeft), (110, 320));
        assert_eq!(at(BottomCenter), (260, 320));
        assert_eq!(at(BottomRight), (410, 320));
    }

    #[test]
    fn confirm_bar_is_centered_with_warning_border_and_fits_long_text() {
        let Ok(mut fonts) = Fonts::new(&bt_render::FontConfig::default()) else {
            return;
        };
        let theme = Theme::ghostty_default();
        let (w, h) = (800u32, 300u32);
        let mut buf = vec![0u32; (w * h) as usize];
        let mut c = Canvas::new(&mut buf, w, h);
        c.clear(theme.background);
        let long = "이 칸에서 ".to_string() + &"아주 긴 명령 ".repeat(40) + "😀이(가) 돌고 있습니다. 닫을까요?";
        draw_confirm(&mut c, &mut fonts, &theme, &long, "↩ 닫기    esc 취소");
        let m = fonts.metrics();
        // 테두리(노랑)가 위쪽 가운데에 있고, 좌우 끝은 창 안에 들어온다
        let row = m.height;
        let border: Vec<u32> = (0..w).filter(|&x| c.pixel(x, row) == theme.palette[3]).collect();
        assert!(!border.is_empty(), "노란 테두리");
        let (l, r) = (border[0], *border.last().unwrap());
        assert!(l > 0 && r < w - 1, "창 안: {l}..{r}");
        assert!(
            (l as i32 - (w - 1 - r) as i32).abs() <= 1,
            "가운데: {l} / {}",
            w - 1 - r
        );
    }
}
