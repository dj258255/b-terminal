//! 명령 표시(#927): 셸 통합이 알린 명령의 결과를 프롬프트 줄 왼쪽 끝 막대로 보이고, 마우스를 올리면 종료 코드·걸린 시간을 알린다.
//! 격자 칸·커서는 바꾸지 않는다. 막대는 `DrawCell::mark`로 CPU·GPU 두 길이 같은 규칙으로 그린다(`bt_render::mark_bar_width`).
//! 이 모듈은 어떤 줄에 어떤 색을 줄지와 알림 글만 정한다(창과 상관없는 순수 함수)

use std::time::Duration;

use bt_core::config::CommandMarks;
use bt_core::{Rgb, Theme};
use bt_render::DrawCell;
use bt_term::command_marks::{MarkState, RowMark};

/// 막대 색. 끝난 명령만 칠한다: 실패는 팔레트 빨강(1번), 성공은 팔레트 초록(2번)을 바탕에 절반쯤 섞어 눈에 덜 띄게.
/// 실행 중·프롬프트만 있는 줄·종료 코드를 모르는 줄은 칠하지 않는다
pub fn bar_color(state: MarkState, mode: CommandMarks, theme: &Theme, background: Rgb) -> Option<Rgb> {
    let MarkState::Done { exit_code, .. } = state else {
        return None;
    };
    if !mode.shows(exit_code) {
        return None;
    }
    Some(if exit_code == Some(0) {
        background.mix(theme.palette[2], 0.6)
    } else {
        theme.palette[1]
    })
}

/// 화면의 표시들을 줄 맨 앞 칸에 붙인다. 칸 수에 맞지 않는 줄은 건너뛴다
pub fn apply(
    cells: &mut [DrawCell],
    cols: usize,
    marks: &[RowMark],
    mode: CommandMarks,
    theme: &Theme,
    background: Rgb,
) {
    if mode == CommandMarks::False || cols == 0 {
        return;
    }
    for m in marks {
        if let (Some(cell), Some(color)) = (cells.get_mut(m.row * cols), bar_color(m.state, mode, theme, background)) {
            cell.mark = Some(color);
        }
    }
}

/// 걸린 시간 글: 1초 밑은 `0.4초`, 1분 밑은 `3.2초`, 한 시간 밑은 `2분 5초`, 그 위는 `1시간 2분`
pub fn duration_text(d: Duration) -> String {
    let secs = d.as_secs_f64();
    if secs < 60.0 {
        return format!("{:.1}초", (secs * 10.0).floor() / 10.0);
    }
    let total = d.as_secs();
    if total < 3600 {
        format!("{}분 {}초", total / 60, total % 60)
    } else {
        format!("{}시간 {}분", total / 3600, (total % 3600) / 60)
    }
}

/// 막대 위에 마우스를 올렸을 때 창 왼쪽 아래 띠에 보일 글. 막대가 없는 줄이면 없다
pub fn hint_text(state: MarkState, mode: CommandMarks) -> Option<String> {
    let MarkState::Done { exit_code, duration } = state else {
        return None;
    };
    if !mode.shows(exit_code) {
        return None;
    }
    let code = exit_code?;
    Some(format!("종료 코드 {code} · 걸린 시간 {}", duration_text(duration)))
}

/// 격자 왼쪽 끝에서 `x`픽셀 떨어진 곳이 첫 칸 열(막대가 있는 열)인가. 막대는 가늘어 첫 칸 폭 전체를 맞히는 자리로 쓴다
pub fn over_bar_column(x: f64, cell_width: u32) -> bool {
    x >= 0.0 && x < cell_width as f64
}

#[cfg(test)]
mod tests {
    use super::*;

    fn done(code: Option<i32>, secs: f64) -> MarkState {
        MarkState::Done {
            exit_code: code,
            duration: Duration::from_secs_f64(secs),
        }
    }

    fn theme() -> Theme {
        Theme::default()
    }

    const BG: Rgb = Rgb { r: 10, g: 10, b: 10 };

    #[test]
    fn failed_commands_get_palette_red_and_successes_a_quieter_green() {
        let t = theme();
        assert_eq!(
            bar_color(done(Some(1), 0.1), CommandMarks::True, &t, BG),
            Some(t.palette[1])
        );
        let ok = bar_color(done(Some(0), 0.1), CommandMarks::True, &t, BG).unwrap();
        assert_ne!(ok, t.palette[2], "성공은 초록 그대로가 아니라 바탕에 섞는다");
        assert_ne!(ok, t.palette[1]);
    }

