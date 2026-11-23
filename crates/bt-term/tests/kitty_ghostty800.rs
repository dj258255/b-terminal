//! kitty 그림 셋(#801 그림 뒤 커서 이동, #799 화면 지우기·쫓아내기, #800 스크롤 영역)을 Ghostty 1.4.0과 견준다.
//! 바이트는 `Emulator::feed`(세션과 같은 스캐너·명령 처리·VT 엔진)로 먹이고, 화면에 보이는 그림(`Screen::images`)과 답으로 확인한다.
//!
//! 옮긴 Ghostty 시험(`src/terminal/kitty/`, 1.4.0):
//! - #801 `graphics_exec.zig`: `kittygfx placement bounds cursor movement for untrusted dimensions`(1938),
//!   `kittygfx placement moves cursor past a tall image`(1960), `display`의 `.after`(403)
//! - #799 `graphics_storage.zig`: `storage: erase display preserves scrollback and reclaims unplaced images`(3243),
//!   `storage: evicts images in priority order`(3768). `graphics_command.zig`: `transmission command with transient hint`(1245)
//! - #800 `graphics_storage.zig`: `storage: scroll margins placement inside region scrolls and clips at top`(4206),
//!   `... straddling region does not move`(4260), `... below region does not move`(4295), `... reverse index clips at bottom`(4328),
//!   `... scaled placement clips proportionally`(4368), `... left/right margins respect columns`(4405),
//!   `... insert/delete lines do not move placements`(4454), `storage: scroll without margins moves placement into scrollback`(4497),
//!   `storage: scroll margins large scroll deletes inside placement`(4532), `... straddling placement pin inside region restored`(4556),
//!   `... multi-line scroll up with scrollback`(4591)
//!
//! Ghostty 시험은 핀(`trackPin`)으로 자리를 잡고 `Terminal.index`·`scrollUp` 같은 함수를 부르지만, 여기서는 커서를 옮겨 그림을 놓고
//! 같은 일을 하는 바이트(IND `ESC D`, RI `ESC M`, SU `CSI S`, IL·DL)를 보낸다. 줄 번호는 보이는 화면(0부터) 기준이다.

use bt_core::Theme;
use bt_term::graphics::{Cursor, Graphics};
use bt_term::{Emulator, GridSize};

fn emu(cols: u16, rows: u16, cw: u16, ch: u16) -> Emulator {
    Emulator::new(
        GridSize {
            cols,
            rows,
            cell_width: cw,
            cell_height: ch,
        },
        Theme::default(),
    )
}

fn send(e: &mut Emulator, apc_body: &str) -> String {
    e.feed(format!("\x1b_G{apc_body}\x1b\\").as_bytes());
    String::from_utf8_lossy(&e.take_replies()).into_owned()
}

/// 칸 크기에 맞는 단색 RGB 그림 `w`x`h`(바이트는 모두 0)를 번호 `id`로 놓는다. 커서를 `(row, col)`(0부터)로 옮긴 뒤 놓고 커서는 안 옮긴다(`C=1`)
fn tile(e: &mut Emulator, id: u32, pid: u32, row: usize, col: usize, w: u32, h: u32) {
    e.feed(format!("\x1b[{};{}H", row + 1, col + 1).as_bytes());
    let px = "A".repeat((w * h * 4) as usize);
    let p = if pid == 0 { String::new() } else { format!(",p={pid}") };
    let r = send(e, &format!("a=T,t=d,f=24,i={id}{p},s={w},v={h},C=1,q=1;{px}"));
    assert_eq!(r, "", "q=1이라 답이 없다");
}

/// 보이는 그림 `(줄, 칸, 잘라내기 x y w h, 칸 수 cols rows)`. 놓인 차례
type View = (i32, i32, [u32; 4], (u32, u32));

fn views(e: &mut Emulator) -> Vec<View> {
    e.snapshot()
        .images
        .iter()
        .map(|i| (i.row, i.col, i.geom.src, (i.cols, i.rows)))
        .collect()
}

fn rows_of(e: &mut Emulator) -> Vec<i32> {
    views(e).iter().map(|v| v.0).collect()
}

