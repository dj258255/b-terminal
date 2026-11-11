//! 글리프 캐시: 글리프를 처음 볼 때 `Fonts`로 래스터해 아틀라스(회색조·색 두 장)에 채운다

use std::collections::HashMap;
use std::rc::Rc;
use std::sync::Arc;

use bt_render::font::{CellMetrics, Glyph};
use bt_render::{FontStyle, Fonts};

use crate::atlas::Atlas;

/// 아틀라스에 놓은 글리프를 가리키는 열쇠
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum Key {
    Char(char, FontStyle),
    /// 합자로 다듬은 글리프 번호(`bt_render::ligatures`, #359). 같은 번호도 꼴마다 다른 그림이다
    GlyphId(u16, FontStyle),
    /// 이모지 모양(U+FE0F), 차지하는 칸 수
    Emoji(char, u32),
    Cluster(Arc<str>),
    /// 밑줄 마스크: (모양 `Underline as u8`, 칸 폭, 칸 높이, 윗변 위치, 두께)
    Underline(u8, u32, u32, u32, u32),
    /// 직접 그리는 상자·블록 글자 마스크: (글자, 칸 폭, 칸 높이, 선 두께)
    BoxChar(char, u32, u32, u32),
    /// 글리프 없는 글자의 빈 상자: (칸 폭, 칸 높이)
    Missing(u32, u32),
    /// 칸 크기 단색 마스크(조합 중 글의 밑줄 막대): (폭, 높이)
    Solid(u32, u32),
}

/// 아틀라스 안의 글리프 하나
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Entry {
    pub x: u32,
    pub y: u32,
    pub w: u32,
    pub h: u32,
    /// 셀 왼쪽에서 비트맵 왼쪽까지, 기준선에서 비트맵 위쪽까지(`Glyph`와 같다)
    pub left: i32,
    pub top: i32,
    pub color: bool,
}

/// 새로 채운 자리. 렌더러가 GPU 텍스처로 올린다
#[derive(Debug, Clone, Copy)]
pub struct Upload {
    pub color: bool,
    pub x: u32,
    pub y: u32,
    pub w: u32,
    pub h: u32,
}

/// 색 아틀라스를 처음 잡을 때의 한 변(픽셀). 이모지를 처음 볼 때 4MiB를 한꺼번에 잡고 올리면 그 프레임이 0.9ms 늘었다(#400).
/// 모자라면 두 배씩 큰다
pub const COLOR_ATLAS_FIRST: u32 = 256;

/// 글리프를 그린 글꼴 크기(#422). 같은 글자도 크기마다 다른 그림이라 열쇠에 이 값이 들어간다.
/// 칸마다 글자 크기가 달라 한 프레임에 크기가 여럿 섞인다. 픽셀 크기(비트)와 칸 치수가 모두 같아야 같은 크기다
/// (칸 치수만 같고 픽셀 크기가 다른 글꼴이나, 글꼴 설정만 바뀌어 칸 치수가 달라진 글꼴을 같게 보지 않는다)
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SizeSig {
    px_bits: u32,
    metrics: CellMetrics,
}

impl SizeSig {
    pub fn of(fonts: &Fonts) -> Self {
        Self {
            px_bits: fonts.px().to_bits(),
            metrics: fonts.metrics(),
        }
    }
}

pub struct GlyphCache {
    pub gray: Atlas,
    pub color: Atlas,
    /// 열쇠는 (크기 번호, 글리프): 크기 번호는 `sizes`의 자리다
    map: HashMap<(u16, Key), Option<Entry>>,
    /// 캐시가 아는 글꼴 크기들(자리가 크기 번호)과 지금 글리프를 넣는 크기
    sizes: Vec<SizeSig>,
    cur: u16,
    pub uploads: Vec<Upload>,
}

impl GlyphCache {
    pub fn new(initial: u32, max: u32) -> Self {
        Self {
            gray: Atlas::new(initial, 1, max),
            color: Atlas::lazy(initial.min(COLOR_ATLAS_FIRST), 4, max),
            map: HashMap::new(),
            sizes: Vec::new(),
            cur: 0,
            uploads: Vec::new(),
        }
    }

