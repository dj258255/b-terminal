//! 창·탭 동작(#206)의 판단 부분: `move_tab`, `goto_window`, `reset_window_size`, `toggle_visibility`.
//! 창을 띄우지 않고 테스트하려고 순수 함수로 뺐다

/// `len`칸 목록에서 `cur`에서 `delta`만큼 가고 끝을 넘으면 돌아간 자리(Ghostty `move_tab`의 "wrapped around cyclically")
pub fn wrap_index(cur: usize, len: usize, delta: isize) -> usize {
    if len == 0 {
        return 0;
    }
    (cur as i128 + delta as i128).rem_euclid(len as i128) as usize
}

/// 탭을 새 창으로 떼어 낼 수 있는가. 탭이 하나뿐이면 떼어 낼 것이 없다(창이 그대로 남는다)
pub fn can_detach_tab(tabs: usize) -> bool {
    tabs >= 2
}

/// `goto_window`가 갈 창. `serials`는 창을 만든 차례 번호(순서가 뒤섞여 있어도 된다), `current`는 지금 창.
/// 만든 차례대로 `delta`만큼 가고 끝에서 돌아간다. 갈 다른 창이 없으면 `None`
pub fn pick_window(serials: &[u64], current: u64, delta: isize) -> Option<u64> {
    let mut v: Vec<u64> = serials.to_vec();
    v.sort_unstable();
    v.dedup();
    if v.iter().all(|&s| s == current) {
        return None;
    }
    match v.iter().position(|&s| s == current) {
        Some(i) => Some(v[wrap_index(i, v.len(), delta)]),
        // 지금 창이 목록에 없으면(퀵 창 등) 다음은 처음, 앞은 마지막
        None if delta >= 0 => v.first().copied(),
        None => v.last().copied(),
    }
}

/// `reset_window_size`가 만들 안쪽 크기(물리 픽셀). 설정의 `window-width`·`window-height`(칸)가 둘 다 있으면
/// 칸 크기와 여백으로 구하고, 없으면 새 창의 기본 안쪽 크기(논리 `default_logical`)
pub fn window_inner_px(
    cells: Option<(u32, u32)>,
    cell: (u32, u32),
    pad: (u32, u32),
    scale: f64,
    default_logical: (f64, f64),
) -> (u32, u32) {
    match cells {
        Some((cols, rows)) => (cell.0 * cols + pad.0, cell.1 * rows + pad.1),
        None => (
            (default_logical.0 * scale).round() as u32,
            (default_logical.1 * scale).round() as u32,
        ),
    }
}

/// `toggle_visibility`가 할 일
#[cfg_attr(not(target_os = "macos"), allow(dead_code))]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Visibility {
    Hide,
    Show,
}

/// 숨겨져 있으면 보이고, 보이면 숨긴다. 보고 있는 칸이 전체 화면이면 아무것도 하지 않는다(Ghostty 주석).
/// 숨은 동안에는 보던 칸을 알 수 없으므로 전체 화면 검사는 숨기기 전에만 한다
#[cfg_attr(not(target_os = "macos"), allow(dead_code))]
pub fn visibility_plan(app_hidden: bool, focused_fullscreen: bool) -> Option<Visibility> {
    match (app_hidden, focused_fullscreen) {
        (true, _) => Some(Visibility::Show),
        (false, true) => None,
        (false, false) => Some(Visibility::Hide),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wrap_index_goes_around_both_ways() {
        assert_eq!(wrap_index(0, 3, 1), 1);
        assert_eq!(wrap_index(2, 3, 1), 0, "마지막에서 앞으로 한 칸이면 처음");
        assert_eq!(wrap_index(0, 3, -1), 2, "처음에서 뒤로 한 칸이면 마지막");
        assert_eq!(wrap_index(1, 3, 7), 2);
        assert_eq!(wrap_index(1, 3, -7), 0);
        assert_eq!(wrap_index(1, 3, 3), 1, "한 바퀴면 제자리");
        assert_eq!(wrap_index(0, 0, 5), 0);
        assert_eq!(wrap_index(0, 4, isize::MIN), 0, "아주 큰 값에도 넘치지 않는다");
    }

    #[test]
    fn tab_detach_needs_two_tabs() {
        assert!(!can_detach_tab(0));
        assert!(!can_detach_tab(1));
        assert!(can_detach_tab(2));
    }

    #[test]
    fn pick_window_cycles_in_creation_order() {
        // 바꿔 끼우기로 목록 순서가 뒤섞여도 만든 차례를 따른다
        let order = [7, 3, 5];
        assert_eq!(pick_window(&order, 3, 1), Some(5));
        assert_eq!(pick_window(&order, 5, 1), Some(7));
        assert_eq!(pick_window(&order, 7, 1), Some(3), "끝에서 돌아간다");
        assert_eq!(pick_window(&order, 3, -1), Some(7));
        assert_eq!(pick_window(&order, 5, -1), Some(3));
    }

    #[test]
    fn pick_window_needs_another_window() {
        assert_eq!(pick_window(&[4], 4, 1), None);
        assert_eq!(pick_window(&[], 4, 1), None);
        // 지금 창이 목록에 없으면(퀵 창) 처음·마지막으로
        assert_eq!(pick_window(&[2, 9], 100, 1), Some(2));
        assert_eq!(pick_window(&[2, 9], 100, -1), Some(9));
    }

    #[test]
    fn window_inner_size_uses_cells_or_default() {
        assert_eq!(
            window_inner_px(Some((80, 24)), (10, 20), (8, 6), 2.0, (1000.0, 680.0)),
            (808, 486)
        );
        assert_eq!(
            window_inner_px(None, (10, 20), (8, 6), 2.0, (1000.0, 680.0)),
            (2000, 1360)
        );
        assert_eq!(
            window_inner_px(None, (10, 20), (8, 6), 1.0, (1000.0, 680.0)),
            (1000, 680)
        );
    }

    #[test]
    fn visibility_toggles_and_skips_fullscreen() {
        assert_eq!(visibility_plan(false, false), Some(Visibility::Hide));
        assert_eq!(visibility_plan(true, false), Some(Visibility::Show));
        assert_eq!(
            visibility_plan(false, true),
            None,
            "전체 화면인 칸에서는 아무것도 하지 않는다"
        );
        assert_eq!(
            visibility_plan(true, true),
            Some(Visibility::Show),
            "숨은 뒤에는 되살린다"
        );
    }
}
