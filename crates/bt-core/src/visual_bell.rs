//! 시각 벨(`visual-bell-duration`·`visual-bell-color`, kitty `visual_bell_duration`·`visual_bell_color`처럼, #893).
//! 벨이 울린 칸 전체를 잠깐 번쩍이는 세기를 시간에서 계산하는 순수한 계산만 둔다(창·시계 없음).
//!
//! 모양은 kitty 0.44.0 소스를 따른다: `shaders.c`의 `get_visual_bell_intensity`·`draw_visual_bell_flash`(세기 0~1을 곡선으로 올렸다 내리고,
//! 색 × 세기 × 0.4(밝은 색은 0.6)를 칸 위에 얹는다), `options/utils.py`의 `parse_animation`(설정 글), `animation.c`(곡선).
//! 본 것은 소스 글뿐이고 kitty 창은 띄워 보지 않았다

use crate::Rgb;

/// 곡선 하나(CSS easing function). 입력은 0~1
#[derive(Debug, Clone, PartialEq)]
pub enum Easing {
    /// `cubic-bezier(x1, y1, x2, y2)`(x는 0~1로 자른다)
    Bezier(f64, f64, f64, f64),
    /// `linear(...)`: 점 (x, y)들(x는 늘어나는 순서)
    Linear(Vec<f64>, Vec<f64>),
    /// `steps(n, 점프 종류)`
    Steps { n: usize, jump: Jump },
}

/// `steps()`의 점프 종류
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Jump {
    Start,
    End,
    None,
    Both,
}

fn unit(x: f64) -> f64 {
    x.clamp(0.0, 1.0)
}

/// kitty `animation.c`의 `solve_unit_bezier`: x에서 y. 0~1 밖 값은 쓰지 않는다
fn bezier_at(x1: f64, y1: f64, x2: f64, y2: f64, x: f64) -> f64 {
    let x = unit(x);
    // kitty `is_bezier_linear`: 네 점이 한 직선이면 그대로
    if x1 == y1 && x2 == y2 {
        return x;
    }
    let (cx, cy) = (3.0 * x1, 3.0 * y1);
    let (bx, by) = (3.0 * (x2 - x1) - cx, 3.0 * (y2 - y1) - cy);
    let (ax, ay) = (1.0 - cx - bx, 1.0 - cy - by);
    let sx = |t: f64| ((ax * t + bx) * t + cx) * t;
    let sy = |t: f64| ((ay * t + by) * t + cy) * t;
    // x(t)는 x1·x2가 0~1이라 늘기만 한다: 이분법으로 푼다
    let (mut lo, mut hi) = (0.0f64, 1.0f64);
    for _ in 0..60 {
        let mid = (lo + hi) / 2.0;
        if sx(mid) < x {
            lo = mid;
        } else {
            hi = mid;
        }
    }
    sy((lo + hi) / 2.0).abs()
}

impl Easing {
    /// 곡선 값(자르지 않은 값). `t`는 0~1. kitty `animation.c`의 각 곡선과 같다
    pub fn at(&self, t: f64) -> f64 {
        match self {
            Easing::Bezier(x1, y1, x2, y2) => bezier_at(*x1, *y1, *x2, *y2, t),
            Easing::Linear(xs, ys) => {
                let t = unit(t);
                let (mut start_pos, mut stop_pos, mut start_val, mut stop_val) = (0.0, 1.0, 0.0, 1.0);
                for i in 0..xs.len() {
                    if xs[i] >= t {
                        stop_pos = xs[i];
                        stop_val = ys[i];
                        if i > 0 {
                            start_val = ys[i - 1];
                            start_pos = xs[i - 1];
                        }
                        break;
                    }
                }
                if stop_pos > start_pos {
                    start_val + (t - start_pos) / (stop_pos - start_pos) * (stop_val - start_val)
                } else {
                    stop_val
                }
            }
            Easing::Steps { n, jump } => {
                let n = *n as f64;
                let (mut buckets, mut size, mut first) = (n, 1.0 / n, 0.0);
                match jump {
                    Jump::Start => first = size,
                    Jump::End => {}
                    Jump::None => size = 1.0 / (buckets - 1.0),
                    Jump::Both => {
                        buckets += 1.0;
                        size = 1.0 / buckets;
                        first = size;
                    }
                }
                let bucket = (unit(t) * buckets).floor();
                first + bucket * size
            }
        }
    }

