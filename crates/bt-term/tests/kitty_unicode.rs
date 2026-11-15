//! kitty 그래픽 유니코드 자리표시(U=1)와 상대 배치(P Q H V, #553): 바이트를 VT에 먹이고 화면 값의 그림으로 확인한다.
//! Ghostty `graphics_unicode.zig`·`renderer/image.zig`의 같은 이름 시험을 우리 자료 꼴로 옮겼다

use bt_core::Theme;
use bt_term::{Emulator, GridSize, ImageView};

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

/// 모든 바이트가 0인 RGB 그림(base64는 `A`만 이어진다)을 번호 `id`로 보낸다
fn send_blank(e: &mut Emulator, id: u32, w: u32, h: u32) {
    let n = (w * h * 3) as usize;
    assert_eq!(n % 3, 0);
    let b64 = "A".repeat(n / 3 * 4);
    // 4096바이트씩 조각으로 보낸다
    let chunks: Vec<&str> = b64
        .as_bytes()
        .chunks(4096)
        .map(|c| std::str::from_utf8(c).unwrap())
        .collect();
    let last = chunks.len() - 1;
    for (i, c) in chunks.iter().enumerate() {
        let m = u8::from(i != last);
        if i == 0 {
            e.feed(format!("\x1b_Ga=t,f=24,s={w},v={h},i={id},q=1,m={m};{c}\x1b\\").as_bytes());
        } else {
            e.feed(format!("\x1b_Gm={m},q=1;{c}\x1b\\").as_bytes());
        }
    }
    assert!(e.take_replies().is_empty());
}

/// 줄·칸 결합 문자(Ghostty 표의 앞쪽 값): 0 -> U+0305, 1 -> U+030D, 2 -> U+030E, 3 -> U+0310
const DIAC: [char; 4] = ['\u{0305}', '\u{030D}', '\u{030E}', '\u{0310}'];

fn cell(row: usize, col: usize) -> String {
    format!("\u{10EEEE}{}{}", DIAC[row], DIAC[col])
}

fn pieces(e: &mut Emulator) -> Vec<ImageView> {
    e.snapshot().images
}

/// 줄마다 `cols`칸의 자리표시 글자를 `fg` 팔레트색으로 찍는다(줄 `r`의 `c`째 칸에 줄 `r`·칸 `c` 결합 문자)
fn placeholder_grid(rows: usize, cols: usize, fg: u8) -> String {
    let mut text = format!("\x1b[38;5;{fg}m");
    for r in 0..rows {
        for c in 0..cols {
            text += &cell(r, c);
        }
        text += &format!("\x1b[39m\r\n\x1b[38;5;{fg}m");
    }
    text
}

/// 조각이 `cw x ch` 칸에서 그릴 곳: (칸 안 여백, 그릴 크기, 그림에서 쓰는 부분)
fn resolved(p: &ImageView, cw: u32, ch: u32) -> ((u32, u32), (u32, u32), [u32; 4]) {
    let r = p.geom.resolve(cw, ch);
    ((r.off_x, r.off_y), (r.dest_w, r.dest_h), r.src)
}

/// Ghostty "unicode render placement: dog 4x2": 500x306 그림, 칸 36x80, 4칸 2줄 가상 배치, 글자색 팔레트 1
#[test]
fn dog_4x2_splits_into_one_piece_per_row() {
    let mut e = emu(20, 6, 36, 80);
    send_blank(&mut e, 1, 500, 306);
    e.feed(b"\x1b_Ga=p,i=1,U=1,q=2,c=4,r=2\x1b\\");
    assert!(e.take_replies().is_empty(), "q=2는 답하지 않는다");
    e.feed(placeholder_grid(2, 4, 1).as_bytes());
    let v = pieces(&mut e);
    assert_eq!(v.len(), 2, "줄마다 한 조각");
    assert!(v.iter().all(|p| p.z == -1 && p.col == 0 && p.cols == 4));
    assert_eq!((v[0].row, v[1].row), (0, 1));
    assert_eq!(resolved(&v[0], 36, 80), ((0, 36), (144, 44), [0, 0, 500, 153]));
    assert_eq!(resolved(&v[1], 36, 80), ((0, 0), (144, 44), [0, 153, 500, 153]));
}

