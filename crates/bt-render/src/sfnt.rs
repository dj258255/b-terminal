//! 글꼴 파일에서 필요한 부분만 읽는다(#59, #65).
//!
//! 시스템 글꼴은 크다. Apple Color Emoji(180MiB)의 거의 전부는 크기별 비트맵 9벌(sbix 표)이고,
//! Apple SD Gothic Neo(53MiB)는 여러 굵기를 담은 모음 파일이다. 파일을 통째로 읽거나 메모리에 매핑(unsafe)하지 않고,
//! 한 꼴의 표들만, sbix는 원하는 크기 한 벌만 찾아 읽어 홀로 서는 글꼴 바이트를 만든다

use std::fs::File;
use std::io::{self, Read, Seek, SeekFrom};
use std::path::Path;

fn be16(b: &[u8], at: usize) -> io::Result<u16> {
    b.get(at..at + 2)
        .map(|s| u16::from_be_bytes([s[0], s[1]]))
        .ok_or_else(|| bad("짧은 글꼴 자료"))
}

fn be32(b: &[u8], at: usize) -> io::Result<u32> {
    b.get(at..at + 4)
        .map(|s| u32::from_be_bytes([s[0], s[1], s[2], s[3]]))
        .ok_or_else(|| bad("짧은 글꼴 자료"))
}

fn bad(m: &str) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, m.to_string())
}

/// 표 하나의 상한. 넘으면 글꼴로 보지 않는다(망가진 파일이 엄청난 길이를 적어도 메모리를 잡지 않게)
const MAX_TABLE: u32 = 256 * 1024 * 1024;

fn read_at(f: &mut File, offset: u64, len: u32) -> io::Result<Vec<u8>> {
    if len > MAX_TABLE {
        return Err(bad("표가 너무 크다"));
    }
    let mut buf = vec![0u8; len as usize];
    f.seek(SeekFrom::Start(offset))?;
    f.read_exact(&mut buf)?;
    Ok(buf)
}

/// 읽어 낸 글꼴
#[derive(Debug)]
pub struct Face {
    /// 홀로 서는 글꼴(모음이 아닌 sfnt) 바이트
    pub data: Vec<u8>,
    /// sbix를 한 벌만 남겼으면 그 크기(px)
    pub sbix_ppem: Option<u16>,
}

/// `path`의 `index`번째 꼴을 읽는다. `sbix_px`를 주면 sbix 표에서 그 크기 이상인 가장 작은 한 벌만 남긴다(없으면 가장 큰 것)
pub fn read_face(path: &Path, index: u32, sbix_px: Option<u16>) -> io::Result<Face> {
    read_face_with(path, index, sbix_px, false)
}

/// 가변 글꼴의 변형 표(`VARIATION_TABLES`)를 뺀 고정 글꼴로 읽는다(#467). 기본 인스턴스 그대로 쓰는 글꼴용
pub fn read_face_static(path: &Path, index: u32) -> io::Result<Face> {
    read_face_with(path, index, None, true)
}

/// 가변 글꼴 축에만 쓰이는 표. 빼면 swash는 축 없는 고정 글꼴로 보고 기본 인스턴스(`glyf`·`hmtx`·`GPOS` 기본값)를 그대로 쓴다.
/// OpenType 명세: `fvar`가 축을 정의하고, `gvar`·`cvar`(글리프·CVT 변형), `HVAR`·`VVAR`(진행 폭·높이 변형), `MVAR`(`OS/2`·`hhea` 등 글꼴 단위 값 변형),
/// `avar`(축 값 재배치)는 모두 `fvar`가 있어야 뜻이 있고 기본 인스턴스에는 기여하지 않는다.
/// `GDEF`·`GPOS`의 ItemVariationStore 참조는 축 좌표가 없으면 쓰이지 않는다(기본값이 이미 표 본문에 있다).
/// `STAT`(스타일 이름 표)은 8KB 정도라 남긴다
const VARIATION_TABLES: [[u8; 4]; 7] = [*b"gvar", *b"HVAR", *b"VVAR", *b"MVAR", *b"cvar", *b"avar", *b"fvar"];