// ───────────── #801: 그림 뒤 커서 이동 ─────────────

/// `kittygfx placement bounds cursor movement for untrusted dimensions`(graphics_exec.zig 1938):
/// 5x5 화면에 c·r이 u32 끝이어도 아래로 4줄 + 화면 한 높이(5줄)만 내려 스크롤백은 5줄이다(전체 10줄)
#[test]
fn cursor_movement_after_a_huge_placement_is_bounded_by_the_screen() {
    let mut e = emu(5, 5, 10, 20);
    let r = send(&mut e, "a=T,t=d,f=24,i=1,s=1,v=1,c=4294967295,r=4294967295;AAAA");
    assert_eq!(r, "\x1b_Gi=1;OK\x1b\\");
    assert_eq!(e.snapshot().history, 5, "Ghostty: 전체 10줄 = 스크롤백 5");
}

/// 같은 시험, 스크롤 영역 안: 영역 맨 아래까지 남은 줄 + 화면 높이. 영역 위가 맨 위가 아니면 스크롤백은 생기지 않는다
#[test]
fn cursor_movement_bound_counts_rows_left_in_the_scroll_region() {
    let mut e = emu(5, 5, 10, 20);
    e.feed(b"\x1b[2;3r\x1b[2;1H"); // 영역 2~3줄, 커서 (1,0)
    send(&mut e, "a=T,t=d,f=24,i=1,s=1,v=1,c=1,r=4294967295;AAAA");
    let s = e.snapshot();
    assert_eq!(s.history, 0, "맨 위가 아닌 영역의 스크롤은 스크롤백을 만들지 않는다");
    assert_eq!(
        (s.cursor.col, s.cursor.row),
        (1, 2),
        "영역 맨 아래 줄에 멈추고 그림 오른쪽 칸"
    );
}

/// 영역 밖(아래 여백 줄 아래)의 커서는 영역을 스크롤하지 않고 화면 맨 아래에서 멈춘다(Ghostty `index`)
#[test]
fn cursor_movement_outside_the_scroll_region_stops_at_the_screen_bottom() {
    let mut e = emu(5, 5, 10, 20);
    e.feed(b"\x1b[1;2r\x1b[4;1H");
    send(&mut e, "a=T,t=d,f=24,i=1,s=1,v=1,c=1,r=4294967295;AAAA");
    let s = e.snapshot();
    assert_eq!(s.history, 0);
    assert_eq!((s.cursor.col, s.cursor.row), (1, 4));
}

/// `kittygfx placement moves cursor past a tall image`(graphics_exec.zig 1960): 8줄 그림 다음 자리는 8줄 아래에서 시작한다
#[test]
fn a_placement_after_a_tall_image_starts_eight_rows_below() {
    let mut e = emu(5, 5, 10, 20);
    send(&mut e, "a=t,t=d,f=24,i=1,s=1,v=1;////");
    assert_eq!(send(&mut e, "a=p,i=1,p=1,c=5,r=8"), "\x1b_Gi=1,p=1;OK\x1b\\");
    assert_eq!(send(&mut e, "a=p,i=1,p=2,C=1"), "\x1b_Gi=1,p=2;OK\x1b\\");
    let s = e.snapshot();
    let mut ys: Vec<i64> = s.images.iter().map(|i| s.history as i64 + i64::from(i.row)).collect();
    ys.sort();
    assert_eq!(ys, vec![0, 8], "첫 자리는 줄 0, 둘째는 8줄 아래");
}