    /// 한 프레임을 만들기 전에 그 프레임이 쓰는 글꼴 크기를 알린다. 전에 쓰던 크기 가운데 이번에 쓰지 않는 것이 있으면
    /// (글자 크기를 바꿨거나 그 크기의 칸이 사라졌다) 캐시를 비운다(아틀라스 크기는 그대로, 채움만 처음부터).
    /// 크기가 여럿 섞인 프레임은 각 크기의 글리프를 열쇠로 구분해 한 아틀라스에 함께 둔다
    pub fn begin_frame(&mut self, in_use: &[SizeSig]) {
        if self.sizes.iter().any(|s| !in_use.contains(s)) {
            self.map.clear();
            self.gray = self.gray.emptied();
            self.color = self.color.emptied();
            self.uploads.clear();
            self.sizes.clear();
        }
        for s in in_use {
            self.use_size(*s);
        }
    }

    /// 이어서 넣고 찾을 글리프의 글꼴 크기를 고른다. 처음 보는 크기면 더한다
    pub fn use_size(&mut self, sig: SizeSig) {
        self.cur = match self.sizes.iter().position(|s| *s == sig) {
            Some(i) => i as u16,
            None => {
                self.sizes.push(sig);
                (self.sizes.len() - 1) as u16
            }
        };
    }

    /// 캐시가 아는 글꼴 크기의 수
    pub fn size_count(&self) -> usize {
        self.sizes.len()
    }

    pub fn len(&self) -> usize {
        self.map.len()
    }

    pub fn is_empty(&self) -> bool {
        self.map.is_empty()
    }

    /// 열쇠의 글리프 자리. 처음이면 `make`로 래스터해 아틀라스에 채운다. 글리프가 없거나 아틀라스를 한도까지 키워도 자리가 없으면 없다
    pub fn get(&mut self, key: Key, make: impl FnOnce() -> Option<Rc<Glyph>>) -> Option<Entry> {
        let key = (self.cur, key);
        if let Some(e) = self.map.get(&key) {
            return *e;
        }
        let e = make().and_then(|g| self.put(&g));
        self.map.insert(key, e);
        e
    }

    /// 마스크(`w × h` 회색조)를 글리프처럼 아틀라스에 놓는다. 셀 왼쪽 위가 기준(`left`·`top` 0)이다.
    /// `make`가 없으면(그리지 않는 모양) 없다
    pub fn get_mask(&mut self, key: Key, w: u32, h: u32, make: impl FnOnce() -> Option<Vec<u8>>) -> Option<Entry> {
        let key = (self.cur, key);
        if let Some(e) = self.map.get(&key) {
            return *e;
        }
        let e = make().and_then(|mask| {
            let pos = self.gray.insert_growing(w, h, &mask);
            let (x, y) = pos.or_else(|| {
                log::warn!("글리프 아틀라스가 가득 찼다({}x{})", w, h);
                None
            })?;
            self.uploads.push(Upload {
                color: false,
                x,
                y,
                w,
                h,
            });
            Some(Entry {
                x,
                y,
                w,
                h,
                left: 0,
                top: 0,
                color: false,
            })
        });
        self.map.insert(key, e);
        e
    }

    fn put(&mut self, g: &Glyph) -> Option<Entry> {
        let (w, h) = (g.width as u32, g.height as u32);
        let (color, pos) = match &g.color {
            Some(rgba) if rgba.len() >= (w * h * 4) as usize => (true, self.color.insert_growing(w, h, rgba)),
            Some(_) => return None,
            None if g.bitmap.len() >= (w * h) as usize => (false, self.gray.insert_growing(w, h, &g.bitmap)),
            None => return None,
        };
        let (x, y) = pos.or_else(|| {
            log::warn!("글리프 아틀라스가 가득 찼다({}x{})", w, h);
            None
        })?;
        if w > 0 && h > 0 {
            self.uploads.push(Upload { color, x, y, w, h });
        }
        Some(Entry {
            x,
            y,
            w,
            h,
            left: g.left,
            top: g.top,
            color,
        })
    }
}
