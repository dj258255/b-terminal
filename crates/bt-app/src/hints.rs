//! 힌트 모드(#865): 화면의 글에 라벨을 붙여 키보드로 고르는 상태, 라벨 입력, 그리기, 동작 결과.
//! 찾기와 라벨 차례는 `bt_term::hints`(창 없이 시험)가 하고, 여기는 창 쪽 몫이다. 창을 만지는 일(열기·클립보드·붙여넣기)은 `app.rs`가 한다
use bt_core::{Rgb, Theme};
use bt_render::{DrawCell, Underline};
use bt_term::hints::{DEFAULT_ALPHABET, editor_command, labels_for, matching};
pub use bt_term::hints::{HintAction, HintKind, HintMark};
use bt_term::links::LinkSpan;

/// 화면 문구에 쓰는 종류 이름
pub fn kind_label(kind: HintKind) -> &'static str {
    match kind {
        HintKind::Url => "주소",
        HintKind::Path => "경로",
        HintKind::Linenum => "파일:줄",
        HintKind::Line => "줄",
        HintKind::Hash => "해시",
        HintKind::Word => "낱말",
        HintKind::Hyperlink => "링크",
    }
}

/// 글자 하나를 쳤을 때
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Typed {
    /// 라벨 글자가 아니거나 맞는 라벨이 없어 받지 않았다
    Ignored,
    /// 받았고 후보가 둘 이상 남았다
    Narrowed,
    /// 후보가 하나만 남아 그 맞은 곳을 골랐다(kitty와 같다)
    Chosen(usize),
}

/// 힌트 모드: 한 칸의 화면에서 찾은 맞은 곳과 지금까지 친 라벨
pub struct HintsState {
    pub pane: u64,
    pub kind: HintKind,
    pub action: HintAction,
    pub marks: Vec<HintMark>,
    /// 맞은 곳마다의 라벨(위에서 아래 차례, `marks`와 같은 자리)
    pub labels: Vec<String>,
    /// 지금까지 친 라벨 글자
    pub input: String,
}

/// 한 맞은 곳을 어떻게 그릴지
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PaintMark {
    pub spans: Vec<LinkSpan>,
    /// 아직 안 친 라벨 글자(칸 맨 앞에 덧그린다)
    pub label: String,
    /// 친 글자와 맞지 않아 흐리게
    pub dim: bool,
}

/// 라벨 색과 흐림. kitty 기본(`hints_foreground_color black`, `hints_background_color green`)을 테마 색으로
#[derive(Debug, Clone, Copy)]
pub struct HintStyle {
    pub label_fg: Rgb,
    pub label_bg: Rgb,
    pub dim: f32,
}

impl HintStyle {
    pub fn from_theme(theme: &Theme) -> Self {
        HintStyle {
            label_fg: theme.palette[0],
            label_bg: theme.palette[2],
            dim: theme.faint_opacity,
        }
    }
}

/// 고른 뒤 할 일
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Outcome {
    /// `open_link`로 연다. 거기서 확인 막대·허용 목록을 거친다
    Open { text: String, explicit: bool },
    /// 시스템 클립보드로
    Copy(String),
    /// `paste_to`로 붙여넣는다
    Paste(String),
}

impl HintsState {
    pub fn new(pane: u64, kind: HintKind, action: HintAction, marks: Vec<HintMark>) -> Self {
        let labels = labels_for(marks.len(), DEFAULT_ALPHABET);
        HintsState {
            pane,
            kind,
            action,
            marks,
            labels,
            input: String::new(),
        }
    }

    /// 글자를 친다. 알파벳 밖이거나 맞는 라벨이 없으면 받지 않고, 후보가 하나 남으면 그것을 고른다(kitty `OnText`).
    /// 맞는 라벨이 없는 글자를 받지 않는 것은 kitty와 다르다(kitty는 받고 모두 흐려져 지워야 한다)
    pub fn type_char(&mut self, ch: char) -> Typed {
        if !DEFAULT_ALPHABET.contains(ch) {
            return Typed::Ignored;
        }
        let mut next = self.input.clone();
        next.push(ch);
        let hits = matching(&self.labels, &next);
        match hits.as_slice() {
            [] => Typed::Ignored,
            [one] => {
                self.input = next;
                Typed::Chosen(*one)
            }
            _ => {
                self.input = next;
                Typed::Narrowed
            }
        }
    }