/// `display`의 `.after`(403): 그림이 오른쪽 끝에 닿으면(`x + c >= 열 수`) 한 줄 더 내려 열 0으로 감긴다. 아니면 그림 오른쪽 칸
#[test]
fn cursor_wraps_to_column_zero_when_the_placement_reaches_the_right_edge() {
    let mut e = emu(5, 5, 10, 20);
    send(&mut e, "a=T,t=d,f=24,i=1,s=1,v=1,c=5,r=1;AAAA");
    let c = e.snapshot().cursor;
    assert_eq!((c.col, c.row), (0, 1), "Ghostty: (0,1)");

    let mut e = emu(5, 5, 10, 20);
    send(&mut e, "a=T,t=d,f=24,i=1,s=1,v=1,c=3,r=1;AAAA");
    let c = e.snapshot().cursor;
    assert_eq!((c.col, c.row), (3, 0));

    // 오른쪽 끝에 닿은 줄바꿈 대기 상태도 푼다: 다음 글자는 0열에 찍힌다
    let mut e = emu(5, 5, 10, 20);
    send(&mut e, "a=T,t=d,f=24,i=1,s=1,v=1,c=5,r=1;AAAA");
    e.feed(b"x");
    let s = e.snapshot();
    assert_eq!(s.cell(0, 1).ch, 'x');
}

/// 줄 이동은 줄바꿈 모드(LNM)와 상관없이 index다: 칸은 `target_x`로 가고 0열로 돌아가지 않는다
#[test]
fn the_row_move_is_an_index_not_a_newline() {
    let mut e = emu(8, 5, 10, 20);
    e.feed(b"\x1b[20h"); // LNM: LF가 CR도 한다
    send(&mut e, "a=T,t=d,f=24,i=1,s=1,v=1,c=2,r=3;AAAA");
    let c = e.snapshot().cursor;
    assert_eq!((c.col, c.row), (2, 2));
}

// ───────────── #799: 화면 지우기와 쫓아내기 ─────────────

/// `erase display preserves scrollback and reclaims unplaced images`(graphics_storage.zig 3243):
/// ED 2는 보이는 자리의 그림을 지우고, 자리가 없는 그림 자료도 거둔다. 스크롤백에 다 들어간 자리의 그림만 남는다
#[test]
fn erase_display_reclaims_unplaced_images_and_keeps_history_placements() {
    let mut e = emu(10, 3, 10, 20);
    send(&mut e, "a=T,f=24,s=1,v=1,i=1,C=1;AAAA"); // 줄 0
    e.feed(b"\r\n\r\n\r\n\r\n\r\n"); // 그림 1이 스크롤백으로 올라감
    assert!(e.snapshot().history > 0);
    send(&mut e, "a=T,f=24,s=1,v=1,i=2,C=1;AAAA"); // 보이는 자리
    send(&mut e, "a=t,f=24,s=1,v=1,i=3;AAAA"); // 자리 없는 그림
    e.feed(b"\x1b[2J");
    assert_eq!(
        send(&mut e, "a=p,i=1,C=1"),
        "\x1b_Gi=1;OK\x1b\\",
        "스크롤백 자리의 그림은 남는다"
    );
    assert!(
        send(&mut e, "a=p,i=2").contains(";ENOENT"),
        "보이는 자리는 지우고 그림도 거둔다"
    );
    assert!(send(&mut e, "a=p,i=3").contains(";ENOENT"), "자리 없던 그림도 거둔다");
    // ED 3은 이 일과 따로다(ADR-010 결정 4): 주 화면의 자리를 모두 지우고 그림 자료는 남긴다
    e.feed(b"\x1b[3J");
    assert_eq!(send(&mut e, "a=p,i=1,C=1"), "\x1b_Gi=1;OK\x1b\\");
}

/// ED 22(`CSI 22 J`, scroll_complete)도 Ghostty는 `clearScreen`을 부른다(Terminal.zig 3716)
#[test]
fn erase_display_scroll_complete_reclaims_unplaced_images() {
    let mut e = emu(10, 5, 10, 20);
    e.feed(b"hello");
    send(&mut e, "a=t,f=24,s=1,v=1,i=3;AAAA");
    e.feed(b"\x1b[22J");
    assert!(send(&mut e, "a=p,i=3").contains(";ENOENT"));
}

