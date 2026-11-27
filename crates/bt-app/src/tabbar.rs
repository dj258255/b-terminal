//! 가로 탭 줄(Ghostty처럼). 탭이 둘 이상일 때만 창 위쪽에 그린다

use bt_core::tabstyle::{TabBarAlign, TabBarLook, TabBarStyle, TabPowerlineStyle};
use bt_core::{Rgb, Theme};
use bt_render::proportional::{draw_text_proportional, draw_text_proportional_styled};
use bt_render::{Canvas, Fonts, draw_text, draw_text_styled, text_cells, truncate};

/// 탭 줄 한 칸
#[derive(Debug, Clone)]
pub struct TabLabel {
    pub title: String,
    pub active: bool,
    /// 보지 않은 알림(OSC 9·99·777, 벨)이 있다
    pub unread: bool,
    /// 진행 표시(OSC 9;4): (상태 1 보통·2 오류·3 끝 모름·4 멈춤, 퍼센트).
    /// 프로그램 상태(OSC 7501)도 같은 막대로 보인다: 5는 사용자를 기다림, 6은 끝남(`status_bar`)
    pub progress: Option<(u8, u8)>,
    /// 사람이 고른 탭 색(#852). 있으면 제목 오른쪽에 작은 점
    pub color: crate::tabcolor::TabColor,
}

/// 막대 상태 코드: 프로그램 상태(OSC 7501)에서 온 것(`status_bar`)
const BAR_BLOCKED: u8 = 5;
const BAR_DONE: u8 = 6;
/// 오류(2)인데 값이 없다: 빨간 끝 모름 막대(Ghostty macOS는 값 없는 오류를 빨간 맥박으로 그린다)
const BAR_ERROR_UNKNOWN: u8 = 7;

/// 칸들의 프로그램 상태 가운데 가장 급한 것(`blocked` > `error` > `working` > `done`). 같으면 앞의 것
pub fn most_urgent(
    list: impl IntoIterator<Item = bt_term::program_status::Summary>,
) -> Option<bt_term::program_status::Summary> {
    use bt_term::program_status::State;
    let rank = |s: State| match s {
        State::Blocked => 3,
        State::Error => 2,
        State::Working => 1,
        _ => 0,
    };
    let mut best: Option<bt_term::program_status::Summary> = None;
    for s in list {
        if best.is_none_or(|b| rank(s.state) > rank(b.state)) {
            best = Some(s);
        }
    }
    best
}

/// 프로그램 상태(OSC 7501)를 진행 막대 값으로 바꾼다. 사용자를 기다림·오류·끝남은 가득 찬 막대(마젠타·빨강·초록)이고,
/// 일하는 중은 퍼센트가 있으면 보통 막대, 없으면 가운데 3분의 1을 칠한 정지한 막대다(움직이지 않는다).
/// `bars`가 거짓(`progress-style = false`)이면 일하는 중은 그리지 않는다. 기다림·오류·끝남은 사람이 봐야 하므로 그대로 그린다.
/// 같은 자리의 안 읽음 점(노랑)과 색·모양이 겹치지 않는다
pub fn status_bar(s: bt_term::program_status::Summary, bars: bool) -> Option<(u8, u8)> {
    use bt_term::program_status::State;
    match s.state {
        State::Blocked => Some((BAR_BLOCKED, 100)),
        State::Error => Some((2, 100)),
        State::Done => Some((BAR_DONE, 100)),
        State::Working => bars.then(|| s.progress.map_or((3, 0), |p| (1, p))),
        State::Idle | State::Clear => None,
    }
}

/// 프로그램이 보낸 진행 표시(OSC 9;4)를 받는가: `progress-style`이 꺼져 있으면 조용히 버린다.
/// 상태 0(지움)도 표시를 남기지 않는다. 받으면 (상태, 퍼센트).
/// 값이 없을 때는 Ghostty macOS(`SurfaceProgressBar.swift`)처럼: 보통·오류는 끝 모름 막대(오류는 빨강), 멈춤은 가득 찬 막대(주황)이고 끝 모름은 그대로다(#764)
pub fn accept_progress(style: bool, state: u8, percent: Option<u8>) -> Option<(u8, u8)> {
    if !style || state == 0 {
        return None;
    }
    Some(match (state, percent) {
        (2, None) => (BAR_ERROR_UNKNOWN, 0),
        (4, None) => (4, 100),
        (1, None) => (3, 0),
        (_, p) => (state, p.unwrap_or(0)),
    })
}

/// 진행 막대를 (x, y)에서 폭 w로 그린다. 끝을 모르면(3) 가운데 3분의 1을 칠한다
pub fn draw_progress(canvas: &mut Canvas, theme: &Theme, x: i32, y: i32, w: u32, p: (u8, u8), h: u32) {
    let (state, percent) = p;
    let color = match state {
        2 | BAR_ERROR_UNKNOWN => theme.palette[1],
        4 => theme.palette[3],
        BAR_BLOCKED => theme.palette[5],
        BAR_DONE => theme.palette[2],
        _ => theme.palette[4],
    };
    let h = h.max(2);
    canvas.fill_rect(x, y, w, h, theme.background.mix(theme.foreground, 0.12));
    match state {
        3 | BAR_ERROR_UNKNOWN => canvas.fill_rect(x + w as i32 / 3, y, w / 3, h, color),
        _ => canvas.fill_rect(
            x,
            y,
            (w as f32 * percent.min(100) as f32 / 100.0).round() as u32,
            h,
            color,
        ),
    }
}

/// 탭 줄의 높이·구분선 같은 자리를 셀 글꼴: 제목 글꼴이 있으면 그것(터미널 글꼴 크기를 따르지 않는다, #474),
/// 없으면 터미널 글꼴. 그리는 쪽(`draw_with_title_font`)과 높이를 세는 쪽이 이것을 거친다. 탭 자리(`slots`·`hit`)는 글꼴을 보지 않는다(#478)
pub fn bar_fonts<'a>(terminal: &'a Fonts, title: Option<&'a Fonts>) -> &'a Fonts {
    title.unwrap_or(terminal)
}

/// 터미널 글꼴로 셀 때의 탭 줄 높이(px): 셀 높이의 1.5배
fn height_px(fonts: &Fonts) -> u32 {
    let m = fonts.metrics();
    m.height + m.height / 2
}

/// 탭 줄 높이(px). 그리는 쪽·격자 자리·새 탭 줄 자리가 모두 이것을 쓴다.
/// 제목 글꼴이 있으면 그 셀 높이의 5/3이다(#474). 시스템 UI 글꼴(SF) 13pt는 셀 높이가 터미널 글꼴보다 낮아(배율 2에서 31px)
/// 1.5배면 46px로 0.50.0의 52px보다 눈에 띄게 낮아진다. 5/3이면 51px다. 제목 글꼴이 없으면 터미널 셀 높이의 1.5배 그대로다
pub fn bar_height(terminal: &Fonts, title: Option<&Fonts>) -> u32 {
    match title {
        Some(t) => {
            let h = t.metrics().height;
            h + h * 2 / 3
        }
        None => height_px(terminal),
    }
}

/// 탭 자리를 고르는 방식. 기본 모양은 창 폭을 탭 수로 고르게 나눠 가득 채운다(`max_tab`이 없음).
/// kitty식 모양(`tab-bar-style = fade` 등)은 탭 하나의 폭에 상한이 있어 탭이 모자라면 `align`대로 한쪽에 모인다(#901)
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Layout {
    pub max_tab: Option<u32>,
    pub align: TabBarAlign,
}

/// kitty식 모양에서 탭 하나의 상한을 몇 칸으로 보는가
const MAX_TAB_CELLS: u32 = 24;

impl Layout {
    /// 설정과 탭 줄 글꼴의 칸 폭(px)에서 자리 방식을 정한다
    pub fn of(look: &TabBarLook, cell_w: u32) -> Self {
        match look.style {
            TabBarStyle::Default | TabBarStyle::Hidden => Self::default(),
            _ => Self {
                max_tab: Some(cell_w.max(1) * MAX_TAB_CELLS),
                align: look.align,
            },
        }
    }
}

/// 탭마다 (x, 폭). 상한 없이 창 폭을 탭 수로 고르게 나누고, 나머지 픽셀은 마지막 탭이 가져 오른쪽 끝까지 채운다(#478).
/// 폭이 탭 수보다 작으면 탭마다 1px로 하고 창 밖으로 넘어간다(패닉은 없다).
/// `max_tab`이 있고 나눈 폭이 그보다 크면 모든 탭이 그 폭이고 줄 전체를 `align`대로 놓는다
fn slots(width: u32, count: usize, layout: Layout) -> Vec<(i32, u32)> {
    if count == 0 {
        return Vec::new();
    }
    let w = (width / count as u32).max(1);
    if let Some(cap) = layout.max_tab.filter(|&cap| w > cap) {
        let total = cap * count as u32;
        let start = match layout.align {
            TabBarAlign::Left => 0,
            TabBarAlign::Center => (width.saturating_sub(total) / 2) as i32,
            TabBarAlign::Right => width.saturating_sub(total) as i32,
        };
        return (0..count).map(|i| (start + (i as u32 * cap) as i32, cap)).collect();
    }
    let last = width.saturating_sub(w * (count as u32 - 1)).max(1);
    (0..count)
        .map(|i| (i as i32 * w as i32, if i + 1 == count { last } else { w }))
        .collect()
}

/// 탭 줄에서 쓸 색들. 적지 않은 색은 지금처럼 테마에서 고른다
struct Palette {
    bar: Rgb,
    active_bg: Rgb,
    active_fg: Rgb,
    /// 기본 모양에서는 정했을 때만 칠한다(`None`이면 막대 바탕이 보인다)
    inactive_bg: Option<Rgb>,
    inactive_fg: Rgb,
}

impl Palette {
    fn new(theme: &Theme, look: &TabBarLook) -> Self {
        let styled = !matches!(look.style, TabBarStyle::Default | TabBarStyle::Hidden);
        Self {
            bar: look
                .bar_background
                .unwrap_or_else(|| theme.background.mix(theme.foreground, 0.06)),
            active_bg: look.active_background.unwrap_or(theme.background),
            active_fg: look.active_foreground.unwrap_or(theme.foreground),
            inactive_bg: look
                .inactive_background
                .or_else(|| styled.then(|| theme.background.mix(theme.foreground, 0.14))),
            inactive_fg: look
                .inactive_foreground
                .unwrap_or_else(|| theme.foreground.mix(theme.background, 0.45)),
        }
    }
    fn bg(&self, active: bool) -> Rgb {
        if active {
            self.active_bg
        } else {
            self.inactive_bg.unwrap_or(self.bar)
        }
    }
}

