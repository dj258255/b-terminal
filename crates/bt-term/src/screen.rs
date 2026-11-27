//! 렌더러와 앱이 읽는 화면 값. VT 엔진(alacritty_terminal)의 타입을 밖으로 내보내지 않는다(ADR-001)

use bt_core::Rgb;

/// 셀 한 칸의 꾸밈
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct CellFlags(u32);

impl CellFlags {
    pub const BOLD: Self = Self(1);
    pub const ITALIC: Self = Self(1 << 1);
    pub const UNDERLINE: Self = Self(1 << 2);
    pub const STRIKE: Self = Self(1 << 3);
    /// 두 칸을 차지하는 글자의 앞 칸
    pub const WIDE: Self = Self(1 << 4);
    /// 넓은 글자의 뒤 칸. 그리지 않는다
    pub const SPACER: Self = Self(1 << 5);
    pub const HIDDEN: Self = Self(1 << 6);
    /// 마우스로 고른 칸
    pub const SELECTED: Self = Self(1 << 7);
    pub const UNDERCURL: Self = Self(1 << 8);
    /// 줄의 마지막 칸: 글이 길어 다음 줄로 접혔다(soft wrap). 링크 찾기가 줄을 잇는다
    pub const WRAPLINE: Self = Self(1 << 9);
    /// 찾기에 맞은 칸(#39)
    pub const MATCH: Self = Self(1 << 10);
    /// 지금 고른 맞은 곳(Enter로 옮겨 다니는 것)
    pub const MATCH_CURRENT: Self = Self(1 << 11);
    /// 윗줄(SGR 53, #277)
    pub const OVERLINE: Self = Self(1 << 12);
    /// 밑줄 모양(SGR 4:2·4:4·4:5). 곧은 밑줄 `UNDERLINE`도 함께 켜진다. 물결은 `UNDERCURL`(#276)
    pub const DOUBLE_UNDERLINE: Self = Self(1 << 13);
    pub const DOTTED_UNDERLINE: Self = Self(1 << 14);
    pub const DASHED_UNDERLINE: Self = Self(1 << 15);
    /// kitty 그래픽 자리표시 글자(U+10EEEE, #553). 글꼴에 없는 글자이고 그림이 그 칸에 그려지므로 글자로 그리지 않는다.
    /// `ch`와 결합 문자는 그대로 둔다(복사·화면 읽기는 Ghostty처럼 글자를 그대로 낸다)
    pub const PLACEHOLDER: Self = Self(1 << 16);
    /// 흐림(SGR 2). 글자색은 그대로 두고 그릴 때 `faint-opacity`를 건다(#835)
    pub const FAINT: Self = Self(1 << 17);
    /// 깜빡임(SGR 5·6, #862). 렌더러가 커서 깜빡임과 같은 박자로 글자를 숨겼다 보인다
    pub const BLINK: Self = Self(1 << 18);

    pub const fn empty() -> Self {
        Self(0)
    }

    pub const fn contains(self, other: Self) -> bool {
        self.0 & other.0 == other.0
    }

    pub fn insert(&mut self, other: Self) {
        self.0 |= other.0;
    }
}

impl std::ops::BitOr for CellFlags {
    type Output = Self;
    fn bitor(self, rhs: Self) -> Self {
        Self(self.0 | rhs.0)
    }
}

/// 화면 격자의 한 칸. 색은 이미 RGB로 풀었다(반전·흐림·굵은 글자의 밝은 색 반영)
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Cell {
    pub ch: char,
    /// 앞 글자에 붙는 결합 문자(한글 자모 조합 뒤 남은 것, 이모지 변형 선택자 등)
    pub combining: Vec<char>,
    pub fg: Rgb,
    pub bg: Rgb,
    pub flags: CellFlags,
    /// 프로그램이 OSC 8로 단 링크(`Screen::links`의 자리)
    pub link: Option<u32>,
    /// 밑줄 색(SGR 58)을 RGB로 푼 것. 없으면 글자색(#276)
    pub underline_color: Option<Rgb>,
}

impl Cell {
    /// 그릴 글자: 자리표시 칸은 빈 칸이다(Ghostty 글자 묶음 만들기가 자리표시를 공백으로 바꾼다)
    pub fn draw_ch(&self) -> char {
        if self.flags.contains(CellFlags::PLACEHOLDER) {
            ' '
        } else {
            self.ch
        }
    }

