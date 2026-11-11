//! 칸마다 글자 크기(#422, Ghostty는 surface마다다): 창이 크기별로 글꼴 묶음을 한 벌씩 들고, 같은 크기의 칸끼리 나눠 쓴다.
//! 이 모듈은 창·글꼴 없이 시험할 수 있게 글꼴 묶음 자리를 제네릭(`T`)으로 둔다. 앱은 `T = Fonts`로 쓴다.
//!
//! 창 덧그림(탭 줄·알림 띠·확인 막대·찾기 막대 등)은 "기준 크기"의 묶음으로 그린다. 기준 크기는 그 창에서 가장 최근에
//! ⌘+·⌘-·⌘0·`set_font_size`로 바꾼 크기이고, 처음에는 새 창의 크기다. 고른 칸을 옮길 때마다 바뀌지 않으므로 크기가 다른
//! 칸 사이로 초점을 옮겨도 탭 줄 높이와 칸 배치가 흔들리지 않는다. 칸 안(격자·커서·조합 글·스크롤 막대·그림)은 그 칸의 크기다

use bt_core::Config;

/// 크기를 같은지 견줄 열쇠. 설정·키 입력으로 정한 값이라 비트가 같으면 같은 크기다(NaN은 들어오지 않는다: `clamp_font_size`)
pub fn size_key(size: f32) -> u32 {
    size.to_bits()
}

/// 창이 들고 있어야 할 글자 크기들: 기준 크기가 먼저이고 그다음 칸들의 크기(같은 것은 한 번, 처음 나온 차례)
pub fn needed_sizes(base: f32, panes: impl IntoIterator<Item = f32>) -> Vec<f32> {
    let mut out = vec![base];
    for s in panes {
        if !out.iter().any(|o| size_key(*o) == size_key(s)) {
            out.push(s);
        }
    }
    out
}

/// 새 칸이 어떻게 열리는가
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NewPane {
    /// 고른 칸을 나눈다: 늘 고른 칸의 크기
    Split,
    /// 새 탭: `window-inherit-font-size`가 켜져 있으면 고른 칸의 크기, 아니면 `font-size`
    Tab,
}

/// 새 칸의 글자 크기. `focused`는 지금 창에서 고른 칸의 크기(창에 칸이 아직 없으면 없다), `window_base`는 창의 기준 크기.
/// 창에 칸이 없으면(새 창의 첫 칸) 창을 열 때 정한 기준 크기다(`App::new_window_font_size`가 이미 설정을 따랐다)
pub fn new_pane_size(config: &Config, kind: NewPane, focused: Option<f32>, window_base: f32) -> f32 {
    match (kind, focused) {
        (_, None) => window_base,
        (NewPane::Split, Some(size)) => bt_core::config::clamp_font_size(size),
        (NewPane::Tab, Some(size)) => config.new_window_font_size(Some(size)),
    }
}

/// 설정을 다시 읽은 뒤 칸들의 크기: `font-size`가 바뀌었으면 모두 새 값, 아니면 칸마다 그대로(0.40 규칙을 칸마다)
pub fn sizes_after_reload(old_config: f32, new_config: f32, current: &[f32]) -> Vec<f32> {
    current
        .iter()
        .map(|c| bt_core::config::font_size_after_reload(old_config, new_config, *c))
        .collect()
}

/// 크기별 글꼴 묶음. 기준 크기(창 덧그림)의 묶음과 그 밖의 크기들. 같은 크기는 한 벌만 든다
pub struct SizedFonts<T> {
    chrome: Option<(f32, T)>,
    others: Vec<(f32, T)>,
}

impl<T> Default for SizedFonts<T> {
    fn default() -> Self {
        Self {
            chrome: None,
            others: Vec::new(),
        }
    }
}

impl<T> SizedFonts<T> {
    /// 기준 크기의 묶음(없으면 글꼴을 읽지 못했다)
    pub fn chrome(&self) -> Option<&T> {
        self.chrome.as_ref().map(|(_, t)| t)
    }

    /// 기준 크기(묶음이 있으면)
    pub fn chrome_size(&self) -> Option<f32> {
        self.chrome.as_ref().map(|(s, _)| *s)
    }

    /// 들고 있는 크기의 수(기준 크기 포함)
    #[cfg(test)]
    pub fn len(&self) -> usize {
        self.others.len() + usize::from(self.chrome.is_some())
    }

    pub fn clear(&mut self) {
        self.chrome = None;
        self.others.clear();
    }

    /// 크기의 묶음. 기준 크기든 다른 크기든 찾는다
    pub fn get(&self, size: f32) -> Option<&T> {
        let key = size_key(size);
        self.chrome
            .iter()
            .chain(self.others.iter())
            .find(|(s, _)| size_key(*s) == key)
            .map(|(_, t)| t)
    }

    /// 들고 있는 크기들(기준 크기가 먼저)
    #[cfg(test)]
    pub fn sizes(&self) -> Vec<f32> {
        self.chrome.iter().chain(self.others.iter()).map(|(s, _)| *s).collect()
    }