    /// 마지막 글자를 지운다. 지울 것이 있었으면 참
    pub fn backspace(&mut self) -> bool {
        self.input.pop().is_some()
    }

    /// Enter·Space: 친 글자와 똑같은 라벨의 맞은 곳(kitty `decode_hint`)
    pub fn confirm(&self) -> Option<usize> {
        if self.input.is_empty() {
            return None;
        }
        self.labels.iter().position(|l| *l == self.input)
    }

    /// 맞은 곳 아래 칸 글자가 처음 그대로인가(`ok`가 맞은 곳 하나를 지금 화면과 견준다). 하나라도 바뀌었으면 거짓
    pub fn valid_with(&self, ok: impl Fn(&HintMark) -> bool) -> bool {
        self.marks.iter().all(ok)
    }

    /// 지금 입력에 맞춘 그리기 계획
    pub fn plan(&self) -> Vec<PaintMark> {
        self.marks
            .iter()
            .zip(&self.labels)
            .map(|(m, l)| match l.strip_prefix(self.input.as_str()) {
                Some(rest) => PaintMark {
                    spans: m.spans.clone(),
                    label: rest.to_string(),
                    dim: false,
                },
                None => PaintMark {
                    spans: m.spans.clone(),
                    label: String::new(),
                    dim: true,
                },
            })
            .collect()
    }
}

/// 라벨이 덮는 칸 하나를 라벨 칸으로 바꾼다. 이 칸이 넓은 글자의 앞 칸이면 뒤 칸을 라벨 색 빈 칸으로 바꿔 격자를 맞춘다
fn label_cell(cells: &mut [DrawCell], i: usize, cols_end: usize, ch: char, style: &HintStyle) {
    // 앞 칸에서 시작한 이모지 묶음이 이 칸을 덮고 있으면 묶음을 푼다
    let from = i.saturating_sub(8);
    for (k, c) in cells[from..i].iter_mut().enumerate() {
        if c.cluster.is_some() && from + k + c.cluster_cells as usize > i {
            c.cluster = None;
            c.cluster_cells = 0;
        }
    }
    let was_wide = cells[i].wide;
    let c = &mut cells[i];
    *c = DrawCell {
        ch,
        fg: style.label_fg,
        bg: style.label_bg,
        bold: true,
        italic: false,
        underline: Underline::None,
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
    };
    if was_wide && i + 1 < cols_end {
        let s = &mut cells[i + 1];
        let mut blank = s.clone();
        blank.ch = ' ';
        blank.spacer = false;
        blank.wide = false;
        blank.fg = style.label_fg;
        blank.bg = style.label_bg;
        blank.underline = Underline::None;
        blank.underline_color = None;
        blank.strike = false;
        blank.overline = false;
        blank.hidden = false;
        blank.selected = false;
        blank.emoji = false;
        blank.cluster = None;
        blank.cluster_cells = 0;
        blank.faint = None;
        *s = blank;
    }
}

/// 렌더러 셀에 힌트를 덧그린다: 맞은 곳 글자는 굵게 + 밑줄, 그 맨 앞 칸들에 아직 안 친 라벨, 친 글자와 맞지 않는 곳은 흐리게.
/// 칸 폭은 VT 엔진이 정한 `wide`·`spacer`를 그대로 따른다(렌더러가 따로 세지 않는다)
pub fn paint(cells: &mut [DrawCell], cols: usize, plan: &[PaintMark], style: &HintStyle) {
    for pm in plan {
        for sp in &pm.spans {
            for c in sp.start..sp.end.min(cols) {
                let Some(cell) = cells.get_mut(sp.row * cols + c) else {
                    continue;
                };
                if pm.dim {
                    cell.faint = Some(style.dim);
                } else {
                    cell.bold = true;
                    cell.underline = Underline::Single;
                }
            }
        }
        let Some(first) = pm.spans.first().filter(|_| !pm.dim) else {
            continue;
        };
        let row_start = first.row * cols;
        let row_end = row_start + cols;
        for (col, ch) in (first.start..).zip(pm.label.chars()) {
            let i = row_start + col;
            if col >= cols || i >= cells.len() {
                break;
            }
            label_cell(cells, i, row_end.min(cells.len()), ch, style);
        }
    }
}