/// 화면은 따로다: 주 화면의 ED 2는 대체 화면에 보낸 그림을, 대체 화면의 ED 2는 주 화면 그림을 거두지 않는다(Ghostty는 화면마다 저장소)
#[test]
fn erase_display_only_reclaims_images_of_its_own_screen() {
    let mut e = emu(10, 5, 10, 20);
    send(&mut e, "a=t,f=24,s=1,v=1,i=5;AAAA"); // 주 화면의 자리 없는 그림
    e.feed(b"\x1b[?1049h");
    send(&mut e, "a=t,f=24,s=1,v=1,i=6;AAAA"); // 대체 화면의 자리 없는 그림
    e.feed(b"\x1b[2J");
    assert!(send(&mut e, "a=p,i=6").contains(";ENOENT"), "대체 화면 그림은 거둔다");
    assert!(send(&mut e, "a=p,i=5").contains(";OK"), "주 화면 그림은 남는다");
}

const IMG: usize = 4 * 4 * 4 + 4096; // 4x4 RGBA 한 장이 저장 상한에서 차지하는 몫(고정 몫 포함)

const CUR: Cursor = Cursor {
    line: 100,
    col: 4,
    cell_w: 10,
    cell_h: 20,
    alt: false,
    top: 90,
    rows: 24,
    cols: 80,
};

fn b64(data: &[u8]) -> String {
    const T: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut s = String::new();
    for ch in data.chunks(3) {
        let n = (ch[0] as u32) << 16 | (*ch.get(1).unwrap_or(&0) as u32) << 8 | *ch.get(2).unwrap_or(&0) as u32;
        for i in 0..4 {
            if i <= ch.len() {
                s.push(T[(n >> (18 - 6 * i) & 63) as usize] as char);
            } else {
                s.push('=');
            }
        }
    }
    s
}

/// 그림 `id`를 `extra` 제어 키와 함께 보낸다. 세 장이 들어가는 상한에서 시작한다
fn put(g: &mut Graphics, id: u32, extra: &str) {
    let px = b64(&[7u8; 64]);
    let out = g.handle(format!("a=t,f=32,s=4,v=4,i={id}{extra};{px}").as_bytes(), CUR);
    assert!(out.reply.is_some(), "그림 {id}를 받았다");
}

fn show(g: &mut Graphics, id: u32) {
    g.handle(format!("a=p,i={id},C=1,q=1").as_bytes(), CUR);
}

fn alive(g: &Graphics, ids: &[u32]) -> Vec<u32> {
    ids.iter().copied().filter(|&i| g.image(i).is_some()).collect()
}

/// `storage: evicts images in priority order`(graphics_storage.zig 3768): 임시이고 안 쓰는 그림 → 안 쓰는 그림 →
/// 임시이고 쓰는 그림 → 쓰는 그림, 같은 급이면 오래된 것부터
#[test]
fn eviction_follows_ghosttys_priority_order() {
    // 1: 일반·안 씀, 2: 임시·씀, 3: 임시·안 씀
    let mut g = Graphics::with_limit(IMG * 3);
    put(&mut g, 1, "");
    put(&mut g, 2, ",N=1");
    show(&mut g, 2);
    put(&mut g, 3, ",N=1");
    assert_eq!(alive(&g, &[1, 2, 3]), vec![1, 2, 3]);

    put(&mut g, 4, ""); // 임시이고 안 쓰는 3이 먼저 나간다
    assert_eq!(
        alive(&g, &[1, 2, 3, 4]),
        vec![1, 2, 4],
        "임시이고 안 쓰는 그림 3이 먼저"
    );
    put(&mut g, 5, ""); // 안 쓰는 그림 가운데 오래된 1
    assert_eq!(alive(&g, &[1, 2, 4, 5]), vec![2, 4, 5], "안 쓰는 그림 중 오래된 1");
    put(&mut g, 6, ""); // 안 쓰는 4
    assert_eq!(alive(&g, &[2, 4, 5, 6]), vec![2, 5, 6]);
    put(&mut g, 7, ""); // 안 쓰는 5
    put(&mut g, 8, ""); // 안 쓰는 6, 쓰는 2는 마지막까지 남는다
    assert_eq!(alive(&g, &[2, 5, 6, 7, 8]), vec![2, 7, 8]);
}