    /// 크기 `size`의 묶음이 없으면 `build`로 만들어 더한다. 만들지 못하면 거짓
    pub fn ensure(&mut self, size: f32, base: f32, build: impl FnOnce(f32) -> Option<T>) -> bool {
        if self.get(size).is_some() {
            return true;
        }
        match build(size) {
            Some(t) if size_key(size) == size_key(base) => {
                // 기준 크기가 바뀌는 중이면 옛 기준 묶음은 다른 크기들로 내려간다
                if let Some(old) = self.chrome.replace((size, t)) {
                    self.others.push(old);
                }
                true
            }
            Some(t) => {
                self.others.push((size, t));
                true
            }
            None => false,
        }
    }

    /// 필요한 크기(`needed_sizes`)에 맞춘다: 필요 없는 묶음은 버리고, 이미 있는 것은 그대로 쓰고, 없는 것만 `build`로 만든다.
    /// 기준 크기(`base`)가 바뀌었으면 묶음을 옮겨 기준으로 삼는다. 새로 만든 크기들을 돌려준다
    pub fn sync(&mut self, base: f32, needed: &[f32], mut build: impl FnMut(f32) -> Option<T>) -> Vec<f32> {
        let keep = |s: f32| needed.iter().any(|n| size_key(*n) == size_key(s));
        // 풀어서 다시 담는다
        let mut pool: Vec<(f32, T)> = self.chrome.take().into_iter().chain(self.others.drain(..)).collect();
        pool.retain(|(s, _)| keep(*s));
        let mut built = Vec::new();
        for n in needed {
            let key = size_key(*n);
            let item = match pool.iter().position(|(s, _)| size_key(*s) == key) {
                Some(i) => Some(pool.swap_remove(i)),
                None => build(*n).map(|t| {
                    built.push(*n);
                    (*n, t)
                }),
            };
            if let Some(item) = item {
                if key == size_key(base) {
                    self.chrome = Some(item);
                } else {
                    self.others.push(item);
                }
            }
        }
        built
    }

    /// 기준 묶음과 나머지 묶음을 함께 빌린다(창을 그리는 동안 칸마다 크기를 골라 쓴다)
    pub fn parts_mut(&mut self) -> Option<FontsMut<'_, T>> {
        let (chrome_size, chrome) = self.chrome.as_mut().map(|(s, t)| (*s, t))?;
        Some(FontsMut {
            chrome_size,
            chrome,
            others: &mut self.others,
        })
    }
}

/// `SizedFonts::parts_mut`가 빌려 준 묶음들
pub struct FontsMut<'a, T> {
    chrome_size: f32,
    chrome: &'a mut T,
    others: &'a mut Vec<(f32, T)>,
}

