//! 칸마다 제목 막대(kitty `window_title_bar`, #916)의 자리와 그리기.
//!
//! kitty처럼 막대가 보이면 칸의 글자 영역에서 막대 높이(글자 한 줄)만큼을 뺀다(`kitty/window.py` `set_geometry`):
//! `top`이면 위 한 줄이 막대고 글자 영역은 그 아래, `bottom`이면 아래 한 줄이 막대다. 그래서 칸 줄 수가 하나 준다.
//! 막대는 칸의 바깥 사각형 안(여백 밖)에 있고, 여백·격자는 막대를 뺀 나머지 안에서 정해진다.
//! 막대를 보이지 않는 설정(`window-title-bar-min-windows = 0`, 기본)이면 이 모듈의 함수는 아무것도 바꾸지 않는다

use crate::split::Rect;
use bt_core::Rgb;
use bt_core::Theme;
use bt_core::tabtitle::{Span, SpanColor};
use bt_core::wintitle::{BarAlign, align_pad};
use bt_render::{Canvas, Fonts, draw_text_styled, text_cells};

/// 그릴 막대 하나
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WinBar {
    pub rect: Rect,
    /// 틀로 그린 조각들. 모두 비었으면 막대를 그리지 않는다(kitty: 틀이 빈 글을 내면 막대를 숨긴다. 자리는 그대로다)
    pub spans: Vec<Span>,
    /// 막대의 글자색·바탕색(`SpanColor::Tab`이 가리키는 색)
    pub fg: Rgb,
    pub bg: Rgb,
    pub align: BarAlign,
}

/// 칸의 바깥 사각형 `outer`에서 막대 높이 `bar_h`를 뺀다: (막대 사각형, 나머지 사각형).
/// 막대를 놓고도 나머지가 있어야 한다(없으면 `None`: 막대를 두지 않는다)
pub fn cut(outer: Rect, bar_h: u32, bottom: bool) -> Option<(Rect, Rect)> {
    if bar_h == 0 || outer.h <= bar_h {
        return None;
    }
    let rest = outer.h - bar_h;
    Some(if bottom {
        (
            Rect::new(outer.x, outer.y + rest as i32, outer.w, bar_h),
            Rect::new(outer.x, outer.y, outer.w, rest),
        )
    } else {
        (
            Rect::new(outer.x, outer.y, outer.w, bar_h),
            Rect::new(outer.x, outer.y + bar_h as i32, outer.w, rest),
        )
    })
}

/// 칸의 바깥 사각형 `outer`에서 (막대 사각형, 글자 영역의 바탕 사각형). 바탕에서 여백을 빼면 격자 자리다.
/// kitty처럼 칸 테두리(#915)는 칸 바깥 둘레이고 제목 막대는 그 안쪽이다(`kitty/window.py` `set_geometry`는 테두리를 뺀
/// 칸 크기에서 막대를 자른다). 그래서 테두리를 먼저 빼고(`border`) 그 안에서 막대를 자른다(`cut`). 막대가 없으면 테두리만 뺀 사각형이다.
/// 테두리 계산은 "탭에 칸이 하나인가"를 바깥 사각형으로 판단하므로 `border`에는 늘 바깥 사각형이 들어가야 한다
pub fn frame_and_cut(
    outer: Rect,
    border: impl Fn(Rect) -> Rect,
    cut: impl Fn(Rect) -> Option<(Rect, Rect)>,
) -> (Option<Rect>, Rect) {
    let framed = border(outer);
    match cut(framed) {
        Some((bar, rest)) => (Some(bar), rest),
        None => (None, framed),
    }
}

/// 막대를 둘지와 자리를 정한다(kitty `layout/base.py`·`window.py set_geometry`): 탭에 보이는 칸이 `visible`개이고 설정이
/// 막대를 보이게 하며(`window-title-bar-min-windows`) 칸이 `pane_rows`줄(막대 줄 포함)로 두 줄 넘게 있을 때만 (막대, 나머지)를 돌려준다.
/// 설정이 없으면(기본 0) 늘 `None`이라 칸 배치가 바뀌지 않는다
pub fn plan(
    cfg: &bt_core::wintitle::WindowTitleConfig,
    visible: usize,
    outer: Rect,
    bar_h: u32,
    pane_rows: u32,
) -> Option<(Rect, Rect)> {
    if !cfg.shown(visible) || pane_rows < 2 {
        return None;
    }
    cut(outer, bar_h, cfg.edge == bt_core::wintitle::BarEdge::Bottom)
}

