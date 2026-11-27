//! 창 여백(#136): 남는 자리 나누기(Ghostty `window-padding-balance`)와
//! 여백 색(Ghostty `window-padding-color`). 창과 상관없는 순수 함수라 여기서 테스트한다

use bt_core::Rgb;
use bt_core::config::{PaddingBalance, PaddingColor};
use bt_render::DrawCell;

use crate::split::Rect;

/// 칸의 바깥 사각형 `r`에서 여백 `(왼쪽, 오른쪽, 위, 아래)`(픽셀)를 빼고 남는 자리를 나눈 격자 사각형
/// (Ghostty renderer/size.zig `Size.balancePadding`). 격자 칸 수는 여백을 뺀 자리로 정하고, 나눌 때는 여백까지 합친
/// 바깥 사각형 전체의 남는 자리를 반씩 나눈다. `True`는 위를 (왼쪽 + 오른쪽 + 칸 너비) / 2까지로 묶고 넘는 것을 아래로 보낸다
pub fn balance_in(r: Rect, pad: (i32, i32, i32, i32), cw: u32, ch: u32, mode: PaddingBalance) -> Rect {
    let (left, right, top, bottom) = pad;
    let inner = Rect::new(
        r.x + left,
        r.y + top,
        (r.w as i32 - left - right).max(0) as u32,
        (r.h as i32 - top - bottom).max(0) as u32,
    );
    if mode == PaddingBalance::False || cw == 0 || ch == 0 {
        return inner;
    }
    let (gw, gh) = ((inner.w / cw) * cw, (inner.h / ch) * ch);
    let pad_x = r.w.saturating_sub(gw) / 2;
    let mut pad_top = r.h.saturating_sub(gh) / 2;
    if mode == PaddingBalance::True {
        let max_top = (left.max(0) as u32 + right.max(0) as u32 + cw) / 2;
        pad_top = pad_top.min(max_top);
    }
    Rect::new(r.x + pad_x as i32, r.y + pad_top as i32, gw, gh)
}

/// 여백을 칠할 색들. 없는 쪽(`None`)은 기본 바탕색
#[derive(Debug, Clone, PartialEq)]
pub struct PadPaint {
    /// 위·아래 여백: 열마다 색(가장 가까운 줄의 바탕색). 늘이지 않으면 없다
    pub top: Option<Vec<Rgb>>,
    pub bottom: Option<Vec<Rgb>>,
    /// 왼쪽·오른쪽 여백: 줄마다 색(그 줄 가장자리 칸의 바탕색)
    pub left: Vec<Rgb>,
    pub right: Vec<Rgb>,
}

/// powerline의 꼭 맞춤 글자(칸 끝까지 칠하는 삼각형·반원). 이런 줄은 위아래로 늘이지 않는다(Ghostty와 같은 규칙)
fn perfect_fit_powerline(c: char) -> bool {
    matches!(c as u32, 0xE0B0..=0xE0BF | 0xE0D2 | 0xE0D4)
}

/// 여백 색 계산. `alt`: 대체 화면(전체 화면 프로그램), `prompt_rows`: 보이는 프롬프트 줄들.
/// `extend`는 주 화면에서 가장 가까운 줄에 기본 바탕색 칸·프롬프트·powerline 꼭 맞춤 글자가 있으면 위아래로 늘이지 않는다
pub fn pad_paint(
    mode: PaddingColor,
    cells: &[DrawCell],
    cols: usize,
    rows: usize,
    default_bg: Rgb,
    alt: bool,
    prompt_rows: &[usize],
) -> PadPaint {
    let bg_at = |r: usize, c: usize| cells.get(r * cols + c).map_or(default_bg, |x| x.bg);
    if mode == PaddingColor::Background || cols == 0 || rows == 0 {
        return PadPaint {
            top: None,
            bottom: None,
            left: vec![default_bg; rows],
            right: vec![default_bg; rows],
        };
    }
    let row_colors = |r: usize| (0..cols).map(|c| bg_at(r, c)).collect::<Vec<_>>();
    // 위아래로 늘여도 되는 줄인가
    let vertical_ok = |r: usize| {
        if mode == PaddingColor::ExtendAlways || alt {
            return true;
        }
        let row = &cells[r * cols..((r + 1) * cols).min(cells.len())];
        !prompt_rows.contains(&r)
            && row.iter().all(|x| x.bg != default_bg)
            && !row.iter().any(|x| perfect_fit_powerline(x.ch))
    };
    PadPaint {
        top: vertical_ok(0).then(|| row_colors(0)),
        bottom: vertical_ok(rows - 1).then(|| row_colors(rows - 1)),
        left: (0..rows).map(|r| bg_at(r, 0)).collect(),
        right: (0..rows).map(|r| bg_at(r, cols - 1)).collect(),
    }
}

