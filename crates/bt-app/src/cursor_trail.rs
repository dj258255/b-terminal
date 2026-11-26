//! 커서 흔적(kitty `cursor_trail`, #891)을 칸마다 이어 가는 앱 쪽 일: 칸별 상태 보관, 설정 → 매개변수, 색과 알파, 깨우기.
//! 움직임 규칙과 그리기는 `bt_render::trail`에 있다(여기는 시계를 읽지 않고 시간을 값으로 받는다).
//!
//! 고른 칸에만 흔적이 있다(kitty는 고른 창의 커서만 따라간다). 칸을 바꾸면 이전 칸의 흔적은 바로 지우고 새 칸은 커서
//! 자리에 붙는다(kitty는 창을 넘는 흔적을 그리지만 여기 흔적은 칸 안에만 그린다)

use std::collections::HashMap;
use std::sync::OnceLock;
use std::time::{Duration, Instant};

use bt_core::Rgb;
use bt_core::config::Config;
use bt_render::trail::{CursorTrail, RectF, TrailInput, TrailLayout, TrailPaint, TrailParams};

/// 흔적을 켠 설정. 끄면(`cursor-trail = 0`) 없다
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Settings {
    pub params: TrailParams,
    /// 정하지 않으면 커서 색
    pub color: Option<Rgb>,
}

impl Settings {
    pub fn from_config(c: &Config) -> Option<Self> {
        (c.cursor_trail_ms > 0).then(|| Self {
            params: TrailParams {
                delay: Duration::from_millis(c.cursor_trail_ms as u64),
                decay_fast: c.cursor_trail_decay.0,
                decay_slow: c.cursor_trail_decay.1,
                threshold: c.cursor_trail_start_threshold,
            },
            color: c.cursor_trail_color,
        })
    }
}

/// 이 앱이 시작한 뒤 흐른 시간. 흔적 상태가 받는 시계다
pub fn now() -> Duration {
    static START: OnceLock<Instant> = OnceLock::new();
    START.get_or_init(Instant::now).elapsed()
}

/// 칸 하나의 이번 프레임 입력
#[derive(Debug, Clone, Copy)]
pub struct PaneInput {
    pub id: u64,
    pub focused: bool,
    pub cell: (usize, usize),
    pub rect: RectF,
    pub visible: bool,
    pub layout: TrailLayout,
    /// 커서 색(`cursor-trail-color`를 정하지 않았을 때 흔적 색)
    pub cursor_color: Rgb,
}

/// 한 프레임의 결과. 모두 `panes`와 같은 순서다
#[derive(Debug, Default, PartialEq)]
pub struct Step {
    pub paints: Vec<Option<TrailPaint>>,
    /// 칸을 통째로 다시 그려야 한다(흔적이 있거나, 방금 끝나 지워야 한다)
    pub repaint: Vec<bool>,
    /// 이 뒤에 다시 그리자고 깨워 달라는 요청. 움직이는 칸도 지연을 세는 칸도 없으면 없다
    pub wake: Option<Duration>,
}

/// 창 하나의 칸별 흔적 상태
#[derive(Debug, Default)]
pub struct Book {
    trails: HashMap<u64, CursorTrail>,
    /// 앞 프레임에 흔적을 그려 이번에 지워야 할 수도 있는 칸
    drawn: Vec<u64>,
}