/// 파워라인 기호의 꼴. 글꼴에 기호가 없어도 그려지게 직접 칠한다(U+E0B0·E0B4·E0BC)
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Shape {
    /// U+E0B0: 오른쪽을 가리키는 세모
    Angled,
    /// U+E0B4: 오른쪽 반원
    Round,
    /// U+E0BC: 왼쪽 위 직각 세모(왼쪽에서 오른쪽 아래로 기운다)
    Slanted,
}

/// 칸 높이 `h` 중 위에서 `half`(0~1)쯤 되는 줄에서 기호가 왼쪽에서 차지하는 폭의 비율(0~1)
fn shape_edge(shape: Shape, half: f32) -> f32 {
    match shape {
        Shape::Angled => 1.0 - (2.0 * half - 1.0).abs(),
        Shape::Round => (1.0 - (2.0 * half - 1.0).powi(2)).max(0.0).sqrt(),
        Shape::Slanted => 1.0 - half,
    }
}

/// 칸 `(x, y, cw, h)`의 줄마다 `edge`(px, 소수)만큼을 왼쪽(또는 오른쪽)에서 `color`로 칠한다. 가장자리 한 픽셀은 덮음 정도로 섞는다
#[allow(clippy::too_many_arguments)]
fn fill_cover(
    canvas: &mut Canvas,
    x: i32,
    y: i32,
    cw: u32,
    h: u32,
    edge: impl Fn(f32) -> f32,
    from_right: bool,
    color: Rgb,
) {
    for r in 0..h {
        let cover = (edge((r as f32 + 0.5) / h as f32) * cw as f32).clamp(0.0, cw as f32);
        let full = cover.floor() as u32;
        let frac = cover - full as f32;
        let (fx, ax) = if from_right {
            (x + cw as i32 - full as i32, x + cw as i32 - full as i32 - 1)
        } else {
            (x, x + full as i32)
        };
        canvas.fill_rect(fx, y + r as i32, full, 1, color);
        if frac > 0.02 && full < cw {
            canvas.blend_px(ax, y + r as i32, color, (frac * 255.0).round() as u8);
        }
    }
}

/// 가장자리(`shape_edge`)를 따라 두께 `t` px의 선만 긋는다(파워라인의 비어 있는 구분 기호)
#[allow(clippy::too_many_arguments)]
fn stroke_edge(canvas: &mut Canvas, x: i32, y: i32, cw: u32, h: u32, shape: Shape, t: u32, color: Rgb) {
    for r in 0..h {
        let e = (shape_edge(shape, (r as f32 + 0.5) / h as f32) * cw as f32).round() as i32;
        let start = (e - t as i32).max(0);
        canvas.fill_rect(x + start, y + r as i32, (e - start).max(0) as u32, 1, color);
    }
}

/// 이 모양에서 한 탭의 안쪽(제목·점·진행 막대가 들어가는 자리): (x, 폭)
fn content_rect(look: &TabBarLook, x: i32, w: u32, cw: u32, last: bool) -> (i32, u32) {
    let cells = |n: u32| (n * cw).min(w / 2);
    match look.style {
        TabBarStyle::Default | TabBarStyle::Hidden => (x, w),
        TabBarStyle::Fade => {
            let n = fade_cells(look, w, cw);
            (x + (n * cw) as i32, w.saturating_sub((2 * n + 1) * cw).max(1))
        }
        TabBarStyle::Slant => (x + cells(1) as i32, w.saturating_sub(2 * cells(1)).max(1)),
        TabBarStyle::Separator => {
            let sep = if last { 0 } else { separator_width(look, cw) };
            (x, w.saturating_sub(sep).max(1))
        }
        TabBarStyle::Powerline => (x, w.saturating_sub(cells(1)).max(1)),
    }
}

/// 탭 폭 `w`에 들어가는 번짐 칸 수: 양쪽 번짐과 끝 한 칸을 빼고도 제목 자리가 남게 줄인다
fn fade_cells(look: &TabBarLook, w: u32, cw: u32) -> u32 {
    let room = (w / cw.max(1)).saturating_sub(3) / 2;
    (look.fade.len() as u32).min(room)
}

fn separator_width(look: &TabBarLook, cw: u32) -> u32 {
    let glyph = bt_core::tabstyle::separator_glyph(&look.separator);
    cw * text_cells(glyph) as u32
}

/// 탭 하나의 바탕과 모양(번짐·기울임·구분 기호)을 칠한다. 제목·점·막대는 `draw_with_title_font`가 그 위에 그린다
#[allow(clippy::too_many_arguments)]
fn draw_tab_shape(
    canvas: &mut Canvas,
    look: &TabBarLook,
    pal: &Palette,
    tab: &TabLabel,
    next: Option<&TabLabel>,
    (x, w): (i32, u32),
    top: i32,
    h: u32,
    cw: u32,
    stroke: u32,
) {
    let bg = pal.bg(tab.active);
    let cwi = cw as i32;
    match look.style {
        TabBarStyle::Default | TabBarStyle::Hidden => {}
        TabBarStyle::Fade => {
            let n = fade_cells(look, w, cw);
            let right = x + w as i32 - cwi - (n * cw) as i32;
            canvas.fill_rect(
                x + (n * cw) as i32,
                top,
                (right - x - (n * cw) as i32).max(0) as u32,
                h,
                bg,
            );
            for i in 0..n as usize {
                let c = pal.bar.mix(bg, look.fade[i]);
                canvas.fill_rect(x + (i as u32 * cw) as i32, top, cw, h, c);
                let c = pal.bar.mix(bg, look.fade[n as usize - 1 - i]);
                canvas.fill_rect(right + (i as u32 * cw) as i32, top, cw, h, c);
            }
        }
        TabBarStyle::Slant => {
            canvas.fill_rect(x, top, w, h, bg);
            let c = cw.min(w / 2);
            // 막대 바탕이 위쪽 모서리를 깎는다: 왼쪽은 U+E0BC, 오른쪽은 U+E0BE(좌우 뒤집음)
            fill_cover(
                canvas,
                x,
                top,
                c,
                h,
                |half| shape_edge(Shape::Slanted, half),
                false,
                pal.bar,
            );
            fill_cover(
                canvas,
                x + w as i32 - c as i32,
                top,
                c,
                h,
                |half| shape_edge(Shape::Slanted, half),
                true,
                pal.bar,
            );
        }
        TabBarStyle::Separator => {
            let sepw = if next.is_some() { separator_width(look, cw) } else { 0 };
            canvas.fill_rect(x, top, w - sepw.min(w), h, bg);
            if sepw > 0 {
                let sx = x + w as i32 - sepw as i32;
                canvas.fill_rect(sx, top, sepw, h, pal.inactive_bg.unwrap_or(pal.bar));
            }
        }
        TabBarStyle::Powerline => {
            let c = cw.min(w / 2);
            canvas.fill_rect(x, top, w - c, h, bg);
            let sx = x + (w - c) as i32;
            let next_bg = next.map_or(pal.bar, |n| pal.bg(n.active));
            if next.is_some() && next_bg == bg {
                // 이웃 탭과 색이 같으면 이어 붙지 않게 속이 빈 구분 기호(kitty의 soft separator)
                canvas.fill_rect(sx, top, c, h, bg);
                let line = soft_color(pal, bg);
                let shape = shape_of(look.powerline);
                stroke_edge(canvas, sx, top, c, h, shape, stroke.max(1), line);
            } else {
                canvas.fill_rect(sx, top, c, h, next_bg);
                let shape = shape_of(look.powerline);
                fill_cover(canvas, sx, top, c, h, |half| shape_edge(shape, half), false, bg);
            }
        }
    }
}

fn shape_of(style: TabPowerlineStyle) -> Shape {
    match style {
        TabPowerlineStyle::Angled => Shape::Angled,
        TabPowerlineStyle::Round => Shape::Round,
        TabPowerlineStyle::Slanted => Shape::Slanted,
    }
}

/// kitty `draw_tab_with_powerline`의 soft separator 색
fn soft_color(pal: &Palette, tab_bg: Rgb) -> Rgb {
    let fg = pal.inactive_fg;
    if tab_bg == fg {
        pal.bar
    } else if tab_bg != pal.bar {
        let inactive = pal.inactive_bg.unwrap_or(pal.bar);
        if inactive.contrast(pal.bar) < inactive.contrast(fg) {
            pal.bar
        } else {
            fg
        }
    } else {
        fg
    }
}

