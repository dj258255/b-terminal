//! 키 입력 → 화면 지연 로그(#417). 시각을 적고 지우는 규칙은 `bt_term::latency`(시계를 주입하는 순수 구조체)에 있고,
//! 여기는 프레임 앞뒤에서 칸마다 그것을 부르고 올라갔을 때 debug 로그 한 줄을 찍는 자리다.
//! debug 로그가 꺼져 있으면 칸은 시각을 적지 않으므로(`bt_term::latency::enabled`) 여기도 아무 값이 없다

use super::*;

/// 이번 프레임에 출력이 든 칸의 (칸 번호, 값)
pub(super) type Samples = Vec<(u64, bt_term::latency::Sample)>;

impl App {
    /// 칸의 화면을 뜬 뒤에 부른다. `at`은 화면을 뜨기 전에 잰 때다(그때까지 온 출력만 이 프레임에 있다)
    pub(super) fn latency_begin(&self, views: &[PaneView], at: Instant) -> Samples {
        views
            .iter()
            .filter_map(|v| {
                let p = &self.panes[self.pane_index(v.id)?];
                Some((v.id, p.session.latency_begin_frame(at)?))
            })
            .collect()
    }

    /// 프레임이 실제로 올라간 직후에 부른다(올리지 못했거나 건너뛴 프레임에서는 부르지 않는다). `path`는 `GPU`·`CPU`
    pub(super) fn latency_presented(&self, samples: &Samples, path: &str) {
        let now = Instant::now();
        for (id, sample) in samples {
            let Some(i) = self.pane_index(*id) else { continue };
            if let Some(r) = self.panes[i].session.latency_presented(sample, now) {
                log::debug!("{}", r.line(*id, path));
            }
        }
    }
}
