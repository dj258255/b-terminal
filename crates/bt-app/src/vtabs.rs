//! 세로 탭 줄의 자리 계산(#935, kitty 0.48 `tab_bar_edge = left | right`). 창 없이 시험할 수 있게 숫자만 다룬다.
//! 너비는 kitty `vertical_tab_bar_cols`(`kitty/state.c` 809~823줄)를 옮겼고, 탭을 쌓는 규칙은 kitty `update_vertical`
//! (`kitty/tab_bar.py` 1218~1320줄)과 `tab_id_at`(1363~1381줄)을 옮겼다.

use bt_core::tabstyle::{TabBarAlign, TabBarLook, TabBarStyle};

/// 제목 길이를 정하지 않았을 때 kitty가 바라는 세로 탭 줄의 칸 수(`state.c` 818줄)
pub const DEFAULT_COLS: u32 = 28;

/// 탭 하나 안에서 제목 양옆에 남기는 칸 수(`tabbar::draw_placed`가 제목 방을 `안쪽 폭 - 4칸`으로 잡는다)
pub const TITLE_MARGIN_CELLS: u32 = 4;

/// 이 탭 줄 모양이 탭 안쪽 가장자리에 쓰는 칸 수(번짐·기울임·파워라인 기호). `tabbar::content_rect`가 안쪽 폭에서 빼는 칸이다
/// (kitty는 `vertical_tab_bar.padding`을 스타일마다 정해 두고, 여기는 이쪽 그리기가 실제로 쓰는 칸을 센다)
pub fn decoration_cells(look: &TabBarLook) -> u32 {
    match look.style {
        TabBarStyle::Default | TabBarStyle::Hidden | TabBarStyle::Separator => 0,
        TabBarStyle::Fade => 2 * look.fade.len() as u32 + 1,
        TabBarStyle::Slant => 2,
        TabBarStyle::Powerline => 1,
    }
}

/// 바라는 칸 수: `tab-title-max-length`가 0이면 28칸, 아니면 그 길이의 제목이 잘리지 않을 만큼
/// (kitty `state.c` 818~819줄. 제목 칸 + 양옆 여유 + 모양이 쓰는 칸)
pub fn desired_cols(look: &TabBarLook, max_length: u32) -> u32 {
    if max_length == 0 {
        DEFAULT_COLS
    } else {
        (max_length + TITLE_MARGIN_CELLS + decoration_cells(look)).max(1)
    }
}

/// 세로 탭 줄의 칸 수(kitty `vertical_tab_bar_cols`). `avail_px`는 창 폭에서 바깥·안 여백을 뺀 것. 자리가 없으면 0.
/// 창 폭의 3분의 1(칸 수로 6칸 미만이면 자리 전부)을 넘지 않는다
pub fn bar_cols(avail_px: i64, cell_w: u32, desired: u32) -> u32 {
    if avail_px <= 0 {
        return 0;
    }
    let avail_cols = ((avail_px as u64 / cell_w.max(1) as u64).min(u32::MAX as u64) as u32).max(1);
    let mut soft_max = avail_cols / 3;
    if soft_max < 6 {
        soft_max = avail_cols;
    }
    avail_cols.min(desired.min(soft_max.max(1))).max(1)
}

/// 탭 사이 빈 줄의 높이(px). kitty는 탭 한 줄(셀 높이)과 같은 빈 줄을 두지만 여기 탭은 셀의 1.5배라 그 절반으로 둔다
pub fn gap_px(row_h: u32) -> u32 {
    row_h / 2
}

/// 세로 줄에 쌓인 탭들의 자리(탭 줄 맨 위에서 잰 y)
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Rows {
    /// 그려지는 탭마다 (y, 높이). 앞에서부터 차례로 들어가는 만큼만 있다
    pub tabs: Vec<(i32, u32)>,
    /// 탭 사이의 빈 줄 높이(자리가 모자라면 0). 빈 줄은 위 탭의 것이다
    pub gap: u32,
    /// 다 들어가지 않을 때 `…`을 그리는 줄의 y
    pub ellipsis: Option<i32>,
}

/// 높이 `avail`에 탭이 최대 몇 개 들어가나: (개수, 쓴 높이)
fn fit(count: usize, avail: u32, row_h: u32, gap: u32) -> (usize, u32) {
    let (mut n, mut total) = (0usize, 0u32);
    while n < count {
        let need = row_h + if n > 0 { gap } else { 0 };
        if total.saturating_add(need) > avail {
            break;
        }
        total += need;
        n += 1;
    }
    (n, total)
}