    fn parse_keyword(name: &str) -> Option<Easing> {
        Some(match name {
            "ease-in-out" => Easing::Bezier(0.42, 0.0, 0.58, 1.0),
            "linear" => Easing::Bezier(0.0, 0.0, 1.0, 1.0),
            "ease" => Easing::Bezier(0.25, 0.1, 0.25, 1.0),
            "ease-out" => Easing::Bezier(0.0, 0.0, 0.58, 1.0),
            "ease-in" => Easing::Bezier(0.42, 0.0, 1.0, 1.0),
            "step-start" => Easing::Steps {
                n: 1,
                jump: Jump::Start,
            },
            "step-end" => Easing::Steps { n: 1, jump: Jump::End },
            _ => return None,
        })
    }

    fn parse_function(name: &str, params: &str) -> Option<Easing> {
        let words = || params.replace(',', " ");
        let num = |w: &str| w.parse::<f64>().ok().filter(|v| v.is_finite());
        match name {
            "cubic-bezier" => {
                let w = words();
                let p: Vec<f64> = w.split_whitespace().map(num).collect::<Option<_>>()?;
                let [x1, y1, x2, y2] = p[..] else { return None };
                Some(Easing::Bezier(unit(x1), y1, unit(x2), y2))
            }
            "steps" => {
                let w = words();
                let p: Vec<&str> = w.split_whitespace().collect();
                let n: i64 = p.first()?.parse().ok()?;
                match p.len() {
                    1 => Some(Easing::Steps {
                        n: n.max(1) as usize,
                        jump: Jump::End,
                    }),
                    2 => {
                        let jump = match p[1].to_ascii_lowercase().as_str() {
                            "jump-start" | "start" => Jump::Start,
                            "jump-end" | "end" => Jump::End,
                            "jump-none" => Jump::None,
                            "jump-both" => Jump::Both,
                            _ => return None,
                        };
                        let n = if jump == Jump::None { n.max(2) } else { n.max(1) };
                        Some(Easing::Steps { n: n as usize, jump })
                    }
                    _ => None,
                }
            }
            "linear" => Self::parse_linear(params),
            _ => None,
        }
    }

    /// kitty `EasingFunction.linear`: 점 사이 x가 빠진 곳은 고르게 채운다
    fn parse_linear(params: &str) -> Option<Easing> {
        let parts: Vec<&str> = params.split(',').collect();
        if parts.len() < 2 {
            return None;
        }
        let (mut xs, mut ys): (Vec<f64>, Vec<f64>) = (Vec::new(), Vec::new());
        fn percent(w: &str) -> Option<f64> {
            let v: f64 = w.strip_suffix('%')?.trim().parse().ok()?;
            v.is_finite().then_some(unit(v / 100.0))
        }
        let mut points: Vec<(f64, Option<f64>)> = Vec::new();
        for part in &parts {
            let w: Vec<&str> = part.split_whitespace().collect();
            let y: f64 = w.first()?.parse().ok().filter(|v: &f64| v.is_finite())?;
            let y = unit(y);
            match w.len() {
                1 => points.push((y, None)),
                2 => points.push((y, Some(percent(w[1])?))),
                3 => {
                    points.push((y, Some(percent(w[1])?)));
                    points.push((y, Some(percent(w[2])?)));
                }
                _ => return None,
            }
        }
        for (y, x) in points {
            if let Some(x) = x {
                balance(&mut xs, &ys, x)?;
                xs.push(x);
            }
            ys.push(y);
        }
        balance(&mut xs, &ys, 1.0)?;
        Some(Easing::Linear(xs, ys))
    }
}

