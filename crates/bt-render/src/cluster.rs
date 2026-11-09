//! 이모지 묶음 찾기(#87). 한 줄의 칸들에서 한 글리프로 그릴 묶음을 찾는다(칸 폭은 엔진 그대로).
//! - 글자 묶음 폭(mode 2027, #180)이 켜져 있으면 VT 엔진이 묶음을 한 칸(넓으면 두 칸)에 담아 주므로,
//!   첫 칸의 결합 문자에 ZWJ·피부색·지역 표시 글자·이모지가 들어 있는 칸 하나가 곧 묶음이다.
//! - 꺼져 있으면 엔진은 ZWJ(U+200D)를 앞 칸의 결합 문자로 두고, 그 뒤 이모지와 피부색 수식자,
//!   국기의 지역 표시 글자를 각자의 칸에 둔다. 칸들을 이어 묶음을 찾는다.

/// 칸 하나(묶음을 찾는 데 필요한 것만)
#[derive(Debug, Clone, Copy)]
pub struct ClusterCell<'a> {
    pub ch: char,
    pub combining: &'a [char],
    /// 넓은 글자의 뒤 칸
    pub spacer: bool,
}

/// 찾은 묶음: (처음 칸, 차지한 칸 수, 묶음 글)
pub type Cluster = (usize, u16, String);

const ZWJ: char = '\u{200d}';

fn is_skin_tone(c: char) -> bool {
    matches!(c as u32, 0x1F3FB..=0x1F3FF)
}

fn is_regional(c: char) -> bool {
    matches!(c as u32, 0x1F1E6..=0x1F1FF)
}

/// 한 줄에서 묶음들을 찾는다. 칸 둘 이상을 잇는 것만
pub fn find_clusters(row: &[ClusterCell]) -> Vec<Cluster> {
    let next_glyph = |from: usize| (from..row.len()).find(|&k| !row[k].spacer);
    let mut out = Vec::new();
    let mut i = 0;
    while let Some(start) = next_glyph(i) {
        let mut text: String = std::iter::once(row[start].ch)
            .chain(row[start].combining.iter().copied())
            .collect();
        let mut last = start;
        let mut parts = 1;
        while let Some(k) = next_glyph(last + 1) {
            let next = row[k].ch;
            let joins = text.ends_with(ZWJ)
                || is_skin_tone(next)
                // 국기: 지역 표시 글자 둘
                || (parts == 1 && is_regional(row[start].ch) && is_regional(next));
            if !joins {
                break;
            }
            text.push(next);
            text.extend(row[k].combining.iter());
            last = k;
            parts += 1;
            if is_regional(next) {
                break;
            }
        }
        // 마지막 글자의 뒤 칸까지
        let end = next_glyph(last + 1).unwrap_or(row.len());
        // 엔진이 이미 한 칸에 담은 묶음(mode 2027)
        let packed = row[start]
            .combining
            .iter()
            .any(|&c| c == ZWJ || is_skin_tone(c) || is_regional(c) || (c as u32) >= 0x1F000);
        if parts > 1 || packed {
            out.push((start, (end - start) as u16, text));
        }
        i = end;
    }
    out
}

/// 화면의 줄마다 묶음을 찾아 첫 칸에 표시한다. `src(i)`는 i번째 칸(줄 우선)
pub fn mark<'a>(cells: &mut [crate::DrawCell], cols: usize, src: impl Fn(usize) -> ClusterCell<'a>) {
    if cols == 0 {
        return;
    }
    for r in 0..cells.len() / cols {
        let row: Vec<ClusterCell> = (0..cols).map(|c| src(r * cols + c)).collect();
        // 이모지가 없는 줄은 건너뛴다(대부분의 줄)
        if !row.iter().any(|c| (c.ch as u32) >= 0x1F000 || !c.combining.is_empty()) {
            continue;
        }
        for (start, n, text) in find_clusters(&row) {
            let cell = &mut cells[r * cols + start];
            cell.cluster = Some(text.into());
            cell.cluster_cells = n;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 글을 칸으로: 넓은 글자는 뒤 칸을 둔다, ZWJ·FE0F는 앞 칸의 결합 문자
    /// (글자, 결합 문자, 뒤 칸인가)
    type TestCell = (char, Vec<char>, bool);

    fn row(s: &str) -> Vec<TestCell> {
        let mut cells: Vec<TestCell> = Vec::new();
        for c in s.chars() {
            if c == ZWJ || c == '\u{fe0f}' {
                if let Some(last) = cells.iter_mut().rev().find(|c| !c.2) {
                    last.1.push(c);
                }
                continue;
            }
            let wide = (c as u32) >= 0x1F000;
            cells.push((c, Vec::new(), false));
            if wide {
                cells.push((' ', Vec::new(), true));
            }
        }
        cells
    }

    fn clusters(s: &str) -> Vec<Cluster> {
        let cells = row(s);
        let view: Vec<ClusterCell> = cells
            .iter()
            .map(|(ch, comb, sp)| ClusterCell {
                ch: *ch,
                combining: comb,
                spacer: *sp,
            })
            .collect();
        find_clusters(&view)
    }

    /// 엔진이 묶음을 한 칸(과 뒤 칸)에 담아 준 줄(mode 2027): 칸 폭이 곧 묶음 폭
    #[test]
    fn finds_clusters_packed_by_the_engine() {
        let cell = |ch: char, comb: &'static [char], spacer: bool| ClusterCell {
            ch,
            combining: comb,
            spacer,
        };
        let family: &[char] = &[ZWJ, '👩', ZWJ, '👧'];
        let row = [
            cell('x', &[], false),
            cell('👨', family, false),
            cell(' ', &[], true),
            cell('🇰', &['🇷'], false),
            cell(' ', &[], true),
            cell('👍', &['🏽'], false),
            cell(' ', &[], true),
            // 하트 + U+FE0F는 칸 하나가 이모지 글리프로 그리므로(`emoji`) 묶음이 아니다
            cell('❤', &['\u{fe0f}'], false),
            cell(' ', &[], true),
            cell('e', &['\u{301}'], false),
        ];
        assert_eq!(
            find_clusters(&row),
            vec![
                (1, 2, "👨\u{200d}👩\u{200d}👧".into()),
                (3, 2, "🇰🇷".into()),
                (5, 2, "👍🏽".into()),
            ]
        );
    }

    #[test]
    fn finds_zwj_skin_tone_and_flags() {
        assert!(clusters("a😀b").is_empty(), "홑 이모지는 묶음이 아니다");
        assert_eq!(clusters("👍🏽"), vec![(0, 4, "👍🏽".into())]);
        assert_eq!(clusters("x👨‍👩‍👧y"), vec![(1, 6, "👨\u{200d}👩\u{200d}👧".into())]);
        // 국기는 지역 표시 둘씩: 🇰🇷🇯🇵 → 둘
        let flags = clusters("🇰🇷🇯🇵");
        assert_eq!(flags.len(), 2);
        assert_eq!(flags[0].2, "🇰🇷");
        // 묶음 뒤 글자는 묶음이 아니다
        assert_eq!(clusters("👍🏽 ok").len(), 1);
    }
}