/// `count`개 탭을 높이 `avail`에 쌓는다(kitty `update_vertical`): 다 들어가면 탭 사이에 `gap`만큼 띄우고,
/// 안 되면 띄우지 않고, 그래도 모자라면 `…` 한 줄을 남기고 앞의 탭만 그린다. 놓는 쪽은 `align`
pub fn rows(count: usize, avail: u32, row_h: u32, gap: u32, align: TabBarAlign) -> Rows {
    if count == 0 {
        return Rows::default();
    }
    let row_h = row_h.max(1);
    let mut spacing = gap;
    let (mut n, mut total) = fit(count, avail, row_h, spacing);
    if n < count {
        spacing = 0;
        (n, total) = fit(count, avail, row_h, spacing);
    }
    // `…` 줄: 줄이 둘 이상이고 한 줄을 남겨도 탭이 하나는 들어갈 때만(아니면 탭을 우선한다)
    let mut ellipsis = n < count && avail > row_h;
    if ellipsis {
        let (n2, total2) = fit(count, avail - row_h, row_h, spacing);
        if n2 > 0 {
            (n, total) = (n2, total2);
        } else {
            ellipsis = false;
        }
    }
    let used = total + if ellipsis { row_h } else { 0 };
    let start = match align {
        TabBarAlign::Left => 0,
        TabBarAlign::Center => avail.saturating_sub(used) / 2,
        TabBarAlign::Right => avail.saturating_sub(used),
    } as i32;
    let tabs = (0..n)
        .map(|i| (start + (i as u32 * (row_h + spacing)) as i32, row_h))
        .collect();
    Rows {
        tabs,
        gap: spacing,
        ellipsis: ellipsis.then_some(start + total as i32),
    }
}

/// `y`(탭 줄 맨 위에서 잰 값)의 탭 번호. 탭 아래 빈 줄도 그 탭이다(kitty `tab_id_at`)
pub fn tab_at(rows: &Rows, y: f64) -> Option<usize> {
    rows.tabs
        .iter()
        .position(|&(ty, h)| y >= ty as f64 && y < (ty + h as i32 + rows.gap as i32) as f64)
}

/// 끌어서 놓은 자리의 탭 번호: 맨 위보다 위면 처음, 탭이 아닌 곳(아래·`…`)이면 끝 탭
pub fn drop_index(rows: &Rows, count: usize, y: f64) -> Option<usize> {
    if count == 0 {
        return None;
    }
    if rows.tabs.first().is_some_and(|&(ty, _)| y < ty as f64) {
        return Some(0);
    }
    tab_at(rows, y).or(Some(count - 1))
}

#[cfg(test)]
mod tests {
    use super::*;
    use bt_core::tabstyle::{DEFAULT_FADE, TabBarStyle};

    fn look(style: TabBarStyle) -> TabBarLook {
        TabBarLook {
            style,
            ..TabBarLook::default()
        }
    }

    #[test]
    fn width_follows_kitty_vertical_tab_bar_cols() {
        // 기본 28칸. 창이 넓으면 28칸이 3분의 1 상한(soft_max) 안이다
        assert_eq!(bar_cols(2000, 10, 28), 28);
        // 3분의 1 상한: 200칸이면 66칸까지, 90칸이면 30칸까지
        assert_eq!(bar_cols(900, 10, 28), 28);
        assert_eq!(bar_cols(600, 10, 28), 20, "60칸의 3분의 1");
        // soft_max가 6보다 작으면(창이 18칸 미만) 상한을 쓰지 않고 자리 전부까지
        assert_eq!(bar_cols(170, 10, 28), 17);
        assert_eq!(bar_cols(50, 10, 28), 5);
        // 자리가 없거나 칸 폭이 0이어도 패닉이 없고 0 또는 1칸
        assert_eq!(bar_cols(0, 10, 28), 0);
        assert_eq!(bar_cols(-5, 10, 28), 0);
        assert_eq!(bar_cols(9, 10, 28), 1, "한 칸 미만이어도 1칸(kitty는 MAX(1, ...))");
        assert_eq!(bar_cols(100, 0, 28), 28, "칸 폭 0은 1로 센다");
        // 바라는 칸 수가 작으면 그만큼
        assert_eq!(bar_cols(2000, 10, 12), 12);
        assert_eq!(bar_cols(2000, 10, 0), 1, "0칸을 바라도 1칸");
    }

    #[test]
    fn desired_cols_fits_the_title_length_with_the_style_decorations() {
        let d = look(TabBarStyle::Default);
        assert_eq!(desired_cols(&d, 0), DEFAULT_COLS);
        assert_eq!(decoration_cells(&d), 0);
        assert_eq!(desired_cols(&d, 20), 20 + TITLE_MARGIN_CELLS);
        // 번짐은 양옆 번짐 칸과 끝 한 칸
        let fade = look(TabBarStyle::Fade);
        let n = DEFAULT_FADE.len() as u32;
        assert_eq!(decoration_cells(&fade), 2 * n + 1);
        assert_eq!(desired_cols(&fade, 10), 10 + TITLE_MARGIN_CELLS + 2 * n + 1);
        assert_eq!(decoration_cells(&look(TabBarStyle::Slant)), 2);
        assert_eq!(decoration_cells(&look(TabBarStyle::Powerline)), 1);
        assert_eq!(decoration_cells(&look(TabBarStyle::Separator)), 0);
        // 길이를 정하면 바라는 칸이 늘 1 이상이고, 정하지 않으면 28칸(제목 방 24칸)이다
        assert!(desired_cols(&look(TabBarStyle::Hidden), 1) >= 1);
    }

