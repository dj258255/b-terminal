//! 상자 선·블록 글자를 글꼴 대신 직접 그린다.
//!
//! 글꼴의 상자 선은 셀 높이와 줄 간격이 맞지 않으면 줄 사이가 끊긴다. TUI(b-studio desk의 둥근 테두리, 반칸 블록 그림,
//! 8단계 막대)가 깔끔하게 이어지도록 셀 크기에 맞춰 그린다

use bt_core::Rgb;

use crate::canvas::Canvas;

/// 직접 그렸으면 참. 모르는 글자면 거짓(글꼴로 그린다)
#[allow(clippy::too_many_arguments)]
pub fn draw(canvas: &mut Canvas, c: char, x: i32, y: i32, w: u32, h: u32, stroke: u32, color: Rgb) -> bool {
    let (wi, hi) = (w as i32, h as i32);
    let t = stroke.max(1);
    let heavy = t * 2;
    let cx = x + (wi - t as i32) / 2;
    let cy = y + (hi - t as i32) / 2;
    // 선 조각: 왼·오른·위·아래 반쪽
    let left = |c: &mut Canvas, th: u32| {
        c.fill_rect(
            x,
            y + (hi - th as i32) / 2,
            (wi / 2 + th as i32 / 2 + 1) as u32,
            th,
            color,
        )
    };
    let right = |c: &mut Canvas, th: u32| {
        let sx = x + wi / 2 - th as i32 / 2;
        c.fill_rect(sx, y + (hi - th as i32) / 2, (x + wi - sx) as u32, th, color)
    };
    let up = |c: &mut Canvas, th: u32| {
        c.fill_rect(
            x + (wi - th as i32) / 2,
            y,
            th,
            (hi / 2 + th as i32 / 2 + 1) as u32,
            color,
        )
    };
    let down = |c: &mut Canvas, th: u32| {
        let sy = y + hi / 2 - th as i32 / 2;
        c.fill_rect(x + (wi - th as i32) / 2, sy, th, (y + hi - sy) as u32, color)
    };

    match c {
        // 가는 선
        '─' => canvas.fill_rect(x, cy, w, t, color),
        '│' => canvas.fill_rect(cx, y, t, h, color),
        '━' => canvas.fill_rect(x, y + (hi - heavy as i32) / 2, w, heavy, color),
        '┃' => canvas.fill_rect(x + (wi - heavy as i32) / 2, y, heavy, h, color),
        '┌' => {
            right(canvas, t);
            down(canvas, t);
        }
        '┐' => {
            left(canvas, t);
            down(canvas, t);
        }
        '└' => {
            right(canvas, t);
            up(canvas, t);
        }
        '┘' => {
            left(canvas, t);
            up(canvas, t);
        }
        '├' => {
            canvas.fill_rect(cx, y, t, h, color);
            right(canvas, t);
        }
        '┤' => {
            canvas.fill_rect(cx, y, t, h, color);
            left(canvas, t);
        }
        '┬' => {
            canvas.fill_rect(x, cy, w, t, color);
            down(canvas, t);
        }
        '┴' => {
            canvas.fill_rect(x, cy, w, t, color);
            up(canvas, t);
        }
        '┼' => {
            canvas.fill_rect(x, cy, w, t, color);
            canvas.fill_rect(cx, y, t, h, color);
        }
        // 둥근 모서리: 1/4 원호를 픽셀로
        '╭' | '╮' | '╰' | '╯' => rounded(canvas, c, x, y, wi, hi, t, color),
        // 블록
        '█' => canvas.fill_rect(x, y, w, h, color),
        '▀' => canvas.fill_rect(x, y, w, (hi / 2) as u32, color),
        '▄' => canvas.fill_rect(x, y + hi / 2, w, (hi - hi / 2) as u32, color),
        '▌' => canvas.fill_rect(x, y, (wi / 2) as u32, h, color),
        '▐' => canvas.fill_rect(x + wi / 2, y, (wi - wi / 2) as u32, h, color),
        // 왼쪽에서 채우는 8단계(막대)
        '▏' | '▎' | '▍' | '▋' | '▊' | '▉' => {
            let eighths = match c {
                '▏' => 1,
                '▎' => 2,
                '▍' => 3,
                '▋' => 5,
                '▊' => 6,
                _ => 7,
            };
            canvas.fill_rect(x, y, ((wi * eighths) / 8).max(1) as u32, h, color);
        }
        // 아래에서 채우는 8단계
        '▁' | '▂' | '▃' | '▅' | '▆' | '▇' => {
            let eighths = match c {
                '▁' => 1,
                '▂' => 2,
                '▃' => 3,
                '▅' => 5,
                '▆' => 6,
                _ => 7,
            };
            let bh = ((hi * eighths) / 8).max(1);
            canvas.fill_rect(x, y + hi - bh, w, bh as u32, color);
        }
        '░' | '▒' | '▓' => {
            let a = match c {
                '░' => 64,
                '▒' => 128,
                _ => 192,
            };
            canvas.blend_rect(x, y, w, h, color, a);
        }
        _ => return false,
    }
    true
}

