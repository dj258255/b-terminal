//! Dock 아이콘의 진행 막대와 벨 배지(#864).
//!
//! 판단은 이 파일의 순수 함수가 하고, 그리는 일(`NSDockTile`)은 `macos` 모듈이 한다.
//! - 진행 막대는 kitty처럼: 여러 칸이 진행 중이면 퍼센트가 있는 칸(보통·멈춤)의 평균을 하나로 그리고,
//!   그런 칸이 없고 끝 모름이 있으면 끝 모름 막대를 그린다(kitty `boss.py`의 `update_progress_in_dock`).
//!   오류만 있으면 그리지 않는다(kitty는 오류 칸을 어느 쪽에도 세지 않는다).
//! - 벨 배지는 Ghostty 1.4.0처럼 `bell-features`의 `attention`을 따르고(`AppDelegate.setDockBadge`), 벨이 울린 칸의
//!   수를 적는다(99를 넘으면 `99+`). 앱이 앞에 오면 지운다

/// Dock 아이콘에 그릴 진행 막대
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DockProgress {
    /// 그리지 않는다(아이콘 그대로)
    None,
    /// 끝 모름: 막대가 좌우로 오간다
    Indeterminate,
    /// 0~100 퍼센트
    Percent(u8),
}

/// 칸마다의 진행 표시(`tabbar::accept_progress`의 (상태, 퍼센트))를 Dock 막대 하나로 합친다.
/// 상태: 1 보통, 2 오류, 3 끝 모름, 4 멈춤(그 밖은 오류 쪽 코드이거나 모르는 값이라 세지 않는다)
pub fn merge_progress(items: impl IntoIterator<Item = (u8, u8)>) -> DockProgress {
    let (mut sum, mut n, mut indeterminate) = (0u32, 0u32, false);
    for (state, percent) in items {
        match state {
            1 | 4 => {
                sum += u32::from(percent.min(100));
                n += 1;
            }
            3 => indeterminate = true,
            _ => {}
        }
    }
    match (sum + n / 2).checked_div(n) {
        Some(avg) => DockProgress::Percent(avg as u8),
        None if indeterminate => DockProgress::Indeterminate,
        None => DockProgress::None,
    }
}

/// 배지에 적을 글. 벨이 울린 칸이 없으면 없음, 99를 넘으면 `99+`
pub fn badge_label(count: usize) -> Option<String> {
    match count {
        0 => None,
        1..=99 => Some(count.to_string()),
        _ => Some("99+".into()),
    }
}

/// 벨이 울렸을 때 배지에 칠 칸으로 셀지: `bell-features`에 `attention`이 있고 앱이 앞에 있지 않을 때
pub fn bell_counts_for_badge(attention: bool, app_active: bool) -> bool {
    attention && !app_active
}

/// 지금 배지에 적을 글: 앱이 앞이거나 `attention`이 꺼졌으면 없음
pub fn badge_for(attention: bool, app_active: bool, bell_panes: usize) -> Option<String> {
    if attention && !app_active {
        badge_label(bell_panes)
    } else {
        None
    }
}

/// 끝 모름 막대의 채움(0~100): 1.6초에 한 번 0에서 100까지 갔다가 돌아오는 삼각파.
/// `NSProgressIndicator`의 끝 모름 움직임은 `startAnimation`(objc2에서 `unsafe fn`)이 있어야 해서 쓰지 않고,
/// 퍼센트 막대를 우리가 흔들어 그린다. `elapsed_ms`는 끝 모름이 시작된 뒤 지난 시간
pub fn sweep_percent(elapsed_ms: u64) -> u8 {
    let phase = (elapsed_ms % 1600) as u32; // 0..1600
    let up = if phase < 800 { phase } else { 1600 - phase };
    (up * 100 / 800) as u8
}

/// 끝 모름 막대를 다시 그리는 간격
pub const SWEEP_FRAME: std::time::Duration = std::time::Duration::from_millis(80);