    /// 그릴 결합 문자: 자리표시 칸에 붙은 줄·칸 결합 문자(U+0305 등)는 그리지 않는다
    pub fn draw_combining(&self) -> &[char] {
        if self.flags.contains(CellFlags::PLACEHOLDER) {
            &[]
        } else {
            &self.combining
        }
    }

    pub fn blank(fg: Rgb, bg: Rgb) -> Self {
        Self {
            ch: ' ',
            combining: Vec::new(),
            fg,
            bg,
            flags: CellFlags::empty(),
            link: None,
            underline_color: None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CursorShape {
    Block,
    Beam,
    Underline,
    HollowBlock,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Cursor {
    pub col: usize,
    pub row: usize,
    pub shape: CursorShape,
    /// 커서 칸이 넓은 글자면 두 칸으로 그린다
    pub wide: bool,
    pub visible: bool,
    /// 깜빡이는 커서(앱이 0.6초마다 숨겼다 보인다)
    pub blinking: bool,
}

/// 마우스 보고 좌표 형식(DECSET 1005·1006·1015·1016, 아무것도 없으면 옛 X10 꼴)
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum MouseFormat {
    #[default]
    X10,
    Utf8,
    Sgr,
    Urxvt,
    SgrPixels,
}

/// 앱이 입력을 바꿀 때 보는 터미널 모드
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Modes {
    pub app_cursor: bool,
    pub app_keypad: bool,
    pub bracketed_paste: bool,
    pub alt_screen: bool,
    pub mouse_report: bool,
    pub mouse_drag: bool,
    pub mouse_motion: bool,
    pub sgr_mouse: bool,
    /// X10 이벤트 모드(DECSET 9): 누름만 보고하고 수식 키를 넣지 않는다(#300)
    pub mouse_x10: bool,
    /// 마우스 좌표 형식. 하나만 켜고 마지막에 켠 것이 이긴다(#300)
    pub mouse_format: MouseFormat,
    pub focus_events: bool,
    pub alternate_scroll: bool,
    /// kitty 키보드 프로토콜 단계
    pub kitty: crate::input::KittyFlags,
    /// 키 입력 모드(DECBKM 67, 1035, 1036, 1039, #301)
    pub key: crate::input::KeyModes,
    /// 화면 반전(DECSCNM, ?5)이 켜져 있다. 칸의 색은 이미 맞바뀐 채 `Screen.cells`에 있고, 커서의 기본 색을 맞바꿀 때 앱이 본다(#302)
    pub reverse_colors: bool,
    /// xterm modifyOtherKeys 상태 2(`CSI > 4 ; 2 m`)가 켜져 있다(#312)
    pub modify_other_keys: bool,
}

/// 화면에 보이는 kitty 그림 하나(#76)
#[derive(Debug, Clone)]
pub struct ImageView {
    /// 곱하지 않은 알파 RGBA
    pub rgba: std::sync::Arc<Vec<u8>>,
    pub width: u32,
    pub height: u32,
    /// 왼쪽 위 칸(보이는 화면 기준. 위나 왼쪽으로 일부 가려졌으면 음수: 위로 스크롤된 그림과 왼쪽으로 오프셋한 상대 배치)
    pub row: i32,
    pub col: i32,
    /// 놓을 때 정한 칸 수. 칸 크기가 같으면 `geom.resolve`의 값과 같다
    pub cols: u32,
    pub rows: u32,
    /// 잘라내기·칸 수 요청·칸 안 오프셋. 그리는 쪽이 자기 칸 크기로 `resolve`해 크기와 자리를 정한다
    pub geom: bt_core::kitty_geom::Geom,
    /// z 순서. 층은 `bt_core::kitty_geom::layer_of_z`
    pub z: i32,
}

/// 한 순간의 화면. 줄 우선으로 `rows * cols`칸
#[derive(Debug, Clone)]
pub struct Screen {
    pub cols: usize,
    pub rows: usize,
    pub cells: Vec<Cell>,
    pub cursor: Cursor,
    pub modes: Modes,
    /// 스크롤백을 얼마나 올려 보고 있나(0이면 맨 아래)
    pub display_offset: usize,
    /// 스크롤백 줄 수
    pub history: usize,
    /// 픽셀 스크롤(kitty `pixel_scroll`, #902): 화면을 위로 밀어 그릴 픽셀(줄 높이 미만). 0이면 줄에 맞춘 화면이다.
    /// 0보다 크면 `edge`에 맨 아래에 걸쳐 보이는 다음 줄이 있다
    pub pixel_shift: u32,
    /// `pixel_shift > 0`일 때 화면 맨 아래 줄 다음 줄(`cols`칸). 화면 맨 아래에 `pixel_shift` 픽셀만큼 걸쳐 보인다.
    /// 고르기·찾기 맞은 곳·링크 표시는 `cells`와 같이 얹혀 있다. 아니면 비어 있다
    pub edge: Vec<Cell>,
    pub background: Rgb,
    /// 지금 기본 글자색(OSC 10으로 바꿨으면 그 색, 화면 반전이면 맞바뀐 색). 정하지 않은 커서·고름 색이 여기서 나온다(#838)
    pub foreground: Rgb,
    /// 프로그램이 OSC 12로 정한 커서색(OSC 112로 되돌리면 없음). 있으면 커서 색 설정보다 앞선다(#838)
    pub cursor_color: Option<Rgb>,
    /// 화면 반전(DECSCNM) 전의 기본 바탕색(OSC 11로 바꿨으면 그 색). 제목 막대가 쓴다. Ghostty의 제목 막대 색은 반전을 따르지 않는다(#302)
    pub chrome_background: Rgb,
    /// 화면에 보이는 OSC 8 링크 주소들(칸의 `link`가 가리킨다)
    pub links: Vec<String>,
    /// 화면에 보이는 kitty 그림들(놓인 차례)
    pub images: Vec<ImageView>,
}

impl Screen {
    pub fn cell(&self, col: usize, row: usize) -> &Cell {
        &self.cells[row * self.cols + col]
    }

    /// 그릴 줄 수: 보이는 줄에 픽셀 스크롤로 걸쳐 보이는 줄(`edge`)이 있으면 하나 더한다(#902)
    pub fn draw_rows(&self) -> usize {
        self.rows + usize::from(!self.edge.is_empty())
    }

    /// 걸친 픽셀을 없애고 줄에 맞춘 화면으로 되돌린다(#902). 그림이 보이는 화면이나 설정이 꺼진 때 쓴다.
    /// 걸친 줄에 있던 커서는 보이지 않게 한다
    pub fn snap_to_lines(&mut self) {
        if self.pixel_shift == 0 && self.edge.is_empty() {
            return;
        }
        self.pixel_shift = 0;
        self.edge.clear();
        if self.cursor.row >= self.rows {
            self.cursor.visible = false;
        }
    }

    /// 한 줄을 글로 꺼낸다(뒤 칸·끝 공백 제외). 테스트와 화면 읽기(btctl read)에 쓴다
    pub fn row_text(&self, row: usize) -> String {
        let mut s = String::new();
        for col in 0..self.cols {
            let cell = self.cell(col, row);
            if cell.flags.contains(CellFlags::SPACER) {
                continue;
            }
            s.push(cell.ch);
            s.extend(cell.combining.iter());
        }
        s.trim_end().to_string()
    }

    /// b-terminal(#324): 격자가 창보다 넓을 때(DECCOLM 132열) 앞쪽 `cols`열만 남긴다. 그리는 쪽이 창 밖으로 넘치지 않게 한다.
    /// 잘린 자리에 넓은 글자의 앞 칸이 남으면 뒤 칸이 없으니 빈 칸으로 바꾼다. 커서가 잘린 열에 있으면 숨긴다.
    /// 이미 `cols` 이하면 아무것도 하지 않는다
    pub fn crop_columns(&mut self, cols: usize) {
        let cols = cols.max(1);
        if self.cols <= cols {
            return;
        }
        let old = self.cols;
        let mut cells = Vec::with_capacity(cols * self.rows);
        for row in 0..self.rows {
            let line = &self.cells[row * old..row * old + cols];
            cells.extend(line.iter().cloned());
            if let Some(last) = cells.last_mut()
                && last.flags.contains(CellFlags::WIDE)
            {
                *last = Cell::blank(last.fg, last.bg);
            }
        }
        self.cells = cells;
        self.cols = cols;
        if self.cursor.col >= cols {
            self.cursor.visible = false;
            self.cursor.col = cols - 1;
        }
        // 잘린 열에 걸친 그림은 그리는 쪽이 격자 사각형으로 자른다
    }

    /// 화면 전체를 글로 꺼낸다. 끝의 빈 줄은 뺀다
    pub fn text(&self) -> String {
        let mut lines: Vec<String> = (0..self.rows).map(|r| self.row_text(r)).collect();
        while lines.last().is_some_and(|l| l.is_empty()) {
            lines.pop();
        }
        lines.join("\n")
    }
}