/// 화면에서 찾은 글을 붙이거나 복사하기 전에 다듬는다. 제어 문자(ESC·Ctrl-C·C1)와 줄 구분자는 공백으로, 글 순서를 뒤집거나 숨기는 서식 문자는 지운다.
/// 화면 글은 원격 프로그램이 정한 글이라 믿지 않는다
fn sanitize(text: &str) -> String {
    bt_core::proto::printable(text)
}

/// 고른 맞은 곳으로 할 일. `경로:줄`은 열기도 편집기 명령을 붙여넣을 뿐이고 Enter는 보내지 않는다
pub fn outcome(kind: HintKind, action: HintAction, mark: &HintMark) -> Outcome {
    if kind == HintKind::Linenum {
        let cmd = match (&mark.path, mark.line) {
            (Some(p), Some(l)) => editor_command(&sanitize(p), l),
            _ => sanitize(&mark.text),
        };
        return match action {
            HintAction::Copy => Outcome::Copy(cmd),
            HintAction::Open | HintAction::Paste => Outcome::Paste(cmd),
        };
    }
    match action {
        HintAction::Open => Outcome::Open {
            text: mark.text.clone(),
            explicit: kind == HintKind::Hyperlink,
        },
        HintAction::Copy => Outcome::Copy(sanitize(&mark.text)),
        HintAction::Paste => Outcome::Paste(sanitize(&mark.text)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use bt_term::hints::MAX_MARKS;

    fn mark(text: &str, row: usize, start: usize, end: usize) -> HintMark {
        HintMark {
            text: text.into(),
            spans: vec![LinkSpan { row, start, end }],
            path: None,
            line: None,
            sig: 0,
        }
    }

    fn marks(n: usize) -> Vec<HintMark> {
        (0..n)
            .map(|i| mark(&format!("https://e.example/{i}"), i, 0, 5))
            .collect()
    }

    fn state(n: usize) -> HintsState {
        HintsState::new(7, HintKind::Url, HintAction::Open, marks(n))
    }

    // ---- 라벨 입력 ----

    #[test]
    fn single_remaining_candidate_is_chosen_at_once() {
        let mut s = state(3); // 라벨 3, 2, 1(맨 아래가 1)
        assert_eq!(s.type_char('1'), Typed::Chosen(2));
        let mut s = state(3);
        assert_eq!(s.type_char('3'), Typed::Chosen(0));
    }

    #[test]
    fn two_char_labels_narrow_then_choose() {
        let mut s = state(40); // 00~04가 위 다섯 줄
        assert_eq!(s.type_char('0'), Typed::Narrowed);
        assert_eq!(s.input, "0");
        assert_eq!(s.type_char('4'), Typed::Chosen(0));
    }

    #[test]
    fn chars_that_match_no_label_are_ignored() {
        let mut s = state(3);
        assert_eq!(s.type_char('q'), Typed::Ignored, "라벨에 없는 글자");
        assert_eq!(s.type_char('A'), Typed::Ignored, "대문자는 알파벳 밖");
        assert_eq!(s.type_char('한'), Typed::Ignored);
        assert_eq!(s.type_char(' '), Typed::Ignored);
        assert_eq!(s.input, "");
        let mut s = state(40);
        assert_eq!(s.type_char('0'), Typed::Narrowed);
        assert_eq!(s.type_char('9'), Typed::Ignored, "09는 없다");
        assert_eq!(s.input, "0");
    }

    #[test]
    fn backspace_removes_the_last_typed_char() {
        let mut s = state(40);
        s.type_char('0');
        assert!(s.backspace());
        assert_eq!(s.input, "");
        assert!(!s.backspace(), "지울 것이 없다");
    }

    #[test]
    fn enter_picks_the_exact_label_even_when_longer_ones_share_it() {
        // 1332개면 한 글자 "1"과 두 글자 "10"이 함께 있다. "1"을 치면 아직 여럿이라 좁히기만 하고, Enter가 "1"을 고른다
        let mut s = state(MAX_MARKS);
        assert_eq!(s.type_char('1'), Typed::Narrowed);
        assert_eq!(s.confirm(), Some(MAX_MARKS - 1), "맨 아래 맞은 곳이 \"1\"");
        let mut s = state(MAX_MARKS);
        assert_eq!(s.confirm(), None, "아무것도 안 쳤으면 고르지 않는다");
        s.type_char('0');
        assert_eq!(s.confirm(), None, "\"0\"이라는 라벨은 없다");
    }

    #[test]
    fn hints_end_when_the_text_under_a_mark_changes() {
        let mut s = state(1);
        s.marks[0].sig = 42;
        assert!(s.valid_with(|m| m.sig == 42));
        assert!(!s.valid_with(|m| m.sig == 43));
    }

    // ---- 그리기 계획 ----

    #[test]
    fn plan_shows_remaining_label_and_dims_the_others() {
        let mut s = state(40);
        let p = s.plan();
        assert_eq!(p.len(), 40);
        assert_eq!((p[0].label.as_str(), p[0].dim), ("04", false));
        assert_eq!((p[39].label.as_str(), p[39].dim), ("1", false));
        s.type_char('0');
        let p = s.plan();
        assert_eq!((p[0].label.as_str(), p[0].dim), ("4", false), "친 글자는 뺀 나머지만");
        assert!(p[39].dim && p[39].label.is_empty(), "다른 마크는 흐리게");
    }

    fn cell(ch: char, wide: bool, spacer: bool) -> DrawCell {
        DrawCell {
            ch,
            fg: Rgb::new(200, 200, 200),
            bg: Rgb::new(10, 10, 10),
            bold: false,
            italic: true,
            underline: Underline::Curly,
            strike: true,
            wide,
            spacer,
            selected: true,
            emoji: true,
            cluster: None,
            cluster_cells: 0,
            overline: true,
            underline_color: Some(Rgb::new(1, 2, 3)),
            hidden: true,
            faint: Some(0.5),
        }
    }

    fn style() -> HintStyle {
        HintStyle {
            label_fg: Rgb::new(0, 0, 0),
            label_bg: Rgb::new(0, 200, 0),
            dim: 0.4,
        }
    }

    fn row(text: &str) -> Vec<DrawCell> {
        // 한글은 두 칸: 앞 칸 `wide`, 뒤 칸 `spacer`
        let mut v = Vec::new();
        for ch in text.chars() {
            if matches!(ch as u32, 0xAC00..=0xD7A3) {
                v.push(cell(ch, true, false));
                v.push(cell(' ', false, true));
            } else {
                v.push(cell(ch, false, false));
            }
        }
        v
    }

    fn text_of(cells: &[DrawCell]) -> String {
        cells.iter().filter(|c| !c.spacer).map(|c| c.ch).collect()
    }

    #[test]
    fn label_is_painted_over_the_first_cells_of_the_mark() {
        let mut cells = row("go https://a.b/c");
        let plan = [PaintMark {
            spans: vec![LinkSpan {
                row: 0,
                start: 3,
                end: 16,
            }],
            label: "ab".into(),
            dim: false,
        }];
        paint(&mut cells, 16, &plan, &style());
        assert_eq!(text_of(&cells), "go abtps://a.b/c");
        for i in [3, 4] {
            let c = &cells[i];
            assert_eq!(
                (c.fg, c.bg, c.bold),
                (Rgb::new(0, 0, 0), Rgb::new(0, 200, 0), true),
                "{i}"
            );
            // 원래 칸의 꾸밈은 라벨 칸에 남지 않는다
            assert!(
                !c.italic && !c.strike && !c.overline && !c.hidden && !c.selected && !c.emoji,
                "{i}"
            );
            assert_eq!(
                (c.underline, c.underline_color, c.faint),
                (Underline::None, None, None),
                "{i}"
            );
        }
        // 마크의 나머지 글자는 굵게 + 밑줄
        assert!(cells[5].bold && cells[5].underline == Underline::Single);
        // 마크 밖은 그대로
        assert_eq!(cells[0], cell('g', false, false));
    }

    #[test]
    fn label_over_a_wide_char_replaces_the_whole_char_and_keeps_columns() {
        // "가나다"는 6칸. 라벨 한 글자는 앞 칸만 덮고 뒤 칸을 빈 칸으로 만들어 격자가 어긋나지 않는다
        let mut cells = row("가나다");
        assert_eq!(cells.len(), 6);
        let plan = [PaintMark {
            spans: vec![LinkSpan {
                row: 0,
                start: 0,
                end: 6,
            }],
            label: "x".into(),
            dim: false,
        }];
        paint(&mut cells, 6, &plan, &style());
        assert_eq!(cells.len(), 6, "칸 수는 그대로");
        assert!(!cells[0].wide && cells[0].ch == 'x');
        assert!(
            !cells[1].spacer && cells[1].ch == ' ' && cells[1].bg == Rgb::new(0, 200, 0),
            "뒤 칸은 라벨 색 빈 칸"
        );
        assert!(
            cells[2].wide && cells[2].ch == '나' && cells[3].spacer,
            "다음 글자는 그대로"
        );
        // 두 글자 라벨은 뒤 칸까지 쓴다
        let mut cells = row("가나다");
        let plan = [PaintMark {
            spans: vec![LinkSpan {
                row: 0,
                start: 0,
                end: 6,
            }],
            label: "xy".into(),
            dim: false,
        }];
        paint(&mut cells, 6, &plan, &style());
        assert_eq!((cells[0].ch, cells[1].ch), ('x', 'y'));
        assert!(!cells[0].wide && !cells[1].spacer && !cells[1].wide);
        assert!(cells[2].wide && cells[3].spacer);
        // 셋 글자 라벨이 둘째 한글의 앞 칸까지 걸치면 그 한글도 통째로 바뀐다
        let mut cells = row("가나다");
        let plan = [PaintMark {
            spans: vec![LinkSpan {
                row: 0,
                start: 0,
                end: 6,
            }],
            label: "xyz".into(),
            dim: false,
        }];
        paint(&mut cells, 6, &plan, &style());
        assert_eq!(
            (cells[2].ch, cells[2].wide, cells[3].spacer, cells[3].ch),
            ('z', false, false, ' ')
        );
        assert!(cells[4].wide && cells[5].spacer);
        assert_eq!(
            cells.iter().filter(|c| c.wide).count(),
            cells.iter().filter(|c| c.spacer).count(),
            "넓은 칸과 뒤 칸은 늘 짝"
        );
    }

    #[test]
    fn dimmed_marks_only_fade_and_label_stops_at_row_end() {
        let mut cells = row("abcdef");
        let plan = [
            PaintMark {
                spans: vec![LinkSpan {
                    row: 0,
                    start: 0,
                    end: 3,
                }],
                label: String::new(),
                dim: true,
            },
            PaintMark {
                spans: vec![LinkSpan {
                    row: 0,
                    start: 4,
                    end: 6,
                }],
                label: "xyz".into(),
                dim: false,
            },
        ];
        paint(&mut cells, 6, &plan, &style());
        assert!(cells[..3].iter().all(|c| c.faint == Some(0.4)));
        assert_eq!(cells[0].ch, 'a', "흐린 마크는 글자를 바꾸지 않는다");
        // 라벨이 줄 끝을 넘으면 넘는 글자는 버린다(다음 줄을 덮지 않는다)
        assert_eq!(text_of(&cells), "abcdxy");
    }

    // ---- 동작 결과 ----

    #[test]
    fn outcomes_follow_kind_and_action() {
        let m = mark("https://a.b/c", 0, 0, 13);
        assert_eq!(
            outcome(HintKind::Url, HintAction::Open, &m),
            Outcome::Open {
                text: "https://a.b/c".into(),
                explicit: false
            }
        );
        assert_eq!(
            outcome(HintKind::Hyperlink, HintAction::Open, &m),
            Outcome::Open {
                text: "https://a.b/c".into(),
                explicit: true
            },
            "OSC 8는 주소만 본다"
        );
        assert_eq!(
            outcome(HintKind::Url, HintAction::Copy, &m),
            Outcome::Copy("https://a.b/c".into())
        );
        assert_eq!(
            outcome(HintKind::Word, HintAction::Paste, &m),
            Outcome::Paste("https://a.b/c".into())
        );
    }

    #[test]
    fn linenum_outcomes_are_an_editor_command_without_enter() {
        let mut m = mark("src/a b.rs:12:3", 0, 0, 15);
        m.path = Some("src/a b.rs".into());
        m.line = Some(12);
        let want = "$EDITOR +12 'src/a b.rs'";
        // 열기도 붙여넣기다. 실행(Enter)은 하지 않는다
        for a in [HintAction::Open, HintAction::Paste] {
            assert_eq!(outcome(HintKind::Linenum, a, &m), Outcome::Paste(want.into()), "{a:?}");
        }
        assert_eq!(
            outcome(HintKind::Linenum, HintAction::Copy, &m),
            Outcome::Copy(want.into())
        );
    }

    /// 화면에서 찾은 글은 원격 프로그램이 정한 글이다. 붙이거나 복사하는 글에 제어 문자·보이지 않는 서식 문자가 남지 않는다
    #[test]
    fn copied_and_pasted_text_has_no_control_or_hidden_chars() {
        let hostile = "ok\x1b[201~rm -rf ~\x03\x15\u{9b}\u{202e}end\u{2028}x";
        let m = mark(hostile, 0, 0, 10);
        for out in [
            outcome(HintKind::Word, HintAction::Paste, &m),
            outcome(HintKind::Word, HintAction::Copy, &m),
        ] {
            let (Outcome::Paste(t) | Outcome::Copy(t)) = out else {
                panic!("{out:?}");
            };
            assert!(
                !t.chars()
                    .any(|c| c.is_control() || matches!(c, '\u{202e}' | '\u{2028}')),
                "{t:?}"
            );
            assert!(!t.contains("\x1b[201~"));
            assert!(t.starts_with("ok") && t.contains("end"));
        }
        // 열기는 원문 그대로 `open_link`로 간다(거기서 종류별로 거른다)
        let Outcome::Open { text, .. } = outcome(HintKind::Url, HintAction::Open, &m) else {
            panic!()
        };
        assert_eq!(text, hostile);
    }

    /// 붙여넣기 바이트는 `bt_term::encode_paste`를 지난다(괄호 붙여넣기 모드 틀, 위험 바이트 바꾸기). 힌트가 따로 쓰지 않는 길이다
    #[test]
    fn hint_paste_goes_through_the_normal_paste_encoder() {
        let Outcome::Paste(t) = outcome(HintKind::Word, HintAction::Paste, &mark("ab\x1b[201~cd", 0, 0, 9)) else {
            panic!()
        };
        let bytes = bt_term::encode_paste(&t, true);
        assert!(bytes.starts_with(b"\x1b[200~") && bytes.ends_with(b"\x1b[201~"));
        assert_eq!(
            bytes.windows(6).filter(|w| *w == b"\x1b[201~").count(),
            1,
            "글 안의 끝 표시는 없다"
        );
    }

    /// 힌트가 보안 경계를 지나는 길: 열기는 `open_link`(확인 막대, 허용 목록, 사용자 정보 `@` 막기)로만, 붙여넣기는 `paste_to`(괄호 붙여넣기 틀,
    /// 위험 바이트 바꾸기, 붙여넣기 보호)로만 간다. PTY나 `desktop::open_link`에 곧바로 쓰는 길이 생기면 이 시험이 막는다
    #[test]
    fn choose_hint_uses_only_the_guarded_open_and_paste_paths() {
        let src = include_str!("app.rs");
        let start = src.find("    fn choose_hint(").expect("choose_hint");
        let end = start
            + src[start..]
                .find("\n    /// 고른 칸에 찾기 막대를 연다")
                .expect("다음 함수");
        let body = &src[start..end];
        assert!(body.contains("self.open_link(&text, explicit, h.pane)"));
        assert!(body.contains("self.paste_to(h.pane, text)"));
        for banned in [
            "desktop::open_link",
            "write_input",
            "write_active",
            "session.write",
            "encode_paste",
            "classify",
        ] {
            assert!(!body.contains(banned), "choose_hint가 {banned}를 직접 쓴다");
        }
    }
}