/// 탭 줄을 `top`(창 위에서 내려온 픽셀)부터 그린다. 제목 막대 밑으로 콘텐츠가 들어가는 창에서는 `top`이 제목 막대 높이다(#192).
/// 제목 글꼴을 따로 쓰는 탭 줄(#241 `window-title-font-family`). `title_fonts`가 없으면 터미널 글꼴로 셀 격자에 맞춰 그린다.
/// 있으면 글자마다 advance로 자리를 잡고(비례폭도 겹치지 않게) 잘림도 그 폭으로 센다. 탭 높이·구분선·안 읽음 점·진행 막대도 제목 글꼴 지표로 센다(#474).
/// `look`은 `tab-bar-style`과 탭 색·글꼴 모양이다. 기본값이면 지금까지의 픽셀과 같다(#901)
pub fn draw_with_title_font(
    canvas: &mut Canvas,
    fonts: &mut Fonts,
    mut title_fonts: Option<&mut Fonts>,
    theme: &Theme,
    look: &TabBarLook,
    tabs: &[TabLabel],
    top: i32,
) {
    // 높이·구분선·점·진행 막대는 모두 제목 글꼴이 있으면 그 지표로 센다(#474). 탭 자리는 누른 자리 판정(`hit`)과 같은 `slots`다
    let m = bar_fonts(fonts, title_fonts.as_deref()).metrics();
    let h = bar_height(fonts, title_fonts.as_deref());
    let pal = Palette::new(theme, look);
    let default_look = matches!(look.style, TabBarStyle::Default | TabBarStyle::Hidden);
    let border = theme.background.mix(theme.foreground, 0.16);
    canvas.fill_rect(0, top, canvas.width, h, pal.bar);
    if default_look {
        canvas.fill_rect(0, top + h as i32 - 1, canvas.width, 1, border);
    }
    let cw = m.width as i32;
    let layout = Layout::of(look, m.width);
    let slot_list = slots(canvas.width, tabs.len(), layout);
    for (i, (tab, &(x, w))) in tabs.iter().zip(&slot_list).enumerate() {
        if default_look {
            if tab.active {
                canvas.fill_rect(x, top, w, h, pal.active_bg);
            } else if let Some(c) = pal.inactive_bg {
                canvas.fill_rect(x, top, w, h, c);
            }
            canvas.fill_rect(x + w as i32 - 1, top + m.height as i32 / 4, 1, h - m.height / 2, border);
        } else {
            draw_tab_shape(
                canvas,
                look,
                &pal,
                tab,
                tabs.get(i + 1),
                (x, w),
                top,
                h,
                m.width,
                m.stroke,
            );
        }
        if look.style == TabBarStyle::Separator && i + 1 < tabs.len() {
            let glyph = bt_core::tabstyle::separator_glyph(&look.separator);
            let sepw = separator_width(look, m.width) as i32;
            let sx = x + w as i32 - sepw;
            if let Some(tf) = title_fonts.as_deref_mut() {
                let ty = top + (h as i32 - tf.metrics().height as i32) / 2;
                draw_text_proportional(canvas, tf, sx, ty, glyph, pal.inactive_fg);
            } else if !glyph.is_empty() {
                let ty = top + (h as i32 - m.height as i32) / 2;
                draw_text(canvas, fonts, sx, ty, glyph, pal.inactive_fg, false);
            }
        }
        let (cx, cwid) = content_rect(look, x, w, m.width, i + 1 == tabs.len());
        // 가운데 맞춘 제목. 안 읽음 점 자리를 남긴다
        let color = if tab.active { pal.active_fg } else { pal.inactive_fg };
        let style = bt_render::FontStyle::from_flags(
            if tab.active {
                look.active_font
            } else {
                look.inactive_font
            }
            .bold(),
            if tab.active {
                look.active_font
            } else {
                look.inactive_font
            }
            .italic(),
        );
        if let Some(tf) = title_fonts.as_deref_mut() {
            let room_px = (cwid as i32 - 4 * cw).max(cw) as f32;
            let (title, tw) = fit_title(&tab.title, room_px, |c| tf.advance(c));
            let tx = cx + (cwid as i32 - tw.round() as i32) / 2;
            let ty = top + (h as i32 - tf.metrics().height as i32) / 2;
            draw_text_proportional_styled(canvas, tf, tx, ty, &title, color, style);
        } else {
            let max_cells = ((cwid as i32 / cw) - 4).max(1) as usize;
            let title = truncate(&tab.title, max_cells);
            let tw = cw * text_cells(&title) as i32;
            let tx = cx + (cwid as i32 - tw) / 2;
            let ty = top + (h as i32 - m.height as i32) / 2;
            draw_text_styled(canvas, fonts, tx, ty, &title, color, style);
        }
        if let Some(p) = tab.progress {
            draw_progress(
                canvas,
                theme,
                cx,
                top + h as i32 - (m.stroke * 3) as i32 - 1,
                cwid,
                p,
                m.stroke * 3,
            );
        }
        if tab.unread {
            dot(canvas, cx + cw, top + h as i32 / 2, (cw / 3).max(2), theme.palette[3]);
        }
        // 탭 색(#852): Ghostty처럼 탭 오른쪽 끝(제목 뒤)의 작은 점. 고른 탭과 아닌 탭이 같다. 제목 자리(양쪽 두 칸)의 오른쪽 칸 가운데
        if let Some(rgb) = tab.color.rgb(crate::tabcolor::is_dark(theme.background)) {
            dot(
                canvas,
                cx + cwid as i32 - cw - cw / 2,
                top + h as i32 / 2,
                (cw * 3 / 8).max(2),
                rgb,
            );
        }
    }
}

/// 제목 글꼴을 따로 쓸 때(#241) 탭 하나의 제목을 방 `room_px` 안에 맞춘다: (잘린 제목, 그린 폭 px).
/// 폭은 글자마다의 advance 합이라 비례폭 글꼴에서도 글자가 겹치거나 탭 밖으로 나가지 않는다
pub fn fit_title(title: &str, room_px: f32, mut advance: impl FnMut(char) -> f32) -> (String, f32) {
    let fitted = bt_render::proportional::truncate_width(title, room_px, &mut advance);
    let width = bt_render::proportional::text_width(&fitted, &mut advance);
    (fitted, width)
}

/// 창 좌표가 몇 번째 탭인가
pub fn hit(width: u32, x: f64, count: usize, layout: Layout) -> Option<usize> {
    slots(width, count, layout)
        .iter()
        .position(|&(sx, w)| x >= sx as f64 && x < (sx + w as i32) as f64)
}

/// 탭을 끌기로 보는 가로 거리(물리 픽셀). 누른 자리에서 이만큼 안 움직인 누름은 고르기다(손 떨림으로 탭이 옮겨지지 않게)
pub const DRAG_START_PX: f64 = 6.0;

/// 탭 줄에서 왼쪽 단추를 누른 뒤의 끌기 상태. 창 없이 시험할 수 있게 좌표와 숫자만 안다
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct TabDrag {
    press: Option<(usize, f64)>,
    dragging: bool,
}

/// 단추를 놓았을 때 할 일
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TabDrop {
    /// 짧은 누름: 이미 골랐으니 더 할 것이 없다
    None,
    /// `from` 탭을 `to` 자리로 옮긴다
    Move { from: usize, to: usize },
}

impl TabDrag {
    /// 탭 `from`을 `x`에서 눌렀다
    pub fn press(&mut self, from: usize, x: f64) {
        self.press = Some((from, x));
        self.dragging = false;
    }
    /// 누르는 중인가(끌기 시작 전 포함). 이 동안 다른 누름은 아무 데도 가지 않는다
    pub fn is_pressed(&self) -> bool {
        self.press.is_some()
    }
    /// 끌기가 시작됐는가(시험용)
    #[cfg(test)]
    pub fn is_dragging(&self) -> bool {
        self.dragging
    }
    /// 마우스가 `x`로 움직였다. 이번에 끌기가 새로 시작됐으면 참
    pub fn moved(&mut self, x: f64) -> bool {
        match self.press {
            Some((_, x0)) if !self.dragging && (x - x0).abs() >= DRAG_START_PX => {
                self.dragging = true;
                true
            }
            _ => false,
        }
    }
    /// 단추를 `x`에서 놓았다. 상태는 지워진다
    pub fn release(&mut self, x: f64, width: u32, count: usize, layout: Layout) -> TabDrop {
        let (press, dragging) = (self.press.take(), std::mem::take(&mut self.dragging));
        match press {
            Some((from, _)) if dragging && count > 1 => match drop_index(width, x, count, layout) {
                Some(to) if to != from => TabDrop::Move { from, to },
                _ => TabDrop::None,
            },
            _ => TabDrop::None,
        }
    }
    /// 누름을 버린다(창이 초점을 잃었을 때 등)
    pub fn cancel(&mut self) {
        *self = Self::default();
    }
}

/// 끌어서 놓은 자리(`x`)의 탭 번호. 탭 줄 밖(왼쪽·오른쪽)이면 처음·끝 탭으로 자른다
pub fn drop_index(width: u32, x: f64, count: usize, layout: Layout) -> Option<usize> {
    if count == 0 {
        return None;
    }
    let x = x.clamp(0.0, (width as f64 - 1.0).max(0.0));
    hit(width, x, count, layout).or(Some(count - 1))
}

/// `from` 탭을 `to` 자리로 옮긴 뒤 `active` 탭의 새 번호
pub fn active_after_move(active: usize, from: usize, to: usize) -> usize {
    if active == from {
        to
    } else if from < active && to >= active {
        active - 1
    } else if from > active && to <= active {
        active + 1
    } else {
        active
    }
}

/// 두 번 누르기: 앞 누름이 같은 탭이고 `interval` 안이면 참
pub fn is_double_click(
    last: Option<(std::time::Instant, usize)>,
    now: std::time::Instant,
    index: usize,
    interval: std::time::Duration,
) -> bool {
    matches!(last, Some((t, i)) if i == index && now.saturating_duration_since(t) < interval)
}

fn dot(canvas: &mut Canvas, cx: i32, cy: i32, r: i32, color: Rgb) {
    for dy in -r..=r {
        for dx in -r..=r {
            if dx * dx + dy * dy <= r * r {
                canvas.fill_rect(cx + dx, cy + dy, 1, 1, color);
            }
        }
    }
}

/// 토스트의 글(줄인 것), 왼쪽 x, 폭. 창 오른쪽에 한 칸 띄워 붙인다.
/// 글은 48칸까지, 창이 좁으면 창 안(양옆 안쪽 여백과 바깥 한 칸을 뺀 칸)에 들어가게 줄인다. 왼쪽은 창 밖으로 나가지 않는다
pub fn toast_layout(canvas_w: u32, cell_w: u32, text: &str) -> (String, i32, u32) {
    let cell_w = cell_w.max(1);
    let room = ((canvas_w / cell_w) as usize).saturating_sub(3).clamp(1, 48);
    let text = truncate(text, room);
    let w = cell_w * text_cells(&text) as u32 + cell_w * 2;
    let x = (canvas_w as i32 - w as i32 - cell_w as i32).max(0);
    (text, x, w)
}

/// 창 오른쪽 아래의 알림 띠(클립보드 복사 같은 일을 사람에게 보인다)
pub fn draw_toast(canvas: &mut Canvas, fonts: &mut Fonts, theme: &Theme, text: &str) {
    let m = fonts.metrics();
    let (text, x, w) = toast_layout(canvas.width, m.width, text);
    let h = m.height + m.height / 2;
    let y = canvas.height as i32 - h as i32 - m.height as i32 / 2;
    let bg = theme.background.mix(theme.foreground, 0.85);
    canvas.fill_rect(x, y, w, h, bg);
    draw_text(
        canvas,
        fonts,
        x + m.width as i32,
        y + (h as i32 - m.height as i32) / 2,
        &text,
        theme.background,
        false,
    );
}

/// 보안 키보드 입력이 켜져 있는 동안 창 오른쪽 위에 자물쇠를 띄운다(Ghostty `macos-secure-input-indication`, #258)
pub fn draw_secure_badge(canvas: &mut Canvas, fonts: &mut Fonts, theme: &Theme, top: u32) {
    let m = fonts.metrics();
    let side = m.height + m.height / 2;
    let margin = (m.height / 2).max(4);
    let x = canvas.width as i32 - side as i32 - margin as i32;
    let y = top as i32 + margin as i32;
    let edge = theme.background.mix(theme.foreground, 0.5);
    canvas.fill_rect(x, y, side, side, edge);
    canvas.fill_rect(x + 1, y + 1, side - 2, side - 2, theme.background);
    let pad = side / 5;
    for (rx, ry, rw, rh) in bt_render::lock_rects(x + pad as i32, y + pad as i32, side - 2 * pad, side - 2 * pad) {
        canvas.fill_rect(rx, ry, rw, rh, theme.foreground);
    }
}