/// `balance`의 본체. x가 아직 하나도 없으면 0부터 `end`까지 고르게(맨 앞 점이 0), 있으면 마지막 x 뒤로 `end`까지 고르게 채운다
fn balance(xs: &mut Vec<f64>, ys: &[f64], end: f64) -> Option<()> {
    let extra = ys.len().saturating_sub(xs.len());
    if extra == 0 {
        return Some(());
    }
    let start = xs.last().copied().unwrap_or(0.0);
    let delta = (end - start) / extra.saturating_sub(1).max(1) as f64;
    if delta <= 0.0 {
        return None;
    }
    let had = !xs.is_empty();
    for i in 0..extra {
        xs.push(if had {
            start + (i + 1) as f64 * delta
        } else {
            i as f64 * delta
        });
    }
    Some(())
}

/// `visual-bell-duration`: 번쩍이는 전체 시간과 올라갈 때·내려갈 때 곡선
#[derive(Debug, Clone, PartialEq)]
pub struct VisualBell {
    /// 초. 0이면 끔
    pub duration: f64,
    pub fade_in: Easing,
    pub fade_out: Easing,
    /// 설정 글에서 곡선 부분(`--show-config`에 그대로 되돌려 쓴다)
    easing_src: Vec<String>,
}

impl Default for VisualBell {
    fn default() -> Self {
        let ease = Easing::Bezier(0.42, 0.0, 0.58, 1.0);
        VisualBell {
            duration: 0.0,
            fade_in: ease.clone(),
            fade_out: ease,
            easing_src: Vec::new(),
        }
    }
}

impl VisualBell {
    /// 설정 글을 읽는다(`2.0`, `0.3 linear`, `0.5 ease-in ease-out`, `1 cubic-bezier(0.1, 0.7, 1, 0.1)` 등). 틀리면 `None`.
    /// kitty `parse_animation`처럼 낱말은 `[-+.0-9A-Za-z]+` 뒤에 `(…)`가 붙을 수 있고, 그 밖의 글자는 건너뛴다.
    /// 숫자는 곡선 앞뒤 어디에 와도 되고, 곡선은 둘까지다(하나면 올라갈 때·내려갈 때 같이 쓴다)
    pub fn parse(spec: &str) -> Option<Self> {
        let spec = spec.trim();
        let mut duration: Option<f64> = None;
        let mut easings: Vec<(Easing, String)> = Vec::new();
        let b = spec.as_bytes();
        let word = |c: u8| c.is_ascii_alphanumeric() || matches!(c, b'-' | b'+' | b'.');
        let mut i = 0;
        while i < b.len() {
            if !word(b[i]) {
                i += 1;
                continue;
            }
            let start = i;
            while i < b.len() && word(b[i]) {
                i += 1;
            }
            let name = &spec[start..i];
            if b.get(i) == Some(&b'(') {
                let close = spec[i..].find(')')? + i;
                let params = &spec[i + 1..close];
                easings.push((
                    Easing::parse_function(name, params)?,
                    format!("{name}({})", params.trim()),
                ));
                i = close + 1;
            } else if let Ok(v) = name.parse::<f64>() {
                if !v.is_finite() || v < 0.0 {
                    return None;
                }
                duration = Some(v);
            } else {
                easings.push((Easing::parse_keyword(name)?, name.to_string()));
            }
            if easings.len() > 2 {
                return None;
            }
        }
        let mut out = VisualBell {
            duration: duration?,
            ..VisualBell::default()
        };
        let mut it = easings.into_iter();
        if let Some((first, src)) = it.next() {
            out.fade_in = first.clone();
            out.fade_out = first;
            out.easing_src.push(src);
            if let Some((second, src)) = it.next() {
                out.fade_out = second;
                out.easing_src.push(src);
            }
        }
        Some(out)
    }

    /// 켜져 있는가
    pub fn enabled(&self) -> bool {
        self.duration > 0.0
    }