impl<T> FontsMut<'_, T> {
    /// 창 덧그림에 쓰는 기준 크기의 묶음
    pub fn chrome(&mut self) -> &mut T {
        self.chrome
    }

    /// 칸의 크기에 맞는 묶음. 없으면(글꼴을 읽지 못한 크기) 기준 묶음
    pub fn for_size(&mut self, size: f32) -> &mut T {
        let i = self.index_of(size);
        self.nth(i)
    }

    /// `all`에서 크기의 자리(0이 기준 묶음). 없으면 0
    pub fn index_of(&self, size: f32) -> usize {
        let key = size_key(size);
        if size_key(self.chrome_size) == key {
            return 0;
        }
        self.others
            .iter()
            .position(|(s, _)| size_key(*s) == key)
            .map_or(0, |i| i + 1)
    }

    fn nth(&mut self, i: usize) -> &mut T {
        if i == 0 { self.chrome } else { &mut self.others[i - 1].1 }
    }

    /// 모든 묶음(기준이 0번). GPU 렌더러에 칸마다 자리를 골라 넘긴다
    pub fn all(&mut self) -> Vec<&mut T> {
        std::iter::once(&mut *self.chrome)
            .chain(self.others.iter_mut().map(|(_, t)| t))
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn needed_sizes_start_with_the_base_and_skip_repeats() {
        assert_eq!(needed_sizes(13.0, [13.0, 20.0, 20.0, 9.0]), [13.0, 20.0, 9.0]);
        assert_eq!(needed_sizes(13.0, []), [13.0]);
        // 기준 크기를 쓰는 칸이 없어도 기준은 든다(창 덧그림이 쓴다)
        assert_eq!(needed_sizes(13.0, [20.0]), [13.0, 20.0]);
    }

    fn cfg(inherit: bool) -> Config {
        Config {
            font_size: 13.0,
            window_inherit_font_size: inherit,
            ..Config::default()
        }
    }

    #[test]
    fn a_split_always_takes_the_focused_panes_size() {
        for inherit in [true, false] {
            assert_eq!(new_pane_size(&cfg(inherit), NewPane::Split, Some(20.0), 15.0), 20.0);
        }
    }

    #[test]
    fn a_new_tab_follows_window_inherit_font_size() {
        assert_eq!(new_pane_size(&cfg(true), NewPane::Tab, Some(20.0), 15.0), 20.0);
        assert_eq!(new_pane_size(&cfg(false), NewPane::Tab, Some(20.0), 15.0), 13.0);
    }

    #[test]
    fn the_first_pane_of_a_window_takes_the_window_size() {
        for inherit in [true, false] {
            assert_eq!(new_pane_size(&cfg(inherit), NewPane::Tab, None, 17.0), 17.0);
            assert_eq!(new_pane_size(&cfg(inherit), NewPane::Split, None, 17.0), 17.0);
        }
    }

    /// 0.40 규칙을 칸마다: `font-size`가 바뀌었으면 모든 칸이 새 값, 아니면 칸마다 그대로
    #[test]
    fn reload_changes_every_pane_only_when_font_size_changed() {
        assert_eq!(sizes_after_reload(13.0, 16.0, &[13.0, 20.0, 9.0]), [16.0, 16.0, 16.0]);
        assert_eq!(sizes_after_reload(13.0, 13.0, &[13.0, 20.0, 9.0]), [13.0, 20.0, 9.0]);
    }

    fn build_log(log: &std::cell::RefCell<Vec<f32>>) -> impl FnMut(f32) -> Option<String> + '_ {
        |s| {
            log.borrow_mut().push(s);
            Some(format!("fonts@{s}"))
        }
    }

    #[test]
    fn same_size_panes_share_one_set() {
        let mut f = SizedFonts::<String>::default();
        let log = std::cell::RefCell::new(Vec::new());
        // 칸 셋이 모두 13pt: 묶음 한 벌
        f.sync(13.0, &needed_sizes(13.0, [13.0, 13.0, 13.0]), build_log(&log));
        assert_eq!(f.len(), 1);
        assert_eq!(*log.borrow(), [13.0]);
        assert_eq!(f.chrome().map(String::as_str), Some("fonts@13"));
    }

    #[test]
    fn two_sizes_are_two_sets_and_unchanged_ones_are_kept() {
        let mut f = SizedFonts::<String>::default();
        let log = std::cell::RefCell::new(Vec::new());
        f.sync(13.0, &needed_sizes(13.0, [13.0, 20.0]), build_log(&log));
        assert_eq!(f.len(), 2);
        assert_eq!(f.get(20.0).map(String::as_str), Some("fonts@20"));
        log.borrow_mut().clear();
        // 같은 필요를 다시 맞추면 아무것도 만들지 않는다
        let built = f.sync(13.0, &needed_sizes(13.0, [13.0, 20.0]), build_log(&log));
        assert!(built.is_empty() && log.borrow().is_empty());
    }

    #[test]
    fn changing_the_base_reuses_a_set_the_panes_still_use() {
        let mut f = SizedFonts::<String>::default();
        let log = std::cell::RefCell::new(Vec::new());
        f.sync(13.0, &needed_sizes(13.0, [13.0, 20.0]), build_log(&log));
        log.borrow_mut().clear();
        // 20pt 칸에서 ⌘+: 기준이 21이 되고 그 칸은 21, 다른 칸은 13 그대로. 13 묶음은 다시 만들지 않는다
        f.sync(21.0, &needed_sizes(21.0, [13.0, 21.0]), build_log(&log));
        assert_eq!(*log.borrow(), [21.0], "새 크기 하나만 만든다");
        assert_eq!(f.chrome().map(String::as_str), Some("fonts@21"));
        assert_eq!(f.get(13.0).map(String::as_str), Some("fonts@13"));
        assert!(f.get(20.0).is_none(), "쓰지 않는 크기는 버린다");
        assert_eq!(f.len(), 2);
    }

    #[test]
    fn ensure_adds_a_size_without_touching_the_others() {
        let mut f = SizedFonts::<String>::default();
        f.sync(13.0, &[13.0], |s| Some(format!("fonts@{s}")));
        assert!(f.ensure(18.0, 13.0, |s| Some(format!("fonts@{s}"))));
        assert_eq!(f.sizes(), [13.0, 18.0]);
        assert!(!f.ensure(30.0, 13.0, |_| None), "만들지 못한 크기는 더하지 않는다");
        assert_eq!(f.len(), 2);
    }

    #[test]
    fn parts_pick_a_set_per_pane_size() {
        let mut f = SizedFonts::<String>::default();
        f.sync(13.0, &[13.0, 20.0], |s| Some(format!("fonts@{s}")));
        let mut p = f.parts_mut().unwrap();
        assert_eq!(p.chrome(), "fonts@13");
        assert_eq!(p.for_size(13.0), "fonts@13");
        assert_eq!(p.for_size(20.0), "fonts@20");
        // 없는 크기는 기준 묶음으로 물러난다
        assert_eq!(p.for_size(99.0), "fonts@13");
        assert_eq!(p.index_of(13.0), 0);
        assert_eq!(p.index_of(20.0), 1);
        let all = p.all();
        assert_eq!(all.len(), 2);
        assert_eq!(all[1], "fonts@20");
    }
}