/// 가상 배치는 커서를 옮기지 않고 자리가 없다. 답은 보통 놓기와 같다(`C`와 상관없이)
#[test]
fn virtual_placement_replies_without_moving_the_cursor() {
    let mut e = emu(20, 6, 10, 20);
    send_blank(&mut e, 7, 30, 40);
    e.feed(b"\x1b[3;5H\x1b_Ga=p,i=7,p=3,U=1,c=3,r=2\x1b\\");
    assert_eq!(e.take_replies(), b"\x1b_Gi=7,p=3;OK\x1b\\");
    let s = e.snapshot();
    assert_eq!((s.cursor.row, s.cursor.col), (2, 4), "커서는 그대로");
    assert!(s.images.is_empty(), "자리표시 글자가 없으면 그릴 것이 없다");
    // U=2도 가상 배치다(kitty는 bool로 저장한다)
    e.feed(b"\x1b_Ga=T,f=24,s=1,v=1,i=8,U=2;AAAA\x1b\\");
    assert_eq!(e.take_replies(), b"\x1b_Gi=8;OK\x1b\\");
    let s = e.snapshot();
    assert_eq!((s.cursor.row, s.cursor.col), (2, 4));
    assert!(s.images.is_empty());
    // 가상 배치는 부모를 가질 수 없다. 그림이 없어도 이 오류가 먼저다
    e.feed(b"\x1b_Ga=p,i=99,U=1,P=7\x1b\\");
    assert_eq!(e.take_replies(), b"\x1b_Gi=99;EINVAL\x1b\\");
}

/// 자리표시 글자는 글자로 그리지 않고(칸 값에 표시), 화면 읽기에는 글자가 그대로 남는다
#[test]
fn placeholder_cells_are_flagged_and_keep_their_text() {
    let mut e = emu(20, 6, 10, 20);
    send_blank(&mut e, 1, 20, 20);
    e.feed(b"\x1b_Ga=p,i=1,U=1,q=2,c=2,r=1\x1b\\");
    e.feed(format!("A\x1b[38;5;1m{}{}\x1b[39mB", cell(0, 0), cell(0, 1)).as_bytes());
    let s = e.snapshot();
    assert_eq!(
        s.row_text(0),
        format!("A{}{}B", cell(0, 0), cell(0, 1)),
        "복사·화면 읽기는 글자 그대로"
    );
    let flagged: Vec<bool> = (0..4)
        .map(|c| s.cell(c, 0).flags.contains(bt_term::CellFlags::PLACEHOLDER))
        .collect();
    assert_eq!(flagged, [false, true, true, false]);
    assert_eq!((s.cell(1, 0).draw_ch(), s.cell(1, 0).draw_combining().len()), (' ', 0));
    assert_eq!(s.cell(0, 0).draw_ch(), 'A');
    assert_eq!(s.images.len(), 1, "두 칸은 한 조각");
    assert_eq!((s.images[0].col, s.images[0].cols), (1, 2));
}

/// 가상 배치가 없거나 그림이 없으면 그릴 것이 없다. 자리 번호(밑줄색)가 맞지 않아도 그렇다
#[test]
fn placeholders_without_a_matching_virtual_placement_draw_nothing() {
    let mut e = emu(20, 6, 10, 20);
    // 그림 없이 자리표시만
    e.feed(placeholder_grid(1, 2, 1).as_bytes());
    assert!(e.snapshot().images.is_empty());
    send_blank(&mut e, 1, 20, 20);
    // 가상 배치 없음
    assert!(e.snapshot().images.is_empty());
    e.feed(b"\x1b_Ga=p,i=1,p=5,U=1,q=2,c=2,r=1\x1b\\");
    // 밑줄색 없음(자리 번호 0)은 그림의 가상 배치 가운데 하나로 이어진다
    assert_eq!(e.snapshot().images.len(), 1);
    // 자리 번호 4를 가리키면 없다(자리 번호 5뿐)
    e.feed(b"\x1b[2;1H\x1b[58;5;4m");
    e.feed(placeholder_grid(1, 2, 1).as_bytes());
    assert_eq!(e.snapshot().images.len(), 1, "자리 번호 4에는 가상 배치가 없다");
    e.feed(b"\x1b[3;1H\x1b[58;5;5m");
    e.feed(placeholder_grid(1, 2, 1).as_bytes());
    assert_eq!(e.snapshot().images.len(), 2);
}