/// 여백을 칠한다. `grid`는 격자가 차지하는 사각형, `rows`가 있으면 그 줄들의 왼쪽·오른쪽 띠만(위아래 띠는 칠하지 않음)
#[allow(clippy::too_many_arguments)]
pub fn paint(
    canvas: &mut bt_render::Canvas,
    outer: Rect,
    grid: Rect,
    cw: u32,
    ch: u32,
    pad: &PadPaint,
    default_bg: Rgb,
    rows: Option<&[bool]>,
) {
    let (left_w, top_h) = ((grid.x - outer.x).max(0) as u32, (grid.y - outer.y).max(0) as u32);
    let right_x = grid.x + grid.w as i32;
    let right_w = (outer.x + outer.w as i32 - right_x).max(0) as u32;
    let bottom_y = grid.y + grid.h as i32;
    let bottom_h = (outer.y + outer.h as i32 - bottom_y).max(0) as u32;
    for (r, (&l, &rt)) in pad.left.iter().zip(&pad.right).enumerate() {
        if rows.is_some_and(|d| !d.get(r).copied().unwrap_or(false)) {
            continue;
        }
        let y = grid.y + (r as u32 * ch) as i32;
        canvas.fill_rect(outer.x, y, left_w, ch, l);
        canvas.fill_rect(right_x, y, right_w, ch, rt);
    }
    if rows.is_some() {
        return;
    }
    // 위·아래 띠: 열마다 가장 가까운 칸의 색, 모서리는 모서리 칸의 색. 늘이지 않으면 기본 바탕색
    let mut band = |y: i32, h: u32, colors: Option<&Vec<Rgb>>| match colors {
        Some(cs) if !cs.is_empty() => {
            canvas.fill_rect(outer.x, y, left_w, h, cs[0]);
            for (c, &col) in cs.iter().enumerate() {
                canvas.fill_rect(grid.x + (c as u32 * cw) as i32, y, cw, h, col);
            }
            canvas.fill_rect(right_x, y, right_w, h, cs[cs.len() - 1]);
        }
        _ => canvas.fill_rect(outer.x, y, outer.w, h, default_bg),
    };
    band(outer.y, top_h, pad.top.as_ref());
    band(bottom_y, bottom_h, pad.bottom.as_ref());
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cell(ch: char, bg: Rgb) -> DrawCell {
        DrawCell {
            ch,
            fg: Rgb::new(255, 255, 255),
            bg,
            bold: false,
            italic: false,
            underline: bt_render::Underline::None,
            strike: false,
            wide: false,
            spacer: false,
            selected: false,
            emoji: false,
            cluster: None,
            cluster_cells: 0,
            overline: false,
            underline_color: None,
            hidden: false,
            faint: None,
            mark: None,
        }
    }

    #[test]
    fn balance_modes_match_ghostty_size_zig() {
        use bt_core::config::PaddingBalance;
        // Ghostty size.zig "equal distributes whitespace equally": 화면 1050×850, 칸 10×20, 여백 4씩
        // 격자 104열 42줄(1040×840), 남는 10·10 → 네 변 5
        let r = Rect::new(0, 0, 1050, 850);
        assert_eq!(
            balance_in(r, (4, 4, 4, 4), 10, 20, PaddingBalance::Equal),
            Rect::new(5, 5, 1040, 840)
        );
        // "true shifts excess top to bottom": 화면 1090×1070, 칸 20×40, 여백 0
        // 남는 10·30 → 왼쪽 5, 위는 (0+0+20)/2=10으로 묶고 나머지는 아래(20)
        let r = Rect::new(0, 0, 1090, 1070);
        assert_eq!(
            balance_in(r, (0, 0, 0, 0), 20, 40, PaddingBalance::True),
            Rect::new(5, 10, 1080, 1040)
        );
        // 같은 화면을 equal로: 위아래 15씩
        assert_eq!(
            balance_in(r, (0, 0, 0, 0), 20, 40, PaddingBalance::Equal),
            Rect::new(5, 15, 1080, 1040)
        );
        // false: 여백만 뺀 자리(격자에 맞추지 않음)
        assert_eq!(
            balance_in(r, (3, 5, 7, 9), 20, 40, PaddingBalance::False),
            Rect::new(3, 7, 1082, 1054)
        );
        // 칸 밖으로 옮긴 사각형도 같은 계산
        let r = Rect::new(100, 30, 1090, 1070);
        assert_eq!(
            balance_in(r, (0, 0, 0, 0), 20, 40, PaddingBalance::True),
            Rect::new(105, 40, 1080, 1040)
        );
    }

    /// #889: 기본 여백(가로 10·세로 8, 2배 화면 20·16픽셀)에 `true`를 쓰면 남는 자리가 양옆에 고르게 나뉘고,
    /// 격자는 항상 창 안에 들어가며, 설정으로 2·false를 적으면 예전처럼 왼쪽 위에 붙는다
    #[test]
    fn default_padding_balances_leftover_and_old_values_still_hug_the_corner() {
        use bt_core::config::{Config, PaddingBalance};
        let d = Config::default();
        assert_eq!(d.padding_balance, PaddingBalance::True);
        let px = |pt: f32| (pt as f64 * 2.0).round() as i32;
        let pad = (
            px(d.padding_x.0),
            px(d.padding_x.1),
            px(d.padding_y.0),
            px(d.padding_y.1),
        );
        assert_eq!(pad, (20, 20, 16, 16));
        // 창 1000×700, 칸 17×34: 격자는 (960/17=56열=952, 668/34=19줄=646). 남는 48·54를 반씩
        let r = Rect::new(0, 0, 1000, 700);
        let g = balance_in(r, pad, 17, 34, d.padding_balance);
        assert_eq!((g.w, g.h), (952, 646));
        assert_eq!(g.x, 24);
        // 위는 (20+20+17)/2=28로 묶이고 27이므로 그대로
        assert_eq!(g.y, 27);
        assert!(g.x + g.w as i32 <= 1000 && g.y + g.h as i32 <= 700);
        // 양옆 남는 자리 차이는 1픽셀 이하
        assert!(((1000 - g.x - g.w as i32) - g.x).abs() <= 1);
        // 예전 모양(2pt, false): 격자가 여백 4픽셀 자리에 붙는다
        assert_eq!(
            balance_in(r, (4, 4, 4, 4), 17, 34, PaddingBalance::False),
            Rect::new(4, 4, 992, 692)
        );
        // 여백 0, false: 창 전체
        assert_eq!(balance_in(r, (0, 0, 0, 0), 17, 34, PaddingBalance::False), r);
    }

    #[test]
    fn extend_uses_edge_cells_but_not_vertically_over_default_or_prompt_rows() {
        let (bg, red, blue) = (Rgb::new(0, 0, 0), Rgb::new(200, 0, 0), Rgb::new(0, 0, 200));
        // 2열 3줄: 첫 줄 빨강 막대(상태줄), 가운데 기본, 마지막 줄 파랑
        let cells = vec![
            cell('a', red),
            cell('b', red),
            cell('x', bg),
            cell('y', bg),
            cell('c', blue),
            cell('d', blue),
        ];
        let p = pad_paint(PaddingColor::Extend, &cells, 2, 3, bg, false, &[]);
        assert_eq!(p.top, Some(vec![red, red]), "꽉 찬 색 줄은 위로 늘인다");
        assert_eq!(p.bottom, Some(vec![blue, blue]));
        assert_eq!(p.left, vec![red, bg, blue]);
        assert_eq!(p.right, vec![red, bg, blue]);
        // 기본 바탕색 칸이 있는 줄은 위아래로 늘이지 않는다(왼쪽·오른쪽은 늘 늘인다)
        let mut half = cells.clone();
        half[1] = cell('b', bg);
        let p = pad_paint(PaddingColor::Extend, &half, 2, 3, bg, false, &[]);
        assert_eq!(p.top, None);
        // 프롬프트 줄, powerline 꼭 맞춤 글자도
        assert_eq!(
            pad_paint(PaddingColor::Extend, &cells, 2, 3, bg, false, &[2]).bottom,
            None
        );
        let mut pl = cells.clone();
        pl[1] = cell('\u{E0B0}', red);
        assert_eq!(pad_paint(PaddingColor::Extend, &pl, 2, 3, bg, false, &[]).top, None);
        // 대체 화면(vim 등)과 extend-always는 따지지 않고 늘인다
        assert_eq!(
            pad_paint(PaddingColor::Extend, &half, 2, 3, bg, true, &[]).top,
            Some(vec![red, bg])
        );
        assert_eq!(
            pad_paint(PaddingColor::ExtendAlways, &half, 2, 3, bg, false, &[0]).top,
            Some(vec![red, bg])
        );
        // background면 모두 기본 바탕색
        let p = pad_paint(PaddingColor::Background, &cells, 2, 3, bg, true, &[]);
        assert_eq!((p.top, p.left), (None, vec![bg; 3]));
    }
}