    #[test]
    fn rows_leave_a_gap_while_everything_fits_and_drop_it_when_tight() {
        // 높이 10, 간격 5: 3개면 10*3+5*2 = 40
        let r = rows(3, 40, 10, 5, TabBarAlign::Left);
        assert_eq!(r.tabs, vec![(0, 10), (15, 10), (30, 10)]);
        assert_eq!((r.gap, r.ellipsis), (5, None));
        // 39면 간격이 안 들어가 0으로 붙인다(30 <= 39)
        let r = rows(3, 39, 10, 5, TabBarAlign::Left);
        assert_eq!(r.tabs, vec![(0, 10), (10, 10), (20, 10)]);
        assert_eq!((r.gap, r.ellipsis), (0, None));
    }

    #[test]
    fn rows_that_do_not_fit_end_with_an_ellipsis_row() {
        // 5개, 탭 높이 10, 방 35: 간격 없이 3개(30)가 들어가지만 `…` 한 줄을 남기려 2개 + `…`
        let r = rows(5, 35, 10, 5, TabBarAlign::Left);
        assert_eq!(r.tabs, vec![(0, 10), (10, 10)]);
        assert_eq!((r.gap, r.ellipsis), (0, Some(20)));
        // 높이 40: 간격 없이 4개(40)가 들어가지만 `…` 한 줄을 남기려 3개(30) + `…`
        let r = rows(5, 40, 10, 5, TabBarAlign::Left);
        assert_eq!(r.tabs.len(), 3);
        assert_eq!(r.ellipsis, Some(30));
        // 한 줄뿐이면 `…` 대신 탭을 보인다(kitty: 탭이 하나도 안 들어가면 탭을 우선)
        let r = rows(5, 10, 10, 5, TabBarAlign::Left);
        assert_eq!(r.tabs, vec![(0, 10)]);
        assert_eq!(r.ellipsis, None);
        // 높이 0, 탭 0개는 비어 있다
        assert_eq!(rows(0, 100, 10, 5, TabBarAlign::Left), Rows::default());
        assert!(rows(3, 0, 10, 5, TabBarAlign::Left).tabs.len() <= 1);
    }

    #[test]
    fn rows_align_start_center_end() {
        // 탭 2개(10) + 간격 5 = 25, 방 100
        let l = rows(2, 100, 10, 5, TabBarAlign::Left);
        let c = rows(2, 100, 10, 5, TabBarAlign::Center);
        let e = rows(2, 100, 10, 5, TabBarAlign::Right);
        assert_eq!(l.tabs[0].0, 0);
        assert_eq!(c.tabs[0].0, (100 - 25) / 2);
        assert_eq!(e.tabs[0].0, 100 - 25);
        assert_eq!(e.tabs[1].0, 100 - 10);
    }

    #[test]
    fn the_blank_row_below_a_tab_belongs_to_it() {
        let r = rows(3, 40, 10, 5, TabBarAlign::Left);
        assert_eq!(tab_at(&r, 0.0), Some(0));
        assert_eq!(tab_at(&r, 9.9), Some(0));
        assert_eq!(tab_at(&r, 10.0), Some(0), "탭 아래 빈 줄(kitty 10810)");
        assert_eq!(tab_at(&r, 14.9), Some(0));
        assert_eq!(tab_at(&r, 15.0), Some(1));
        assert_eq!(tab_at(&r, 39.9), Some(2));
        assert_eq!(tab_at(&r, 44.9), Some(2), "마지막 탭의 빈 줄도 kitty처럼 그 탭이다");
        assert_eq!(tab_at(&r, 45.0), None);
        assert_eq!(tab_at(&r, -1.0), None);
        // 간격이 0이면 빈 줄이 없다
        let r = rows(3, 30, 10, 5, TabBarAlign::Left);
        assert_eq!(tab_at(&r, 10.0), Some(1));
        // `…` 줄은 탭이 아니다
        let r = rows(5, 35, 10, 5, TabBarAlign::Left);
        assert_eq!(tab_at(&r, 20.0), None);
    }

    #[test]
    fn drop_clamps_to_the_ends() {
        let r = rows(3, 40, 10, 5, TabBarAlign::Center);
        assert_eq!(drop_index(&r, 3, -50.0), Some(0));
        assert_eq!(drop_index(&r, 3, 500.0), Some(2));
        assert_eq!(drop_index(&r, 3, 17.0), Some(1));
        // 숨은 탭이 있으면 `…` 줄 위·아래는 끝 탭(전체 기준)
        let r = rows(5, 35, 10, 5, TabBarAlign::Left);
        assert_eq!(drop_index(&r, 5, 25.0), Some(4));
        assert_eq!(drop_index(&r, 0, 5.0), None);
    }
}