    #[test]
    fn mode_filters_bars_and_unfinished_or_unknown_commands_get_none() {
        let t = theme();
        assert!(bar_color(done(Some(0), 1.0), CommandMarks::Failed, &t, BG).is_none());
        assert!(bar_color(done(Some(3), 1.0), CommandMarks::Failed, &t, BG).is_some());
        assert!(bar_color(done(Some(3), 1.0), CommandMarks::False, &t, BG).is_none());
        for s in [MarkState::Prompt, MarkState::Running, done(None, 1.0)] {
            assert!(bar_color(s, CommandMarks::True, &t, BG).is_none(), "{s:?}");
        }
    }

    fn blank() -> DrawCell {
        DrawCell {
            ch: ' ',
            fg: Rgb::new(255, 255, 255),
            bg: BG,
            bold: false,
            italic: false,
            underline: bt_render::Underline::None,
            strike: false,
            wide: false,
            spacer: false,
            selected: false,
            emoji: false,
            cluster: None,
            cluster_cells: 0,
            overline: false,
            underline_color: None,
            hidden: false,
            faint: None,
            mark: None,
        }
    }

    #[test]
    fn apply_marks_only_the_first_cell_of_marked_rows() {
        let mut cells = vec![blank(); 4 * 3];
        let marks = [
            RowMark {
                row: 1,
                state: done(Some(2), 1.0),
            },
            RowMark {
                row: 2,
                state: MarkState::Running,
            },
            RowMark {
                row: 9,
                state: done(Some(2), 1.0),
            },
        ];
        apply(&mut cells, 4, &marks, CommandMarks::True, &theme(), BG);
        let marked: Vec<usize> = cells
            .iter()
            .enumerate()
            .filter(|(_, c)| c.mark.is_some())
            .map(|(i, _)| i)
            .collect();
        assert_eq!(marked, vec![4]);
        // 끄면 아무것도 칠하지 않는다
        let mut off = vec![blank(); 12];
        apply(&mut off, 4, &marks, CommandMarks::False, &theme(), BG);
        assert!(off.iter().all(|c| c.mark.is_none()));
    }

    #[test]
    fn duration_text_scales_from_tenths_of_a_second_to_hours() {
        for (secs, want) in [
            (0.0, "0.0초"),
            (0.45, "0.4초"),
            (3.25, "3.2초"),
            (59.99, "59.9초"),
            (60.0, "1분 0초"),
            (125.0, "2분 5초"),
            (3600.0, "1시간 0분"),
            (3725.0, "1시간 2분"),
        ] {
            assert_eq!(duration_text(Duration::from_secs_f64(secs)), want, "{secs}");
        }
    }

    #[test]
    fn hint_names_exit_code_and_duration_and_has_no_internal_doc_numbers() {
        let text = hint_text(done(Some(1), 3.2), CommandMarks::True).unwrap();
        assert_eq!(text, "종료 코드 1 · 걸린 시간 3.2초");
        assert!(!text.contains("ADR"));
        assert!(
            hint_text(done(Some(0), 1.0), CommandMarks::Failed).is_none(),
            "막대가 없는 줄은 띠도 없다"
        );
        assert!(hint_text(done(None, 1.0), CommandMarks::True).is_none());
        assert!(hint_text(MarkState::Running, CommandMarks::True).is_none());
    }

    /// 두 그리기 길이 같은 규칙을 쓴다: 막대는 한 곳(`apply`)에서 `DrawCell::mark`로 정해 두 길이 함께 읽고, 띠는 두 길 모두 그린다
    #[test]
    fn both_render_paths_share_the_bar_and_draw_the_hover_hint() {
        let app = include_str!("app.rs");
        let gpu = include_str!("app/gpu_frame.rs");
        assert_eq!(
            app.matches(concat!("cmd_marks::", "apply(")).count(),
            1,
            "막대는 `drawn` 한 곳에서 칸에 붙인다"
        );
        assert!(
            !gpu.contains(concat!("cmd_marks::", "apply(")),
            "GPU 길은 같은 `drawn` 칸을 받는다"
        );
        for (name, src) in [("CPU", app), ("GPU", gpu)] {
            assert!(
                src.contains(concat!("draw_link_hint(", "")) && src.contains("mark_hint"),
                "{name} 길에 띠가 없다"
            );
        }
        assert!(gpu.contains("|| mark_hint.is_some()"), "층 재사용 키가 띠를 센다");
    }

    #[test]
    fn the_hover_column_is_the_first_cell_only() {
        assert!(over_bar_column(0.0, 16) && over_bar_column(15.9, 16));
        assert!(!over_bar_column(16.0, 16) && !over_bar_column(-1.0, 16));
    }
}
