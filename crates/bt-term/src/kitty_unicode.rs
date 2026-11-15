//! kitty 그래픽 유니코드 자리표시(`U=1`, 가상 배치)와 상대 배치(`P Q H V`, #553).
//! Ghostty `src/terminal/kitty/graphics_unicode.zig`·`graphics_storage.zig`(resolveParent·resolveChain·removeOrphans)·
//! `src/terminal/render.zig`(자리표시 훑기·상대 배치 자리 정하기)의 규칙을 옮겼다.
//!
//! 프로그램이 가상 배치를 만들고 화면에 자리표시 글자(U+10EEEE)를 찍으면, 글자 하나하나가 "그림 번호(글자색)·자리 번호(밑줄색)·
//! 그림의 몇째 줄 몇째 칸(결합 문자)"을 말한다. 이웃한 같은 그림의 칸을 한 줄 조각으로 묶어 그 칸들에 그림의 해당 부분을 그린다.
//! 칸 크기에 달린 맞춤은 그리는 쪽이 `bt_core::kitty_geom::Geom::resolve`로 푼다(`Geom.piece`)

use std::collections::{HashMap, HashSet};

use alacritty_terminal::grid::Dimensions;
use alacritty_terminal::index::{Column, Line};
use alacritty_terminal::term::Term;
use alacritty_terminal::vte::ansi::Color;
use bt_core::kitty_geom::{Geom, Piece};

use crate::graphics::{Graphics, Loc, Placement, PlacementKey, Relative};
use crate::screen::ImageView;

/// 자리표시 글자(Ghostty `placeholder`)
pub const PLACEHOLDER: char = '\u{10EEEE}';

/// 부모 사슬의 최대 연결 수(Ghostty `parent_chain_limit`, 명세가 정한 최소값과 kitty의 실제 한도)
pub const PARENT_CHAIN_LIMIT: usize = 8;

/// 줄·칸·그림 번호 높은 바이트를 나타내는 결합 문자. 색인이 값이다(Ghostty `diacritics`, kitty `rowcolumn-diacritics.txt`).
/// 이진 탐색을 쓰므로 오름차순이어야 한다
const DIACRITICS: [u32; 297] = [
    0x0305, 0x030D, 0x030E, 0x0310, 0x0312, 0x033D, 0x033E, 0x033F, 0x0346, 0x034A, 0x034B, 0x034C, 0x0350, 0x0351,
    0x0352, 0x0357, 0x035B, 0x0363, 0x0364, 0x0365, 0x0366, 0x0367, 0x0368, 0x0369, 0x036A, 0x036B, 0x036C, 0x036D,
    0x036E, 0x036F, 0x0483, 0x0484, 0x0485, 0x0486, 0x0487, 0x0592, 0x0593, 0x0594, 0x0595, 0x0597, 0x0598, 0x0599,
    0x059C, 0x059D, 0x059E, 0x059F, 0x05A0, 0x05A1, 0x05A8, 0x05A9, 0x05AB, 0x05AC, 0x05AF, 0x05C4, 0x0610, 0x0611,
    0x0612, 0x0613, 0x0614, 0x0615, 0x0616, 0x0617, 0x0657, 0x0658, 0x0659, 0x065A, 0x065B, 0x065D, 0x065E, 0x06D6,
    0x06D7, 0x06D8, 0x06D9, 0x06DA, 0x06DB, 0x06DC, 0x06DF, 0x06E0, 0x06E1, 0x06E2, 0x06E4, 0x06E7, 0x06E8, 0x06EB,
    0x06EC, 0x0730, 0x0732, 0x0733, 0x0735, 0x0736, 0x073A, 0x073D, 0x073F, 0x0740, 0x0741, 0x0743, 0x0745, 0x0747,
    0x0749, 0x074A, 0x07EB, 0x07EC, 0x07ED, 0x07EE, 0x07EF, 0x07F0, 0x07F1, 0x07F3, 0x0816, 0x0817, 0x0818, 0x0819,
    0x081B, 0x081C, 0x081D, 0x081E, 0x081F, 0x0820, 0x0821, 0x0822, 0x0823, 0x0825, 0x0826, 0x0827, 0x0829, 0x082A,
    0x082B, 0x082C, 0x082D, 0x0951, 0x0953, 0x0954, 0x0F82, 0x0F83, 0x0F86, 0x0F87, 0x135D, 0x135E, 0x135F, 0x17DD,
    0x193A, 0x1A17, 0x1A75, 0x1A76, 0x1A77, 0x1A78, 0x1A79, 0x1A7A, 0x1A7B, 0x1A7C, 0x1B6B, 0x1B6D, 0x1B6E, 0x1B6F,
    0x1B70, 0x1B71, 0x1B72, 0x1B73, 0x1CD0, 0x1CD1, 0x1CD2, 0x1CDA, 0x1CDB, 0x1CE0, 0x1DC0, 0x1DC1, 0x1DC3, 0x1DC4,
    0x1DC5, 0x1DC6, 0x1DC7, 0x1DC8, 0x1DC9, 0x1DCB, 0x1DCC, 0x1DD1, 0x1DD2, 0x1DD3, 0x1DD4, 0x1DD5, 0x1DD6, 0x1DD7,
    0x1DD8, 0x1DD9, 0x1DDA, 0x1DDB, 0x1DDC, 0x1DDD, 0x1DDE, 0x1DDF, 0x1DE0, 0x1DE1, 0x1DE2, 0x1DE3, 0x1DE4, 0x1DE5,
    0x1DE6, 0x1DFE, 0x20D0, 0x20D1, 0x20D4, 0x20D5, 0x20D6, 0x20D7, 0x20DB, 0x20DC, 0x20E1, 0x20E7, 0x20E9, 0x20F0,
    0x2CEF, 0x2CF0, 0x2CF1, 0x2DE0, 0x2DE1, 0x2DE2, 0x2DE3, 0x2DE4, 0x2DE5, 0x2DE6, 0x2DE7, 0x2DE8, 0x2DE9, 0x2DEA,
    0x2DEB, 0x2DEC, 0x2DED, 0x2DEE, 0x2DEF, 0x2DF0, 0x2DF1, 0x2DF2, 0x2DF3, 0x2DF4, 0x2DF5, 0x2DF6, 0x2DF7, 0x2DF8,
    0x2DF9, 0x2DFA, 0x2DFB, 0x2DFC, 0x2DFD, 0x2DFE, 0x2DFF, 0xA66F, 0xA67C, 0xA67D, 0xA6F0, 0xA6F1, 0xA8E0, 0xA8E1,
    0xA8E2, 0xA8E3, 0xA8E4, 0xA8E5, 0xA8E6, 0xA8E7, 0xA8E8, 0xA8E9, 0xA8EA, 0xA8EB, 0xA8EC, 0xA8ED, 0xA8EE, 0xA8EF,
    0xA8F0, 0xA8F1, 0xAAB0, 0xAAB2, 0xAAB3, 0xAAB7, 0xAAB8, 0xAABE, 0xAABF, 0xAAC1, 0xFE20, 0xFE21, 0xFE22, 0xFE23,
    0xFE24, 0xFE25, 0xFE26, 0x10A0F, 0x10A38, 0x1D185, 0x1D186, 0x1D187, 0x1D188, 0x1D189, 0x1D1AA, 0x1D1AB, 0x1D1AC,
    0x1D1AD, 0x1D242, 0x1D243, 0x1D244,
];

