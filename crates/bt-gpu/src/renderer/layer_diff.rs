//! 층 올리기 줄이기(ADR-037): 앞서 올린 층과 이번 층을 줄 단위로 견줘 달라진 사각형만 찾는다

/// 층 안의 사각형(픽셀)
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Rect {
    pub x: u32,
    pub y: u32,
    pub w: u32,
    pub h: u32,
}

/// 달라진 줄 사이가 이 줄 수 이하면 한 묶음으로 본다(작은 올리기 둘보다 한 번이 싸다)
pub const MERGE_GAP_ROWS: u32 = 16;
/// 묶음이 이보다 많으면 모두를 감싸는 사각형 하나로 한다
pub const MAX_RECTS: usize = 4;

/// 옛 방식(사본과 견주기). 이제 올리기는 `TileHashes`를 쓰고, 이것은 시험의 기준과 시간 비교에만 남았다.
/// `prev`와 `new`(둘 다 `w`×`h`, 줄 순서)에서 달라진 곳을 덮는 사각형들. 같으면 비었다.
/// 길이가 다르거나 `w`·`h`와 맞지 않으면 통째 한 사각형이다
#[cfg(test)]
pub fn changed_rects(prev: &[u32], new: &[u32], w: u32, h: u32) -> Vec<Rect> {
    let (wu, hu) = (w as usize, h as usize);
    if wu == 0 || hu == 0 || prev.len() != wu * hu || new.len() != wu * hu {
        return vec![Rect { x: 0, y: 0, w, h }];
    }
    // 묶음: (첫 줄, 마지막 줄, 가장 왼쪽 열, 가장 오른쪽 열)
    let mut bands: Vec<(u32, u32, u32, u32)> = Vec::new();
    for (y, (a, b)) in prev.chunks_exact(wu).zip(new.chunks_exact(wu)).enumerate() {
        if a == b {
            continue;
        }
        let first = a.iter().zip(b).position(|(p, q)| p != q).unwrap_or(0) as u32;
        let last = a.iter().zip(b).rposition(|(p, q)| p != q).unwrap_or(0) as u32;
        let y = y as u32;
        match bands.last_mut() {
            Some(band) if y - band.1 <= MERGE_GAP_ROWS => {
                band.1 = y;
                band.2 = band.2.min(first);
                band.3 = band.3.max(last);
            }
            _ => bands.push((y, y, first, last)),
        }
    }
    bands_to_rects(bands)
}

/// 묶음((첫 줄, 마지막 줄, 왼쪽 열, 오른쪽 열))들을 사각형으로. 묶음이 `MAX_RECTS`보다 많으면 모두를 감싸는 하나로
fn bands_to_rects(bands: Vec<(u32, u32, u32, u32)>) -> Vec<Rect> {
    let rect = |(y0, y1, x0, x1): (u32, u32, u32, u32)| Rect {
        x: x0,
        y: y0,
        w: x1 - x0 + 1,
        h: y1 - y0 + 1,
    };
    if bands.len() > MAX_RECTS {
        let all = bands.iter().fold((u32::MAX, 0, u32::MAX, 0), |m, b| {
            (m.0.min(b.0), m.1.max(b.1), m.2.min(b.2), m.3.max(b.3))
        });
        return vec![rect(all)];
    }
    bands.into_iter().map(rect).collect()
}

/// 칸 폭(픽셀). 한 칸은 이 폭 × 1줄이다. 칸이 좁을수록 바뀐 사각형이 꼭 맞고 칸 정보는 늘어난다
pub const TILE_W: usize = 128;

/// 곱하기 섞기 상수(소수에 가까운 홀수 64비트)
const MUL: u64 = 0x9e37_79b9_7f4a_7c15;

/// 프로세스마다 다른 해시 시드. kitty 그림 픽셀은 프로그램이 정하므로, 시드를 모르면 일부러 같은 해시가 나오는
/// 픽셀을 만들기 어렵게 한다(`RandomState`가 프로세스 시작 때 운영체제 난수로 정한 값을 쓴다)
fn seed() -> u64 {
    use std::hash::{BuildHasher, Hasher};
    use std::sync::OnceLock;
    static SEED: OnceLock<u64> = OnceLock::new();
    *SEED.get_or_init(|| std::collections::hash_map::RandomState::new().build_hasher().finish() | 1)
}

