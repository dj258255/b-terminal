//! 글리프 캐시: 글리프를 처음 볼 때 `Fonts`로 래스터해 아틀라스(회색조·색 두 장)에 채운다

use std::collections::HashMap;
use std::rc::Rc;
use std::sync::Arc;

use bt_render::FontStyle;
use bt_render::font::{CellMetrics, Glyph};

use crate::atlas::Atlas;

/// 아틀라스에 놓은 글리프를 가리키는 열쇠
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum Key {
    Char(char, FontStyle),
    /// 이모지 모양(U+FE0F), 차지하는 칸 수
    Emoji(char, u32),
    Cluster(Arc<str>),
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

pub struct GlyphCache {
    pub gray: Atlas,
    pub color: Atlas,
    map: HashMap<Key, Option<Entry>>,
    /// 캐시를 채운 때의 글꼴 치수. 달라지면(글꼴 크기·배율 변경) 비운다
    metrics: Option<CellMetrics>,
    pub uploads: Vec<Upload>,
}

impl GlyphCache {
    pub fn new(initial: u32, max: u32) -> Self {
        Self {
            gray: Atlas::new(initial, 1, max),
            color: Atlas::new(initial, 4, max),
            map: HashMap::new(),
            metrics: None,
            uploads: Vec::new(),
        }
    }

    /// 글꼴 치수가 바뀌었으면 캐시를 비운다(아틀라스 크기는 그대로, 채움만 처음부터)
    pub fn check_metrics(&mut self, m: CellMetrics) {
        if self.metrics != Some(m) {
            if self.metrics.is_some() {
                self.map.clear();
                self.gray = Atlas::new(self.gray.size(), 1, self.gray.max());
                self.color = Atlas::new(self.color.size(), 4, self.color.max());
                self.uploads.clear();
            }
            self.metrics = Some(m);
        }
    }

    pub fn len(&self) -> usize {
        self.map.len()
    }

    pub fn is_empty(&self) -> bool {
        self.map.is_empty()
    }

    /// 열쇠의 글리프 자리. 처음이면 `make`로 래스터해 아틀라스에 채운다. 글리프가 없거나 아틀라스를 한도까지 키워도 자리가 없으면 없다
    pub fn get(&mut self, key: Key, make: impl FnOnce() -> Option<Rc<Glyph>>) -> Option<Entry> {
        if let Some(e) = self.map.get(&key) {
            return *e;
        }
        let e = make().and_then(|g| self.put(&g));
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