/// 임시이고 쓰는 그림이 임시가 아니고 쓰는 그림보다 먼저 나간다. 쓰는 그림의 자리도 함께 간다
#[test]
fn eviction_prefers_transient_among_used_images_and_takes_its_placements() {
    let mut g = Graphics::with_limit(IMG * 2);
    put(&mut g, 1, ""); // 일반·씀(오래됨)
    show(&mut g, 1);
    put(&mut g, 2, ",N=1"); // 임시·씀(새로움)
    show(&mut g, 2);
    assert_eq!(g.placements.len(), 2);
    put(&mut g, 3, ""); // 임시인 2가 먼저
    assert_eq!(alive(&g, &[1, 2, 3]), vec![1, 3]);
    assert!(
        g.placements.iter().all(|p| p.image == 1),
        "나간 그림의 자리도 같이 간다"
    );
}

/// `transmission command with transient hint`(graphics_command.zig 1245): `N`의 맨 아래 비트가 임시다. `N=2`는 임시가 아니다
#[test]
fn the_transient_hint_is_the_low_bit_of_n() {
    let mut g = Graphics::with_limit(IMG * 2);
    put(&mut g, 1, ",N=2"); // 일반
    put(&mut g, 2, ",N=3"); // 임시(맨 아래 비트)
    put(&mut g, 3, "");
    assert_eq!(
        alive(&g, &[1, 2, 3]),
        vec![1, 3],
        "N=3은 임시라 먼저, N=2는 아니라 남는다"
    );
}

/// 안 쓰는 그림이 쓰는 그림보다 먼저 나간다(오래돼도 쓰는 그림이 남는다)
#[test]
fn eviction_prefers_unused_images_over_older_used_ones() {
    let mut g = Graphics::with_limit(IMG * 2);
    put(&mut g, 1, "");
    show(&mut g, 1);
    put(&mut g, 2, "");
    put(&mut g, 3, "");
    assert_eq!(alive(&g, &[1, 2, 3]), vec![1, 3]);
}

// ───────────── #800: 스크롤 영역 ─────────────

fn scroll_emu() -> Emulator {
    emu(10, 6, 10, 10) // width_px 100, height_px 60: 칸 10x10
}

/// `scroll margins placement inside region scrolls and clips at top`(4206):
/// 영역 안의 그림은 글과 함께 올라가고, 위 여백을 넘으면 위쪽 한 줄이 잘리고, 다 나가면 지운다(그림 자료는 남는다)
#[test]
fn scroll_margins_inside_region_scrolls_and_clips_at_top() {
    let mut e = scroll_emu();
    tile(&mut e, 1, 1, 2, 0, 10, 20); // 2줄 그림이 줄 2~3
    e.feed(b"\x1b[2;5r\x1b[5;1H"); // 영역 1~4줄, 커서는 맨 아래 줄
    assert_eq!(views(&mut e)[0].0, 2);

    e.feed(b"\x1bD");
    assert_eq!(
        views(&mut e),
        vec![(1, 0, [0, 0, 0, 0], (1, 2))],
        "글과 함께 한 줄 위로"
    );

    e.feed(b"\x1bD");
    assert_eq!(
        views(&mut e),
        vec![(1, 0, [0, 10, 10, 10], (1, 1))],
        "위 여백에 붙고 위 한 줄이 잘림: source_y 10, source_height 10"
    );

    e.feed(b"\x1bD");
    assert!(views(&mut e).is_empty(), "다 잘리면 자리를 지운다");
    assert!(send(&mut e, "a=p,i=1,C=1").contains(";OK"), "그림 자료는 남는다");
}

/// `placement straddling region does not move`(4260): 영역에 다 들어가지 않는 그림은 영역 스크롤에 움직이지 않는다(스크롤백이 생기는 길)
#[test]
fn scroll_margins_straddling_placement_does_not_move() {
    let mut e = scroll_emu();
    tile(&mut e, 1, 1, 0, 0, 10, 30); // 3줄 그림이 줄 0~2, 영역은 1~2줄
    e.feed(b"\x1b[1;2r\x1b[2;1H\x1bD");
    assert_eq!(views(&mut e), vec![(0, 0, [0, 0, 0, 0], (1, 3))]);
}