/// 대체 화면의 가상 배치는 그 화면에서만 쓰이고, 나오면 사라진다
#[test]
fn virtual_placement_belongs_to_its_screen() {
    let mut e = emu(20, 6, 10, 20);
    send_blank(&mut e, 1, 20, 20);
    e.feed(b"\x1b[?1049h\x1b_Ga=p,i=1,U=1,q=2,c=2,r=1\x1b\\");
    e.feed(placeholder_grid(1, 2, 1).as_bytes());
    assert_eq!(e.snapshot().images.len(), 1);
    e.feed(b"\x1b[?1049l");
    e.feed(placeholder_grid(1, 2, 1).as_bytes());
    assert!(e.snapshot().images.is_empty(), "주 화면에는 가상 배치가 없다");
}

/// Ghostty `renderer/image.zig` "kitty renderer positions relative placements from virtual parent placeholders":
/// 칸 10x10, 5x5 칸. 가상 배치 p=1의 자리표시 칸이 (1, 1)과 (3, 2)에 있고, 자식은 (+1, +2) 오프셋: 왼쪽 위 칸 (1, 1) 기준 (2, 3)
#[test]
fn relative_placement_is_positioned_from_the_virtual_parents_placeholders() {
    let mut e = emu(5, 5, 10, 10);
    send_blank(&mut e, 1, 10, 10);
    e.feed(b"\x1b_Ga=p,i=1,p=1,U=1,q=2\x1b\\");
    // 팔레트 글자색 1, 밑줄색 1. 칸 (1, 1)과 (3, 2)(CUP은 1부터)
    e.feed(format!("\x1b[38;5;1m\x1b[58;5;1m\x1b[2;2H{}\x1b[3;4H{}", cell(0, 0), cell(0, 0)).as_bytes());
    e.feed(b"\x1b_Ga=p,i=1,p=2,P=1,Q=1,H=1,V=2,c=1,r=1,z=5,q=2\x1b\\");
    assert!(e.take_replies().is_empty());
    let v = pieces(&mut e);
    // 자리표시 조각 둘(z=-1)과 자식(z=5)
    assert_eq!(v.len(), 3);
    assert_eq!(v.iter().filter(|p| p.z == -1).count(), 2);
    let child = v.iter().find(|p| p.z == 5).expect("자식");
    assert_eq!((child.col, child.row), (2, 3));
}

/// 자식의 칸이 하나라도 보이면 그려지고, 왼쪽으로 나가면 열이 음수가 된다(위로 스크롤된 부모도 같다).
/// 부모(커서에 놓인 그림)를 지우면 자식도 사라진다
#[test]
fn relative_placement_follows_a_pinned_parent_and_dies_with_it() {
    let mut e = emu(20, 6, 10, 10);
    send_blank(&mut e, 1, 10, 10);
    send_blank(&mut e, 2, 10, 10);
    // 부모: 칸 (4, 2)에 놓는다(번호 p=1). 자식: 그림 2를 (-6, +1) 오프셋
    e.feed(b"\x1b[3;5H\x1b_Ga=p,i=1,p=1,C=1\x1b\\");
    e.take_replies();
    e.feed(b"\x1b_Ga=p,i=2,p=1,P=1,Q=1,H=-6,V=1,c=3,r=1,C=1\x1b\\");
    assert_eq!(e.take_replies(), b"\x1b_Gi=2,p=1;OK\x1b\\");
    let s = e.snapshot();
    assert_eq!(s.images.len(), 2);
    let child = s
        .images
        .iter()
        .find(|p| p.width == 10 && (p.col, p.row) == (-2, 3))
        .expect("자식 (-2, 3)");
    assert_eq!((child.cols, child.rows), (3, 1), "3칸 중 오른쪽 한 칸만 화면 안");
    // 커서는 상대 배치로 움직이지 않는다
    assert_eq!((s.cursor.row, s.cursor.col), (2, 4));
    // 부모 그림을 지우면 자식도 간다
    e.feed(b"\x1b_Ga=d,d=i,i=1,p=1\x1b\\");
    assert_eq!(e.snapshot().images.len(), 0);
}
