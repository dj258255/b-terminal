//! 글리프 아틀라스: 선반(shelf) 방식으로 채우고, 가득 차면 두 배로 키운다(Ghostty Atlas.zig처럼).
//! 픽셀은 CPU 쪽 복사본에 두고, GPU 텍스처는 렌더러가 이 복사본에서 올린다

/// 더 키울 수 없는데 자리가 없다
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Full;

/// 같은 높이의 조각이 왼쪽에서 오른쪽으로 놓이는 줄
#[derive(Debug, Clone, Copy)]
struct Shelf {
    y: u32,
    h: u32,
    /// 다음 조각이 놓일 x
    x: u32,
}

pub struct Atlas {
    size: u32,
    /// 픽셀당 바이트(회색조 1, 색 4)
    bpp: u32,
    /// 키울 수 있는 한도(장치의 텍스처 한 변 한도)
    max: u32,
    data: Vec<u8>,
    shelves: Vec<Shelf>,
    grows: u32,
}

impl Atlas {
    pub fn new(size: u32, bpp: u32, max: u32) -> Self {
        let size = size.max(1);
        Self {
            size,
            bpp,
            max: max.max(size),
            data: vec![0; (size as usize).pow(2) * bpp as usize],
            shelves: Vec::new(),
            grows: 0,
        }
    }

    pub fn size(&self) -> u32 {
        self.size
    }

    /// 키울 수 있는 한 변의 한도
    pub fn max(&self) -> u32 {
        self.max
    }

    /// 지금까지 키운 횟수
    pub fn grows(&self) -> u32 {
        self.grows
    }

    /// 줄 우선 픽셀 전체(`size × size × bpp`)
    pub fn data(&self) -> &[u8] {
        &self.data
    }

    /// `w × h` 조각을 놓을 자리를 잡고 `pixels`(줄 우선, `w * h * bpp` 바이트)를 복사한다. 키우지는 않는다
    pub fn insert(&mut self, w: u32, h: u32, pixels: &[u8]) -> Result<(u32, u32), Full> {
        if w == 0 || h == 0 {
            return Ok((0, 0));
        }
        if w > self.size || h > self.size {
            return Err(Full);
        }
        // 들어가는 줄 중 높이가 가장 알맞은 것(낭비가 적다)
        let fit = self
            .shelves
            .iter()
            .enumerate()
            .filter(|(_, s)| s.h >= h && s.x + w <= self.size)
            .min_by_key(|(_, s)| s.h)
            .map(|(i, _)| i);
        let (x, y) = match fit {
            Some(i) => {
                let s = &mut self.shelves[i];
                let at = (s.x, s.y);
                s.x += w;
                at
            }
            None => {
                let y = self.shelves.last().map_or(0, |s| s.y + s.h);
                if y + h > self.size {
                    return Err(Full);
                }
                self.shelves.push(Shelf { y, h, x: w });
                (0, y)
            }
        };
        let bpp = self.bpp as usize;
        let row = w as usize * bpp;
        debug_assert!(pixels.len() >= row * h as usize);
        for r in 0..h as usize {
            let dst = ((y as usize + r) * self.size as usize + x as usize) * bpp;
            self.data[dst..dst + row].copy_from_slice(&pixels[r * row..(r + 1) * row]);
        }
        Ok((x, y))
    }

    /// 두 배로 키운다. 이미 놓은 조각의 자리는 그대로다. 한도에 닿았으면 거짓
    pub fn grow(&mut self) -> bool {
        let new = (self.size * 2).min(self.max);
        if new <= self.size {
            return false;
        }
        let bpp = self.bpp as usize;
        let mut data = vec![0u8; (new as usize).pow(2) * bpp];
        let old_row = self.size as usize * bpp;
        for r in 0..self.size as usize {
            let dst = r * new as usize * bpp;
            data[dst..dst + old_row].copy_from_slice(&self.data[r * old_row..(r + 1) * old_row]);
        }
        self.data = data;
        self.size = new;
        self.grows += 1;
        true
    }

    /// 자리가 없으면 키워 가며 놓는다. 한도까지 키워도 없으면 없다
    pub fn insert_growing(&mut self, w: u32, h: u32, pixels: &[u8]) -> Option<(u32, u32)> {
        loop {
            match self.insert(w, h, pixels) {
                Ok(p) => return Some(p),
                Err(Full) if self.grow() => continue,
                Err(Full) => return None,
            }
        }
    }
}