/// 앞에서부터 `cells`칸어치의 조각들. 넓은 글자가 걸치면 그 글자는 뺀다(kitty는 줄바꿈을 끈 한 줄이라 넘치는 글은 잘린다)
fn fit_cells(spans: &[Span], cells: usize) -> Vec<Span> {
    let mut used = 0;
    let mut out = Vec::new();
    for sp in spans {
        let mut text = String::new();
        let mut full = false;
        for c in sp.text.chars() {
            let w = bt_render::char_cells(c);
            if used + w > cells {
                full = true;
                break;
            }
            text.push(c);
            used += w;
        }
        out.push(Span { text, ..sp.clone() });
        if full {
            break;
        }
    }
    out
}

/// 막대를 그린다. CPU 길과 GPU 길이 같은 함수를 아래층 캔버스에 쓴다(같은 픽셀)
pub fn draw(canvas: &mut Canvas, fonts: &mut Fonts, theme: &Theme, bar: &WinBar) {
    if bar.spans.iter().all(|s| s.text.is_empty()) {
        return;
    }
    let r = bar.rect;
    canvas.fill_rect(r.x, r.y, r.w, r.h, bar.bg);
    let m = fonts.metrics();
    let cw = m.width.max(1);
    let columns = (r.w / cw) as usize;
    let spans = fit_cells(&bar.spans, columns);
    let total: usize = spans.iter().map(|s| text_cells(&s.text)).sum();
    let mut pen = r.x + (align_pad(bar.align, columns, total) as u32 * cw) as i32;
    let y = r.y + (r.h as i32 - m.height as i32) / 2;
    let pick = |c: SpanColor, bar_color: Rgb| match c {
        SpanColor::Tab | SpanColor::Default => bar_color,
        SpanColor::Rgb(c) => c,
        SpanColor::Indexed(n) => theme.indexed(n),
    };
    for sp in &spans {
        let fg = pick(sp.fg, bar.fg);
        let w = cw * text_cells(&sp.text) as u32;
        // 막대 색과 다른 바탕만 칠한다
        let bg = pick(sp.bg, bar.bg);
        if bg != bar.bg {
            canvas.fill_rect(pen, r.y, w, r.h, bg);
        }
        let style = bt_render::FontStyle::from_flags(sp.bold, sp.italic);
        pen += draw_text_styled(canvas, fonts, pen, y, &sp.text, fg, style);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sp(t: &str) -> Span {
        Span {
            text: t.into(),
            fg: SpanColor::Tab,
            bg: SpanColor::Tab,
            bold: false,
            italic: false,
        }
    }

    /// 칸 테두리(#915)와 함께: 테두리는 칸 바깥 둘레, 막대는 그 안쪽이다(kitty `set_geometry`). 테두리 계산은 바깥 사각형을 받는다
    #[test]
    fn border_comes_off_first_and_the_bar_sits_inside_it() {
        use std::cell::RefCell;
        let outer = Rect::new(0, 0, 200, 100);
        let seen = RefCell::new(Vec::new());
        let border = |r: Rect| {
            seen.borrow_mut().push(r);
            Rect::new(r.x + 2, r.y + 2, r.w - 4, r.h - 4)
        };
        let (bar, base) = frame_and_cut(outer, border, |r| cut(r, 16, false));
        assert_eq!(
            seen.borrow().as_slice(),
            &[outer],
            "테두리 계산은 바깥 사각형을 받아야 칸 하나를 알아본다"
        );
        assert_eq!(bar, Some(Rect::new(2, 2, 196, 16)), "막대는 테두리 안쪽 맨 위");
        assert_eq!(base, Rect::new(2, 18, 196, 80), "글자 바탕은 막대 아래, 테두리 안쪽");
        // 아래쪽 막대도 테두리 안쪽
        let (bar, base) = frame_and_cut(outer, border, |r| cut(r, 16, true));
        assert_eq!((bar, base), (Some(Rect::new(2, 82, 196, 16)), Rect::new(2, 2, 196, 80)));
        // 막대가 없으면 테두리만 뺀다(지금과 같다)
        assert_eq!(frame_and_cut(outer, border, |_| None), (None, Rect::new(2, 2, 196, 96)));
    }

    #[test]
    fn cut_takes_the_bar_from_the_top_or_the_bottom_and_keeps_the_rest() {
        let outer = Rect::new(10, 20, 300, 200);
        let (bar, rest) = cut(outer, 16, false).unwrap();
        assert_eq!((bar, rest), (Rect::new(10, 20, 300, 16), Rect::new(10, 36, 300, 184)));
        let (bar, rest) = cut(outer, 16, true).unwrap();
        assert_eq!((bar, rest), (Rect::new(10, 204, 300, 16), Rect::new(10, 20, 300, 184)));
        // 막대와 나머지가 바깥 사각형을 빈틈없이 채운다
        assert_eq!(bar.h + rest.h, outer.h);
        // 나머지가 없으면 막대를 두지 않는다
        assert_eq!(cut(Rect::new(0, 0, 100, 16), 16, false), None);
        assert_eq!(cut(outer, 0, false), None);
    }

    fn cfg(min: u32, bottom: bool) -> bt_core::wintitle::WindowTitleConfig {
        let mut c = bt_core::wintitle::WindowTitleConfig::default();
        c.apply("window-title-bar-min-windows", &min.to_string());
        c.apply("window-title-bar", if bottom { "bottom" } else { "top" });
        c
    }

    /// 설정이 없으면 칸이 몇 개든 막대가 없고 칸 사각형이 그대로다
    #[test]
    fn default_config_never_cuts_anything() {
        let outer = Rect::new(0, 0, 640, 400);
        let c = bt_core::wintitle::WindowTitleConfig::default();
        for visible in 0..12 {
            assert_eq!(plan(&c, visible, outer, 16, 25), None, "{visible}");
        }
    }

    /// 칸이 둘이고 `min-windows = 2`이면 칸마다 막대가 서고 칸 줄 수가 하나씩 준다. 칸이 하나면 없다
    #[test]
    fn two_panes_get_a_bar_each_and_lose_one_row_but_one_pane_gets_none() {
        let (cw, ch) = (8u32, 16u32);
        let c = cfg(2, false);
        let outer = Rect::new(0, 0, 80 * cw, 24 * ch);
        let rows = |r: Rect| r.h / ch;
        assert_eq!(rows(outer), 24);
        for pane in 0..2 {
            let (bar, rest) = plan(&c, 2, outer, ch, rows(outer)).unwrap_or_else(|| panic!("칸 {pane}"));
            assert_eq!(rows(rest), 23);
            assert_eq!(bar, Rect::new(0, 0, 80 * cw, ch));
            assert_eq!(rest.y, ch as i32);
        }
        assert_eq!(plan(&c, 1, outer, ch, 24), None, "칸이 하나면 막대가 없다");
        // 아래에 두면 막대가 맨 아래 한 줄이다
        let (bar, rest) = plan(&cfg(2, true), 2, outer, ch, 24).unwrap();
        assert_eq!((bar.y, rest.y, rows(rest)), (23 * ch as i32, 0, 23));
        // 1이면 칸이 하나여도 늘 보인다
        assert!(plan(&cfg(1, false), 1, outer, ch, 24).is_some());
        // 칸이 한 줄뿐이면 막대를 두지 않는다(kitty: ynum > 1)
        assert_eq!(plan(&cfg(1, false), 1, Rect::new(0, 0, 640, ch), ch, 1), None);
        // 여백 때문에 줄이 하나뿐인 칸도(바깥 사각형은 더 커도) 막대를 두지 않는다
        assert_eq!(plan(&cfg(1, false), 1, Rect::new(0, 0, 640, 3 * ch), ch, 1), None);
        // 셋 이상에서 시작하는 설정
        assert_eq!(plan(&cfg(3, false), 2, outer, ch, 24), None);
        assert!(plan(&cfg(3, false), 3, outer, ch, 24).is_some());
    }

    #[test]
    fn fit_cells_cuts_wide_chars_and_keeps_the_style() {
        let mut a = sp("ab");
        a.bold = true;
        let out = fit_cells(&[a.clone(), sp("가나다")], 5);
        assert_eq!(out.iter().map(|s| s.text.as_str()).collect::<String>(), "ab가");
        assert!(out[0].bold);
        // 두 칸 글자가 걸치면 뺀다
        assert_eq!(fit_cells(&[sp("a가")], 2)[0].text, "a");
        assert_eq!(fit_cells(&[sp("abc")], 0)[0].text, "");
        assert_eq!(fit_cells(&[sp("abc")], 9)[0].text, "abc");
    }
    // ---- 그리기(PTY 없이 캔버스에) ----

    const BG: Rgb = Rgb { r: 0, g: 0, b: 0 };
    const FG: Rgb = Rgb { r: 255, g: 255, b: 255 };
    const OUTSIDE: Rgb = Rgb { r: 9, g: 8, b: 7 };

    fn bar_of(spans: Vec<Span>, align: BarAlign, w: u32, h: u32) -> WinBar {
        WinBar {
            rect: Rect::new(10, 5, w, h),
            spans,
            fg: FG,
            bg: BG,
            align,
        }
    }

    /// 막대 사각형 안에서 글자 픽셀(바탕이 아닌 곳)이 있는 가로 범위. 사각형 밖은 건드리지 않았는지도 본다
    fn ink(bar: &WinBar, fonts: &mut Fonts) -> Option<(u32, u32, Vec<Rgb>)> {
        let (cw, ch) = (480u32, 120u32);
        let mut buf = vec![0u32; (cw * ch) as usize];
        let mut c = Canvas::new(&mut buf, cw, ch);
        c.clear(OUTSIDE);
        draw(&mut c, fonts, &Theme::ghostty_default(), bar);
        let r = bar.rect;
        for y in 0..ch {
            for x in 0..cw {
                let inside = x >= r.x as u32 && x < r.x as u32 + r.w && y >= r.y as u32 && y < r.y as u32 + r.h;
                if !inside {
                    assert_eq!(c.pixel(x, y), OUTSIDE, "막대 밖 ({x},{y})을 건드렸다");
                }
            }
        }
        let (mut lo, mut hi, mut seen) = (u32::MAX, 0, Vec::new());
        for y in r.y as u32..r.y as u32 + r.h {
            for x in r.x as u32..r.x as u32 + r.w {
                let p = c.pixel(x, y);
                if p != BG {
                    lo = lo.min(x);
                    hi = hi.max(x);
                    seen.push(p);
                }
            }
        }
        (lo != u32::MAX).then_some((lo, hi, seen))
    }

    #[test]
    fn bar_is_filled_and_text_follows_the_alignment() {
        let Ok(mut fonts) = Fonts::new(&bt_render::FontConfig::default()) else {
            return;
        };
        let m = fonts.metrics();
        let (w, h) = (m.width * 40, m.height);
        let mut at = |align| ink(&bar_of(vec![sp("MMMM")], align, w, h), &mut fonts).expect("글자가 있다");
        let (l0, l1, _) = at(BarAlign::Left);
        let (c0, c1, _) = at(BarAlign::Center);
        let (r0, r1, _) = at(BarAlign::Right);
        let rect = Rect::new(10, 5, w, h);
        // 왼쪽은 막대 왼쪽 끝, 오른쪽은 오른쪽 끝, 가운데는 그 사이 가운데(kitty `(칸 - 글) // 2`칸 뒤)
        assert!(l0 < rect.x as u32 + m.width, "왼쪽 맞춤 {l0}");
        assert!(r1 + m.width >= rect.right() as u32, "오른쪽 맞춤 {r1}");
        let center_pad = (40 - 4) / 2;
        assert!(
            c0 >= rect.x as u32 + center_pad * m.width && c0 < rect.x as u32 + (center_pad + 1) * m.width,
            "가운데 {c0}"
        );
        assert!(l1 < c0 && c1 < r0, "{l1} {c0} {c1} {r0}");
        // 바탕 한 점은 막대 바탕색이다(막대 밖은 `ink`가 이미 건드리지 않았음을 확인했다)
        let mut buf = vec![0u32; 480 * 120];
        let mut c = Canvas::new(&mut buf, 480, 120);
        c.clear(OUTSIDE);
        let mut bar = bar_of(vec![sp("M")], BarAlign::Left, w, h);
        bar.bg = Rgb::new(10, 20, 30);
        draw(&mut c, &mut fonts, &Theme::ghostty_default(), &bar);
        assert_eq!(
            c.pixel(rect.right() as u32 - 1, rect.y as u32 + 1),
            Rgb::new(10, 20, 30)
        );
    }

    /// 빈 글이면 막대를 그리지 않는다(자리만 남는다)
    #[test]
    fn empty_title_draws_nothing() {
        let Ok(mut fonts) = Fonts::new(&bt_render::FontConfig::default()) else {
            return;
        };
        let m = fonts.metrics();
        for spans in [vec![sp("")], Vec::new()] {
            let bar = bar_of(spans, BarAlign::Center, m.width * 20, m.height);
            let mut buf = vec![0u32; 480 * 120];
            let mut c = Canvas::new(&mut buf, 480, 120);
            c.clear(OUTSIDE);
            draw(&mut c, &mut fonts, &Theme::ghostty_default(), &bar);
            let r = bar.rect;
            assert_eq!(c.pixel(r.x as u32 + 3, r.y as u32 + 2), OUTSIDE, "바탕도 칠하지 않는다");
        }
    }

    /// 칸 안 프로그램이 OSC 2로 보낸 `{fmt.fg.red}x`는 글자 그대로 그려지고 색을 바꾸지 못한다: 글자 픽셀이 모두 회색(흰색 글자 + 검정 바탕)이다
    #[test]
    fn osc_title_with_fmt_text_is_drawn_literally_without_color() {
        let Ok(mut fonts) = Fonts::new(&bt_render::FontConfig::default()) else {
            return;
        };
        let m = fonts.metrics();
        let tab = bt_core::tabtitle::TabTitleConfig::default();
        let d = bt_core::tabtitle::WindowData {
            title: "{fmt.fg.red}x\x1b[31m",
            ..Default::default()
        };
        let spans = bt_core::tabtitle::render_window(bt_core::wintitle::DEFAULT_TEMPLATE, &tab, &d);
        assert_eq!(
            spans.iter().map(|s| s.text.as_str()).collect::<String>(),
            "{fmt.fg.red}x[31m"
        );
        let bar = bar_of(spans, BarAlign::Left, m.width * 40, m.height);
        let (_, _, seen) = ink(&bar, &mut fonts).expect("글자가 있다");
        assert!(seen.iter().all(|p| p.r == p.g && p.g == p.b), "글자에 색이 들어갔다");
        // 틀이 꾸민 글(빨강)은 빨갛게 그려진다: 위 시험이 색을 못 보는 것이 아님을 보인다
        let red = bt_core::tabtitle::render_window("{fmt.fg.red}x", &tab, &d);
        let (_, _, seen) = ink(&bar_of(red, BarAlign::Left, m.width * 40, m.height), &mut fonts).unwrap();
        assert!(seen.iter().any(|p| p.r > p.g + 100), "틀의 빨강이 그려지지 않았다");
    }

    /// 같은 입력은 같은 픽셀이다(CPU 길과 GPU 길이 같은 함수를 아래층 캔버스에 쓴다)
    #[test]
    fn same_input_draws_the_same_pixels() {
        let Ok(mut fonts) = Fonts::new(&bt_render::FontConfig::default()) else {
            return;
        };
        let m = fonts.metrics();
        let bar = bar_of(vec![sp("한글 title")], BarAlign::Center, m.width * 30, m.height);
        let frame = |fonts: &mut Fonts| {
            let mut buf = vec![0u32; 480 * 120];
            let mut c = Canvas::new(&mut buf, 480, 120);
            c.clear(OUTSIDE);
            draw(&mut c, fonts, &Theme::ghostty_default(), &bar);
            buf
        };
        assert_eq!(frame(&mut fonts), frame(&mut fonts));
    }
}