    /// 실제로 번쩍이는 시간(초). kitty 코드처럼 적은 시간의 절반이다(문서는 "적은 시간 동안"이라 적지만
    /// `get_visual_bell_intensity`가 `OPT(visual_bell_duration) / 2` 안에 오르고 내리는 두 구간을 모두 끝낸다)
    fn span(&self) -> f64 {
        self.duration / 2.0
    }

    /// 벨 뒤 `elapsed`초의 세기(0~1). 끝났으면 0. 앞 절반은 0에서 1로 `fade_in`, 뒤 절반은 1에서 0으로 `fade_out`
    /// (kitty `get_visual_bell_intensity`: 올라갈 때 `curve`, 내려갈 때 `1 - curve`)
    pub fn intensity(&self, elapsed: f64) -> f64 {
        if !self.enabled() || !(0.0..self.span()).contains(&elapsed) {
            return 0.0;
        }
        let t = elapsed / self.span();
        if t < 0.5 {
            unit(self.fade_in.at(2.0 * t))
        } else {
            1.0 - unit(self.fade_out.at(2.0 * t - 1.0))
        }
    }

    /// 벨 뒤 `elapsed`초에 번쩍임이 끝났는가(끄면 늘 끝남)
    pub fn finished(&self, elapsed: f64) -> bool {
        !self.enabled() || elapsed >= self.span()
    }

    /// `--show-config`에 쓰는 글
    pub fn render(&self) -> String {
        let mut s = self.duration.to_string();
        for e in &self.easing_src {
            s.push(' ');
            s.push_str(e);
        }
        s
    }
}

/// sRGB 한 채널(0~255)의 선형 값(0~1)
fn linear(c: u8) -> f64 {
    let c = c as f64 / 255.0;
    if c <= 0.04045 {
        c / 12.92
    } else {
        ((c + 0.055) / 1.055).powf(2.4)
    }
}

/// 세기와 번쩍임 색에서 칸 위에 얹을 알파(0~255). kitty `draw_visual_bell_flash`: 색 × 세기 × 0.4,
/// 선형 값으로 가장 큰 채널이 0.45를 넘는 밝은 색은 0.6(눈부심을 줄인다)
pub fn flash_alpha(intensity: f64, color: Rgb) -> u8 {
    let max = linear(color.r).max(linear(color.g)).max(linear(color.b));
    let attenuation = if max > 0.45 { 0.6 } else { 0.4 };
    (unit(intensity) * attenuation * 255.0 + 1e-9).round() as u8
}

#[cfg(test)]
mod tests {
    use super::*;

    fn near(a: f64, b: f64) -> bool {
        (a - b).abs() < 1e-3
    }

    #[test]
    fn default_is_off_and_number_alone_means_ease_in_out() {
        let d = VisualBell::default();
        assert!(!d.enabled());
        let v = VisualBell::parse("0.5").unwrap();
        assert!(v.enabled() && near(v.duration, 0.5));
        assert_eq!(v.fade_in, Easing::Bezier(0.42, 0.0, 0.58, 1.0));
        assert_eq!(v.fade_out, v.fade_in);
        assert!(!VisualBell::parse("0").unwrap().enabled());
        assert!(!VisualBell::parse("0.0").unwrap().enabled());
    }

    #[test]
    fn intensity_rises_to_one_in_the_middle_and_returns_to_zero() {
        // 2초를 적으면 실제 번쩍임은 1초(kitty 코드처럼 절반)
        let v = VisualBell::parse("2.0 linear").unwrap();
        for (t, want) in [(0.0, 0.0), (0.25, 0.5), (0.5, 1.0), (0.75, 0.5)] {
            assert!(near(v.intensity(t), want), "{t}: {}", v.intensity(t));
        }
        assert_eq!(v.intensity(1.0), 0.0);
        assert_eq!(v.intensity(5.0), 0.0);
        assert!(!v.finished(0.99) && v.finished(1.0) && v.finished(2.0));
        // 끄면 늘 0, 늘 끝남
        let off = VisualBell::default();
        assert_eq!((off.intensity(0.1), off.finished(0.0)), (0.0, true));
        // 음수 시간은 0
        assert_eq!(v.intensity(-1.0), 0.0);
    }