/// 결합 문자의 색인(0부터). 표에 없으면 없다
pub fn diacritic_index(c: char) -> Option<u32> {
    DIACRITICS.binary_search(&(c as u32)).ok().map(|i| i as u32)
}

/// 칸 색 하나를 그림 번호·자리 번호로 읽을 때의 모양: 기본색, 팔레트 번호, 24비트 RGB
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CellColor {
    None,
    Palette(u8),
    Rgb(u8, u8, u8),
}

impl CellColor {
    /// 색을 번호로(Ghostty `colorToId`): 팔레트 번호는 그대로, RGB는 `r<<16 | g<<8 | b`, 기본색은 0
    pub fn id(self) -> u32 {
        match self {
            CellColor::None => 0,
            CellColor::Palette(i) => u32::from(i),
            CellColor::Rgb(r, g, b) => u32::from(r) << 16 | u32::from(g) << 8 | u32::from(b),
        }
    }

    /// VT 엔진의 색에서. 이름 있는 색 0~15(SGR 30~37·90~97)는 팔레트 번호와 같고, 기본 글자색 등은 기본색이다
    fn of(c: Color) -> Self {
        match c {
            Color::Indexed(i) => CellColor::Palette(i),
            Color::Spec(rgb) => CellColor::Rgb(rgb.r, rgb.g, rgb.b),
            Color::Named(n) => {
                let i = n as usize;
                if i < 16 {
                    CellColor::Palette(i as u8)
                } else {
                    CellColor::None
                }
            }
        }
    }
}

/// 자리표시 칸 하나가 말하는 것. 결합 문자가 빠지면 앞 칸에서 이어받아야 해서 "덜 찬" 값이다(Ghostty `IncompletePlacement`)
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Incomplete {
    /// 그림 번호 아래 24비트(글자색). 늘 있다
    pub image_low: u32,
    /// 그림 번호 위 8비트(셋째 결합 문자). 없을 수 있다
    pub image_high: Option<u8>,
    /// 자리 번호(밑줄색 아래 24비트). 0이면 없다
    pub placement_id: Option<u32>,
    /// 그림 격자 안의 줄·칸(0부터, 첫째·둘째 결합 문자). 없으면 앞 칸에서 이어받는다
    pub row: Option<u32>,
    pub col: Option<u32>,
    /// 지금까지 이어진 칸 수
    pub width: u32,
}

impl Incomplete {
    /// 칸의 글자색·밑줄색과 기본 글자 뒤에 붙은 결합 문자들(없으면 빈 조각)에서 읽는다.
    /// 표에 없는 결합 문자는 없는 것으로 본다(kitty가 그렇게 보인다)
    pub fn new(fg: CellColor, underline: CellColor, cps: &[char]) -> Self {
        let mut r = Incomplete {
            image_low: fg.id() & 0x00ff_ffff,
            image_high: None,
            placement_id: Some(underline.id() & 0x00ff_ffff).filter(|&id| id != 0),
            row: None,
            col: None,
            width: 1,
        };
        if let Some(&c) = cps.first() {
            r.row = diacritic_index(c);
            if let Some(&c) = cps.get(1) {
                r.col = diacritic_index(c);
                if let Some(&c) = cps.get(2) {
                    // 셋째 결합 문자: 그림 번호의 높은 8비트. 255를 넘는 값은 없는 것으로 본다. 그 뒤 결합 문자는 무시한다
                    r.image_high = diacritic_index(c).and_then(|h| u8::try_from(h).ok());
                }
            }
        }
        r
    }

    /// 이 칸을 한 줄 조각의 첫 칸으로 쓴다. 줄·칸이 빠졌어도 이을 앞 칸이 없으니 0이다
    pub fn start(mut self) -> Self {
        self.row.get_or_insert(0);
        self.col.get_or_insert(0);
        self
    }

    /// 오른쪽 칸 `other`가 이 조각에 이어지면 넣고 참을 돌려준다(kitty의 규칙을 옮긴 Ghostty `canAppend`).
    /// 이어지지 않으면 아무것도 바꾸지 않고 거짓
    pub fn append(&mut self, other: &Incomplete) -> bool {
        let next_col = self.col.map(|c| c.saturating_add(self.width));
        let ok = self.image_low == other.image_low
            && self.placement_id == other.placement_id
            && (other.row.is_none() || other.row == self.row)
            && (other.col.is_none() || other.col == next_col)
            && (other.image_high.is_none() || other.image_high == self.image_high);
        if ok {
            self.width += 1;
        }
        ok
    }