impl Book {
    pub fn step(&mut self, now: Duration, settings: Option<&Settings>, panes: &[PaneInput]) -> Step {
        let mut out = Step {
            paints: vec![None; panes.len()],
            repaint: vec![false; panes.len()],
            wake: None,
        };
        let before = std::mem::take(&mut self.drawn);
        let Some(s) = settings else {
            // 껐다: 그려 둔 흔적만 한 번 지우고 상태는 버린다
            self.trails.clear();
            for (i, p) in panes.iter().enumerate() {
                out.repaint[i] = before.contains(&p.id);
            }
            return out;
        };
        // 없어졌거나 고르지 않은 칸의 상태는 버린다
        self.trails
            .retain(|id, _| panes.iter().any(|p| p.id == *id && p.focused));
        for (i, p) in panes.iter().enumerate() {
            if !p.focused {
                out.repaint[i] = before.contains(&p.id);
                continue;
            }
            let t = self.trails.entry(p.id).or_default();
            let f = t.update(
                now,
                &s.params,
                &TrailInput {
                    cell: p.cell,
                    rect: p.rect,
                    visible: p.visible,
                    layout: p.layout,
                },
            );
            out.paints[i] = f.quad.map(|quad| TrailPaint {
                quad,
                mask: p.rect,
                color: s.color.unwrap_or(p.cursor_color),
                alpha: (f.opacity.clamp(0.0, 1.0) * 255.0).round() as u8,
            });
            out.repaint[i] = f.repaint;
            if f.repaint {
                self.drawn.push(p.id);
            }
            out.wake = match (out.wake, f.wake) {
                (Some(a), Some(b)) => Some(a.min(b)),
                (a, b) => a.or(b),
            };
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ms(n: u64) -> Duration {
        Duration::from_millis(n)
    }

    fn settings() -> Settings {
        Settings {
            params: TrailParams {
                delay: ms(3),
                decay_fast: 0.1,
                decay_slow: 0.4,
                threshold: (2, 2),
            },
            color: None,
        }
    }

    fn pane(id: u64, focused: bool, col: usize, row: usize) -> PaneInput {
        PaneInput {
            id,
            focused,
            cell: (col, row),
            rect: RectF::new(
                col as f32 * 10.0,
                row as f32 * 20.0,
                col as f32 * 10.0 + 10.0,
                row as f32 * 20.0 + 20.0,
            ),
            visible: true,
            layout: TrailLayout {
                cell: (10, 20),
                grid: (80, 24),
            },
            cursor_color: Rgb::new(1, 2, 3),
        }
    }

    /// 8ms 간격으로 `until`까지 돌린다
    fn run(b: &mut Book, s: Option<&Settings>, panes: &[PaneInput], from: Duration, until: Duration) -> Vec<Step> {
        let mut now = from;
        let mut v = Vec::new();
        while now <= until {
            v.push(b.step(now, s, panes));
            now += ms(8);
        }
        v
    }

    #[test]
    fn default_config_is_off() {
        assert_eq!(Settings::from_config(&Config::default()), None);
        let c = Config::from_text("cursor-trail = 7\ncursor-trail-color = #102030\n", true);
        let s = Settings::from_config(&c).expect("켬");
        assert_eq!(s.params.delay, ms(7));
        assert_eq!(s.color, Some(Rgb::new(16, 32, 48)));
    }

    #[test]
    fn off_never_draws_or_wakes() {
        let mut b = Book::default();
        for st in run(&mut b, None, &[pane(1, true, 0, 0)], ms(0), ms(100)) {
            assert_eq!((st.paints, st.repaint, st.wake), (vec![None], vec![false], None));
        }
        // 멀리 뛰어도 같다
        for st in run(&mut b, None, &[pane(1, true, 60, 20)], ms(200), ms(400)) {
            assert_eq!((st.paints, st.repaint, st.wake), (vec![None], vec![false], None));
        }
    }

    #[test]
    fn idle_pane_does_not_wake_so_the_app_uses_no_cpu() {
        let s = settings();
        let mut b = Book::default();
        let steps = run(&mut b, Some(&s), &[pane(1, true, 5, 5)], ms(0), ms(2000));
        assert!(
            steps
                .iter()
                .all(|st| st.wake.is_none() && !st.repaint[0] && st.paints[0].is_none())
        );
    }

    #[test]
    fn a_jump_wakes_only_while_the_trail_moves_then_stops_waking() {
        let s = settings();
        let mut b = Book::default();
        run(&mut b, Some(&s), &[pane(1, true, 0, 0)], ms(0), ms(1000));
        let steps = run(&mut b, Some(&s), &[pane(1, true, 60, 20)], ms(1008), ms(2500));
        assert_eq!(steps[0].wake, Some(ms(3)), "지연이 끝나면 깨운다");
        let last_wake = steps.iter().rposition(|st| st.wake.is_some()).expect("깨움");
        let last_paint = steps.iter().rposition(|st| st.paints[0].is_some()).expect("그림");
        assert!(last_wake <= last_paint + 1);
        // 끝난 뒤에는 한 번 지우고(`repaint`), 그 뒤는 깨우지도 다시 그리지도 않는다
        assert!(steps[last_paint + 1].repaint[0] && steps[last_paint + 1].wake.is_none());
        assert!(
            steps[last_paint + 2..]
                .iter()
                .all(|st| !st.repaint[0] && st.wake.is_none() && st.paints[0].is_none())
        );
        // 그린 흔적의 색은 커서 색이고 알파는 불투명도다
        let p = steps.iter().find_map(|st| st.paints[0]).expect("흔적");
        assert_eq!(p.color, Rgb::new(1, 2, 3));
        assert!(p.alpha > 0);
    }

    #[test]
    fn alpha_follows_the_trail_opacity() {
        // 칸을 연 지 0.1초 만에 뛰면 불투명도가 아직 0.3 남짓이라 알파는 255보다 훨씬 작다(kitty도 0에서 올라간다)
        let s = settings();
        let mut b = Book::default();
        run(&mut b, Some(&s), &[pane(1, true, 0, 0)], ms(0), ms(96));
        let steps = run(&mut b, Some(&s), &[pane(1, true, 60, 20)], ms(104), ms(200));
        let first = steps.iter().find_map(|st| st.paints[0]).expect("흔적");
        assert!((40..120).contains(&first.alpha), "{}", first.alpha);
    }

    #[test]
    fn trail_color_setting_wins_over_the_cursor_color() {
        let s = Settings {
            color: Some(Rgb::new(9, 8, 7)),
            ..settings()
        };
        let mut b = Book::default();
        run(&mut b, Some(&s), &[pane(1, true, 0, 0)], ms(0), ms(1000));
        let steps = run(&mut b, Some(&s), &[pane(1, true, 60, 20)], ms(1008), ms(1200));
        assert!(
            steps
                .iter()
                .filter_map(|st| st.paints[0])
                .all(|p| p.color == Rgb::new(9, 8, 7))
        );
    }

    #[test]
    fn only_the_focused_pane_trails_and_losing_focus_clears_it() {
        let s = settings();
        let mut b = Book::default();
        let both = |a: usize, c: usize| [pane(1, true, a, 0), pane(2, false, c, 0)];
        run(&mut b, Some(&s), &both(0, 0), ms(0), ms(1000));
        // 둘 다 멀리 뛰어도 고른 칸만 흔적이 생긴다
        let steps = run(&mut b, Some(&s), &both(60, 60), ms(1008), ms(1100));
        assert!(steps.iter().any(|st| st.paints[0].is_some()));
        assert!(steps.iter().all(|st| st.paints[1].is_none() && !st.repaint[1]));
        // 초점이 다른 칸으로 넘어가면 앞 칸의 흔적을 지우려고 한 번 다시 그린다
        let steps = run(
            &mut b,
            Some(&s),
            &[pane(1, false, 60, 0), pane(2, true, 60, 0)],
            ms(1108),
            ms(1116),
        );
        assert!(steps[0].repaint[0] && steps[0].paints[0].is_none());
        assert!(!steps[1].repaint[0]);
        // 새로 고른 칸은 커서 자리에 붙는다(원점에서 날아오지 않는다)
        assert!(steps.iter().all(|st| st.paints[1].is_none()));
    }

    #[test]
    fn turning_it_off_erases_a_trail_once() {
        let s = settings();
        let mut b = Book::default();
        run(&mut b, Some(&s), &[pane(1, true, 0, 0)], ms(0), ms(1000));
        run(&mut b, Some(&s), &[pane(1, true, 60, 20)], ms(1008), ms(1060));
        let steps = run(&mut b, None, &[pane(1, true, 60, 20)], ms(1068), ms(1100));
        assert!(steps[0].repaint[0], "설정을 끄면 그려 둔 흔적을 한 번 지운다");
        assert!(steps[1..].iter().all(|st| !st.repaint[0] && st.wake.is_none()));
    }

    #[test]
    fn closed_panes_are_forgotten() {
        let s = settings();
        let mut b = Book::default();
        run(
            &mut b,
            Some(&s),
            &[pane(1, true, 0, 0), pane(2, true, 0, 0)],
            ms(0),
            ms(100),
        );
        run(&mut b, Some(&s), &[pane(1, true, 0, 0)], ms(108), ms(116));
        assert_eq!(b.trails.len(), 1);
    }
}