/// Dock 아이콘에 닿는 부분(`NSDockTile`). macOS 밖에서는 하는 일이 없다
#[cfg(target_os = "macos")]
pub mod tile {
    use super::DockProgress;
    use objc2::rc::Retained;
    use objc2_app_kit::{NSApplication, NSImageScaling, NSImageView, NSProgressIndicator, NSView};
    use objc2_foundation::{NSPoint, NSRect, NSSize, NSString};
    use std::cell::RefCell;

    /// 아이콘 위에 얹은 막대 뷰들(진행 중일 때만 있다)
    struct Bar {
        view: Retained<NSView>,
        indicator: Retained<NSProgressIndicator>,
        /// 마지막으로 그린 퍼센트(같으면 다시 그리지 않는다)
        shown: Option<u8>,
    }

    thread_local! {
        static BAR: RefCell<Option<Bar>> = const { RefCell::new(None) };
    }

    /// 진행 막대를 그린다. `sweep`는 끝 모름일 때 지금 채울 퍼센트(`sweep_percent`)
    pub fn set_progress(p: DockProgress, sweep: u8) {
        let Some(mtm) = objc2::MainThreadMarker::new() else {
            return;
        };
        let tile = NSApplication::sharedApplication(mtm).dockTile();
        let percent = match p {
            DockProgress::None => None,
            DockProgress::Indeterminate => Some(sweep),
            DockProgress::Percent(n) => Some(n),
        };
        BAR.with(|b| {
            let mut b = b.borrow_mut();
            let Some(percent) = percent else {
                if b.take().is_some() {
                    // 막대를 걷고 사용자 그림(있으면)이나 앱 꾸러미의 아이콘으로 돌아간다
                    match crate::icon::custom_image() {
                        Some(img) => {
                            let v = NSImageView::imageViewWithImage(&img, mtm);
                            v.setImageScaling(NSImageScaling::ScaleProportionallyUpOrDown);
                            tile.setContentView(Some(&v));
                        }
                        None => tile.setContentView(None),
                    }
                    tile.display();
                }
                return;
            };
            if b.is_none() {
                let size = tile.size();
                let full = NSRect::new(NSPoint::new(0.0, 0.0), size);
                let view = NSView::initWithFrame(mtm.alloc::<NSView>(), full);
                let icon = crate::icon::custom_image()
                    .or_else(|| NSApplication::sharedApplication(mtm).applicationIconImage());
                if let Some(icon) = icon {
                    let iv = NSImageView::imageViewWithImage(&icon, mtm);
                    iv.setImageScaling(NSImageScaling::ScaleProportionallyUpOrDown);
                    iv.setFrame(full);
                    view.addSubview(&iv);
                }
                // 막대: 아이콘 아래쪽 가운데, 폭 80%·높이 12%(kitty는 안쪽으로 4pt 들인 둥근 막대)
                let (w, h) = (size.width, size.height);
                let bar_frame = NSRect::new(
                    NSPoint::new(w * 0.1, h * 0.08),
                    NSSize::new(w * 0.8, (h * 0.12).max(8.0)),
                );
                let indicator = NSProgressIndicator::initWithFrame(mtm.alloc::<NSProgressIndicator>(), bar_frame);
                indicator.setIndeterminate(false);
                indicator.setMinValue(0.0);
                indicator.setMaxValue(100.0);
                view.addSubview(&indicator);
                tile.setContentView(Some(&view));
                *b = Some(Bar {
                    view,
                    indicator,
                    shown: None,
                });
            }
            let Some(bar) = b.as_mut() else { return };
            if bar.shown != Some(percent) {
                bar.indicator.setDoubleValue(f64::from(percent));
                bar.view.setNeedsDisplay(true);
                bar.shown = Some(percent);
                tile.display();
            }
        });
    }

    /// 막대를 올려 둔 뷰를 잊는다: 아이콘을 바꾸는 쪽(`icon::apply`)이 Dock의 내용 뷰를 바꿨을 때 부른다
    pub fn forget_bar() {
        BAR.with(|b| *b.borrow_mut() = None);
    }