    /// 그림 번호 전체: 아래 24비트(글자색)와 위 8비트(셋째 결합 문자)
    pub fn image_id(&self) -> u32 {
        self.image_low | u32::from(self.image_high.unwrap_or(0)) << 24
    }
}

/// 한 줄에서 이어진 자리표시 칸 묶음 하나(Ghostty `RenderState.Placeholder`)
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Run {
    /// 첫 칸과 칸 수
    pub x: usize,
    pub width: u32,
    pub image_id: u32,
    /// 0이면 그림의 가상 배치 가운데 정해진 것
    pub placement_id: u32,
    /// 첫 칸이 보여 줄 그림 격자 안의 줄·칸(0부터). 뒤 칸은 칸이 하나씩 늘어난다
    pub row: u32,
    pub col: u32,
}

/// 한 줄의 자리표시 칸들(왼쪽부터)을 묶음으로 묶는다. 인수는 (칸, 글자색, 밑줄색, 기본 글자 뒤 결합 문자들).
/// 자리표시가 아닌 칸이 끼어 칸 번호가 이어지지 않으면 묶음이 끊긴다(Ghostty `PlaceholderDecoder`)
pub fn scan_row<'a>(cells: impl IntoIterator<Item = (usize, CellColor, CellColor, &'a [char])>) -> Vec<Run> {
    let mut out = Vec::new();
    let mut cur: Option<(Incomplete, usize, usize)> = None; // (묶음, 시작 칸, 끝 칸(다음 칸))
    let flush = |cur: &mut Option<(Incomplete, usize, usize)>, out: &mut Vec<Run>| {
        if let Some((run, start, _)) = cur.take() {
            out.push(Run {
                x: start,
                width: run.width,
                image_id: run.image_id(),
                placement_id: run.placement_id.unwrap_or(0),
                row: run.row.unwrap_or(0),
                col: run.col.unwrap_or(0),
            });
        }
    };
    for (x, fg, ul, cps) in cells {
        let next = Incomplete::new(fg, ul, cps);
        if let Some((prev, _, end)) = cur.as_mut() {
            if x == *end && prev.append(&next) {
                *end += 1;
                continue;
            }
            flush(&mut cur, &mut out);
        }
        cur = Some((next.start(), x, x + 1));
    }
    flush(&mut cur, &mut out);
    out
}

// ---- 자리 사이의 관계: 가상 배치 고르기, 부모 자리, 부모 사슬 ----

/// 자리표시가 가리키는 가상 배치(Ghostty `placeholderTarget`): 자리 번호가 있으면 그 배치(가상일 때만), 0이면 그림의 가상 배치 중
/// 정해진 차례로 하나. 같은 화면(`alt`)의 것만
pub fn placeholder_target(ps: &[Placement], image: u32, pid: u32, alt: bool) -> Option<&Placement> {
    let mut virt = ps
        .iter()
        .filter(|p| p.alt == alt && p.image == image && matches!(p.loc, Loc::Virtual));
    if pid > 0 {
        return virt.find(|p| p.pid == pid);
    }
    virt.fold(None::<&Placement>, |best, p| match best {
        Some(b) if !p.key().preferred_over(&b.key()) => Some(b),
        _ => Some(p),
    })
}

/// 부모 자리를 정하지 못한 까닭(Ghostty `ParentError`). `code`가 답에 쓰는 오류 부호다
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ParentError {
    /// 부모 그림(`P`)이 없다
    ImageNotFound,
    /// 부모 그림은 있는데 그 자리(`Q`, 없으면 아무 자리)가 없다
    PlacementNotFound,
    /// 자기 자신을 부모로 삼았다
    SelfParent,
    /// 부모 사슬이 지금 놓는 자리로 되돌아온다
    Cycle,
    /// 부모 사슬이 `PARENT_CHAIN_LIMIT`을 넘는다
    TooDeep,
    /// 사슬 중간의 조상이 없다(`reap_orphans`가 지켜 주지만 믿지 않고 본다)
    AncestorNotFound,
}

impl ParentError {
    pub fn code(self) -> &'static str {
        match self {
            ParentError::ImageNotFound | ParentError::PlacementNotFound => "ENOPARENT",
            ParentError::SelfParent => "EINVAL",
            ParentError::Cycle => "ECYCLE",
            ParentError::TooDeep => "ETOODEEP",
            ParentError::AncestorNotFound => "ENOENT",
        }
    }
}

/// 상대 배치의 부모(`P`·`Q`)를 정하고 사슬을 검사한다(Ghostty `resolveParent`).
/// `child`는 지금 놓는 자리의 열쇠(자리 번호가 있을 때만: 없으면 아무도 가리킬 수 없다), `image_exists`는 부모 그림 자료가 있는지.
/// 부모 자리를 `Q`로 정하지 않으면 부모 그림의 자리 가운데 정해진 차례(`preferred_over`)로 하나 고른다
pub fn resolve_parent(
    ps: &[Placement],
    image_exists: bool,
    alt: bool,
    child: Option<PlacementKey>,
    parent_image: u32,
    parent_pid: u32,
) -> Result<PlacementKey, ParentError> {
    if !image_exists {
        return Err(ParentError::ImageNotFound);
    }
    let find = |k: &PlacementKey| ps.iter().find(|p| p.key() == *k);
    let parent: PlacementKey = if parent_pid > 0 {
        let key = PlacementKey {
            image: parent_image,
            pid: parent_pid,
            internal: 0,
            alt,
        };
        find(&key).ok_or(ParentError::PlacementNotFound)?;
        key
    } else {
        ps.iter()
            .filter(|p| p.alt == alt && p.image == parent_image)
            .map(Placement::key)
            .fold(None::<PlacementKey>, |best, k| match best {
                Some(b) if !k.preferred_over(&b) => Some(b),
                _ => Some(k),
            })
            .ok_or(ParentError::PlacementNotFound)?
    };
    // 자기 자신은 사슬을 따라가기 전에 거른다(순환이 아니라 EINVAL)
    if child == Some(parent) {
        return Err(ParentError::SelfParent);
    }
    // 앞으로 만들 연결이 이미 하나이므로 깊이는 1에서 센다
    let mut depth = 1;
    let mut key = parent;
    loop {
        if child == Some(key) {
            return Err(ParentError::Cycle);
        }
        let p = find(&key).ok_or(ParentError::AncestorNotFound)?;
        match p.loc {
            Loc::Relative(rel) => {
                if depth >= PARENT_CHAIN_LIMIT {
                    return Err(ParentError::TooDeep);
                }
                depth += 1;
                key = rel.parent;
            }
            Loc::Pin | Loc::Virtual => break,
        }
    }
    Ok(parent)
}