fn read_face_with(path: &Path, index: u32, sbix_px: Option<u16>, drop_variation: bool) -> io::Result<Face> {
    let mut f = File::open(path)?;
    let head = read_at(&mut f, 0, 12)?;
    let face_offset = if &head[0..4] == b"ttcf" {
        let n = be32(&head, 8)?;
        if index >= n || n > 1024 {
            return Err(bad("모음에 그 꼴이 없다"));
        }
        let offs = read_at(&mut f, 12 + 4 * index as u64, 4)?;
        be32(&offs, 0)? as u64
    } else if index == 0 {
        0
    } else {
        return Err(bad("모음이 아닌 글꼴에 꼴 번호를 줬다"));
    };
    let offset_table = read_at(&mut f, face_offset, 12)?;
    let version = be32(&offset_table, 0)?;
    let num_tables = be16(&offset_table, 4)?;
    let dir = read_at(&mut f, face_offset + 12, 16 * num_tables as u32)?;
    let mut entries = Vec::with_capacity(num_tables as usize);
    for i in 0..num_tables as usize {
        let at = i * 16;
        let tag: [u8; 4] = dir[at..at + 4].try_into().unwrap_or_default();
        entries.push((tag, be32(&dir, at + 4)?, be32(&dir, at + 8)?, be32(&dir, at + 12)?));
    }
    // sbix를 줄이려면 글리프 수(maxp)가 필요하다
    let num_glyphs = match entries.iter().find(|e| &e.0 == b"maxp") {
        Some(&(_, _, off, len)) if len >= 6 => be16(&read_at(&mut f, off as u64, 6)?, 4)? as usize,
        _ => 0,
    };
    let mut tables = Vec::with_capacity(entries.len());
    let mut sbix_ppem = None;
    for &(tag, checksum, off, len) in &entries {
        if drop_variation && VARIATION_TABLES.contains(&tag) {
            continue;
        }
        let data = match (&tag, sbix_px) {
            (b"sbix", Some(px)) if num_glyphs > 0 => {
                let (t, ppem) = one_strike(&mut f, off as u64, len, num_glyphs, px)?;
                sbix_ppem = Some(ppem);
                t
            }
            _ => read_at(&mut f, off as u64, len)?,
        };
        tables.push((tag, checksum, data));
    }
    Ok(Face {
        data: write_sfnt(version, &tables),
        sbix_ppem,
    })
}

/// sbix 표에서 한 벌만 남긴 새 표와 그 크기
fn one_strike(f: &mut File, table: u64, len: u32, num_glyphs: usize, px: u16) -> io::Result<(Vec<u8>, u16)> {
    let head = read_at(f, table, 8)?;
    let num_strikes = be32(&head, 4)?;
    if num_strikes == 0 || num_strikes > 64 {
        return Err(bad("sbix 벌 수가 이상하다"));
    }
    let offs = read_at(f, table + 8, 4 * num_strikes)?;
    let mut strikes = Vec::new();
    for i in 0..num_strikes as usize {
        let so = be32(&offs, i * 4)?;
        let ppem = be16(&read_at(f, table + so as u64, 2)?, 0)?;
        strikes.push((ppem, so));
    }
    strikes.sort();
    let &(ppem, so) = strikes
        .iter()
        .find(|(p, _)| *p >= px)
        .or(strikes.last())
        .ok_or_else(|| bad("sbix 벌이 없다"))?;
    // 한 벌: ppem·ppi(4바이트), 글리프마다 자료 자리(벌 처음 기준, num_glyphs+1개), 그 뒤 자료
    let index_len = 4 + 4 * (num_glyphs as u32 + 1);
    let index = read_at(f, table + so as u64, index_len)?;
    let end = be32(&index, 4 + 4 * num_glyphs)?;
    if end < index_len || so as u64 + end as u64 > len as u64 {
        return Err(bad("sbix 벌이 표 밖을 가리킨다"));
    }
    let strike = read_at(f, table + so as u64, end)?;
    // 새 표: 머리(판 2, 깃발 2, 벌 수 4) + 벌 자리 하나(4) + 벌(자리는 벌 처음 기준이라 그대로 맞는다)
    let mut out = Vec::with_capacity(12 + strike.len());
    out.extend_from_slice(&head[0..4]);
    out.extend_from_slice(&1u32.to_be_bytes());
    out.extend_from_slice(&12u32.to_be_bytes());
    out.extend_from_slice(&strike);
    Ok((out, ppem))
}