    /// kitty는 문서와 달리 오르고 내리는 두 구간을 합쳐 적은 시간의 절반 안에 끝낸다
    /// (`shaders.c` `get_visual_bell_intensity`의 `OPT(visual_bell_duration) / 2`, 곡선 두 구간을 `apply_easing_curve`가 [0,1]에 나눠 씀).
    /// 같은 설정이 kitty와 같은 모양이 되게 그 동작을 따른다
    #[test]
    fn whole_flash_takes_half_the_duration_like_kittys_code() {
        let v = VisualBell::parse("1.0 linear").unwrap();
        assert!(
            near(v.intensity(0.25), 1.0),
            "가장 밝은 때는 1초의 1/4: {}",
            v.intensity(0.25)
        );
        assert!(near(v.intensity(0.125), 0.5));
        assert_eq!(v.intensity(0.5), 0.0);
        assert!(!v.finished(0.49) && v.finished(0.5));
    }

    #[test]
    fn ease_in_out_is_slow_at_the_ends_and_fast_in_the_middle() {
        let v = VisualBell::parse("2.0").unwrap();
        // 올라가는 쪽 앞 10%(전체의 5%)는 선형(0.1)보다 한참 낮다
        assert!(v.intensity(0.05) < 0.05, "{}", v.intensity(0.05));
        assert!(near(v.intensity(0.5), 1.0));
        // 올라가는 곡선과 내려가는 곡선은 같은 곡선이라 가운데에서 대칭이다
        assert!(near(v.intensity(0.25), v.intensity(0.75)));
        assert!(near(v.intensity(0.25), 0.5));
        let mut prev = 0.0;
        for i in 0..=50 {
            let x = v.intensity(i as f64 / 100.0);
            assert!(x >= prev - 1e-9, "올라가는 동안 줄지 않는다");
            prev = x;
        }
    }

    #[test]
    fn two_easings_split_fade_in_and_fade_out_and_one_is_used_for_both() {
        let v = VisualBell::parse("1 ease-in ease-out").unwrap();
        assert_eq!(v.fade_in, Easing::Bezier(0.42, 0.0, 1.0, 1.0));
        assert_eq!(v.fade_out, Easing::Bezier(0.0, 0.0, 0.58, 1.0));
        let one = VisualBell::parse("1 ease-in").unwrap();
        assert_eq!(one.fade_in, one.fade_out);
        // 숫자는 곡선 앞뒤 어디에 와도 된다
        assert_eq!(VisualBell::parse("ease-in 1"), VisualBell::parse("1 ease-in"));
        // 내려갈 때 곡선은 1 - 곡선: ease-in으로 내려가면 처음엔 천천히 줄어든다
        let w = VisualBell::parse("2 linear ease-in").unwrap();
        assert!(
            w.intensity(0.55) > 0.9 && w.intensity(0.55) < 1.0,
            "{}",
            w.intensity(0.55)
        );
    }