/// 부모가 없어진 상대 배치를 지운다. 한 번 지우고 나면 그 자식이 또 고아가 되므로 더 없을 때까지 거듭한다.
/// 지운 자리의 그림 번호들을 돌려준다
pub fn reap_orphans(ps: &mut Vec<Placement>) -> Vec<u32> {
    let mut gone = Vec::new();
    while ps.iter().any(|p| matches!(p.loc, Loc::Relative(_))) {
        let keys: HashSet<PlacementKey> = ps.iter().map(Placement::key).collect();
        let before = ps.len();
        ps.retain(|p| match p.loc {
            Loc::Relative(rel) if !keys.contains(&rel.parent) => {
                gone.push(p.image);
                false
            }
            _ => true,
        });
        if ps.len() == before {
            break;
        }
    }
    gone
}

/// 부모 사슬을 따라 올라간 끝(사슬의 뿌리)과 오프셋 합(Ghostty `resolveChain`). 사슬이 끊겼거나 너무 깊으면 없다
fn resolve_chain<'a>(
    index: &HashMap<PlacementKey, &'a Placement>,
    rel: &Relative,
) -> Option<(&'a Placement, i64, i64)> {
    let (mut h, mut v) = (i64::from(rel.h), i64::from(rel.v));
    let mut key = rel.parent;
    let mut depth = 1;
    loop {
        let p = *index.get(&key)?;
        match p.loc {
            Loc::Relative(r) => {
                if depth >= PARENT_CHAIN_LIMIT {
                    return None;
                }
                depth += 1;
                h = h.saturating_add(i64::from(r.h));
                v = v.saturating_add(i64::from(r.v));
                key = r.parent;
            }
            Loc::Pin | Loc::Virtual => return Some((p, h, v)),
        }
    }
}

// ---- 보이는 화면에서 그릴 조각 뽑기 ----

/// 보이는 화면의 자리표시 묶음 하나(줄은 보이는 화면 맨 위가 0)와 그것이 가리키는 가상 배치
struct Seen {
    row: i64,
    run: Run,
    target: Option<PlacementKey>,
}

/// 보이는 화면의 자리표시 칸을 줄마다 훑는다. 칸의 글자색·밑줄색은 테마로 풀기 전의 값(팔레트 번호·RGB)이다
fn scan_screen<T>(term: &Term<T>, ps: &[Placement], alt: bool) -> Vec<Seen> {
    let rows = term.screen_lines() as i32;
    let cols = term.columns();
    let offset = term.grid().display_offset() as i32;
    let mut out = Vec::new();
    for r in 0..rows {
        let row = &term.grid()[Line(r - offset)];
        let cells = (0..cols).filter_map(|c| {
            let cell = &row[Column(c)];
            (cell.c == PLACEHOLDER).then(|| {
                (
                    c,
                    CellColor::of(cell.fg),
                    cell.underline_color().map_or(CellColor::None, CellColor::of),
                    cell.zerowidth().unwrap_or(&[]),
                )
            })
        });
        for run in scan_row(cells) {
            let target = placeholder_target(ps, run.image_id, run.placement_id, alt).map(Placement::key);
            out.push(Seen {
                row: i64::from(r),
                run,
                target,
            });
        }
    }
    out
}

/// 그릴 양의 상한을 나눠 쓴다(보안 검토): 보이는 칸 넓이의 합이 `budget`을 넘지 않게 `used`를 올린다
pub struct Budget {
    pub used: i64,
    pub budget: i64,
}

impl Budget {
    /// `area`칸을 더 그려도 되면 쓰고 참
    pub fn take(&mut self, area: i64) -> bool {
        if self.used + area > self.budget {
            return false;
        }
        self.used += area;
        true
    }
}