/// 링크 주소를 창 왼쪽 아래에 띄운다(Cmd를 누른 채 링크 위에 있을 때)
pub fn draw_link_hint(canvas: &mut Canvas, fonts: &mut Fonts, theme: &Theme, url: &str) {
    let m = fonts.metrics();
    // 창 폭의 대부분까지 쓴다
    let max_cells = ((canvas.width / m.width.max(1)) as usize).saturating_sub(6).max(8);
    // 호스트는 늘 다 보이고 가운데를 줄인다(끝만 자르면 `github.com@다른곳`처럼 속일 수 있다, #54)
    let text = bt_term::links::hint_text(url, max_cells);
    let tw = m.width * text_cells(&text) as u32;
    let (w, h) = (tw + m.width * 2, m.height + m.height / 2);
    let x = m.width as i32 / 2;
    let y = canvas.height as i32 - h as i32 - m.height as i32 / 2;
    canvas.fill_rect(x, y, w, h, theme.background.mix(theme.foreground, 0.12));
    canvas.stroke_rect(x, y, w, h, 1, theme.background.mix(theme.foreground, 0.3));
    draw_text(
        canvas,
        fonts,
        x + m.width as i32,
        y + (h as i32 - m.height as i32) / 2,
        &text,
        theme.foreground,
        false,
    );
}

/// 찾기 막대를 칸(`pane`)의 오른쪽 위에 띄운다(#39). 결과가 없으면 붉게 보인다
pub fn draw_search_bar(
    canvas: &mut Canvas,
    fonts: &mut Fonts,
    theme: &Theme,
    pane: crate::split::Rect,
    query: &str,
    status: &str,
    missing: bool,
) {
    let m = fonts.metrics();
    let cw = m.width as i32;
    let label = "찾기 ";
    // 칸 폭의 절반 남짓까지. 글이 길면 앞을 줄여 끝(지금 치는 자리)을 보인다
    let max_cells = ((pane.w as i32 / cw) - 4).clamp(12, 60) as usize;
    let status_cells = text_cells(status);
    let room = max_cells.saturating_sub(text_cells(label) + status_cells + 3).max(4);
    let q: String = if text_cells(query) > room {
        let tail: String = query
            .chars()
            .rev()
            .take(room - 1)
            .collect::<Vec<_>>()
            .into_iter()
            .rev()
            .collect();
        format!("…{tail}")
    } else {
        query.to_string()
    };
    let body_cells = text_cells(label) + text_cells(&q).max(room.min(16)) + status_cells + 3;
    let (w, h) = (body_cells as u32 * m.width, m.height + m.height / 2);
    let x = pane.x + pane.w as i32 - w as i32 - cw;
    let y = pane.y + m.height as i32 / 4;
    let bg = theme.background.mix(theme.foreground, 0.1);
    let border = if missing {
        theme.palette[1]
    } else {
        theme.background.mix(theme.foreground, 0.35)
    };
    canvas.fill_rect(x, y, w, h, bg);
    canvas.stroke_rect(x, y, w, h, 1, border);
    let ty = y + (h as i32 - m.height as i32) / 2;
    let dim = theme.foreground.mix(theme.background, 0.45);
    let mut tx = x + cw;
    draw_text(canvas, fonts, tx, ty, label, dim, false);
    tx += cw * text_cells(label) as i32;
    draw_text(canvas, fonts, tx, ty, &q, theme.foreground, false);
    tx += cw * text_cells(&q) as i32;
    // 치는 자리
    canvas.fill_rect(tx, ty, (m.stroke * 2).max(2), m.height, theme.cursor);
    let sx = x + w as i32 - cw * (status_cells as i32 + 1);
    draw_text(
        canvas,
        fonts,
        sx,
        ty,
        status,
        if missing { theme.palette[1] } else { dim },
        false,
    );
}

/// 명령 팔레트(#77)를 창 위쪽 가운데에 그린다: 친 글 한 줄, 그 아래 맞는 동작들(이름 왼쪽, 단축키 오른쪽)
pub fn draw_palette(
    canvas: &mut Canvas,
    fonts: &mut Fonts,
    theme: &Theme,
    query: &str,
    rows: &[(String, String, String)],
    selected: Option<usize>,
) {
    let m = fonts.metrics();
    let cw = m.width as i32;
    let line_h = m.height + m.height / 3;
    let max_rows = 10usize;
    let width_cells = ((canvas.width / m.width.max(1)) as usize)
        .saturating_sub(4)
        .clamp(20, 60);
    let w = width_cells as u32 * m.width;
    let shown = rows.len().min(max_rows);
    // 고른 줄이 보이게 목록을 민다
    let first = selected
        .unwrap_or(0)
        .saturating_sub(max_rows - 1)
        .min(rows.len().saturating_sub(shown));
    let h = line_h * (1 + shown.max(1) as u32) + m.height / 2;
    let x = (canvas.width as i32 - w as i32) / 2;
    let y = m.height as i32;
    canvas.fill_rect(x, y, w, h, theme.background.mix(theme.foreground, 0.08));
    canvas.stroke_rect(x, y, w, h, 1, theme.background.mix(theme.foreground, 0.35));
    let pad = (line_h as i32 - m.height as i32) / 2;
    let dim = theme.foreground.mix(theme.background, 0.45);
    // 친 글
    let prompt = "› ";
    draw_text(canvas, fonts, x + cw, y + pad, prompt, dim, false);
    let qx = x + cw * (1 + text_cells(prompt) as i32);
    let q = truncate(query, width_cells.saturating_sub(4));
    draw_text(canvas, fonts, qx, y + pad, &q, theme.foreground, false);
    canvas.fill_rect(
        qx + cw * text_cells(&q) as i32,
        y + pad,
        (m.stroke * 2).max(2),
        m.height,
        theme.cursor,
    );
    canvas.fill_rect(x, y + line_h as i32, w, 1, theme.background.mix(theme.foreground, 0.2));
    if rows.is_empty() {
        draw_text(
            canvas,
            fonts,
            x + cw,
            y + line_h as i32 + pad,
            "맞는 명령이 없습니다",
            dim,
            false,
        );
        return;
    }
    for (n, (label, subtitle, keys)) in rows.iter().enumerate().skip(first).take(shown) {
        let ry = y + line_h as i32 * (1 + (n - first) as i32) + m.height as i32 / 4;
        if Some(n) == selected {
            canvas.fill_rect(x + 1, ry, w - 2, line_h, theme.selection_background);
        }
        let fg = if Some(n) == selected {
            theme.selection_foreground.unwrap_or(theme.foreground)
        } else {
            theme.foreground
        };
        let keys_cells = text_cells(keys);
        let label = truncate(label, width_cells.saturating_sub(keys_cells + 4));
        draw_text(canvas, fonts, x + cw, ry + pad, &label, fg, false);
        // 부제(칸 이동 항목의 현재 폴더)는 이름 뒤에 흐리게, 남는 자리만큼
        let left = text_cells(&label) + 2;
        let room = width_cells.saturating_sub(left + keys_cells + 3);
        if !subtitle.is_empty() && room >= 6 {
            let sub = truncate(subtitle, room);
            draw_text(canvas, fonts, x + cw * (1 + left as i32), ry + pad, &sub, dim, false);
        }
        draw_text(
            canvas,
            fonts,
            x + w as i32 - cw * (keys_cells as i32 + 1),
            ry + pad,
            keys,
            dim,
            false,
        );
    }
}

/// 닫기·끝내기 확인 막대(#108)를 창 위쪽 가운데에 그린다: 묻는 글 한 줄, 그 아래 키 안내
/// 확인 막대에 그릴 수 있는 앞말+글의 칸 수(`draw_confirm`이 이 길이로 자른다). 창 폭(픽셀)과 칸 너비로 정한다
pub fn confirm_message_room(canvas_width: u32, cell_w: u32) -> usize {
    bar_cells(canvas_width, cell_w).saturating_sub(4)
}

/// 막대 자체의 최대 칸 수: 창 칸 수에서 좌우 여백 4를 빼고, 아주 좁아도 10은 둔다
fn bar_cells(canvas_width: u32, cell_w: u32) -> usize {
    ((canvas_width / cell_w.max(1)) as usize).saturating_sub(4).max(10)
}

/// 글이 막대에 잘리지 않고 다 들어가는가. 링크를 묻는 막대는 주소가 잘리면 묻지 않는다(질의가 가려진 채 허락하게 된다)
pub fn confirm_message_fits(message: &str, canvas_width: u32, cell_w: u32) -> bool {
    text_cells(message) <= confirm_message_room(canvas_width, cell_w)
}

pub fn draw_confirm(canvas: &mut Canvas, fonts: &mut Fonts, theme: &Theme, message: &str, hint: &str) {
    let m = fonts.metrics();
    let cw = m.width as i32;
    let line_h = m.height + m.height / 3;
    let max_cells = bar_cells(canvas.width, m.width);
    let message = truncate(message, confirm_message_room(canvas.width, m.width));
    let cells = text_cells(&message).max(text_cells(hint)) + 4;
    let w = cells.min(max_cells) as u32 * m.width;
    let h = line_h * 2 + m.height / 2;
    let x = (canvas.width as i32 - w as i32) / 2;
    let y = m.height as i32;
    canvas.fill_rect(x, y, w, h, theme.background.mix(theme.foreground, 0.08));
    // 눈에 띄게 노란 테두리(경고)
    canvas.stroke_rect(x, y, w, h, (m.stroke).max(1), theme.palette[3]);
    let pad = (line_h as i32 - m.height as i32) / 2 + m.height as i32 / 4;
    draw_text(canvas, fonts, x + cw * 2, y + pad, &message, theme.foreground, false);
    let dim = theme.foreground.mix(theme.background, 0.4);
    draw_text(canvas, fonts, x + cw * 2, y + pad + line_h as i32, hint, dim, false);
}