/// 32비트 곱하기 섞기 상수(홀수)
const MUL32: u32 = 0x85eb_ca6b;
/// 한 번에 섞는 픽셀 수(u32 갈래 수). NEON·AVX가 한 번에 곱할 수 있는 꼴이다
const LANES: usize = 16;

/// 한 칸(한 줄의 일부) 픽셀의 64비트 해시. 픽셀(u32)마다 갈래 하나에 `xor → 홀수 곱 → 회전`으로 섞는다.
/// 64비트 곱셈 사슬은 M2에서 벡터화되지 않아 느렸다(#393): u32 곱셈은 벡터로 돌고, 갈래 16개를 칸 끝에서 한 번만
/// 64비트로 모은다. 갈래마다 한 단계는 상태에 대해 일대일이라, 한 픽셀만 바뀐 칸은 그 갈래의 상태가 반드시 달라진다
fn tile_hash(px: &[u32], init: &Init) -> u64 {
    let mut lane = init.lane;
    let mut chunks = px.chunks_exact(LANES);
    for c in chunks.by_ref() {
        // 길이를 아는 배열로 받아 경계 검사 없이 벡터로 돌게 한다
        let c: &[u32; LANES] = c.try_into().unwrap_or(&[0; LANES]);
        for i in 0..LANES {
            lane[i] = (lane[i] ^ c[i]).wrapping_mul(MUL32).rotate_left(13);
        }
    }
    for (l, p) in lane.iter_mut().zip(chunks.remainder()) {
        *l = (*l ^ p).wrapping_mul(MUL32).rotate_left(13);
    }
    // 갈래 16개를 64비트 넷으로 모은다(서로 기다리지 않는 곱셈 넷)
    let mut acc = [
        px.len() as u64 ^ init.seed,
        init.seed.rotate_left(21),
        init.seed.rotate_left(42),
        !init.seed,
    ];
    for (i, a) in acc.iter_mut().enumerate() {
        for j in 0..2 {
            let v = u64::from(lane[4 * i + 2 * j]) | u64::from(lane[4 * i + 2 * j + 1]) << 32;
            *a = (*a ^ v).wrapping_mul(MUL).rotate_left(31);
        }
    }
    let mut h = acc[0] ^ acc[1].rotate_left(16) ^ acc[2].rotate_left(32) ^ acc[3].rotate_left(48);
    h ^= h >> 32;
    h = h.wrapping_mul(MUL);
    h ^ (h >> 29)
}

/// 칸 해시의 출발 상태(시드에서 한 번 만든다)
struct Init {
    seed: u64,
    lane: [u32; LANES],
}

impl Init {
    fn new(seed: u64) -> Self {
        let (lo, hi) = (seed as u32, (seed >> 32) as u32);
        let mut lane = [0u32; LANES];
        for (i, l) in lane.iter_mut().enumerate() {
            *l = lo.wrapping_add(hi.wrapping_mul(i as u32 + 1)) ^ MUL32.rotate_left(i as u32);
        }
        Self { seed, lane }
    }
}

/// 층 픽셀의 사본 대신 쥐는 칸 해시(#393). 층을 `TILE_W`×1 칸으로 나눠 칸마다 64비트 해시와 0이 아닌 픽셀 수를 쥔다.
/// 2000×1360 층이면 사본 10.9MB 대신 약 0.2MB다.
///
/// 한계: 해시가 우연히 같으면(2^-64 수준, 시드는 프로세스마다 다르다) 그 칸은 바뀐 줄 모르고 옛 그림으로 남는다.
/// 그 칸은 다음에 그 칸이 또 바뀔 때 고쳐진다
pub struct TileHashes {
    w: u32,
    h: u32,
    init: Init,
    tiles_per_row: usize,
    hashes: Vec<u64>,
    /// 칸의 0이 아닌 픽셀 수(칸은 `TILE_W`픽셀 이하)
    counts: Vec<u8>,
    nonzero: usize,
}