/// `placement below region does not move`(4295): 영역 아래(상태줄)의 그림은 제자리다(스크롤백이 생기는 길)
#[test]
fn scroll_margins_placement_below_region_does_not_move() {
    let mut e = scroll_emu();
    tile(&mut e, 1, 1, 5, 0, 10, 10);
    e.feed(b"\x1b[1;4r\x1b[4;1H\x1bD\x1bD");
    assert_eq!(rows_of(&mut e), vec![5]);
    assert_eq!(e.snapshot().history, 2, "스크롤백은 쌓인다");
}

/// `reverse index clips at bottom`(4328): 영역 맨 위에서 RI를 하면 그림이 글과 함께 내려가고, 아래 여백에서 잘리고, 다 나가면 지운다
#[test]
fn scroll_margins_reverse_index_clips_at_bottom() {
    let mut e = scroll_emu();
    tile(&mut e, 1, 1, 3, 0, 10, 20); // 줄 3~4
    e.feed(b"\x1b[2;5r\x1b[2;1H"); // 영역 1~4줄, 커서는 맨 위 줄
    e.feed(b"\x1bM");
    assert_eq!(
        views(&mut e),
        vec![(4, 0, [0, 0, 10, 10], (1, 1))],
        "한 줄 아래, 아래 한 줄이 잘림"
    );
    e.feed(b"\x1bM");
    assert!(views(&mut e).is_empty(), "다 잘리면 지운다");
}

/// `scaled placement clips proportionally`(4368): 칸 수를 정한(`c r`) 그림은 칸 비율로 잘린다
#[test]
fn scroll_margins_scaled_placement_clips_proportionally() {
    let mut e = scroll_emu();
    e.feed(b"\x1b[2;1H");
    let px = "A".repeat(10 * 40 * 4);
    send(&mut e, &format!("a=T,t=d,f=24,i=1,p=1,s=10,v=40,c=1,r=2,C=1,q=1;{px}"));
    e.feed(b"\x1b[2;3r\x1b[3;1H\x1bD");
    assert_eq!(
        views(&mut e),
        vec![(1, 0, [0, 20, 10, 20], (1, 1))],
        "source_y 20, source_height 20, 한 줄·한 칸"
    );
}

/// `left/right margins respect columns`(4405): 좌우 여백 밖 칸의 그림은 안 움직이고 안쪽 그림만 올라간다
#[test]
fn scroll_margins_left_right_margins_respect_columns() {
    let mut e = scroll_emu();
    tile(&mut e, 1, 1, 2, 0, 10, 10);
    tile(&mut e, 2, 2, 2, 2, 10, 10); // 같은 번호로 다시 보내면 옛 그림의 자리가 지워지므로 다른 그림 번호
    e.feed(b"\x1b[?69h\x1b[2;8s\x1b[S"); // DECLRMM, 좌우 여백 2~8칸, SU
    let mut v: Vec<(i32, i32)> = views(&mut e).iter().map(|v| (v.0, v.1)).collect();
    v.sort_by_key(|x| x.1);
    assert_eq!(v, vec![(2, 0), (1, 2)], "여백 밖(0칸)은 제자리, 안(2칸)은 한 줄 위");
}

/// `insert/delete lines do not move placements`(4454): IL·DL은 그림을 옮기지 않는다
#[test]
fn scroll_margins_insert_and_delete_lines_do_not_move_placements() {
    let mut e = scroll_emu();
    tile(&mut e, 1, 1, 2, 0, 10, 10);
    e.feed(b"\x1b[2;5r\x1b[2;1H\x1b[M");
    assert_eq!(rows_of(&mut e), vec![2]);
    e.feed(b"\x1b[L");
    assert_eq!(rows_of(&mut e), vec![2]);
}

/// `scroll without margins moves placement into scrollback`(4497): 영역이 없으면 그림은 줄을 따라 스크롤백으로 간다
#[test]
fn scroll_without_margins_moves_placement_into_scrollback() {
    let mut e = scroll_emu();
    tile(&mut e, 1, 1, 0, 0, 10, 20);
    e.feed(b"\x1b[6;1H\x1bD");
    assert_eq!(rows_of(&mut e), vec![-1]);
}