/// 글 입력 막대(#124, 칸 제목 바꾸기)를 창 위쪽 가운데에: 앞말과 친 글·커서 한 줄, 그 아래 키 안내
pub fn draw_prompt(canvas: &mut Canvas, fonts: &mut Fonts, theme: &Theme, label: &str, text: &str, hint: &str) {
    let m = fonts.metrics();
    let cw = m.width as i32;
    let line_h = m.height + m.height / 3;
    let width_cells = ((canvas.width / m.width.max(1)) as usize)
        .saturating_sub(4)
        .clamp(20, 60);
    let w = width_cells as u32 * m.width;
    let h = line_h * 2 + m.height / 2;
    let x = (canvas.width as i32 - w as i32) / 2;
    let y = m.height as i32;
    canvas.fill_rect(x, y, w, h, theme.background.mix(theme.foreground, 0.08));
    canvas.stroke_rect(x, y, w, h, 1, theme.background.mix(theme.foreground, 0.35));
    let pad = (line_h as i32 - m.height as i32) / 2 + m.height as i32 / 4;
    let dim = theme.foreground.mix(theme.background, 0.45);
    draw_text(canvas, fonts, x + cw, y + pad, label, dim, false);
    let tx = x + cw * (1 + text_cells(label) as i32);
    let room = width_cells.saturating_sub(text_cells(label) + 3);
    // 긴 글은 끝(친 자리)이 보이게 앞을 자른다
    let shown: String = {
        let mut cells = 0;
        let mut out: Vec<char> = Vec::new();
        for c in text.chars().rev() {
            cells += bt_render::char_cells(c);
            if cells > room {
                break;
            }
            out.push(c);
        }
        out.into_iter().rev().collect()
    };
    draw_text(canvas, fonts, tx, y + pad, &shown, theme.foreground, false);
    canvas.fill_rect(
        tx + cw * text_cells(&shown) as i32,
        y + pad,
        (m.stroke * 2).max(2),
        m.height,
        theme.cursor,
    );
    let hint = truncate(hint, width_cells.saturating_sub(2));
    draw_text(canvas, fonts, x + cw, y + pad + line_h as i32, &hint, dim, false);
}

/// 크기 표시 띠(`w`×`h`)를 칸 안 어디에 둘지(#122). 가장자리에서 `margin`만큼 띄운다
pub fn overlay_origin(
    pos: bt_core::config::OverlayPosition,
    pane: crate::split::Rect,
    w: u32,
    h: u32,
    margin: i32,
) -> (i32, i32) {
    use bt_core::config::OverlayPosition::*;
    let (left, right) = (pane.x + margin, pane.x + pane.w as i32 - w as i32 - margin);
    let (top, bottom) = (pane.y + margin, pane.y + pane.h as i32 - h as i32 - margin);
    let cx = pane.x + (pane.w as i32 - w as i32) / 2;
    let cy = pane.y + (pane.h as i32 - h as i32) / 2;
    match pos {
        Center => (cx, cy),
        TopLeft => (left, top),
        TopCenter => (cx, top),
        TopRight => (right, top),
        BottomLeft => (left, bottom),
        BottomCenter => (cx, bottom),
        BottomRight => (right, bottom),
    }
}

/// 크기 표시 띠(#122): 칸 안 `pos` 자리에 `80 × 24`
pub fn draw_resize_overlay(
    canvas: &mut Canvas,
    fonts: &mut Fonts,
    theme: &Theme,
    pane: crate::split::Rect,
    pos: bt_core::config::OverlayPosition,
    text: &str,
) {
    let m = fonts.metrics();
    let (w, h) = (m.width * (text_cells(text) as u32 + 2), m.height + m.height / 2);
    if w > pane.w || h > pane.h {
        return;
    }
    let (x, y) = overlay_origin(pos, pane, w, h, m.width as i32);
    canvas.fill_rect(x, y, w, h, theme.background.mix(theme.foreground, 0.85));
    draw_text(
        canvas,
        fonts,
        x + m.width as i32,
        y + (h as i32 - m.height as i32) / 2,
        text,
        theme.background,
        false,
    );
}

#[cfg(test)]
mod tests {
    use std::time::{Duration, Instant};

    /// 좁은 창(#850 "창이 좁아…" 토스트를 띄우는 바로 그때)에서 토스트 왼쪽이 창 밖으로 잘리지 않는다. 넓은 창에서는 예전처럼 48칸까지, 오른쪽에 붙는다
    #[test]
    fn toast_stays_inside_a_narrow_window() {
        let long = "가".repeat(30); // 60칸
        for cols in [20u32, 40, 60] {
            let (text, x, w) = super::toast_layout(cols * 8, 8, &long);
            assert!(x >= 0, "{cols}칸: x={x}");
            assert!(x as u32 + w <= cols * 8, "{cols}칸: x={x} w={w}");
            assert!(text.ends_with('…'), "{cols}칸: {text}");
        }
        let (text, x, w) = super::toast_layout(1000, 8, "짧은 알림");
        assert_eq!(text, "짧은 알림");
        assert_eq!((x, w), (1000 - (9 * 8 + 16) - 8, 9 * 8 + 16));
        let wide = bt_render::text_cells(&super::toast_layout(1000, 8, &long).0);
        assert!((46..=48).contains(&wide), "넓은 창은 48칸까지: {wide}");
        // 좁은 창 링크 토스트 문구는 40칸 창에 줄이지 않고 다 들어간다
        assert_eq!(
            super::toast_layout(40 * 8, 8, crate::app::NARROW_LINK_TOAST).0,
            crate::app::NARROW_LINK_TOAST
        );
    }

    #[test]
    fn short_press_is_a_click_not_a_drag() {
        let mut d = TabDrag::default();
        d.press(1, 100.0);
        assert!(d.is_pressed());
        assert!(!d.moved(103.0), "문턱 안");
        assert!(!d.is_dragging());
        assert_eq!(d.release(103.0, 400, 4, Layout::default()), TabDrop::None);
        assert!(!d.is_pressed());
    }

    #[test]
    fn moving_past_threshold_starts_drag_and_release_moves() {
        let mut d = TabDrag::default();
        d.press(0, 50.0);
        assert!(d.moved(50.0 + DRAG_START_PX));
        assert!(d.is_dragging());
        assert!(!d.moved(300.0), "이미 시작했으면 또 새로 시작하지 않는다");
        assert_eq!(
            d.release(250.0, 400, 4, Layout::default()),
            TabDrop::Move { from: 0, to: 2 }
        );
        assert!(!d.is_dragging());
    }

    #[test]
    fn drag_back_to_same_tab_does_nothing_and_single_tab_never_moves() {
        let mut d = TabDrag::default();
        d.press(1, 150.0);
        d.moved(190.0);
        assert_eq!(d.release(120.0, 400, 4, Layout::default()), TabDrop::None);
        d.press(0, 10.0);
        d.moved(300.0);
        assert_eq!(d.release(300.0, 400, 1, Layout::default()), TabDrop::None);
    }

    #[test]
    fn drop_outside_bar_clamps_to_ends() {
        assert_eq!(drop_index(400, -30.0, 4, Layout::default()), Some(0));
        assert_eq!(drop_index(400, 900.0, 4, Layout::default()), Some(3));
        assert_eq!(drop_index(400, 150.0, 4, Layout::default()), Some(1));
        assert_eq!(drop_index(400, 10.0, 0, Layout::default()), None);
    }

    #[test]
    fn active_follows_the_move() {
        assert_eq!(active_after_move(1, 1, 3), 3);
        assert_eq!(active_after_move(2, 1, 3), 1);
        assert_eq!(active_after_move(2, 3, 0), 3);
        assert_eq!(active_after_move(0, 3, 1), 0);
        assert_eq!(active_after_move(0, 1, 3), 0);
    }

    #[test]
    fn double_click_needs_same_tab_within_interval() {
        let t = Instant::now();
        let iv = Duration::from_millis(500);
        assert!(is_double_click(Some((t, 2)), t + Duration::from_millis(200), 2, iv));
        assert!(!is_double_click(Some((t, 2)), t + Duration::from_millis(200), 1, iv));
        assert!(!is_double_click(Some((t, 2)), t + Duration::from_millis(600), 2, iv));
        assert!(!is_double_click(None, t, 0, iv));
    }

    use super::*;

    /// 가짜 비례폭: i·l 3, W·M 12, 한글 16, `…` 10, 나머지 8
    fn adv(c: char) -> f32 {
        match c {
            'i' | 'l' => 3.0,
            'W' | 'M' => 12.0,
            '…' => 10.0,
            c if (c as u32) >= 0xAC00 => 16.0,
            _ => 8.0,
        }
    }

    #[test]
    fn fit_title_measures_real_width() {
        // 들어가면 그대로, 폭은 advance 합
        assert_eq!(fit_title("iiW", 100.0, adv), ("iiW".to_string(), 18.0));
        // 좁은 글자가 많은 제목은 같은 글자 수의 넓은 제목보다 더 남는다
        let (narrow, nw) = fit_title("iiiiiiiiii", 25.0, adv);
        let (wide, ww) = fit_title("WWWWWWWWWW", 25.0, adv);
        assert_eq!((narrow.as_str(), nw), ("iiiii…", 25.0));
        assert_eq!((wide.as_str(), ww), ("W…", 22.0));
        // 한글은 16px씩
        assert_eq!(fit_title("안녕하세요", 42.0, adv), ("안녕…".to_string(), 42.0));
    }

    #[test]
    fn fit_title_width_never_exceeds_room() {
        for t in ["iiiiiiiiiiii", "WWWWWWWWWWWW", "안녕하세요 abc", "Mixed Wiki 한글"] {
            for room in [20.0, 33.0, 60.0, 101.0] {
                let (out, w) = fit_title(t, room, adv);
                assert!(w <= room, "{t:?} {room}: {out:?} {w}");
                assert_eq!(w, out.chars().map(adv).sum::<f32>(), "{out:?}");
            }
        }
    }

    #[test]
    fn title_font_text_stays_inside_its_tab() {
        let Ok(set) = Fonts::new_with_title(&bt_render::FontConfig::default(), Some("JetBrains Mono")) else {
            return;
        };
        let (mut fonts, mut title) = (set.terminal, set.title);
        assert!(title.is_some());
        let theme = Theme::ghostty_default();
        let (w, h) = (300u32, 60u32);
        let mut buf = vec![0u32; (w * h) as usize];
        let mut c = Canvas::new(&mut buf, w, h);
        c.clear(theme.background);
        let mk = |t: &str| TabLabel {
            title: t.into(),
            active: false,
            unread: false,
            progress: None,
            color: Default::default(),
        };
        let tabs = [mk(&"안녕 W".repeat(30)), mk("")];
        draw_with_title_font(
            &mut c,
            &mut fonts,
            title.as_mut(),
            &theme,
            &TabBarLook::default(),
            &tabs,
            0,
        );
        // 둘째 탭(x 150..299)은 제목이 없으니 바탕·구분선·밑줄 말고는 아무것도 없어야 한다
        let bar = theme.background.mix(theme.foreground, 0.06);
        let bh = bar_height(&fonts, title.as_ref());
        for y in 0..bh - 1 {
            for x in 150..299 {
                assert_eq!(c.pixel(x, y), bar, "({x},{y})에 첫 탭 제목이 넘어 나왔다");
            }
        }
    }