impl TileHashes {
    /// 0으로 채운 `w`×`h` 층의 칸 해시(wgpu가 새 텍스처를 0으로 채우므로 새 층의 출발점이다)
    pub fn zeroed(w: u32, h: u32) -> Self {
        let init = Init::new(seed());
        let tiles_per_row = (w as usize).div_ceil(TILE_W);
        let zeros = [0u32; TILE_W];
        let tail = w as usize % TILE_W;
        let mut hashes = vec![tile_hash(&zeros, &init); tiles_per_row * h as usize];
        if tail != 0 && tiles_per_row > 0 {
            let tail_hash = tile_hash(&zeros[..tail], &init);
            for row in hashes.chunks_exact_mut(tiles_per_row) {
                row[tiles_per_row - 1] = tail_hash;
            }
        }
        let counts = vec![0; hashes.len()];
        Self {
            w,
            h,
            init,
            tiles_per_row,
            hashes,
            counts,
            nonzero: 0,
        }
    }

    /// 새 픽셀(`w`×`h`, 줄 순서)을 쥔 해시와 견주고, 쥔 것을 새 픽셀의 것으로 바꾼다. 달라진 칸을 덮는 사각형들을
    /// 돌려 준다(열 범위는 칸 경계로 넓어진다. 묶는 규칙은 `changed_rects`와 같다). 길이가 맞지 않으면 아무것도
    /// 바꾸지 않고 통째 한 사각형을 돌려 준다
    pub fn update(&mut self, px: &[u32]) -> Vec<Rect> {
        let (wu, hu) = (self.w as usize, self.h as usize);
        if wu == 0 || hu == 0 || px.len() != wu * hu {
            return vec![Rect {
                x: 0,
                y: 0,
                w: self.w,
                h: self.h,
            }];
        }
        let mut bands: Vec<(u32, u32, u32, u32)> = Vec::new();
        for (y, row) in px.chunks_exact(wu).enumerate() {
            let base = y * self.tiles_per_row;
            let (mut first, mut last) = (usize::MAX, 0);
            for (tx, tile) in row.chunks(TILE_W).enumerate() {
                let hash = tile_hash(tile, &self.init);
                if hash == self.hashes[base + tx] {
                    continue;
                }
                self.hashes[base + tx] = hash;
                let count = tile.iter().filter(|p| **p != 0).count();
                self.nonzero = self.nonzero + count - usize::from(self.counts[base + tx]);
                self.counts[base + tx] = count as u8;
                first = first.min(tx);
                last = tx;
            }
            if first == usize::MAX {
                continue;
            }
            let x0 = (first * TILE_W) as u32;
            let x1 = ((last + 1) * TILE_W).min(wu) as u32 - 1;
            let y = y as u32;
            match bands.last_mut() {
                Some(band) if y - band.1 <= MERGE_GAP_ROWS => {
                    band.1 = y;
                    band.2 = band.2.min(x0);
                    band.3 = band.3.max(x1);
                }
                _ => bands.push((y, y, x0, x1)),
            }
        }
        bands_to_rects(bands)
    }

    /// 0이 아닌 픽셀 수. 0이면 층이 비었다
    pub fn nonzero(&self) -> usize {
        self.nonzero
    }

