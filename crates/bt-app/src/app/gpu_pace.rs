//! GPU 길의 프레임 간격 판단(ADR-038). 창·시계 없이 시험하려고 순수한 계산만 둔다.
//!
//! - `plan_frame`: 이번 `redraw`를 그릴지, 가려진 창이라 건너뛸지, 그림판 얻기가 막힐 것 같아 미룰지
//! - `layer_key`: 층(아래·위)을 다시 그리지 않아도 되는지 알려 주는 값. 앞 프레임과 같으면 층을 그리지도 올리지도 않는다

use std::time::Duration;

use bt_gpu::pacing::PaceDecision;

/// 이번 `redraw`를 어떻게 할 것인가
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum FramePlan {
    /// 그려서 올린다
    Draw,
    /// 가려진 창이다. 아무것도 그리지 않고, 앞 프레임 기억을 지워 보이게 될 때 층을 통째로 다시 그리게 한다
    SkipOccluded,
    /// 그림판 얻기가 막힐 것 같다. 아무것도 쓰지 않고 이만큼 뒤에 다시 그린다
    Defer(Duration),
}

/// 가려졌으면 건너뛰고(미루지도 않는다: 보이게 되면 `Occluded(false)`가 다시 그리게 한다), 아니면 간격 판단을 따른다
pub(super) fn plan_frame(occluded: bool, pace: PaceDecision) -> FramePlan {
    if occluded {
        return FramePlan::SkipOccluded;
    }
    match pace {
        PaceDecision::Go => FramePlan::Draw,
        PaceDecision::Wait(d) => FramePlan::Defer(d),
    }
}

/// GPU 길 로그에 남기는 누적 수
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(super) struct PaceStats {
    /// 그림판 얻기가 막힐 것 같아 미룬 프레임
    pub deferred: u64,
    /// 창이 가려져 그리지 않고 건너뛴 프레임
    pub occluded: u64,
    /// 층 입력이 앞 프레임과 같아 층을 그리지도 견주지도 않은 프레임
    pub compare_skipped: u64,
}

/// 칸 하나가 층에 남기는 것 가운데 `Chrome`(칸 자리·크기·바탕)에 들지 않은 것
#[derive(Clone, PartialEq)]
pub(super) struct ViewLayer {
    /// 스크롤 막대를 그리는가
    pub bar_shown: bool,
    /// 막대 위에 마우스가 있는가(굵기·밝기가 달라진다)
    pub bar_hover: bool,
    pub history: usize,
    pub display_offset: usize,
    /// 진행 막대 값(종류, 퍼센트)
    pub progress: Option<(u8, u8)>,
    /// 벨 테두리를 두르는가(#687)
    pub bell_border: bool,
    /// 입력 함께 보내기 묶음의 테두리 색(#860)
    pub broadcast: Option<bt_core::Rgb>,
}

/// 층을 그리는 입력 가운데 `Chrome`에 들지 않은 모두. `layer_key`가 빠짐없이 쓰도록 필드를 하나도 `..`로 넘기지 않는다
pub(super) struct LayerInputs<'a> {
    pub opacity: f32,
    pub win_bg: bt_core::Rgb,
    /// 제목 막대를 칠하는 색(고른 칸 화면의 것)
    pub chrome_bg: Option<bt_core::Rgb>,
    /// 설정을 다시 읽으면 바뀐다(`Chrome`은 바탕·글자색만 든다)
    pub theme: &'a bt_core::Theme,
    pub top_inset: u32,
    pub scale: f64,
    pub single_tab: bool,
    pub views: &'a [ViewLayer],
    // 아래는 `layer_key`가 값을 헤아리지 않는 덧그림이다. 하나라도 있으면 같다고 말하지 못한다
    pub toast: bool,
    pub secure_badge: bool,
    pub hover_hint: bool,
    pub search: bool,
    pub palette: bool,
    pub title_prompt: bool,
    pub confirm: bool,
    pub resize_overlay: bool,
    pub images: bool,
}

