//! 아틀라스 채우기·넘침(GPU 없이)

use bt_gpu::{Atlas, Full};

fn tile(w: u32, h: u32, v: u8) -> Vec<u8> {
    vec![v; (w * h) as usize]
}

#[test]
fn inserted_tiles_land_in_the_buffer_and_do_not_overlap() {
    let mut a = Atlas::new(32, 1, 32);
    let p1 = a.insert(10, 8, &tile(10, 8, 1)).unwrap();
    let p2 = a.insert(10, 8, &tile(10, 8, 2)).unwrap();
    let p3 = a.insert(20, 12, &tile(20, 12, 3)).unwrap();
    // 놓은 자리의 픽셀이 그 조각의 값이다
    let at = |a: &Atlas, (x, y): (u32, u32), dx: u32, dy: u32| a.data()[((y + dy) * a.size() + x + dx) as usize];
    assert_eq!(at(&a, p1, 9, 7), 1);
    assert_eq!(at(&a, p2, 0, 0), 2);
    assert_eq!(at(&a, p3, 19, 11), 3);
    // 첫 조각은 둘째 조각에 덮이지 않았다
    assert_eq!(at(&a, p1, 0, 0), 1);
    let rects = [(p1, 10, 8), (p2, 10, 8), (p3, 20, 12)];
    for (i, ((x1, y1), w1, h1)) in rects.iter().enumerate() {
        for ((x2, y2), w2, h2) in rects.iter().skip(i + 1) {
            let apart = x1 + w1 <= *x2 || x2 + w2 <= *x1 || y1 + h1 <= *y2 || y2 + h2 <= *y1;
            assert!(apart, "({x1},{y1}) {w1}x{h1} 와 ({x2},{y2}) {w2}x{h2} 가 겹친다");
        }
    }
}

#[test]
fn full_atlas_reports_full_until_it_grows() {
    let mut a = Atlas::new(16, 1, 64);
    let mut n = 0;
    while a.insert(8, 8, &tile(8, 8, 9)).is_ok() {
        n += 1;
        assert!(n <= 4, "16x16에 8x8은 네 개까지");
    }
    assert_eq!(n, 4);
    assert_eq!(a.insert(8, 8, &tile(8, 8, 9)), Err(Full));
    assert!(a.grow());
    assert_eq!(a.size(), 32);
    assert_eq!(a.grows(), 1);
    assert!(a.insert(8, 8, &tile(8, 8, 9)).is_ok(), "키운 뒤에는 들어간다");
}

#[test]
fn lazy_atlas_stays_tiny_until_the_first_piece_then_jumps_to_its_first_size() {
    let mut a = Atlas::lazy(64, 4, 256);
    assert_eq!((a.size(), a.data().len()), (1, 4), "쓰기 전에는 1x1");
    assert_eq!(a.insert(8, 8, &[9; 8 * 8 * 4]), Err(Full));
    // 첫 잡기는 한 번에 처음 크기로 가고, 키운 횟수에 세지 않는다
    let p = a.insert_growing(8, 8, &[9; 8 * 8 * 4]).unwrap();
    assert_eq!((a.size(), a.grows(), a.data().len()), (64, 0, 64 * 64 * 4));
    assert_eq!(a.data()[(p.1 * 64 + p.0) as usize * 4], 9);
    // 그 뒤에는 두 배씩
    assert!(a.grow());
    assert_eq!((a.size(), a.grows()), (128, 1));
    // 비운 아틀라스는 같은 크기에서 다시 시작한다
    let e = a.emptied();
    assert_eq!((e.size(), e.grows()), (128, 0));
    // 아직 쓰지 않은 지연 아틀라스를 비우면 여전히 지연이다
    let mut l = Atlas::lazy(64, 4, 256).emptied();
    assert_eq!(l.size(), 1);
    assert!(l.insert_growing(8, 8, &[1; 8 * 8 * 4]).is_some());
    assert_eq!(l.size(), 64);
}

#[test]
fn growing_keeps_existing_tiles_where_they_were() {
    let mut a = Atlas::new(16, 1, 64);
    let p = a.insert(8, 8, &tile(8, 8, 7)).unwrap();
    let q = a.insert(8, 8, &tile(8, 8, 5)).unwrap();
    assert!(a.grow());
    let at = |(x, y): (u32, u32)| a.data()[(y * a.size() + x) as usize];
    assert_eq!(at(p), 7);
    assert_eq!(at(q), 5);
    // 자리가 겹치지 않은 새 조각
    let r = a.insert(8, 8, &tile(8, 8, 3)).unwrap();
    assert_ne!(r, p);
    assert_ne!(r, q);
}

#[test]
fn insert_growing_stops_at_the_limit() {
    let mut a = Atlas::new(8, 1, 16);
    let mut placed = 0;
    for _ in 0..10 {
        if a.insert_growing(8, 8, &tile(8, 8, 1)).is_some() {
            placed += 1;
        }
    }
    assert_eq!(placed, 4, "16x16 한도에는 8x8이 네 개");
    assert_eq!(a.size(), 16);
    assert!(a.grows() >= 1);
}

#[test]
fn four_byte_tiles_keep_their_channels() {
    let mut a = Atlas::new(8, 4, 8);
    let px: Vec<u8> = (0..2 * 2).flat_map(|i| [i as u8, 10, 20, 30]).collect();
    let (x, y) = a.insert(2, 2, &px).unwrap();
    let off = ((y * 8 + x) * 4) as usize;
    assert_eq!(&a.data()[off..off + 8], &px[..8]);
    // 둘째 줄은 아틀라스 폭만큼 아래
    let off2 = (((y + 1) * 8 + x) * 4) as usize;
    assert_eq!(&a.data()[off2..off2 + 8], &px[8..16]);
}