    /// #460: 키를 비우면(macOS) 시스템 UI 글꼴로 그린다. 그 글꼴의 키가 탭 줄 높이 안에 들어가고 아래 구분선을 덮지 않는다.
    /// 시스템 UI 글꼴이 없으면(Linux 등) 건너뛴다
    #[test]
    fn system_ui_title_font_fits_the_tab_bar() {
        let Ok(set) = Fonts::new_with_title(&bt_render::FontConfig::default(), None) else {
            return;
        };
        let (mut fonts, mut title) = (set.terminal, set.title);
        let Some(tf) = title.as_ref() else {
            eprintln!("건너뜀: 시스템 UI 글꼴이 없어 터미널 글꼴로 그린다");
            return;
        };
        let bh = bar_height(&fonts, Some(tf));
        assert!(
            tf.metrics().height <= bh,
            "제목 글꼴 키 {} > 탭 줄 {bh}",
            tf.metrics().height
        );
        let theme = Theme::ghostty_default();
        let (w, h) = (300u32, 60u32);
        let mut buf = vec![0u32; (w * h) as usize];
        let mut c = Canvas::new(&mut buf, w, h);
        c.clear(theme.background);
        let mk = |t: &str| TabLabel {
            title: t.into(),
            active: false,
            unread: false,
            progress: None,
            color: Default::default(),
        };
        let tabs = [mk(&"Hgjpq안녕 Ég ".repeat(10)), mk("")];
        draw_with_title_font(
            &mut c,
            &mut fonts,
            title.as_mut(),
            &theme,
            &TabBarLook::default(),
            &tabs,
            0,
        );
        let border = theme.background.mix(theme.foreground, 0.16);
        for x in 0..149 {
            assert_eq!(c.pixel(x, bh - 1), border, "({x})에서 구분선이 덮였다");
        }
        let bar = theme.background.mix(theme.foreground, 0.06);
        let inked = (0..149).any(|x| (0..bh - 1).any(|y| c.pixel(x, y) != bar));
        assert!(inked, "제목이 그려지지 않았다");
        for y in 0..bh - 1 {
            for x in 150..299 {
                assert_eq!(c.pixel(x, y), bar, "({x},{y})에 첫 탭 제목이 넘어 나왔다");
            }
        }
    }

    /// #474: 기본 설정(시스템 UI 제목 글꼴 13pt, 배율 2)의 탭 줄 높이는 0.50.0의 52px에서 크게 바뀌지 않는다(48~56px)
    #[test]
    fn default_tab_bar_height_stays_near_0_50() {
        let cfg = bt_render::FontConfig {
            scale: 2.0,
            ..bt_render::FontConfig::default()
        };
        let Ok(set) = Fonts::new_with_title(&cfg, None) else {
            return;
        };
        let Some(tf) = set.title.as_ref() else {
            eprintln!("건너뜀: 시스템 UI 글꼴이 없다");
            return;
        };
        let h = bar_height(&set.terminal, Some(tf));
        assert!(
            (48..=56).contains(&h),
            "탭 줄 {h}px, 제목 셀 높이 {}",
            tf.metrics().height
        );
        assert!(tf.metrics().height <= h);
    }

    /// #474: 제목 글꼴이 있으면 탭 줄의 높이와 탭 자리는 터미널 글꼴 크기(13과 20)와 상관없이 같다
    #[test]
    fn tab_bar_ignores_terminal_font_size_with_title_font() {
        let build = |size: f32| {
            let cfg = bt_render::FontConfig {
                size,
                scale: 2.0,
                ..bt_render::FontConfig::default()
            };
            Fonts::new_with_title(&cfg, Some("JetBrains Mono")).ok()
        };
        let (Some(a), Some(b)) = (build(13.0), build(20.0)) else {
            return;
        };
        let (ta, tb) = (a.title.as_ref().unwrap(), b.title.as_ref().unwrap());
        assert_ne!(
            height_px(&a.terminal),
            height_px(&b.terminal),
            "터미널 글꼴 높이는 크기를 따른다"
        );
        assert_eq!(bar_height(&a.terminal, Some(ta)), bar_height(&b.terminal, Some(tb)));
        // 탭 자리(`slots`·`hit`)는 글꼴이 아니라 창 폭과 탭 수만 보므로(#478) 글꼴 크기와 상관없다
        // 제목 글꼴이 없으면 터미널 글꼴로 센다
        assert_eq!(bar_height(&b.terminal, None), height_px(&b.terminal));
    }

    /// #478: 탭 폭에 상한이 없다. 폭 합이 창 폭과 같고 이웃 탭이 빈틈·겹침 없이 이어진다
    #[test]
    fn slots_fill_the_whole_width() {
        for count in [1usize, 2, 3, 7] {
            let s = slots(2000, count, Layout::default());
            assert_eq!(s.len(), count);
            assert_eq!(s.iter().map(|&(_, w)| w).sum::<u32>(), 2000, "탭 {count}: 폭 합");
            assert_eq!(s[0].0, 0);
            for pair in s.windows(2) {
                assert_eq!(pair[0].0 + pair[0].1 as i32, pair[1].0, "탭 {count}: 이웃이 이어짐");
            }
            let last = s[count - 1];
            assert_eq!(last.0 + last.1 as i32, 2000, "탭 {count}: 오른쪽 끝");
            let first_w = s[0].1;
            assert!(
                s[..count - 1].iter().all(|&(_, w)| w == first_w),
                "나머지는 마지막 탭만 가진다"
            );
        }
    }

    /// #478: 누른 자리는 그린 자리와 같다. x=0, 경계 양쪽, 오른쪽 끝(width-1)
    #[test]
    fn hit_matches_the_slots() {
        for count in [1usize, 2, 3, 7] {
            let s = slots(2000, count, Layout::default());
            assert_eq!(hit(2000, 0.0, count, Layout::default()), Some(0));
            assert_eq!(
                hit(2000, 1999.0, count, Layout::default()),
                Some(count - 1),
                "탭 {count}: 오른쪽 끝"
            );
            assert_eq!(hit(2000, 2000.0, count, Layout::default()), None);
            for (i, &(x, _)) in s.iter().enumerate().skip(1) {
                assert_eq!(hit(2000, x as f64 - 1.0, count, Layout::default()), Some(i - 1));
                assert_eq!(hit(2000, x as f64, count, Layout::default()), Some(i));
            }
        }
        assert_eq!(hit(2000, 5.0, 0, Layout::default()), None);
    }

    /// #478: 폭이 0이거나 탭 수보다 작아도 패닉이 나지 않는다
    #[test]
    fn slots_do_not_panic_on_tiny_widths() {
        assert!(slots(2000, 0, Layout::default()).is_empty());
        for (width, count) in [(0u32, 3usize), (1, 3), (3, 7), (5, 5)] {
            let s = slots(width, count, Layout::default());
            assert_eq!(s.len(), count);
            assert!(s.iter().all(|&(_, w)| w >= 1));
            let _ = hit(width, 0.0, count, Layout::default());
        }
    }

    #[test]
    fn status_maps_to_a_bar_and_the_most_urgent_wins() {
        use bt_term::program_status::{State, Summary};
        let sm = |state, progress| Summary { state, progress };
        assert_eq!(status_bar(sm(State::Blocked, None), true), Some((5, 100)));
        assert_eq!(status_bar(sm(State::Error, None), true), Some((2, 100)));
        assert_eq!(status_bar(sm(State::Done, None), true), Some((6, 100)));
        assert_eq!(status_bar(sm(State::Working, Some(40)), true), Some((1, 40)));
        assert_eq!(status_bar(sm(State::Working, None), true), Some((3, 0)));
        assert_eq!(status_bar(sm(State::Idle, None), true), None);
        // progress-style = false: 일하는 중은 숨기고 사람이 봐야 하는 것은 남긴다
        assert_eq!(status_bar(sm(State::Working, Some(40)), false), None);
        assert_eq!(status_bar(sm(State::Blocked, None), false), Some((5, 100)));
        let urgent = most_urgent([
            sm(State::Done, None),
            sm(State::Working, None),
            sm(State::Blocked, None),
            sm(State::Error, None),
        ]);
        assert_eq!(urgent.map(|u| u.state), Some(State::Blocked));
        assert_eq!(most_urgent([]), None);
    }

    #[test]
    fn status_bars_are_drawn_in_their_own_colors() {
        let theme = Theme::ghostty_default();
        for (state, color) in [(5, theme.palette[5]), (6, theme.palette[2]), (2, theme.palette[1])] {
            let mut buf = vec![0u32; 40 * 10];
            let mut c = Canvas::new(&mut buf, 40, 10);
            c.clear(theme.background);
            draw_progress(&mut c, &theme, 0, 0, 40, (state, 100), 4);
            assert_eq!(buf[2 * 40 + 39] & 0xFF_FFFF, color.to_u32() & 0xFF_FFFF, "상태 {state}");
        }
    }

    #[test]
    fn progress_is_ignored_when_style_is_off() {
        assert_eq!(accept_progress(true, 1, Some(40)), Some((1, 40)));
        assert_eq!(accept_progress(true, 0, None), None, "상태 0은 지움");
        assert_eq!(accept_progress(false, 1, Some(40)), None);
        assert_eq!(accept_progress(false, 3, None), None);
    }

    #[test]
    fn progress_without_a_value_follows_ghostty_macos() {
        // 값 없는 보통·오류는 끝 모름 막대(오류는 빨강), 멈춤은 가득(주황), 값이 있으면 그 값
        assert_eq!(accept_progress(true, 1, None), Some((3, 0)));
        assert_eq!(accept_progress(true, 2, None), Some((BAR_ERROR_UNKNOWN, 0)));
        assert_eq!(accept_progress(true, 4, None), Some((4, 100)));
        assert_eq!(accept_progress(true, 3, None), Some((3, 0)));
        assert_eq!(accept_progress(true, 2, Some(30)), Some((2, 30)));
        assert_eq!(accept_progress(true, 4, Some(30)), Some((4, 30)));
        // 빨간 끝 모름 막대는 가운데 3분의 1만 칠한다
        let theme = Theme::ghostty_default();
        let mut buf = vec![0u32; 30 * 10];
        let mut c = Canvas::new(&mut buf, 30, 10);
        c.clear(theme.background);
        draw_progress(&mut c, &theme, 0, 0, 30, (BAR_ERROR_UNKNOWN, 0), 4);
        let red = theme.palette[1].to_u32() & 0xFF_FFFF;
        assert_ne!(buf[2 * 30 + 15] & 0xFF_FFFF, buf[2 * 30 + 2] & 0xFF_FFFF);
        assert_eq!(buf[2 * 30 + 15] & 0xFF_FFFF, red);
    }