    /// 쥔 메모리(바이트)
    pub fn bytes(&self) -> usize {
        self.hashes.capacity() * 8 + self.counts.capacity()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const W: u32 = 40;
    const H: u32 = 120;

    fn blank() -> Vec<u32> {
        vec![0; (W * H) as usize]
    }

    fn set(px: &mut [u32], x: u32, y: u32) {
        px[(y * W + x) as usize] = 0xff00_ff00;
    }

    #[test]
    fn equal_layers_have_no_changed_rect() {
        let a = blank();
        assert_eq!(changed_rects(&a, &a.clone(), W, H), vec![]);
    }

    #[test]
    fn one_changed_pixel_is_a_one_by_one_rect() {
        let a = blank();
        let mut b = blank();
        set(&mut b, 7, 30);
        assert_eq!(
            changed_rects(&a, &b, W, H),
            vec![Rect {
                x: 7,
                y: 30,
                w: 1,
                h: 1
            }]
        );
    }

    #[test]
    fn rows_close_together_make_one_rect_spanning_both_columns() {
        let a = blank();
        let mut b = blank();
        set(&mut b, 3, 10);
        set(&mut b, 30, 12);
        assert_eq!(
            changed_rects(&a, &b, W, H),
            vec![Rect {
                x: 3,
                y: 10,
                w: 28,
                h: 3
            }]
        );
    }

    #[test]
    fn rows_far_apart_make_separate_rects() {
        let a = blank();
        let mut b = blank();
        set(&mut b, 3, 10);
        set(&mut b, 30, 12 + MERGE_GAP_ROWS + 1);
        assert_eq!(
            changed_rects(&a, &b, W, H),
            vec![
                Rect {
                    x: 3,
                    y: 10,
                    w: 1,
                    h: 1
                },
                Rect {
                    x: 30,
                    y: 12 + MERGE_GAP_ROWS + 1,
                    w: 1,
                    h: 1
                }
            ]
        );
    }

    #[test]
    fn gap_of_exactly_the_limit_still_merges() {
        let a = blank();
        let mut b = blank();
        set(&mut b, 5, 10);
        set(&mut b, 5, 10 + 1 + MERGE_GAP_ROWS);
        assert_eq!(changed_rects(&a, &b, W, H).len(), 2);
        let mut c = blank();
        set(&mut c, 5, 10);
        set(&mut c, 5, 10 + MERGE_GAP_ROWS);
        assert_eq!(changed_rects(&a, &c, W, H).len(), 1);
    }

    #[test]
    fn more_groups_than_the_limit_collapse_into_one_bounding_rect() {
        let a = blank();
        let mut b = blank();
        for i in 0..=MAX_RECTS as u32 {
            set(&mut b, 2 + i, i * (MERGE_GAP_ROWS + 2));
        }
        let last = MAX_RECTS as u32 * (MERGE_GAP_ROWS + 2);
        assert_eq!(
            changed_rects(&a, &b, W, H),
            vec![Rect {
                x: 2,
                y: 0,
                w: MAX_RECTS as u32 + 1,
                h: last + 1
            }]
        );
    }

    #[test]
    fn mismatched_size_is_the_whole_layer() {
        let a = vec![0u32; 10];
        let b = blank();
        assert_eq!(changed_rects(&a, &b, W, H), vec![Rect { x: 0, y: 0, w: W, h: H }]);
    }

    #[test]
    fn change_in_the_last_row_and_column_is_found() {
        let a = blank();
        let mut b = blank();
        set(&mut b, W - 1, H - 1);
        assert_eq!(
            changed_rects(&a, &b, W, H),
            vec![Rect {
                x: W - 1,
                y: H - 1,
                w: 1,
                h: 1
            }]
        );
    }

    // ---- 칸 해시 ----

    fn rect_contains(a: &Rect, b: &Rect) -> bool {
        a.x <= b.x && a.y <= b.y && a.x + a.w >= b.x + b.w && a.y + a.h >= b.y + b.h
    }

    /// 새 사각형들만 `shadow`에 옮긴 뒤 `new`와 같아야 한다(빠뜨림 없음)
    fn apply(shadow: &mut [u32], new: &[u32], w: u32, rects: &[Rect]) {
        for r in rects {
            for y in r.y..r.y + r.h {
                let at = (y * w + r.x) as usize;
                shadow[at..at + r.w as usize].copy_from_slice(&new[at..at + r.w as usize]);
            }
        }
    }

    fn lcg(seed: &mut u64) -> u64 {
        *seed = seed.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
        *seed >> 33
    }

    #[test]
    fn tiles_of_a_zero_layer_have_no_changes() {
        let mut t = TileHashes::zeroed(W, H);
        assert_eq!(t.update(&blank()), vec![]);
        assert_eq!(t.nonzero(), 0);
    }

    #[test]
    fn tiles_widen_one_pixel_to_its_tile_and_clamp_at_the_edge() {
        // 폭 40은 칸 하나(128)보다 좁다: 칸은 층 폭으로 자른다
        let mut t = TileHashes::zeroed(W, H);
        let mut b = blank();
        set(&mut b, 7, 30);
        assert_eq!(
            t.update(&b),
            vec![Rect {
                x: 0,
                y: 30,
                w: W,
                h: 1
            }]
        );
        assert_eq!(t.nonzero(), 1);
        // 같은 그림을 다시 주면 바뀐 것이 없다
        assert_eq!(t.update(&b), vec![]);
        // 지우면 다시 바뀜, 개수는 0
        assert_eq!(
            t.update(&blank()),
            vec![Rect {
                x: 0,
                y: 30,
                w: W,
                h: 1
            }]
        );
        assert_eq!(t.nonzero(), 0);
    }

    #[test]
    fn tiles_in_a_wide_layer_only_cover_the_changed_tile_columns() {
        let (w, h) = (3 * TILE_W as u32 + 10, 50);
        let mut t = TileHashes::zeroed(w, h);
        let mut b = vec![0u32; (w * h) as usize];
        b[(5 * w + TILE_W as u32 + 3) as usize] = 1; // 둘째 칸
        b[(5 * w + w - 1) as usize] = 1; // 마지막(좁은) 칸
        let got = t.update(&b);
        assert_eq!(got.len(), 1);
        assert_eq!(
            got[0],
            Rect {
                x: TILE_W as u32,
                y: 5,
                w: w - TILE_W as u32,
                h: 1
            }
        );
        assert_eq!(t.nonzero(), 2);
    }

    #[test]
    fn tiles_keep_the_band_rules() {
        let mut t = TileHashes::zeroed(W, H);
        let mut b = blank();
        set(&mut b, 3, 10);
        set(&mut b, 30, 12 + MERGE_GAP_ROWS + 1);
        assert_eq!(t.update(&b).len(), 2);
        let mut t = TileHashes::zeroed(W, H);
        let mut c = blank();
        set(&mut c, 5, 10);
        set(&mut c, 5, 10 + MERGE_GAP_ROWS);
        assert_eq!(t.update(&c).len(), 1);
        let mut t = TileHashes::zeroed(W, H);
        let mut d = blank();
        for i in 0..=MAX_RECTS as u32 {
            set(&mut d, 2 + i, i * (MERGE_GAP_ROWS + 2));
        }
        let last = MAX_RECTS as u32 * (MERGE_GAP_ROWS + 2);
        assert_eq!(
            t.update(&d),
            vec![Rect {
                x: 0,
                y: 0,
                w: W,
                h: last + 1
            }]
        );
    }

    #[test]
    fn tiles_always_cover_what_the_copy_comparison_finds() {
        let (w, h) = (3 * TILE_W as u32 + 17, 90u32);
        let mut seed = 7u64;
        let mut t = TileHashes::zeroed(w, h);
        let mut prev = vec![0u32; (w * h) as usize];
        let mut shadow = prev.clone();
        for round in 0..40 {
            let mut new = prev.clone();
            for _ in 0..(round % 6) {
                let i = lcg(&mut seed) as usize % new.len();
                new[i] = (lcg(&mut seed) as u32) | 1;
            }
            if round % 7 == 3 {
                new.iter_mut().for_each(|p| *p = 0);
            }
            let old = changed_rects(&prev, &new, w, h);
            let got = t.update(&new);
            for r in &old {
                assert!(
                    got.iter().any(|g| rect_contains(g, r)),
                    "round {round}: {r:?} 빠짐: {got:?}"
                );
            }
            apply(&mut shadow, &new, w, &got);
            assert_eq!(shadow, new, "round {round}: 올린 사각형만으로 층이 같아지지 않는다");
            assert_eq!(t.nonzero(), new.iter().filter(|p| **p != 0).count(), "round {round}");
            prev = new;
        }
    }

    /// 해시 품질: 한 픽셀만 다른 칸 수천 개(칸 안 자리·값을 돌려 가며)에서 해시가 늘 달라야 하고,
    /// 서로 다른 칸 수만 개에서 해시가 겹치지 않아야 한다
    #[test]
    fn tile_hash_differs_for_every_single_pixel_change() {
        let init = Init::new(seed());
        let mut seed_rng = 99u64;
        let mut seen = std::collections::HashSet::new();
        let mut n = 0;
        for len in [TILE_W, 17, 1, 16, 33] {
            let base = vec![0xff1e_1e2eu32; len];
            let hb = tile_hash(&base, &init);
            for pos in 0..len {
                for _ in 0..200 {
                    let mut t = base.clone();
                    let v = lcg(&mut seed_rng) as u32;
                    if v == t[pos] {
                        continue;
                    }
                    t[pos] = v;
                    let h = tile_hash(&t, &init);
                    assert_ne!(h, hb, "len {len} pos {pos}: 한 픽셀이 달라도 해시가 같다");
                    if len == TILE_W {
                        n += 1;
                        assert!(seen.insert(h), "서로 다른 칸 {n}개 안에서 해시가 겹쳤다");
                    }
                }
            }
        }
        assert!(n > 10_000, "{n}");
    }

    #[test]
    fn tile_hashes_are_a_small_fraction_of_the_copy() {
        let t = TileHashes::zeroed(2000, 1360);
        let copy = 2000 * 1360 * 4;
        assert!(t.bytes() * 20 < copy, "{} vs {copy}", t.bytes());
    }

    /// 시간 비교(옛 사본 견주기 vs 칸 해시): `cargo test --release -p bt-gpu -- --ignored --nocapture`
    #[test]
    #[ignore = "시간 측정"]
    fn timing_copy_comparison_vs_tile_hash() {
        use std::time::Instant;
        let (w, h) = (2000u32, 1360u32);
        let n = (w * h) as usize;
        let mut seed = 1u64;
        let noisy: Vec<u32> = (0..n)
            .map(|_| {
                if lcg(&mut seed) & 3 == 0 {
                    lcg(&mut seed) as u32 | 1
                } else {
                    0
                }
            })
            .collect();
        // 실제 아래층과 비슷하게: 거의 모든 픽셀이 같은 불투명 바탕색, 탭 막대만 다른 색
        let mut solid = vec![0xff1e_1e2eu32; n];
        for y in 0..40usize {
            for x in 0..w as usize {
                solid[y * w as usize + x] = 0xff31_3244 ^ ((x as u32 / 90) << 8);
            }
        }
        let median = |f: &mut dyn FnMut()| {
            let mut ts: Vec<f64> = (0..21)
                .map(|_| {
                    let t = Instant::now();
                    f();
                    t.elapsed().as_secs_f64() * 1000.0
                })
                .collect();
            ts.sort_by(f64::total_cmp);
            ts[ts.len() / 2]
        };
        for (input, base) in [("무작위 25%", &noisy), ("바탕색 하나", &solid)] {
            let few = {
                let mut v = base.clone();
                for x in 10..60 {
                    v[700 * w as usize + x] ^= 0x00ff_ff00;
                }
                v
            };
            let all: Vec<u32> = base.iter().map(|p| p ^ 0x0101_0101).collect();
            for (name, new) in [("바뀐 것 없음", base), ("한 줄 바뀜", &few), ("통째 바뀜", &all)] {
                // 옛 방식은 견주기 + 사본 갱신 + 0이 아닌 수 세기(옛 올리기 반복문이 하던 CPU 일)까지 잰다
                let mut shadows: Vec<Vec<u32>> = (0..21).map(|_| base.clone()).collect();
                let mut j = 0;
                let old = median(&mut || {
                    let shadow = &mut shadows[j];
                    j += 1;
                    let mut nz = 0usize;
                    for r in changed_rects(shadow, new, w, h) {
                        for y in r.y..r.y + r.h {
                            let at = (y * w + r.x) as usize;
                            let n = r.w as usize;
                            nz += shadow[at..at + n].iter().filter(|p| **p != 0).count();
                            nz += new[at..at + n].iter().filter(|p| **p != 0).count();
                            shadow[at..at + n].copy_from_slice(&new[at..at + n]);
                        }
                    }
                    std::hint::black_box(nz);
                });
                // 새 방식: base를 담은 상태를 미리 만들어 두고 new를 준다(상태 만들기는 재지 않는다)
                let mut states: Vec<TileHashes> = (0..21)
                    .map(|_| {
                        let mut s = TileHashes::zeroed(w, h);
                        s.update(base);
                        s
                    })
                    .collect();
                let mut i = 0;
                let tile = median(&mut || {
                    std::hint::black_box(states[i].update(new));
                    i += 1;
                });
                eprintln!("{input} {name}: 옛 {old:.3}ms, 칸 해시 {tile:.3}ms");
            }
        }
    }
}