    /// 배지를 정한다(`None`이면 지운다)
    pub fn set_badge(label: Option<&str>) {
        let Some(mtm) = objc2::MainThreadMarker::new() else {
            return;
        };
        let tile = NSApplication::sharedApplication(mtm).dockTile();
        let s = label.map(NSString::from_str);
        tile.setBadgeLabel(s.as_deref());
        tile.display();
    }
}

/// macOS가 아닌 곳은 Dock이 없다
#[cfg(not(target_os = "macos"))]
pub mod tile {
    use super::DockProgress;
    pub fn set_progress(_p: DockProgress, _sweep: u8) {}
    pub fn set_badge(_label: Option<&str>) {}
    pub fn forget_bar() {}
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn single_pane_follows_its_state() {
        assert_eq!(merge_progress([(1, 40)]), DockProgress::Percent(40));
        assert_eq!(
            merge_progress([(4, 70)]),
            DockProgress::Percent(70),
            "멈춤도 퍼센트를 센다"
        );
        assert_eq!(merge_progress([(3, 0)]), DockProgress::Indeterminate);
        assert_eq!(merge_progress([]), DockProgress::None);
    }

    #[test]
    fn several_panes_average_the_percents() {
        assert_eq!(merge_progress([(1, 20), (1, 60)]), DockProgress::Percent(40));
        assert_eq!(merge_progress([(1, 100), (4, 50), (1, 0)]), DockProgress::Percent(50));
        assert_eq!(merge_progress([(1, 33), (1, 34)]), DockProgress::Percent(34), "반올림");
    }

    #[test]
    fn percent_beats_indeterminate_and_errors_are_ignored() {
        assert_eq!(merge_progress([(3, 0), (1, 80)]), DockProgress::Percent(80));
        assert_eq!(
            merge_progress([(2, 0), (1, 80)]),
            DockProgress::Percent(80),
            "오류 칸은 평균에 끼지 않는다"
        );
        assert_eq!(merge_progress([(2, 0), (3, 0)]), DockProgress::Indeterminate);
        assert_eq!(
            merge_progress([(2, 55)]),
            DockProgress::None,
            "오류만 있으면 그리지 않는다(kitty)"
        );
        assert_eq!(merge_progress([(7, 0)]), DockProgress::None, "값 없는 오류도 같다");
        assert_eq!(
            merge_progress([(9, 50)]),
            DockProgress::None,
            "모르는 상태는 세지 않는다"
        );
    }

    #[test]
    fn percent_is_clamped_to_100() {
        assert_eq!(merge_progress([(1, 250)]), DockProgress::Percent(100));
    }

    #[test]
    fn indeterminate_sweeps_back_and_forth() {
        assert_eq!(sweep_percent(0), 0);
        assert_eq!(sweep_percent(400), 50);
        assert_eq!(sweep_percent(800), 100);
        assert_eq!(sweep_percent(1200), 50);
        assert_eq!(sweep_percent(1600), 0);
        assert_eq!(sweep_percent(1600 * 1000 + 800), 100);
    }

    #[test]
    fn badge_text_counts_bell_panes() {
        assert_eq!(badge_label(0), None);
        assert_eq!(badge_label(1).as_deref(), Some("1"));
        assert_eq!(badge_label(99).as_deref(), Some("99"));
        assert_eq!(badge_label(100).as_deref(), Some("99+"));
    }

    #[test]
    fn bell_badges_only_while_app_is_in_background() {
        assert!(bell_counts_for_badge(true, false));
        assert!(!bell_counts_for_badge(true, true), "앱이 앞이면 달지 않는다");
        assert!(!bell_counts_for_badge(false, false), "attention이 꺼지면 달지 않는다");
        assert!(!bell_counts_for_badge(false, true));
    }

    #[test]
    fn badge_clears_when_app_comes_forward_or_attention_is_off() {
        assert_eq!(badge_for(true, false, 2).as_deref(), Some("2"));
        assert_eq!(badge_for(true, true, 2), None, "앞으로 오면 지운다");
        assert_eq!(badge_for(false, false, 2), None, "설정으로 끄면 지운다");
        assert_eq!(badge_for(true, false, 0), None);
    }
}
