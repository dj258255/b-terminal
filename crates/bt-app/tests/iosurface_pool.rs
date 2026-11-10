//! #346: IOSurface 올리기의 순수 논리(묶음 고르기, 줄 복사, 알파 채우기). 창 없이 softbuffer의 공개 함수를 부른다.
#![cfg(target_os = "macos")]

use softbuffer::iosurface_pool::*;

fn slot(w: usize, h: usize, space: u64, in_use: bool) -> SlotInfo {
    SlotInfo {
        width: w,
        height: h,
        space,
        in_use,
    }
}

#[test]
fn pick_reuses_first_free_matching_slot() {
    let slots = [slot(10, 4, 1, true), slot(10, 4, 1, false), slot(10, 4, 1, false)];
    assert_eq!(pick_slot(&slots, 10, 4, 1), Pick::Reuse(1));
}

#[test]
fn pick_skips_in_use_and_creates_when_all_busy() {
    let slots = [slot(10, 4, 1, true), slot(10, 4, 1, true)];
    assert_eq!(pick_slot(&slots, 10, 4, 1), Pick::New);
    assert_eq!(pick_slot(&[], 10, 4, 1), Pick::New);
}

#[test]
fn pick_never_reuses_other_size_or_color_space() {
    let slots = [slot(10, 5, 1, false), slot(11, 4, 1, false), slot(10, 4, 2, false)];
    assert_eq!(pick_slot(&slots, 10, 4, 1), Pick::New);
}

#[test]
fn size_or_color_space_change_empties_pool() {
    let slots = [slot(10, 4, 1, false), slot(10, 4, 1, true)];
    assert_eq!(retain_mask(&slots, 10, 4, 1), vec![true, true]);
    assert_eq!(retain_mask(&slots, 12, 4, 1), vec![false, false]);
    assert_eq!(retain_mask(&slots, 10, 4, 2), vec![false, false]);
    let mixed = [slot(10, 4, 1, false), slot(8, 4, 1, false)];
    assert_eq!(retain_mask(&mixed, 10, 4, 1), vec![true, false]);
}

#[test]
fn pool_is_capped_at_three() {
    assert_eq!(MAX_SLOTS, 3);
    assert_eq!(evict_for_new(0), 0);
    assert_eq!(evict_for_new(2), 0);
    assert_eq!(evict_for_new(3), 1);
    assert_eq!(evict_for_new(4), 2);
}

#[test]
fn color_space_key_differs_per_display_only_when_native() {
    assert_ne!(color_space_key(true, 1), color_space_key(true, 2));
    assert_eq!(color_space_key(false, 1), color_space_key(false, 2));
    assert_ne!(color_space_key(true, 1), color_space_key(false, 1));
}

#[test]
fn copy_respects_bytes_per_row_padding() {
    // 폭 3, 높이 2, 줄 간격 16바이트(폭×4 = 12, 덧칸 4)
    let src = [
        0x0011_2233,
        0x0044_5566,
        0x0077_8899,
        0x00aa_bbcc,
        0x00dd_eeff,
        0x0001_0203,
    ];
    let mut dst = vec![0xeeu8; 32];
    assert!(copy_frame(&mut dst, 16, &src, 3, 2, false));
    // 첫 픽셀 0x00112233은 little-endian으로 33 22 11 00(B G R A)
    assert_eq!(&dst[0..4], &[0x33, 0x22, 0x11, 0x00]);
    assert_eq!(&dst[8..12], &[0x99, 0x88, 0x77, 0x00]);
    assert_eq!(&dst[12..16], &[0xee; 4], "줄 끝 덧칸은 그대로");
    assert_eq!(
        &dst[16..20],
        &[0xcc, 0xbb, 0xaa, 0x00],
        "둘째 줄은 bytes_per_row 뒤에서 시작"
    );
    assert_eq!(&dst[24..28], &[0x03, 0x02, 0x01, 0x00]);
    assert_eq!(&dst[28..32], &[0xee; 4]);
}

#[test]
fn copy_fills_alpha_for_opaque_frames() {
    let src = [0x0011_2233, 0x0000_0000];
    let mut dst = vec![0u8; 8];
    assert!(copy_frame(&mut dst, 8, &src, 2, 1, true));
    assert_eq!(&dst[0..4], &[0x33, 0x22, 0x11, 0xff]);
    assert_eq!(&dst[4..8], &[0x00, 0x00, 0x00, 0xff]);
}

#[test]
fn copy_keeps_premultiplied_alpha_for_transparent_frames() {
    let src = [0x8008_1018, 0x0000_0000];
    let mut dst = vec![0u8; 8];
    assert!(copy_frame(&mut dst, 8, &src, 2, 1, false));
    assert_eq!(&dst[0..4], &[0x18, 0x10, 0x08, 0x80]);
    assert_eq!(&dst[4..8], &[0, 0, 0, 0], "알파 0은 그대로 완전 투명");
}

#[test]
fn copy_refuses_bad_sizes_without_writing() {
    let src = [1u32; 6];
    let mut small = vec![0u8; 20];
    assert!(!copy_frame(&mut small, 16, &src, 3, 2, false), "목적지가 모자람");
    assert_eq!(small, vec![0u8; 20]);
    let mut dst = vec![0u8; 32];
    assert!(
        !copy_frame(&mut dst, 8, &src, 3, 2, false),
        "bytes_per_row가 폭×4보다 작음"
    );
    assert!(!copy_frame(&mut dst, 16, &src[..5], 3, 2, false), "원본이 모자람");
    assert_eq!(dst, vec![0u8; 32]);
}