    /// 탭 색(#852): Ghostty처럼 제목 오른쪽의 작은 점이다. 고른 탭이든 아니든 같은 색이고, 색이 없는 탭에는 점이 없다
    #[test]
    fn tab_color_is_a_dot_right_of_the_title_on_active_and_inactive_tabs() {
        use crate::tabcolor::TabColor;
        let Ok(mut fonts) = Fonts::new(&bt_render::FontConfig::default()) else {
            return;
        };
        let theme = Theme::ghostty_default();
        let (w, h) = (400u32, 60u32);
        let mk = |active: bool, color: TabColor| TabLabel {
            title: "a".into(),
            active,
            unread: false,
            progress: None,
            color,
        };
        let mut buf = vec![0u32; (w * h) as usize];
        let mut c = Canvas::new(&mut buf, w, h);
        c.clear(theme.background);
        let tabs = [
            mk(true, TabColor::Blue),
            mk(false, TabColor::Red),
            mk(false, TabColor::None),
        ];
        draw_with_title_font(&mut c, &mut fonts, None, &theme, &TabBarLook::default(), &tabs, 0);
        let bh = bar_height(&fonts, None);
        let dark = crate::tabcolor::is_dark(theme.background);
        let count = |c: &Canvas, x0: u32, x1: u32, want: Rgb| {
            (0..bh)
                .flat_map(|y| (x0..x1).map(move |x| (x, y)))
                .filter(|&(x, y)| c.pixel(x, y) == want)
                .count()
        };
        let blue = TabColor::Blue.rgb(dark).unwrap();
        let red = TabColor::Red.rgb(dark).unwrap();
        // 탭 폭은 400/3: 133, 133, 134
        assert!(count(&c, 0, 133, blue) >= 6, "고른 탭에 파란 점이 없다");
        assert!(count(&c, 133, 266, red) >= 6, "고르지 않은 탭에 빨간 점이 없다");
        assert_eq!(
            count(&c, 266, 400, blue) + count(&c, 266, 400, red),
            0,
            "색 없는 탭에 점이 생겼다"
        );
        // 점은 탭 오른쪽 절반에 있다(제목은 가운데, 안 읽음 점은 왼쪽)
        assert_eq!(count(&c, 0, 66, blue), 0);
        assert_eq!(count(&c, 133, 199, red), 0);
    }

    #[test]
    fn tab_bar_starts_below_the_top_offset() {
        let Ok(mut fonts) = Fonts::new(&bt_render::FontConfig::default()) else {
            return;
        };
        let theme = Theme::ghostty_default();
        let (w, h) = (300u32, 200u32);
        let mut buf = vec![0u32; (w * h) as usize];
        let mut c = Canvas::new(&mut buf, w, h);
        c.clear(theme.background);
        let tabs = [TabLabel {
            title: "a".into(),
            active: false,
            unread: false,
            progress: None,
            color: Default::default(),
        }];
        let top = 40;
        draw_with_title_font(&mut c, &mut fonts, None, &theme, &TabBarLook::default(), &tabs, top);
        let bar = theme.background.mix(theme.foreground, 0.06).to_u32();
        let at = |y: u32| buf[(y * w + 5) as usize] & 0xFF_FFFF;
        // 제목 막대 자리는 그대로, 그 밑부터 탭 줄 색
        assert_ne!(at(top as u32 - 1), bar);
        assert_eq!(at(top as u32 + 1), bar);
    }

    #[test]
    fn prompt_bar_keeps_the_end_of_long_text_visible() {
        let Ok(mut fonts) = Fonts::new(&bt_render::FontConfig::default()) else {
            return;
        };
        let theme = Theme::ghostty_default();
        let (w, h) = (600u32, 200u32);
        let mut buf = vec![0u32; (w * h) as usize];
        let mut c = Canvas::new(&mut buf, w, h);
        c.clear(theme.background);
        let long = "가".repeat(200) + "끝";
        draw_prompt(&mut c, &mut fonts, &theme, "제목 › ", &long, "↩ 정하기");
        // 커서(theme.cursor 색 막대)가 상자 안에 있다
        let m = fonts.metrics();
        let row = m.height + m.height / 2;
        let cursor: Vec<u32> = (0..w).filter(|&x| c.pixel(x, row) == theme.cursor).collect();
        assert!(!cursor.is_empty(), "커서가 보인다");
        assert!(*cursor.last().unwrap() < w - m.width, "커서가 상자 안");
    }

    #[test]
    fn resize_overlay_sits_at_the_chosen_spot_inside_the_pane() {
        use bt_core::config::OverlayPosition::*;
        let pane = crate::split::Rect {
            x: 100,
            y: 50,
            w: 400,
            h: 300,
        };
        let at = |p| overlay_origin(p, pane, 80, 20, 10);
        assert_eq!(at(Center), (260, 190));
        assert_eq!(at(TopLeft), (110, 60));
        assert_eq!(at(TopCenter), (260, 60));
        assert_eq!(at(TopRight), (410, 60));
        assert_eq!(at(BottomLeft), (110, 320));
        assert_eq!(at(BottomCenter), (260, 320));
        assert_eq!(at(BottomRight), (410, 320));
    }

    #[test]
    fn confirm_message_fits_only_when_the_bar_can_draw_it_whole() {
        // 창 800px, 칸 10px: 80칸 → 막대 칸 76 → 글 칸 72
        assert_eq!(confirm_message_room(800, 10), 72);
        assert!(confirm_message_fits(&"a".repeat(72), 800, 10));
        assert!(!confirm_message_fits(&"a".repeat(73), 800, 10));
        // 넓은 글자는 두 칸으로 센다
        assert!(!confirm_message_fits(&"한".repeat(37), 800, 10));
        // 좁은 창: 막대 칸이 최소 10이라 글은 6칸까지
        assert_eq!(confirm_message_room(100, 10), 6);
        assert!(!confirm_message_fits(
            "다른 앱으로 이 링크를 열까요? ssh://h.example/x",
            300,
            10
        ));
        // draw_confirm은 못 넣는 글을 `…`로 자르므로 이 판단과 같은 길이를 쓴다
        assert!(confirm_message_fits(
            "다른 앱으로 이 링크를 열까요? ssh://h.example/x",
            1200,
            10
        ));
    }

    #[test]
    fn confirm_bar_is_centered_with_warning_border_and_fits_long_text() {
        let Ok(mut fonts) = Fonts::new(&bt_render::FontConfig::default()) else {
            return;
        };
        let theme = Theme::ghostty_default();
        let (w, h) = (800u32, 300u32);
        let mut buf = vec![0u32; (w * h) as usize];
        let mut c = Canvas::new(&mut buf, w, h);
        c.clear(theme.background);
        let long = "이 칸에서 ".to_string() + &"아주 긴 명령 ".repeat(40) + "😀이(가) 돌고 있습니다. 닫을까요?";
        draw_confirm(&mut c, &mut fonts, &theme, &long, "↩ 닫기    esc 취소");
        let m = fonts.metrics();
        // 테두리(노랑)가 위쪽 가운데에 있고, 좌우 끝은 창 안에 들어온다
        let row = m.height;
        let border: Vec<u32> = (0..w).filter(|&x| c.pixel(x, row) == theme.palette[3]).collect();
        assert!(!border.is_empty(), "노란 테두리");
        let (l, r) = (border[0], *border.last().unwrap());
        assert!(l > 0 && r < w - 1, "창 안: {l}..{r}");
        assert!(
            (l as i32 - (w - 1 - r) as i32).abs() <= 1,
            "가운데: {l} / {}",
            w - 1 - r
        );
    }

    // ---- #901 탭 막대 모양과 색 ----

    fn plain(active: bool) -> TabLabel {
        TabLabel {
            title: String::new(),
            active,
            unread: false,
            progress: None,
            color: Default::default(),
        }
    }

    /// 탭 줄을 그려 (줄마다 색) 표로 돌려준다: (그림, 탭 줄 높이, 칸 폭)
    fn render(look: &TabBarLook, tabs: &[TabLabel], w: u32) -> Option<(Vec<Vec<Rgb>>, u32, u32)> {
        let mut fonts = Fonts::new(&bt_render::FontConfig::default()).ok()?;
        let theme = Theme::ghostty_default();
        let bh = bar_height(&fonts, None);
        let mut buf = vec![0u32; (w * (bh + 4)) as usize];
        let mut c = Canvas::new(&mut buf, w, bh + 4);
        c.clear(Rgb::new(1, 2, 3));
        draw_with_title_font(&mut c, &mut fonts, None, &theme, look, tabs, 0);
        let rows = (0..bh).map(|y| (0..w).map(|x| c.pixel(x, y)).collect()).collect();
        Some((rows, bh, fonts.metrics().width))
    }

    const BAR: Rgb = Rgb::new(10, 10, 10);
    const ACT: Rgb = Rgb::new(200, 40, 40);
    const INA: Rgb = Rgb::new(40, 40, 200);

    fn colored(style: TabBarStyle) -> TabBarLook {
        TabBarLook {
            style,
            bar_background: Some(BAR),
            active_background: Some(ACT),
            inactive_background: Some(INA),
            ..TabBarLook::default()
        }
    }

    /// 기본 모양은 #901 이전 그리기(막대 바탕·밑 구분선·고른 탭 바탕·탭 사이 세로 구분선)와 픽셀이 같다
    #[test]
    fn default_look_draws_the_same_pixels_as_before_901() {
        let Ok(fonts) = Fonts::new(&bt_render::FontConfig::default()) else {
            return;
        };
        let theme = Theme::ghostty_default();
        let (w, bh) = (500u32, bar_height(&fonts, None));
        let tabs = [plain(false), plain(true), plain(false)];
        let (rows, _, _) = render(&TabBarLook::default(), &tabs, w).unwrap();
        let m = fonts.metrics();
        let bar = theme.background.mix(theme.foreground, 0.06);
        let border = theme.background.mix(theme.foreground, 0.16);
        let mut want = vec![vec![bar; w as usize]; bh as usize];
        want[bh as usize - 1].fill(border);
        for (i, (x, tw)) in slots(w, 3, Layout::default()).into_iter().enumerate() {
            let (x, tw) = (x as usize, tw as usize);
            if i == 1 {
                for row in want.iter_mut() {
                    row[x..x + tw].fill(theme.background);
                }
            }
            for y in (m.height / 4)..(m.height / 4 + bh - m.height / 2) {
                want[y as usize][x + tw - 1] = border;
            }
        }
        assert_eq!(rows, want);
    }