/// 표 목록 머리의 searchRange·entrySelector·rangeShift. 32비트로 계산해 넘치지 않게 하고(보안 검토),
/// 16비트 필드에 넣을 때는 자른다(표가 4096개를 넘는 글꼴만 해당, swash는 이 값을 쓰지 않는다)
fn search_values(n: u16) -> (u16, u16, u16) {
    let n = u32::from(n.max(1));
    let log = 31 - n.leading_zeros();
    let search = (1u32 << log) * 16;
    (search as u16, log as u16, (n * 16 - search) as u16)
}

/// 표들로 홀로 서는 sfnt를 쓴다(표는 4바이트 경계에 둔다. 검사합은 원래 값을 둔다)
fn write_sfnt(version: u32, tables: &[([u8; 4], u32, Vec<u8>)]) -> Vec<u8> {
    let n = tables.len() as u16;
    let (search, log, shift) = search_values(n);
    let header = 12 + 16 * tables.len();
    let mut out = Vec::with_capacity(header + tables.iter().map(|t| t.2.len() + 3).sum::<usize>());
    out.extend_from_slice(&version.to_be_bytes());
    out.extend_from_slice(&n.to_be_bytes());
    out.extend_from_slice(&search.to_be_bytes());
    out.extend_from_slice(&log.to_be_bytes());
    out.extend_from_slice(&shift.to_be_bytes());
    let mut offset = header;
    for (tag, checksum, data) in tables {
        out.extend_from_slice(tag);
        out.extend_from_slice(&checksum.to_be_bytes());
        out.extend_from_slice(&(offset as u32).to_be_bytes());
        out.extend_from_slice(&(data.len() as u32).to_be_bytes());
        offset += (data.len() + 3) & !3;
    }
    for (_, _, data) in tables {
        out.extend_from_slice(data);
        out.resize((out.len() + 3) & !3, 0);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 표 셋(maxp 포함)짜리 가짜 글꼴. sbix가 있으면 두 벌(10px, 20px)
    fn fake_face(with_sbix: bool) -> Vec<([u8; 4], u32, Vec<u8>)> {
        let mut maxp = vec![0, 0, 0x50, 0];
        maxp.extend_from_slice(&2u16.to_be_bytes()); // 글리프 2개
        let mut t = vec![(*b"cmap", 1, b"CMAPDATA".to_vec()), (*b"maxp", 2, maxp)];
        if with_sbix {
            // 벌: ppem, ppi, 자리 3개(벌 처음 기준), 자료
            let strike = |ppem: u16, a: &[u8], b: &[u8]| {
                let base = 4 + 12;
                let mut s = Vec::new();
                s.extend_from_slice(&ppem.to_be_bytes());
                s.extend_from_slice(&72u16.to_be_bytes());
                for o in [base, base + a.len(), base + a.len() + b.len()] {
                    s.extend_from_slice(&(o as u32).to_be_bytes());
                }
                s.extend_from_slice(a);
                s.extend_from_slice(b);
                s
            };
            let s10 = strike(10, b"small-a", b"small-b");
            let s20 = strike(20, b"BIG-AAAA", b"BIG-BBBB");
            let mut sbix = vec![0, 1, 0, 1];
            sbix.extend_from_slice(&2u32.to_be_bytes());
            let first = 8 + 8;
            sbix.extend_from_slice(&(first as u32).to_be_bytes());
            sbix.extend_from_slice(&((first + s10.len()) as u32).to_be_bytes());
            sbix.extend_from_slice(&s10);
            sbix.extend_from_slice(&s20);
            t.push((*b"sbix", 3, sbix));
        }
        t
    }

    fn tables_of(data: &[u8]) -> Vec<(String, Vec<u8>)> {
        let n = be16(data, 4).unwrap() as usize;
        (0..n)
            .map(|i| {
                let at = 12 + 16 * i;
                let tag = String::from_utf8_lossy(&data[at..at + 4]).into_owned();
                let off = be32(data, at + 8).unwrap() as usize;
                let len = be32(data, at + 12).unwrap() as usize;
                (tag, data[off..off + len].to_vec())
            })
            .collect()
    }

    fn tmp(name: &str, bytes: &[u8]) -> std::path::PathBuf {
        let p = std::env::temp_dir().join(format!("bt-sfnt-{}-{name}", std::process::id()));
        std::fs::write(&p, bytes).unwrap();
        p
    }

    /// 보안 검토: 표가 32768개 이상이어도 머리 값 계산이 넘치거나 끝없이 돌지 않는다
    #[test]
    fn header_search_values_do_not_overflow() {
        assert_eq!(search_values(1), (16, 0, 0));
        assert_eq!(search_values(9), (128, 3, 16));
        // 필드가 16비트라 이만큼 크면 값이 잘리지만(swash는 이 값을 쓰지 않는다) 넘쳐서 멈추지 않는다
        assert_eq!(search_values(40_000).1, 15);
        assert_eq!(search_values(u16::MAX).1, 15);
    }

    /// #467: 고정 글꼴로 읽으면 변형 표만 빠지고 나머지 표는 그대로다. 일반 읽기는 변형 표도 남긴다
    #[test]
    fn static_read_drops_only_variation_tables() {
        let mut tables = fake_face(false);
        for tag in VARIATION_TABLES {
            tables.push((tag, 7, b"VARDATA!".to_vec()));
        }
        tables.push((*b"STAT", 8, b"STATDATA".to_vec()));
        let p = tmp("variable", &write_sfnt(0x0001_0000, &tables));
        let kept = |d: &[u8]| tables_of(d).into_iter().map(|t| t.0).collect::<Vec<_>>();
        let all = kept(&read_face(&p, 0, None).unwrap().data);
        assert!(all.contains(&"gvar".to_string()) && all.contains(&"fvar".to_string()));
        let fixed = kept(&read_face_static(&p, 0).unwrap().data);
        assert_eq!(fixed, ["cmap", "maxp", "STAT"]);
    }

    #[test]
    fn copies_every_table_of_a_single_font() {
        let src = write_sfnt(0x0001_0000, &fake_face(false));
        let p = tmp("single", &src);
        let face = read_face(&p, 0, None).unwrap();
        let t = tables_of(&face.data);
        assert_eq!(t[0], ("cmap".into(), b"CMAPDATA".to_vec()));
        assert_eq!(t.len(), 2);
        assert!(read_face(&p, 1, None).is_err(), "모음이 아닌데 1번 꼴");
        std::fs::remove_file(p).unwrap();
    }

    #[test]
    fn picks_one_face_from_a_collection() {
        let a = write_sfnt(0x0001_0000, &[(*b"name", 0, b"FACE-A".to_vec())]);
        let b = write_sfnt(0x0001_0000, &[(*b"name", 0, b"FACE-B".to_vec())]);
        // 모음: ttcf, 판, 꼴 수, 꼴 자리들. 꼴 안의 표 자리는 파일 처음 기준이라 옮겨 적는다
        let header = 12 + 8;
        let shift = |face: &[u8], base: usize| {
            let mut f = face.to_vec();
            let n = be16(&f, 4).unwrap() as usize;
            for i in 0..n {
                let at = 12 + 16 * i + 8;
                let off = be32(&f, at).unwrap() as usize + base;
                f[at..at + 4].copy_from_slice(&(off as u32).to_be_bytes());
            }
            f
        };
        let mut file = b"ttcf".to_vec();
        file.extend_from_slice(&0x0001_0000u32.to_be_bytes());
        file.extend_from_slice(&2u32.to_be_bytes());
        file.extend_from_slice(&(header as u32).to_be_bytes());
        file.extend_from_slice(&((header + a.len()) as u32).to_be_bytes());
        file.extend_from_slice(&shift(&a, header));
        file.extend_from_slice(&shift(&b, header + a.len()));
        let p = tmp("ttc", &file);
        let face = read_face(&p, 1, None).unwrap();
        assert_eq!(tables_of(&face.data), vec![("name".into(), b"FACE-B".to_vec())]);
        assert!(read_face(&p, 2, None).is_err());
        std::fs::remove_file(p).unwrap();
    }

    #[test]
    fn keeps_only_the_needed_sbix_strike() {
        let src = write_sfnt(0x0001_0000, &fake_face(true));
        let p = tmp("sbix", &src);
        // 15px면 20px 벌
        let face = read_face(&p, 0, Some(15)).unwrap();
        assert_eq!(face.sbix_ppem, Some(20));
        let sbix = tables_of(&face.data).into_iter().find(|t| t.0 == "sbix").unwrap().1;
        assert_eq!(be32(&sbix, 4).unwrap(), 1, "한 벌만");
        let s = be32(&sbix, 8).unwrap() as usize;
        assert_eq!(be16(&sbix, s).unwrap(), 20);
        // 둘째 글리프 자료가 그 자리에 있다
        let (g1, g2) = (
            be32(&sbix, s + 8).unwrap() as usize,
            be32(&sbix, s + 12).unwrap() as usize,
        );
        assert_eq!(&sbix[s + g1..s + g2], b"BIG-BBBB");
        assert!(!sbix.windows(7).any(|w| w == b"small-a"), "다른 벌은 빠진다");
        // 가장 큰 것보다 크면 가장 큰 것, 작으면 가장 작은 것
        assert_eq!(read_face(&p, 0, Some(99)).unwrap().sbix_ppem, Some(20));
        assert_eq!(read_face(&p, 0, Some(1)).unwrap().sbix_ppem, Some(10));
        std::fs::remove_file(p).unwrap();
    }

    #[test]
    fn rejects_garbage_without_huge_allocations() {
        let p = tmp("garbage", b"ttcf\x00\x01\x00\x00\xff\xff\xff\xff");
        assert!(read_face(&p, 0, None).is_err());
        // 표 길이가 엄청난 거짓 글꼴
        let mut f = write_sfnt(0x0001_0000, &[(*b"glyf", 0, vec![1, 2, 3])]);
        f[12 + 12..12 + 16].copy_from_slice(&u32::MAX.to_be_bytes());
        let p2 = tmp("huge", &f);
        assert!(read_face(&p2, 0, None).is_err());
        std::fs::remove_file(p).unwrap();
        std::fs::remove_file(p2).unwrap();
    }

    /// 실제 Apple Color Emoji에서 한 벌만 읽으면 크게 작고, swash가 색 이모지를 그린다(macOS에서만)
    #[test]
    fn apple_color_emoji_subset_is_small_and_renders() {
        let path = Path::new("/System/Library/Fonts/Apple Color Emoji.ttc");
        if !path.exists() {
            return;
        }
        let face = read_face(path, 0, Some(35)).unwrap();
        assert_eq!(face.sbix_ppem, Some(40));
        assert!(face.data.len() < 20 * 1024 * 1024, "{} 바이트", face.data.len());
        let font = swash::FontRef::from_index(&face.data, 0).expect("홀로 서는 글꼴");
        let gid = font.charmap().map('😀');
        assert_ne!(gid, 0);
        let mut ctx = swash::scale::ScaleContext::new();
        let mut scaler = ctx.builder(font).size(35.0).build();
        let img = swash::scale::Render::new(&[swash::scale::Source::ColorBitmap(swash::scale::StrikeWith::BestFit)])
            .render(&mut scaler, gid)
            .expect("색 비트맵");
        assert_eq!(img.content, swash::scale::image::Content::Color);
        assert!(img.placement.width > 20);
    }
}