/// 커서에 놓인 자리 말고 보이는 화면에 그릴 그림들: 가상 배치를 보여 주는 자리표시 칸 조각(줄마다 하나, z=-1)과
/// 상대 배치(부모 자리에서 오프셋만큼). 그리는 양은 `budget`에서 쓴다. `top`은 보이는 화면 맨 위의 절대 줄이다
pub fn extra_images<T>(term: &Term<T>, g: &Graphics, alt: bool, top: i64, budget: &mut Budget) -> Vec<ImageView> {
    let ps = &g.placements;
    let has_virtual = ps.iter().any(|p| p.alt == alt && matches!(p.loc, Loc::Virtual));
    let has_relative = ps.iter().any(|p| p.alt == alt && matches!(p.loc, Loc::Relative(_)));
    if !has_virtual && !has_relative {
        return Vec::new();
    }
    let rows = term.screen_lines() as i64;
    let cols = term.columns() as i64;
    let seen = if has_virtual {
        scan_screen(term, ps, alt)
    } else {
        Vec::new()
    };
    let mut out = Vec::new();

    // 상대 배치: 뿌리가 커서에 놓인 자리면 그 칸, 가상 배치면 그 자리표시 칸들의 왼쪽 위(가장 작은 칸·줄)에서 잰다.
    // 새 것부터 그릴 양을 쓰고 놓인 차례로 돌려놓는다
    if has_relative {
        let index: HashMap<PlacementKey, &Placement> = ps.iter().map(|p| (p.key(), p)).collect();
        let mut origin: HashMap<PlacementKey, (i64, i64)> = HashMap::new();
        for s in &seen {
            if let Some(t) = s.target {
                let o = origin.entry(t).or_insert((s.run.x as i64, s.row));
                o.0 = o.0.min(s.run.x as i64);
                o.1 = o.1.min(s.row);
            }
        }
        let mut kids = Vec::new();
        for p in ps.iter().rev().filter(|p| p.alt == alt) {
            let Loc::Relative(rel) = p.loc else { continue };
            let Some((root, h, v)) = resolve_chain(&index, &rel) else {
                continue;
            };
            let (ox, oy) = match root.loc {
                Loc::Pin => (root.col as i64, root.line - top),
                Loc::Virtual => match origin.get(&root.key()) {
                    Some(&o) => o,
                    None => continue,
                },
                Loc::Relative(_) => continue,
            };
            let (x, y) = (ox.saturating_add(h), oy.saturating_add(v));
            let (pc, pr) = (i64::from(p.cols.max(1)), i64::from(p.rows.max(1)));
            // 한 칸이라도 보여야 한다(보이는 화면 밖으로 일부 나가는 것은 그리는 쪽이 자른다)
            if y + pr - 1 < 0 || y >= rows || x + pc - 1 < 0 || x >= cols {
                continue;
            }
            let (Ok(row), Ok(col)) = (i32::try_from(y), i32::try_from(x)) else {
                continue;
            };
            let Some(img) = g.image(p.image) else { continue };
            let visible_rows = (y + pr).min(rows) - y.max(0);
            let visible_cols = (x + pc).min(cols) - x.max(0);
            if !budget.take(visible_rows.max(0) * visible_cols.max(0)) {
                break;
            }
            kids.push(ImageView {
                rgba: img.rgba.clone(),
                width: img.width,
                height: img.height,
                row,
                col,
                cols: p.cols,
                rows: p.rows,
                geom: p.geom,
                z: p.z,
            });
        }
        kids.reverse();
        out.extend(kids);
    }

    // 자리표시 조각: 글자 아래 층(z=-1)
    for s in &seen {
        let Some(target) = s.target else { continue };
        let Some(vp) = ps.iter().find(|p| p.key() == target) else {
            continue;
        };
        let Some(img) = g.image(s.run.image_id) else { continue };
        if !budget.take(i64::from(s.run.width)) {
            break;
        }
        out.push(ImageView {
            rgba: img.rgba.clone(),
            width: img.width,
            height: img.height,
            row: s.row as i32,
            col: s.run.x as i32,
            cols: s.run.width,
            rows: 1,
            geom: Geom {
                img_w: img.width,
                img_h: img.height,
                piece: Some(Piece {
                    grid_cols: vp.geom.want_cols,
                    grid_rows: vp.geom.want_rows,
                    col: s.run.col,
                    row: s.run.row,
                    width: s.run.width,
                }),
                ..Geom::default()
            },
            z: -1,
        });
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use alacritty_terminal::event::EventListener;
    use alacritty_terminal::term::Config;
    use alacritty_terminal::vte::ansi::{Processor, StdSyncHandler};

    const A: CellColor = CellColor::None;

    fn p(n: u8) -> CellColor {
        CellColor::Palette(n)
    }

    // ---- 결합 문자 표 ----

    /// Ghostty "unicode diacritic sorted"·"unicode diacritic": 오름차순이고 개수는 kitty의 297개다
    #[test]
    fn diacritic_table_is_sorted_and_has_kittys_297_entries() {
        assert_eq!(DIACRITICS.len(), 297);
        assert!(DIACRITICS.windows(2).all(|w| w[0] < w[1]));
        assert_eq!((DIACRITICS[0], DIACRITICS[296]), (0x0305, 0x1D244));
        // kitty 동작에서 따온 값(Ghostty 시험과 같다)
        assert_eq!(diacritic_index('\u{483}'), Some(30));
        assert_eq!(diacritic_index('\u{1d242}'), Some(294));
    }

    /// Ghostty "unicode diacritic lookup matches table": 표의 값은 자기 색인으로, 그 밖은 없음으로
    #[test]
    fn diacritic_lookup_matches_the_table_and_nothing_else() {
        for (i, &cp) in DIACRITICS.iter().enumerate() {
            assert_eq!(diacritic_index(char::from_u32(cp).unwrap()), Some(i as u32));
        }
        let set: HashSet<u32> = DIACRITICS.iter().copied().collect();
        for cp in 0..0x1E000u32 {
            if let Some(c) = char::from_u32(cp).filter(|_| !set.contains(&cp)) {
                assert_eq!(diacritic_index(c), None, "{cp:X}");
            }
        }
    }

    // ---- 칸 하나와 묶음 ----

    #[test]
    fn colors_become_ids_from_palette_or_rgb() {
        assert_eq!(CellColor::None.id(), 0);
        assert_eq!(CellColor::Palette(42).id(), 42);
        assert_eq!(CellColor::Rgb(1, 2, 3).id(), 0x010203);
    }

    #[test]
    fn a_cell_reads_row_col_and_high_byte_from_its_diacritics() {
        let c = |cps: &[char]| Incomplete::new(p(7), p(9), cps);
        let r = c(&[]);
        assert_eq!(
            (r.image_low, r.placement_id, r.row, r.col, r.image_high),
            (7, Some(9), None, None, None)
        );
        let r = c(&['\u{030D}', '\u{030E}', '\u{0310}']);
        assert_eq!((r.row, r.col, r.image_high), (Some(1), Some(2), Some(3)));
        assert_eq!(r.image_id(), 7 | 3 << 24);
        // 표에 없는 결합 문자는 없는 것으로 본다. 그 뒤 것은 이어서 읽는다
        let r = c(&['\u{0301}', '\u{030E}']);
        assert_eq!((r.row, r.col), (None, Some(2)));
        // 높은 바이트가 255를 넘는 색인(표의 296)이면 없는 것으로 본다. 넷째 이후 결합 문자는 무시한다
        let r = c(&['\u{0305}', '\u{0305}', '\u{1D244}', '\u{030D}']);
        assert_eq!(r.image_high, None);
        // 자리 번호 0(밑줄색 없음)은 없다
        assert_eq!(Incomplete::new(CellColor::Rgb(1, 2, 3), A, &[]).placement_id, None);
    }

    #[test]
    fn append_follows_kittys_continuation_rules() {
        let mk = |row, col| Incomplete {
            row,
            col,
            ..Incomplete::new(p(1), A, &[])
        };
        let mut run = mk(Some(0), Some(0)).start();
        // 칸·줄이 빠진 칸은 앞 칸에서 이어진다
        assert!(run.append(&mk(None, None)));
        // 다음 칸 번호가 정확히 맞으면 이어진다
        assert!(run.append(&mk(Some(0), Some(2))));
        assert_eq!(run.width, 3);
        // 칸 번호가 건너뛰거나 줄이 다르면 끊긴다
        assert!(!run.append(&mk(Some(0), Some(9))));
        assert!(!run.append(&mk(Some(1), Some(3))));
        // 다른 그림이나 다른 자리 번호나 다른 높은 바이트도 끊긴다
        assert!(!run.append(&Incomplete::new(p(2), A, &[])));
        assert!(!run.append(&Incomplete::new(p(1), p(5), &[])));
        assert!(!run.append(&Incomplete {
            image_high: Some(1),
            ..mk(None, None)
        }));
        assert_eq!(run.width, 3, "이어지지 않으면 바꾸지 않는다");
        // 시작 칸의 빠진 줄·칸은 0이다
        let s = mk(None, None).start();
        assert_eq!((s.row, s.col), (Some(0), Some(0)));
    }

    // ---- 줄 훑기: Ghostty render.zig "placeholders: runs" ----

    struct L;
    impl EventListener for L {}

    struct D(usize, usize);
    impl Dimensions for D {
        fn total_lines(&self) -> usize {
            self.0
        }
        fn screen_lines(&self) -> usize {
            self.0
        }
        fn columns(&self) -> usize {
            self.1
        }
    }

    fn term_with(rows: usize, cols: usize, input: &str) -> Term<L> {
        let mut t = Term::new(Config::default(), &D(rows, cols), L);
        Processor::<StdSyncHandler>::new().advance(&mut t, input.as_bytes());
        t
    }

    /// 화면의 모든 묶음: (줄, 묶음)
    fn runs_of(input: &str) -> Vec<(i64, Run)> {
        let t = term_with(5, 10, input);
        scan_screen(&t, &[], false)
            .into_iter()
            .map(|s| (s.row, s.run))
            .collect()
    }

    fn run(y: i64, x: usize, width: u32) -> (i64, Run) {
        (
            y,
            Run {
                x,
                width,
                image_id: 0,
                placement_id: 0,
                row: 0,
                col: 0,
            },
        )
    }

    #[test]
    fn placeholder_runs_match_ghostty() {
        let (a, b, c) = (
            "\u{10EEEE}\u{0305}\u{0305}",
            "\u{10EEEE}\u{0305}\u{030D}",
            "\u{10EEEE}\u{0305}\u{030E}",
        );
        // 자리표시가 없다
        assert_eq!(runs_of("hello\r\nworld\r\n1\r\n2"), vec![]);
        // 한 칸
        assert_eq!(runs_of(a), vec![run(0, 0, 1)]);
        // 칸 번호가 건너뛰어 둘로 나뉜다
        let mut want = run(0, 1, 1);
        want.1.col = 2;
        assert_eq!(runs_of(&format!("{a}{c}")), vec![run(0, 0, 1), want]);
        // 줄·칸을 다 적었고 이어진다
        assert_eq!(runs_of(&format!("{a}{b}{c}")), vec![run(0, 0, 3)]);
        // 칸 번호가 없는 칸은 이어진다
        assert_eq!(
            runs_of(&format!("{a}\u{10EEEE}\u{0305}\u{10EEEE}\u{0305}")),
            vec![run(0, 0, 3)]
        );
        // 결합 문자도 스타일도 없는 칸
        assert_eq!(runs_of("\u{10EEEE}\u{10EEEE}\u{10EEEE}"), vec![run(0, 0, 3)]);
        // 스타일이 있는 칸(그림 42)
        let mut r1 = run(0, 1, 3);
        (r1.1.image_id, r1.1.col) = (42, 1);
        let mut r2 = run(0, 5, 2);
        r2.1.image_id = 42;
        assert_eq!(
            runs_of("\x1b[38;5;42mA\u{10EEEE}\u{0305}\u{030D}\u{10EEEE}\u{10EEEE}B\u{10EEEE}\u{10EEEE}"),
            vec![r1, r2]
        );
        // 자리표시가 아닌 칸에서 끊긴다
        assert_eq!(runs_of(&format!("{a}{b}ABC")), vec![run(0, 0, 2)]);
        // 줄 가운데에서 시작
        assert_eq!(runs_of(&format!("ABC{a}{b}")), vec![run(0, 3, 2)]);
        // 팔레트 글자색 = 그림 번호
        let mut r = run(0, 0, 1);
        r.1.image_id = 42;
        assert_eq!(runs_of(&format!("\x1b[38;5;42m{a}")), vec![r]);
        // 셋째 결합 문자 = 그림 번호 높은 8비트. 그것이 없는 칸은 이어지고 다른 값이면 끊긴다
        let mut r1 = run(0, 0, 2);
        r1.1.image_id = 42 | 2 << 24;
        let mut r2 = run(0, 2, 1);
        (r2.1.image_id, r2.1.col) = (42, 2);
        assert_eq!(
            runs_of(
                "\x1b[38;5;42m\u{10EEEE}\u{0305}\u{0305}\u{030E}\u{10EEEE}\u{0305}\u{10EEEE}\u{0305}\u{030E}\u{0305}"
            ),
            vec![r1, r2]
        );
        // 밑줄색 = 자리 번호. 다른 자리 번호에서 끊긴다
        let mut r1 = run(0, 0, 2);
        (r1.1.image_id, r1.1.placement_id) = (42, 21);
        let mut r2 = run(0, 2, 1);
        (r2.1.image_id, r2.1.placement_id) = (42, 22);
        assert_eq!(
            runs_of("\x1b[38;5;42m\x1b[58;5;21m\u{10EEEE}\u{0305}\u{0305}\u{10EEEE}\x1b[58;5;22m\u{10EEEE}"),
            vec![r1, r2]
        );
        // 둘 다 RGB, 여러 줄
        let mut r1 = run(0, 0, 2);
        (r1.1.image_id, r1.1.placement_id) = (0x010203, 7);
        let mut r2 = run(2, 0, 3);
        (r2.1.image_id, r2.1.placement_id, r2.1.row) = (0x010203, 7, 1);
        assert_eq!(
            runs_of(
                "\x1b[38;2;1;2;3m\x1b[58;2;0;0;7m\u{10EEEE}\u{0305}\u{0305}\u{10EEEE}\r\nx\r\n\u{10EEEE}\u{030D}\u{0305}\u{10EEEE}\u{10EEEE}"
            ),
            vec![r1, r2]
        );
    }

    /// SGR 31(이름 있는 색)과 38;5;1(팔레트)은 같은 그림 번호 1이다. 기본 글자색은 0이다(Ghostty는 둘 다 팔레트 1)
    #[test]
    fn named_colors_count_as_palette_numbers() {
        let ids: Vec<u32> = ["\x1b[31m", "\x1b[38;5;1m", "\x1b[91m", "\x1b[0m"]
            .iter()
            .map(|sgr| runs_of(&format!("{sgr}\u{10EEEE}"))[0].1.image_id)
            .collect();
        assert_eq!(ids, [1, 1, 9, 0]);
    }

    /// 줄이 넘쳐 다음 줄로 이어 써도 묶음은 줄마다 따로다
    #[test]
    fn runs_never_continue_across_rows() {
        let r = runs_of(&format!("\x1b[38;5;1m{}", "\u{10EEEE}".repeat(11)));
        assert_eq!(
            r.iter().map(|(y, run)| (*y, run.x, run.width)).collect::<Vec<_>>(),
            [(0, 0, 10), (1, 0, 1)]
        );
    }

    // ---- 부모 자리와 사슬 ----

    use crate::graphics::Cursor;

    const CUR: Cursor = Cursor {
        line: 10,
        col: 3,
        cell_w: 10,
        cell_h: 10,
        alt: false,
        top: 0,
        rows: 5,
        cols: 10,
    };

    fn gfx_with_image(id: u32, w: u32, h: u32) -> Graphics {
        let mut g = Graphics::default();
        let n = (w * h * 3) as usize / 3 * 4;
        g.handle(format!("a=t,f=24,s={w},v={h},i={id};{}", "A".repeat(n)).as_bytes(), CUR);
        g
    }

    #[test]
    fn placeholder_target_prefers_external_ids_then_the_lowest_number() {
        let mut g = gfx_with_image(1, 10, 10);
        for cmd in ["a=p,i=1,U=1", "a=p,i=1,p=9,U=1", "a=p,i=1,p=4,U=1", "a=p,i=1,p=2,C=1"] {
            g.handle(cmd.as_bytes(), CUR);
        }
        let ps = &g.placements;
        // 자리 번호 0: 자리 번호가 있는 가상 배치 중 가장 낮은 것(p=2는 가상이 아니라 제외)
        assert_eq!(placeholder_target(ps, 1, 0, false).unwrap().pid, 4);
        // 자리 번호를 주면 그 배치만, 가상이 아니면 없다
        assert_eq!(placeholder_target(ps, 1, 9, false).unwrap().pid, 9);
        assert!(placeholder_target(ps, 1, 2, false).is_none());
        assert!(placeholder_target(ps, 1, 3, false).is_none());
        // 다른 화면의 가상 배치는 보지 않는다
        assert!(placeholder_target(ps, 1, 0, true).is_none());
        assert!(placeholder_target(ps, 2, 0, false).is_none());
    }

    #[test]
    fn parent_error_codes() {
        assert_eq!(ParentError::ImageNotFound.code(), "ENOPARENT");
        assert_eq!(ParentError::PlacementNotFound.code(), "ENOPARENT");
        assert_eq!(ParentError::SelfParent.code(), "EINVAL");
        assert_eq!(ParentError::Cycle.code(), "ECYCLE");
        assert_eq!(ParentError::TooDeep.code(), "ETOODEEP");
        assert_eq!(ParentError::AncestorNotFound.code(), "ENOENT");
    }

    #[test]
    fn orphans_are_reaped_in_rounds_and_their_images_reported() {
        let mut g = gfx_with_image(1, 10, 10);
        for cmd in [
            "a=p,i=1,p=1,C=1",
            "a=p,i=1,p=2,P=1,Q=1",
            "a=p,i=1,p=3,P=1,Q=2",
            "a=p,i=1,p=4,C=1",
        ] {
            g.handle(cmd.as_bytes(), CUR);
        }
        let mut ps = g.placements.clone();
        ps.retain(|p| p.pid != 1);
        let gone = reap_orphans(&mut ps);
        assert_eq!(gone, vec![1, 1], "자식과 손자");
        assert_eq!(ps.iter().map(|p| p.pid).collect::<Vec<_>>(), [4]);
        assert!(reap_orphans(&mut ps).is_empty());
    }

    // ---- 보이는 화면의 그릴 조각 ----

    fn images_of(g: &Graphics, t: &Term<L>, budget: i64) -> Vec<ImageView> {
        extra_images(t, g, false, 0, &mut Budget { used: 0, budget })
    }

    /// Ghostty image.zig 1117줄 "positions relative placements from virtual parent placeholders"를 우리 자료 꼴로: 칸 10x10, 5x5칸
    #[test]
    fn relative_placement_from_virtual_parent_matches_ghostty() {
        let mut g = gfx_with_image(1, 10, 10);
        // 칸 (1, 1)과 (3, 2)의 자리표시(팔레트 글자색 1·밑줄색 1)
        let t = term_with(
            5,
            5,
            "\x1b[38;5;1m\x1b[58;5;1m\x1b[2;2H\u{10EEEE}\u{0305}\u{0305}\x1b[3;4H\u{10EEEE}\u{0305}\u{0305}",
        );
        g.handle(b"a=p,i=1,p=1,U=1", CUR);
        g.handle(b"a=p,i=1,p=2,P=1,Q=1,H=1,V=2,c=1,r=1,z=5", CUR);
        let v = images_of(&g, &t, 100);
        // 조각 둘(z=-1)과 자식(z=5)
        assert_eq!(v.len(), 3);
        assert_eq!(v.iter().filter(|i| i.z == -1).count(), 2);
        let child = v.iter().find(|i| i.z == 5).unwrap();
        assert_eq!((child.col, child.row), (2, 3), "왼쪽 위 칸 (1, 1) + (1, 2)");
        // 자리표시 칸이 하나도 보이지 않으면 자식도 그리지 않는다
        let t = term_with(5, 5, "");
        assert!(images_of(&g, &t, 100).is_empty());
    }

    /// 가상 부모의 기준은 자리표시 칸 전체의 가장 작은 칸·가장 작은 줄이다(첫 칸이 아니다): 위 줄은 오른쪽에, 아래 줄은 왼쪽에 있어도
    #[test]
    fn virtual_parent_origin_is_the_smallest_column_and_row() {
        let mut g = gfx_with_image(1, 10, 10);
        // 줄 0의 칸 4와 줄 1의 칸 1: 기준은 (1, 0)
        let t = term_with(
            5,
            8,
            "\x1b[38;5;1m\x1b[1;5H\u{10EEEE}\u{0305}\u{0305}\x1b[2;2H\u{10EEEE}\u{0305}\u{0305}",
        );
        g.handle(b"a=p,i=1,p=1,U=1", CUR);
        g.handle(b"a=p,i=1,p=2,P=1,Q=1,H=1,V=2,z=5", CUR);
        let v = images_of(&g, &t, 100);
        let child = v.iter().find(|i| i.z == 5).unwrap();
        assert_eq!((child.col, child.row), (2, 2));
    }

    #[test]
    fn pieces_share_the_draw_budget() {
        let mut g = gfx_with_image(1, 10, 10);
        let t = term_with(
            5,
            5,
            "\x1b[38;5;1m\u{10EEEE}\u{10EEEE}\u{10EEEE}\x1b[2;1H\u{10EEEE}\u{10EEEE}",
        );
        g.handle(b"a=p,i=1,U=1,c=3,r=2", CUR);
        // 조각은 3칸과 2칸: 상한이 4면 첫 줄 조각(3칸)만, 5면 둘 다, 0이면 하나도 그리지 않는다
        assert_eq!(images_of(&g, &t, 4).len(), 1);
        assert_eq!(images_of(&g, &t, 5).len(), 2);
        assert_eq!(images_of(&g, &t, 0).len(), 0);
    }

    /// 사슬의 뿌리가 커서에 놓인 자리면 그 칸에 오프셋 합을 더한다. 줄은 보이는 화면 맨 위(`top`)를 뺀다
    #[test]
    fn relative_chain_to_a_pinned_root_sums_the_offsets() {
        let mut g = gfx_with_image(1, 10, 10);
        // 놓은 자리: 줄 10, 칸 3. 둘째는 (+2, +1), 셋째는 둘째 기준 (-1, +3)이라 뿌리 기준 (+1, +4)
        g.handle(b"a=p,i=1,p=1,C=1", CUR);
        g.handle(b"a=p,i=1,p=2,P=1,Q=1,H=2,V=1,C=1", CUR);
        g.handle(b"a=p,i=1,p=3,P=1,Q=2,H=-1,V=3,C=1", CUR);
        let t = term_with(5, 10, "");
        let at = |top: i64| -> Vec<(i32, i32)> {
            extra_images(&t, &g, false, top, &mut Budget { used: 0, budget: 100 })
                .iter()
                .map(|i| (i.col, i.row))
                .collect()
        };
        // 맨 위가 줄 11이면 뿌리는 줄 -1: 둘째 (5, 0), 셋째 (4, 3)
        assert_eq!(at(11), [(5, 0), (4, 3)]);
        // 맨 위가 줄 8이면 둘째 (5, 3), 셋째는 줄 6이라 5줄 화면 밖
        assert_eq!(at(8), [(5, 3)]);
        // 맨 위가 줄 5면 둘째도 화면 밖이다
        assert!(at(5).is_empty());
    }

    #[test]
    fn overflowing_offsets_do_not_panic() {
        let mut g = gfx_with_image(1, 10, 10);
        g.handle(b"a=p,i=1,p=1,C=1", CUR);
        g.handle(b"a=p,i=1,p=2,P=1,Q=1,H=2147483647,V=-2147483648,C=1", CUR);
        g.handle(b"a=p,i=1,p=3,P=1,Q=2,H=2147483647,V=-2147483648,C=1", CUR);
        let t = term_with(5, 10, "");
        assert!(images_of(&g, &t, 100).is_empty());
    }
}