/// 층의 내용을 정하는 값. 같으면 층 픽셀도 같다
#[derive(Clone, PartialEq)]
pub(super) struct LayerKey {
    opacity_bits: u32,
    win_bg: bt_core::Rgb,
    chrome_bg: Option<bt_core::Rgb>,
    theme: bt_core::Theme,
    top_inset: u32,
    scale_bits: u64,
    single_tab: bool,
    views: Vec<ViewLayer>,
}

/// 층 입력의 값. 값을 헤아리지 않는 덧그림이 있으면 `None`(같다고 말하지 않는다: 잔상을 남기느니 견주는 쪽이 낫다)
pub(super) fn layer_key(i: &LayerInputs<'_>) -> Option<LayerKey> {
    let LayerInputs {
        opacity,
        win_bg,
        chrome_bg,
        theme,
        top_inset,
        scale,
        single_tab,
        views,
        toast,
        secure_badge,
        hover_hint,
        search,
        palette,
        title_prompt,
        confirm,
        resize_overlay,
        images,
    } = i;
    if [
        toast,
        secure_badge,
        hover_hint,
        search,
        palette,
        title_prompt,
        confirm,
        resize_overlay,
        images,
    ]
    .into_iter()
    .any(|on| *on)
    {
        return None;
    }
    Some(LayerKey {
        opacity_bits: opacity.to_bits(),
        win_bg: *win_bg,
        chrome_bg: *chrome_bg,
        theme: (*theme).clone(),
        top_inset: *top_inset,
        scale_bits: scale.to_bits(),
        single_tab: *single_tab,
        views: views
            .iter()
            .map(|v| ViewLayer {
                // 그리지 않는 것은 값을 지운다: 기록 줄 수 같은 것이 바뀌어도 층은 그대로다
                bar_shown: v.bar_shown,
                bar_hover: v.bar_shown && v.bar_hover,
                history: if v.bar_shown { v.history } else { 0 },
                display_offset: if v.bar_shown { v.display_offset } else { 0 },
                progress: v.progress.filter(|_| *single_tab),
                bell_border: v.bell_border,
                broadcast: v.broadcast,
            })
            .collect(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use bt_core::Rgb;

    const GO: PaceDecision = PaceDecision::Go;
    const WAIT: PaceDecision = PaceDecision::Wait(Duration::from_millis(7));

    #[test]
    fn a_visible_window_draws_unless_the_queue_is_full() {
        assert_eq!(plan_frame(false, GO), FramePlan::Draw);
        assert_eq!(plan_frame(false, WAIT), FramePlan::Defer(Duration::from_millis(7)));
    }

    #[test]
    fn an_occluded_window_skips_whatever_the_pace_says() {
        assert_eq!(plan_frame(true, GO), FramePlan::SkipOccluded);
        assert_eq!(plan_frame(true, WAIT), FramePlan::SkipOccluded);
    }

    #[test]
    fn occluded_then_visible_draws_again() {
        // 가려짐 → 보임 순서: 가려진 동안 건너뛰고, 보이게 되면 그린다
        let seq = [true, true, false];
        let plans: Vec<_> = seq.iter().map(|o| plan_frame(*o, GO)).collect();
        assert_eq!(
            plans,
            [FramePlan::SkipOccluded, FramePlan::SkipOccluded, FramePlan::Draw]
        );
    }

    fn view() -> ViewLayer {
        ViewLayer {
            bar_shown: false,
            bar_hover: false,
            history: 0,
            display_offset: 0,
            progress: Some((1, 40)),
            bell_border: false,
            broadcast: None,
        }
    }

    static OTHER_THEME: std::sync::LazyLock<bt_core::Theme> = std::sync::LazyLock::new(|| bt_core::Theme {
        foreground: Rgb { r: 7, g: 7, b: 7 },
        ..bt_core::Theme::default()
    });
    static THEME: std::sync::LazyLock<bt_core::Theme> = std::sync::LazyLock::new(bt_core::Theme::default);

    fn inputs(views: &[ViewLayer]) -> LayerInputs<'_> {
        LayerInputs {
            opacity: 1.0,
            win_bg: Rgb { r: 1, g: 2, b: 3 },
            chrome_bg: Some(Rgb { r: 4, g: 5, b: 6 }),
            theme: &THEME,
            top_inset: 56,
            scale: 2.0,
            single_tab: true,
            views,
            toast: false,
            secure_badge: false,
            hover_hint: false,
            search: false,
            palette: false,
            title_prompt: false,
            confirm: false,
            resize_overlay: false,
            images: false,
        }
    }

    #[test]
    fn same_inputs_give_equal_keys() {
        let v = [view()];
        let a = layer_key(&inputs(&v)).expect("진행 막대만 켜진 프레임은 키가 있다");
        let b = layer_key(&inputs(&v)).expect("키가 있다");
        assert!(a == b);
    }

    #[test]
    fn a_changed_progress_value_changes_the_key() {
        let (v1, mut v2) = ([view()], [view()]);
        v2[0].progress = Some((1, 41));
        assert!(layer_key(&inputs(&v1)) != layer_key(&inputs(&v2)));
    }

    #[test]
    fn progress_is_ignored_when_it_is_not_drawn() {
        // 탭 막대가 있으면 진행 표시는 탭 글에 들고(`Chrome`), 칸 위에는 그리지 않는다
        let (v1, mut v2) = ([view()], [view()]);
        v2[0].progress = Some((1, 41));
        let (mut a, mut b) = (inputs(&v1), inputs(&v2));
        a.single_tab = false;
        b.single_tab = false;
        assert!(layer_key(&a) == layer_key(&b));
        assert!(layer_key(&a).is_some());
    }

    #[test]
    fn scrollbar_state_changes_the_key_only_while_the_bar_is_drawn() {
        let mut shown = [view()];
        shown[0].bar_shown = true;
        let mut moved = shown.clone();
        moved[0].display_offset = 3;
        assert!(layer_key(&inputs(&shown)) != layer_key(&inputs(&moved)));
        let mut hover = shown.clone();
        hover[0].bar_hover = true;
        assert!(layer_key(&inputs(&shown)) != layer_key(&inputs(&hover)));
        // 막대를 그리지 않는 동안 쌓이는 기록 줄 수는 층을 바꾸지 못한다
        let hidden = [view()];
        let mut hidden_more = hidden.clone();
        hidden_more[0].history = 500;
        assert!(layer_key(&inputs(&hidden)) == layer_key(&inputs(&hidden_more)));
    }

    #[test]
    fn window_level_inputs_change_the_key() {
        let v = [view()];
        let base = layer_key(&inputs(&v));
        assert!(base.is_some());
        let tweaks: [fn(&mut LayerInputs<'_>); 7] = [
            |i| i.opacity = 0.9,
            |i| i.win_bg = Rgb { r: 9, g: 2, b: 3 },
            |i| i.top_inset = 28,
            |i| i.scale = 1.0,
            |i| i.single_tab = false,
            |i| i.chrome_bg = None,
            |i| i.theme = &OTHER_THEME,
        ];
        for (n, tweak) in tweaks.iter().enumerate() {
            let mut i = inputs(&v);
            tweak(&mut i);
            assert!(layer_key(&i) != base, "{n}번째 입력이 바뀌어도 키가 같다(잔상)");
        }
    }

    #[test]
    fn overlays_the_key_does_not_count_never_claim_sameness() {
        let v = [view()];
        let flags: [fn(&mut LayerInputs<'_>); 9] = [
            |i| i.toast = true,
            |i| i.secure_badge = true,
            |i| i.hover_hint = true,
            |i| i.search = true,
            |i| i.palette = true,
            |i| i.title_prompt = true,
            |i| i.confirm = true,
            |i| i.resize_overlay = true,
            |i| i.images = true,
        ];
        for (n, set) in flags.iter().enumerate() {
            let mut i = inputs(&v);
            set(&mut i);
            assert!(layer_key(&i).is_none(), "{n}번째 덧그림이 켜졌는데 같다고 한다(잔상)");
        }
    }
}