    #[test]
    fn parses_css_functions_and_rejects_nonsense() {
        let v = VisualBell::parse("0.3 cubic-bezier(0.1, 0.7, 1, 0.1)").unwrap();
        assert_eq!(v.fade_in, Easing::Bezier(0.1, 0.7, 1.0, 0.1));
        // x는 0~1로 자른다
        let c = VisualBell::parse("1 cubic-bezier(-1, 0, 2, 1)").unwrap();
        assert_eq!(c.fade_in, Easing::Bezier(0.0, 0.0, 1.0, 1.0));
        let s = VisualBell::parse("1 steps(4, jump-start)").unwrap();
        assert_eq!(
            s.fade_in,
            Easing::Steps {
                n: 4,
                jump: Jump::Start
            }
        );
        let l = VisualBell::parse("1 linear(0, 0.25 75%, 1)").unwrap();
        assert!(matches!(l.fade_in, Easing::Linear(..)));
        for (name, want) in [
            ("ease", Easing::Bezier(0.25, 0.1, 0.25, 1.0)),
            ("ease-out", Easing::Bezier(0.0, 0.0, 0.58, 1.0)),
            ("linear", Easing::Bezier(0.0, 0.0, 1.0, 1.0)),
            (
                "step-start",
                Easing::Steps {
                    n: 1,
                    jump: Jump::Start,
                },
            ),
            ("step-end", Easing::Steps { n: 1, jump: Jump::End }),
        ] {
            assert_eq!(VisualBell::parse(&format!("1 {name}")).unwrap().fade_in, want, "{name}");
        }
        for bad in [
            "",
            "fast",
            "1 wobble",
            "1 cubic-bezier(1,2,3)",
            "1 steps(x)",
            "1 ease ease-in ease-out",
            "1 linear(1)",
            "-1",
            "nan",
            "inf",
            "1 steps(2, sideways)",
        ] {
            assert_eq!(VisualBell::parse(bad), None, "{bad:?}");
        }
    }

    #[test]
    fn step_and_linear_curves_follow_kitty() {
        let s = Easing::Steps { n: 4, jump: Jump::End };
        assert_eq!((s.at(0.0), s.at(0.3), s.at(0.99)), (0.0, 0.25, 0.75));
        let st = Easing::Steps {
            n: 4,
            jump: Jump::Start,
        };
        assert_eq!((st.at(0.0), st.at(0.3)), (0.25, 0.5));
        let both = Easing::Steps { n: 2, jump: Jump::Both };
        assert!(near(both.at(0.0), 1.0 / 3.0) && near(both.at(0.6), 2.0 / 3.0));
        let none = Easing::Steps { n: 3, jump: Jump::None };
        assert!(near(none.at(0.0), 0.0) && near(none.at(0.5), 0.5));
        let l = match VisualBell::parse("1 linear(0, 1)").unwrap().fade_in {
            e @ Easing::Linear(..) => e,
            e => panic!("{e:?}"),
        };
        assert!(near(l.at(0.5), 0.5));
        let two = match VisualBell::parse("1 linear(0, 0.5 50%, 1)").unwrap().fade_in {
            e @ Easing::Linear(..) => e,
            e => panic!("{e:?}"),
        };
        assert!(near(two.at(0.25), 0.25) && near(two.at(0.5), 0.5) && near(two.at(0.75), 0.75));
    }

    #[test]
    fn flash_alpha_follows_kitty_attenuation() {
        let dark = Rgb { r: 40, g: 40, b: 40 };
        let light = Rgb { r: 255, g: 255, b: 255 };
        assert_eq!(flash_alpha(0.0, light), 0);
        // 어두운 색은 0.4, 밝은 색(선형 값 0.45 초과)은 0.6
        assert_eq!(flash_alpha(1.0, dark), 102);
        assert_eq!(flash_alpha(1.0, light), 153);
        assert_eq!(flash_alpha(0.5, light), 77);
        // 경계: sRGB 0.7 근처(선형 0.45)
        assert_eq!(flash_alpha(1.0, Rgb { r: 170, g: 0, b: 0 }), 102);
        assert_eq!(flash_alpha(1.0, Rgb { r: 180, g: 0, b: 0 }), 153);
        // 범위 밖 세기는 자른다
        assert_eq!(flash_alpha(2.0, light), 153);
        assert_eq!(flash_alpha(-1.0, light), 0);
    }

    #[test]
    fn render_round_trips() {
        for spec in [
            "0.5",
            "0.3 linear",
            "1 ease-in ease-out",
            "0.25 cubic-bezier(0.1, 0.7, 1, 0.1)",
        ] {
            let v = VisualBell::parse(spec).unwrap();
            assert_eq!(VisualBell::parse(&v.render()), Some(v.clone()), "{spec}");
        }
        assert_eq!(VisualBell::default().render(), "0");
        assert_eq!(VisualBell::parse("0.3 linear").unwrap().render(), "0.3 linear");
    }
}