#[allow(clippy::too_many_arguments)]
fn rounded(canvas: &mut Canvas, c: char, x: i32, y: i32, w: i32, h: i32, t: u32, color: Rgb) {
    // 원호의 중심은 선이 이어져 나가는 쪽 모서리. 반지름은 셀 폭의 절반
    let r = (w / 2).max(1);
    let (mx, my) = (x + w / 2, y + h / 2);
    let t = t as i32;
    // 곧은 이음: 원호 끝에서 셀 끝까지
    match c {
        '╭' => {
            canvas.fill_rect(
                mx + r - t / 2,
                my - t / 2,
                (x + w - (mx + r - t / 2)).max(0) as u32,
                t as u32,
                color,
            );
            canvas.fill_rect(
                mx - t / 2,
                my + r - t / 2,
                t as u32,
                (y + h - (my + r - t / 2)).max(0) as u32,
                color,
            );
        }
        '╮' => {
            canvas.fill_rect(x, my - t / 2, (mx - r + t / 2 + 1 - x).max(0) as u32, t as u32, color);
            canvas.fill_rect(
                mx - t / 2,
                my + r - t / 2,
                t as u32,
                (y + h - (my + r - t / 2)).max(0) as u32,
                color,
            );
        }
        '╰' => {
            canvas.fill_rect(
                mx + r - t / 2,
                my - t / 2,
                (x + w - (mx + r - t / 2)).max(0) as u32,
                t as u32,
                color,
            );
            canvas.fill_rect(mx - t / 2, y, t as u32, (my - r + t / 2 + 1 - y).max(0) as u32, color);
        }
        _ => {
            canvas.fill_rect(x, my - t / 2, (mx - r + t / 2 + 1 - x).max(0) as u32, t as u32, color);
            canvas.fill_rect(mx - t / 2, y, t as u32, (my - r + t / 2 + 1 - y).max(0) as u32, color);
        }
    }
    // 원호의 중심(선이 나가는 두 방향 쪽)
    let (ox, oy) = match c {
        '╭' => (mx + r, my + r),
        '╮' => (mx - r, my + r),
        '╰' => (mx + r, my - r),
        _ => (mx - r, my - r),
    };
    let inner = (r as f32 - t as f32 / 2.0).max(0.0);
    let outer = r as f32 + t as f32 / 2.0;
    for py in y..y + h {
        for px in x..x + w {
            let in_quadrant = match c {
                '╭' => px <= ox && py <= oy,
                '╮' => px >= ox && py <= oy,
                '╰' => px <= ox && py >= oy,
                _ => px >= ox && py >= oy,
            };
            if !in_quadrant {
                continue;
            }
            let d = (((px - ox) * (px - ox) + (py - oy) * (py - oy)) as f32).sqrt();
            if d >= inner && d <= outer {
                canvas.fill_rect(px, py, 1, 1, color);
            }
        }
    }
}