/// `large scroll deletes inside placement`(4532): 영역 높이를 넘는 SU는 안에 든 그림을 지운다
#[test]
fn scroll_margins_large_scroll_deletes_inside_placement() {
    let mut e = scroll_emu();
    tile(&mut e, 1, 1, 2, 0, 10, 10);
    e.feed(b"\x1b[2;5r\x1b[10S");
    assert!(views(&mut e).is_empty());
}

/// `straddling placement pin inside region restored`(4556): 앵커는 영역 안이지만 아래로 삐져나온 그림은 영역 스크롤에 움직이지 않는다
#[test]
fn scroll_margins_straddling_placement_with_anchor_inside_is_restored() {
    let mut e = scroll_emu();
    tile(&mut e, 1, 1, 4, 0, 10, 20); // 줄 4~5, 영역은 1~4줄
    e.feed(b"\x1b[2;5r\x1b[5;1H\x1bD");
    assert_eq!(views(&mut e), vec![(4, 0, [0, 0, 0, 0], (1, 2))]);
}

/// `multi-line scroll up with scrollback`(4591): 맨 위에서 시작하는 영역의 SU(스크롤백이 생기는 길)에서도 그림은 영역 안에서 움직인다
#[test]
fn scroll_margins_multi_line_scroll_up_with_scrollback() {
    let mut e = scroll_emu();
    tile(&mut e, 1, 1, 2, 0, 10, 10);
    e.feed(b"\x1b[1;4r\x1b[2S");
    assert_eq!(rows_of(&mut e), vec![0]);
    assert_eq!(e.snapshot().history, 2);
    e.feed(b"\x1b[2S");
    assert!(views(&mut e).is_empty(), "영역 밖으로 나가면 지운다");
}

// ───────────── 위 시험에 없는 경계 ─────────────

/// 스크롤백에 있는 그림과 영역 밖의 칸은 영역 스크롤에 움직이지 않는다
#[test]
fn scroll_margins_leave_scrollback_placements_alone() {
    let mut e = scroll_emu();
    tile(&mut e, 1, 1, 0, 0, 10, 30); // 3줄 그림
    e.feed(b"\x1b[6;1H\n\n"); // 두 줄이 스크롤백으로 올라가 맨 아래 한 줄만 보인다
    let hist = e.snapshot().history as i32;
    assert_eq!(rows_of(&mut e), vec![-2]);
    e.feed(b"\x1b[2;5r\x1b[5;1H\x1bD\x1bD"); // 영역만 스크롤
    assert_eq!(rows_of(&mut e), vec![-2], "스크롤백(history {hist})의 그림은 제자리");
}

/// 대체 화면의 영역 스크롤은 주 화면의 그림을 건드리지 않는다
#[test]
fn scroll_margins_on_the_alternate_screen_do_not_touch_main_screen_placements() {
    let mut e = scroll_emu();
    tile(&mut e, 1, 1, 2, 0, 10, 10);
    e.feed(b"\x1b[?1049h\x1b[2;5r\x1b[5;1H\x1bD\x1bD\x1b[?1049l");
    assert_eq!(rows_of(&mut e), vec![2]);
}

/// 좌우 여백이 있는 영역 스크롤에서 IND(맨 아래 줄)도 그림을 옮긴다(Ghostty `index`가 `scrollUp(1)`을 부른다)
#[test]
fn scroll_margins_index_with_left_right_margins_moves_inside_placements() {
    let mut e = scroll_emu();
    tile(&mut e, 1, 1, 2, 3, 10, 10);
    e.feed(b"\x1b[?69h\x1b[2;8s\x1b[2;5r\x1b[5;3H\x1bD");
    assert_eq!(rows_of(&mut e), vec![1]);
}

