//! 깜빡이는 글자(SGR 5·6, #862). kitty처럼 커서 깜빡임과 같은 박자로 글자를 숨겼다 보인다.
//! 숨길 때는 SGR 8과 같은 `DrawCell::hidden`을 쓴다: 바탕은 그대로 두고 글자·밑줄·윗줄·취소선을 그리지 않는다.
//! 렌더러(CPU·GPU)는 `hidden`만 보므로 두 길이 같은 판단을 따른다

use bt_render::DrawCell;
use bt_term::{Cell, CellFlags};

/// 보이는 깜빡임이 있는가: 깜빡임 표시가 있고 그릴 것(글자나 줄 꾸밈)이 있는 칸이 하나라도 있으면 참.
/// 거짓이면 깜빡임 때문에 다시 그리지 않는다(전력)
pub fn wanted(cells: &[Cell]) -> bool {
    cells.iter().any(inks)
}

/// 깜빡이는 칸에 그릴 것이 있는가. 숨김(SGR 8)·자리 채움 칸과 빈칸(꾸밈 없음)은 깜빡여도 달라 보이지 않는다
fn inks(c: &Cell) -> bool {
    let f = c.flags;
    if !f.contains(CellFlags::BLINK) || f.contains(CellFlags::HIDDEN) || f.contains(CellFlags::SPACER) {
        return false;
    }
    let blank = matches!(c.ch, ' ' | '\0');
    !blank
        || f.contains(CellFlags::STRIKE)
        || f.contains(CellFlags::OVERLINE)
        || f.contains(CellFlags::UNDERLINE)
        || f.contains(CellFlags::DOUBLE_UNDERLINE)
        || f.contains(CellFlags::UNDERCURL)
        || f.contains(CellFlags::DOTTED_UNDERLINE)
        || f.contains(CellFlags::DASHED_UNDERLINE)
}

/// 깜빡임 박자가 "꺼짐"이면 깜빡이는 칸을 숨긴다. "켜짐"이면 그대로 둔다
pub fn apply(draw: &mut [DrawCell], src: &[Cell], visible: bool) {
    if visible {
        return;
    }
    for (d, c) in draw.iter_mut().zip(src) {
        if c.flags.contains(CellFlags::BLINK) {
            d.hidden = true;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use bt_term::{Emulator, GridSize, Screen};

    fn screen(bytes: &[u8]) -> Screen {
        let mut e = Emulator::new(GridSize::new(8, 2), bt_core::Theme::default());
        e.feed(bytes);
        e.snapshot()
    }

    fn draw(s: &Screen) -> Vec<DrawCell> {
        crate::app::to_draw_cells(s, &bt_core::Theme::default())
    }

    #[test]
    fn sgr_5_and_6_cells_are_hidden_only_in_the_off_phase() {
        let s = screen(b"\x1b[5mab\x1b[0mc\x1b[6md");
        let mut on = draw(&s);
        apply(&mut on, &s.cells, true);
        assert!(on.iter().all(|c| !c.hidden), "켜짐 박자에는 숨기지 않는다");
        let mut off = draw(&s);
        apply(&mut off, &s.cells, false);
        let hidden: Vec<bool> = off.iter().take(4).map(|c| c.hidden).collect();
        assert_eq!(hidden, [true, true, false, true], "a b(5) c(깜빡임 아님) d(6)");
    }

    #[test]
    fn cancel_blink_25_ends_the_blinking() {
        let s = screen(b"\x1b[5ma\x1b[25mb");
        let mut off = draw(&s);
        apply(&mut off, &s.cells, false);
        assert_eq!((off[0].hidden, off[1].hidden), (true, false));
    }

    #[test]
    fn wanted_only_when_a_visible_blink_cell_exists() {
        assert!(!wanted(&screen(b"plain text").cells));
        assert!(wanted(&screen(b"\x1b[5mx").cells));
        // 깜빡임 켠 채 빈칸만 있으면(지우기 등) 달라 보이지 않으니 다시 그리지 않는다
        assert!(!wanted(&screen(b"\x1b[5m   \x1b[0m").cells));
        // 빈칸이어도 밑줄이 있으면 깜빡임이 보인다
        assert!(wanted(&screen(b"\x1b[5;4m  ").cells));
        // 숨김(SGR 8)은 깜빡여도 보이지 않는다
        assert!(!wanted(&screen(b"\x1b[5;8mx").cells));
    }

    fn paint(f: &mut bt_render::Fonts, s: &Screen, visible: bool) -> Vec<u32> {
        use bt_render::{Canvas, GridView, draw_grid};
        let theme = bt_core::Theme::default();
        let m = f.metrics();
        let (w, h) = (m.width * s.cols as u32, m.height * s.rows as u32);
        let mut dc = draw(s);
        apply(&mut dc, &s.cells, visible);
        let mut buf = vec![0u32; (w * h) as usize];
        let view = GridView {
            cols: s.cols,
            rows: s.rows,
            cells: &dc,
            cursor: None,
            selection_bg: theme.selection_background,
            selection_fg: theme.selection_foreground,
            selection_bg_cell: Default::default(),
            selection_fg_cell: Default::default(),
            preedit: None,
            min_contrast: 1.0,
            trail: None,
        };
        let mut c = Canvas::new(&mut buf, w, h);
        c.clear(theme.background);
        draw_grid(&mut c, f, 0, 0, &view);
        buf
    }

    /// CPU 길 픽셀: 깜빡임 박자가 꺼짐이면 글자 없는 화면과 같고, 켜짐이면 글자가 그려진다
    #[test]
    fn blinking_glyph_ink_appears_and_disappears_on_the_cpu_path() {
        let Ok(mut f) = bt_render::Fonts::new(&bt_render::FontConfig::default()) else {
            return;
        };
        let blink = screen(b"\x1b[5mMMMM");
        let empty = screen(b"");
        let blank = paint(&mut f, &empty, true);
        assert_ne!(paint(&mut f, &blink, true), blank, "켜짐 박자에 글자가 그려져야 한다");
        assert_eq!(
            paint(&mut f, &blink, false),
            blank,
            "꺼짐 박자에는 글자 픽셀이 없어야 한다"
        );
        // 대조: 깜빡임이 아닌 글자는 박자와 상관없이 그려진다
        let plain = screen(b"MMMM");
        assert_eq!(paint(&mut f, &plain, true), paint(&mut f, &plain, false));
        assert_ne!(paint(&mut f, &plain, false), blank);
    }
}