    #[test]
    fn capped_layout_gathers_tabs_by_align_and_default_still_fills() {
        let cap = Layout {
            max_tab: Some(100),
            align: TabBarAlign::Left,
        };
        assert_eq!(slots(1000, 3, cap), vec![(0, 100), (100, 100), (200, 100)]);
        let c = Layout {
            align: TabBarAlign::Center,
            ..cap
        };
        assert_eq!(slots(1000, 3, c), vec![(350, 100), (450, 100), (550, 100)]);
        let r = Layout {
            align: TabBarAlign::Right,
            ..cap
        };
        assert_eq!(slots(1000, 3, r), vec![(700, 100), (800, 100), (900, 100)]);
        // 탭이 많아 나눈 폭이 상한보다 작으면 줄을 가득 채운다
        assert_eq!(slots(1000, 20, c), slots(1000, 20, Layout::default()));
        assert_eq!(
            slots(1000, 3, Layout::default()),
            vec![(0, 333), (333, 333), (666, 334)]
        );
        // 누르기·끌기가 그린 자리와 같다: 탭 밖(빈 막대)은 없는 자리다
        assert_eq!(hit(1000, 649.0, 3, c), Some(2));
        assert_eq!(hit(1000, 650.0, 3, c), None);
        assert_eq!(hit(1000, 349.0, 3, c), None);
        assert_eq!(hit(1000, 351.0, 3, c), Some(0));
        assert_eq!(hit(1000, 10.0, 3, r), None);
        assert_eq!(
            drop_index(1000, 10.0, 3, r),
            Some(2),
            "끌어 놓는 자리는 막대 밖이면 끝 탭"
        );
        let look = TabBarLook {
            style: TabBarStyle::Slant,
            align: TabBarAlign::Right,
            ..TabBarLook::default()
        };
        assert_eq!(
            Layout::of(&look, 10),
            Layout {
                max_tab: Some(240),
                align: TabBarAlign::Right
            }
        );
        assert_eq!(Layout::of(&TabBarLook::default(), 10), Layout::default());
    }

    #[test]
    fn fade_tabs_blend_their_edges_into_the_bar() {
        let tabs = [plain(true), plain(false), plain(false)];
        let Some((rows, bh, cw)) = render(&colored(TabBarStyle::Fade), &tabs, 1000) else {
            return;
        };
        let cap = (cw * 24) as usize;
        let y = bh as usize / 2;
        let fade = [0.25, 0.5, 0.75, 1.0];
        for (i, a) in fade.iter().enumerate() {
            let x = i * cw as usize + 1;
            assert_eq!(rows[y][x], BAR.mix(ACT, *a), "왼쪽 번짐 {i}");
            // 오른쪽은 거꾸로: 끝 한 칸(막대 바탕) 앞에서부터 안쪽으로 1.0 → 0.25가 아니라 0.25 → 1.0의 반대
            let rx = cap - cw as usize - (i + 1) * cw as usize + 1;
            assert_eq!(rows[y][rx], BAR.mix(ACT, *a), "오른쪽 번짐 {i}");
        }
        assert_eq!(rows[y][cap / 2], ACT);
        assert_eq!(rows[y][cap - 1], BAR, "탭 끝의 한 칸은 막대 바탕");
        assert_eq!(rows[y][cap + cap / 2], INA, "고르지 않은 탭은 그 색");
        assert_eq!(rows[y][3 * cap + 5], BAR, "탭 오른쪽 빈 막대");
    }

    #[test]
    fn slant_tabs_cut_their_top_corners() {
        let tabs = [plain(true), plain(false)];
        let Some((rows, bh, cw)) = render(&colored(TabBarStyle::Slant), &tabs, 800) else {
            return;
        };
        let cap = (cw * 24) as usize;
        let last = bh as usize - 1;
        assert_eq!(rows[0][0], BAR, "왼쪽 위는 깎인다");
        assert_eq!(rows[last][cw as usize / 2], ACT, "왼쪽 아래는 탭 색");
        assert_eq!(rows[0][cap - 1], BAR, "오른쪽 위는 깎인다");
        assert_eq!(rows[last][cap - 1 - cw as usize / 2], ACT);
        assert_eq!(rows[last / 2][cap / 2], ACT);
        // 기울기: 깎인 폭은 위에서 아래로 줄어든다
        let cut = |y: usize| (0..cw as usize).take_while(|&x| rows[y][x] == BAR).count();
        assert!(
            cut(1) > cut(last / 2) && cut(last / 2) > cut(last - 1),
            "{} {} {}",
            cut(1),
            cut(last / 2),
            cut(last - 1)
        );
        assert_eq!(rows[0][cap], BAR, "다음 탭도 제 왼쪽 위가 깎인다");
    }

    #[test]
    fn separator_tabs_put_a_separator_cell_between_tabs_only() {
        let look = TabBarLook {
            separator: " | ".into(),
            ..colored(TabBarStyle::Separator)
        };
        let tabs = [plain(true), plain(false), plain(false)];
        let Some((rows, bh, cw)) = render(&look, &tabs, 900) else {
            return;
        };
        let cap = (cw * 24) as usize;
        let y = 1;
        assert_eq!(rows[y][cap / 2], ACT);
        // `|` 한 칸 폭의 구분 자리는 안 고른 탭 색 바탕이고 마지막 탭에는 없다
        assert_eq!(rows[y][cap - 1], INA);
        assert_eq!(rows[y][cap - cw as usize - 1], ACT);
        let end = 3 * cap;
        assert_eq!(rows[y][end - 1], INA);
        assert_eq!(rows[y][end - 1 - cw as usize], INA);
        // 구분 글자가 실제로 그려졌다(바탕과 다른 픽셀이 구분 자리에 있다)
        let sep_pixels = (0..bh as usize)
            .flat_map(|yy| (cap - cw as usize..cap).map(move |x| (x, yy)))
            .filter(|&(x, yy)| rows[yy][x] != INA)
            .count();
        assert!(sep_pixels > 0, "구분 글자가 없다");
    }

    #[test]
    fn powerline_tabs_end_in_a_shape_that_points_into_the_next_tab() {
        let tabs = [plain(true), plain(false), plain(false)];
        for (style, name) in [
            (TabPowerlineStyle::Angled, "angled"),
            (TabPowerlineStyle::Round, "round"),
            (TabPowerlineStyle::Slanted, "slanted"),
        ] {
            let look = TabBarLook {
                powerline: style,
                ..colored(TabBarStyle::Powerline)
            };
            let Some((rows, bh, cw)) = render(&look, &tabs, 900) else {
                return;
            };
            let cap = (cw * 24) as usize;
            let (c, last) = (cw as usize, bh as usize - 1);
            let mid = last / 2;
            // 구분 칸의 바탕은 다음 탭 색, 기호는 이 탭 색
            match style {
                TabPowerlineStyle::Slanted => {
                    assert_eq!(rows[0][cap - 2], ACT, "{name}: 위쪽은 이 탭 색이 끝까지");
                    assert_eq!(rows[last][cap - 1], INA, "{name}: 아래쪽은 다음 탭 색");
                }
                _ => {
                    assert_eq!(rows[0][cap - 1], INA, "{name}: 위쪽 모서리는 다음 탭 색");
                    assert_eq!(rows[last][cap - 1], INA, "{name}: 아래쪽 모서리");
                    assert_eq!(rows[mid][cap - 2], ACT, "{name}: 가운데는 끝까지 이 탭 색");
                }
            }
            assert_eq!(rows[mid][cap - c - 2], ACT, "{name}");
            // 마지막 탭은 막대 바탕으로 끝난다
            let corner = if style == TabPowerlineStyle::Slanted { last } else { 0 };
            assert_eq!(rows[corner][3 * cap - 1], BAR, "{name}: 막대 쪽");
            // 안 고른 탭끼리는 이어 붙지 않게 속이 빈 기호가 같은 색 칸 안에 그려진다
            let soft = (0..bh as usize)
                .flat_map(|y| (2 * cap - c..2 * cap).map(move |x| (x, y)))
                .filter(|&(x, y)| rows[y][x] != INA)
                .count();
            assert!(soft > 0, "{name}: 속 빈 구분 기호가 없다");
            assert_eq!(rows[0][2 * cap - c - 3], INA, "{name}");
        }
    }

    #[test]
    fn style_colors_and_font_style_apply_to_every_style_and_default_keeps_its_theme() {
        let theme = Theme::ghostty_default();
        let tabs = [plain(true), plain(false)];
        // 기본 모양에서도 막대·탭 색을 정하면 따른다. 안 고른 탭은 정했을 때만 칠한다
        let look = TabBarLook {
            bar_background: Some(BAR),
            active_background: Some(ACT),
            ..TabBarLook::default()
        };
        let Some((rows, _, _)) = render(&look, &tabs, 400) else {
            return;
        };
        assert_eq!(rows[2][5], ACT);
        assert_eq!(rows[2][205], BAR);
        let (rows, _, _) = render(&TabBarLook::default(), &tabs, 400).unwrap();
        assert_eq!(rows[2][205], theme.background.mix(theme.foreground, 0.06));
        // 굵은 제목은 보통 제목과 다른 픽셀을 낸다
        let titled = |bold: bool| {
            let look = TabBarLook {
                active_font: if bold {
                    bt_core::tabstyle::TabFontStyle::Bold
                } else {
                    Default::default()
                },
                ..TabBarLook::default()
            };
            let mut t = plain(true);
            t.title = "WWWW iiii".into();
            render(&look, &[t, plain(false)], 400).unwrap().0
        };
        assert_ne!(titled(true), titled(false), "굵게가 그려지지 않았다");
    }

    #[test]
    fn progress_bar_and_dots_stay_inside_the_tab_content() {
        let mut t0 = plain(true);
        t0.progress = Some((1, 100));
        t0.unread = true;
        t0.color = crate::tabcolor::TabColor::Blue;
        let tabs = [t0, plain(false)];
        let Some((rows, bh, cw)) = render(&colored(TabBarStyle::Slant), &tabs, 800) else {
            return;
        };
        let cap = (cw * 24) as usize;
        let theme = Theme::ghostty_default();
        let blue = theme.palette[4];
        let y = bh as usize - 3;
        let xs: Vec<usize> = (0..cap).filter(|&x| rows[y][x] == blue).collect();
        assert!(!xs.is_empty(), "진행 막대가 없다");
        assert!(
            xs[0] >= cw as usize && *xs.last().unwrap() < cap - cw as usize,
            "기울어진 모서리 칸 밖"
        );
        let yellow = theme.palette[3];
        let dot_x: Vec<usize> = (0..cap).filter(|&x| rows[bh as usize / 2][x] == yellow).collect();
        assert!(
            !dot_x.is_empty() && dot_x[0] >= cw as usize,
            "안 읽음 점이 모서리 안쪽에 없다"
        );
    }
}
