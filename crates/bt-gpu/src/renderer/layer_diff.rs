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

/// `prev`와 `new`(둘 다 `w`×`h`, 줄 순서)에서 달라진 곳을 덮는 사각형들. 같으면 비었다.
/// 길이가 다르거나 `w`·`h`와 맞지 않으면 통째 한 사각형이다
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
}