/// (Ghostty에 없는 우리 상한) 한 번의 반영 사이에 스크롤 기록이 상한(131072)을 넘으면 그 화면의 줄에 붙은 자리를 모두 지운다. 패닉 없이 끝나고
/// 상한 안의 스크롤은 그대로 반영된다(위 시험들)
#[test]
fn scroll_log_overflow_drops_pinned_placements_instead_of_guessing() {
    let mut e = scroll_emu();
    tile(&mut e, 1, 1, 5, 0, 10, 10); // 영역 아래의 그림(제자리여야 하는 자리)
    e.feed(b"\x1b[1;4r\x1b[4;1H");
    // 반영 없이 한 번에 넣는다: `Emulator::feed`는 끝에서만 반영한다
    let flood = "\x1bD".repeat(140_000);
    e.feed(flood.as_bytes());
    assert!(views(&mut e).is_empty(), "기록을 잃으면 자리를 믿을 수 없어 지운다");
    // 그림 자료는 남아 있고, 그 뒤에는 다시 정상으로 쌓는다
    e.feed(b"\x1b[6;1H");
    assert!(send(&mut e, "a=p,i=1,C=1,p=9").contains(";OK"));
    e.feed(b"\x1b[1;4r\x1b[4;1H\x1bD");
    assert_eq!(rows_of(&mut e), vec![5]);
}

// ───────────── 세션(PTY, 읽기 스레드) 길 ─────────────
// 위 시험은 `Emulator::feed`로 같은 처리를 본다. 앱이 쓰는 읽기 스레드도 같은 결과를 내는지 `/bin/sh`와 `printf`로 본다
// (사용자 셸 설정이 결과를 바꾸지 않게 한다)

mod pty {
    use std::sync::Arc;
    use std::time::{Duration, Instant};

    use bt_core::Theme;
    use bt_term::{GridSize, Session, SessionEvent, SpawnOptions};

    fn run(cols: u16, rows: u16, script: String) -> Session {
        let o = SpawnOptions {
            program: Some("/bin/sh".into()),
            args: vec!["-c".into(), format!("{script}; sleep 5"), "sh".into()],
            cwd: Some(std::env::temp_dir()),
            ..Default::default()
        };
        let size = GridSize {
            cols,
            rows,
            cell_width: 10,
            cell_height: 10,
        };
        let sink = Arc::new(|_: SessionEvent| {});
        let s = Session::spawn(o, size, Theme::default(), sink).unwrap_or_else(|e| panic!("spawn: {e:#}"));
        let deadline = Instant::now() + Duration::from_secs(60);
        while !s.snapshot().text().contains("READY") {
            assert!(Instant::now() < deadline, "출력이 끝나지 않았다");
            std::thread::sleep(Duration::from_millis(5));
        }
        s
    }

    /// #801: 읽기 스레드 길에서도 줄 수 상한이 걸린다(5x5 화면, 스크롤백 5줄)
    #[test]
    fn session_bounds_the_cursor_movement_after_a_huge_placement() {
        let s = run(
            5,
            5,
            "printf '\\033_Ga=T,t=d,f=24,i=1,s=1,v=1,c=4294967295,r=4294967295,q=1;AAAA\\033\\\\'; printf READY".into(),
        );
        assert_eq!(s.snapshot().history, 5);
    }

    /// #800: 영역 안 IND가 그림을 글과 함께 올린다. 영역 아래 그림은 제자리다
    #[test]
    fn session_moves_placements_with_the_scroll_region() {
        let px = "A".repeat(10 * 10 * 4);
        let script = format!(
            "printf '\\033[3;1H\\033_Ga=T,t=d,f=24,i=1,s=10,v=10,C=1,q=1;{px}\\033\\\\'; \
             printf '\\033[6;1H\\033_Ga=T,t=d,f=24,i=2,s=10,v=10,C=1,q=1;{px}\\033\\\\'; \
             printf '\\033[2;5r\\033[5;1H\\033D'; printf READY"
        );
        let s = run(10, 6, script);
        let mut rows: Vec<(u32, i32)> = Vec::new();
        for img in s.snapshot().images {
            rows.push((img.width, img.row));
        }
        rows.sort();
        assert_eq!(
            rows,
            vec![(10, 1), (10, 5)],
            "영역 안 그림은 한 줄 위로, 영역 아래 그림은 제자리"
        );
    }
}
