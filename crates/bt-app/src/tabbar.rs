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

/// 탭 줄을 `top`(창 위에서 내려온 픽셀)부터 그린다. 제목 막대 밑으로 콘텐츠가 들어가는 창에서는 `top`이 제목 막대 높이다(#192)
pub fn draw(canvas: &mut Canvas, fonts: &mut Fonts, theme: &Theme, tabs: &[TabLabel], top: i32) {
    let m = fonts.metrics();
    let h = height_px(fonts);
    let bar = theme.background.mix(theme.foreground, 0.06);
    let active_bg = theme.background;
    let border = theme.background.mix(theme.foreground, 0.16);
    let text = theme.foreground;
    let dim = theme.foreground.mix(theme.background, 0.45);
    canvas.fill_rect(0, top, canvas.width, h, bar);
    canvas.fill_rect(0, top + h as i32 - 1, canvas.width, 1, border);
    let cw = m.width as i32;
    for (tab, (x, w)) in tabs.iter().zip(slots(fonts, canvas.width, tabs.len())) {
        if tab.active {
            canvas.fill_rect(x, top, w, h, active_bg);
        }
        canvas.fill_rect(x + w as i32 - 1, top + m.height as i32 / 4, 1, h - m.height / 2, border);
        // 가운데 맞춘 제목. 안 읽음 점 자리를 남긴다
        let max_cells = ((w as i32 / cw) - 4).max(1) as usize;
        let title = truncate(&tab.title, max_cells);
        let tw = cw * text_cells(&title) as i32;
        let tx = x + (w as i32 - tw) / 2;
        let ty = top + (h as i32 - m.height as i32) / 2;
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
        draw(&mut c, &mut fonts, &theme, &tabs, top);
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
