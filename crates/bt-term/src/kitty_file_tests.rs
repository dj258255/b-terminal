//! kitty 그림 파일 전송(`t=f`, #856)의 시험. Ghostty `graphics_image.zig`의 `readFile` 시험(일반 파일, 상대 경로, 크기 S·오프셋 O,
//! 짧은 자료, 막는 경로)에 대응하고, 우리가 더 막는 것(폴더·장치·다른 매체·답에 경로 안 담기)을 더한다

use super::*;
use std::io::Write;

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
            s.push(if i <= ch.len() {
                T[(n >> (18 - 6 * i) & 63) as usize] as char
            } else {
                '='
            });
        }
    }
    s
}

/// 2×1 RGBA PNG(빨강, 반투명 파랑)
fn png_2x1() -> Vec<u8> {
    let mut out = Vec::new();
    let mut enc = png::Encoder::new(&mut out, 2, 1);
    enc.set_color(png::ColorType::Rgba);
    enc.set_depth(png::BitDepth::Eight);
    let mut w = enc.write_header().unwrap();
    w.write_image_data(&[255, 0, 0, 255, 0, 0, 255, 128]).unwrap();
    w.finish().unwrap();
    out
}

fn path_cmd(extra: &str, path: &std::path::Path) -> String {
    format!("{extra};{}", b64(path.as_os_str().as_encoded_bytes()))
}

fn reply(g: &mut Graphics, cmd: &str) -> String {
    String::from_utf8(g.handle(cmd.as_bytes(), CUR).reply.expect("답이 없다")).unwrap()
}

fn tmp_png() -> (tempfile::TempDir, std::path::PathBuf) {
    let dir = tempfile::tempdir().unwrap();
    let canon = dir.path().canonicalize().unwrap();
    let p = canon.join("a.png");
    std::fs::write(&p, png_2x1()).unwrap();
    (dir, p)
}

#[test]
fn png_file_is_transmitted_and_replied_ok() {
    let (_d, p) = tmp_png();
    let mut g = Graphics::default();
    let r = reply(&mut g, &path_cmd("a=T,t=f,f=100,i=3", &p));
    assert_eq!(r, "\x1b_Gi=3;OK\x1b\\");
    let img = g.image(3).unwrap();
    assert_eq!((img.width, img.height), (2, 1));
    assert_eq!(&img.rgba[4..8], &[0, 0, 255, 128]);
    assert_eq!(g.placements.len(), 1);
    assert!(p.exists(), "t=f는 파일을 지우지 않는다");
}

#[test]
fn query_with_a_file_reads_it_without_storing() {
    let (_d, p) = tmp_png();
    let mut g = Graphics::default();
    assert_eq!(reply(&mut g, &path_cmd("a=q,t=f,f=100,i=4", &p)), "\x1b_Gi=4;OK\x1b\\");
    assert!(g.image(4).is_none());
}

#[test]
fn offset_size_and_zlib_apply_to_png_files() {
    let png = png_2x1();
    let dir = tempfile::tempdir().unwrap();
    let p = dir.path().canonicalize().unwrap().join("wrapped.bin");
    // 앞 3바이트와 뒤 5바이트는 쓰레기: O=3,S=PNG 길이면 PNG만 읽는다(Ghostty "regular file size reads exactly requested bytes")
    let mut data = vec![9, 9, 9];
    data.extend_from_slice(&png);
    data.extend_from_slice(&[8; 5]);
    std::fs::write(&p, &data).unwrap();
    let mut g = Graphics::default();
    let n = png.len();
    assert_eq!(
        reply(&mut g, &path_cmd(&format!("a=t,t=f,f=100,i=5,O=3,S={n}"), &p)),
        "\x1b_Gi=5;OK\x1b\\"
    );
    // 오프셋만 주면 뒤 쓰레기까지 읽어 PNG 해독이 실패하지 않을 수 있으나 앞 쓰레기가 남아 거절한다
    assert_eq!(
        reply(&mut g, &path_cmd("a=t,t=f,f=100,i=6", &p)),
        "\x1b_Gi=6;EINVAL\x1b\\"
    );
    // S가 파일 끝을 넘으면(짧은 자료) 거절한다(Ghostty "size rejects short data")
    assert_eq!(
        reply(&mut g, &path_cmd(&format!("a=t,t=f,f=100,i=7,O=3,S={}", n + 100), &p)),
        "\x1b_Gi=7;EINVAL\x1b\\"
    );
    // 오프셋이 파일 끝을 넘으면 거절한다
    assert_eq!(
        reply(&mut g, &path_cmd("a=t,t=f,f=100,i=8,O=1000,S=3", &p)),
        "\x1b_Gi=8;EINVAL\x1b\\"
    );
    // 압축(o=z)은 파일 내용에도 적용한다
    std::fs::write(&p, miniz_oxide::deflate::compress_to_vec_zlib(&png, 6)).unwrap();
    assert_eq!(
        reply(&mut g, &path_cmd("a=t,t=f,f=100,o=z,i=9", &p)),
        "\x1b_Gi=9;OK\x1b\\"
    );
    assert_eq!((g.image(9).unwrap().width, g.image(9).unwrap().height), (2, 1));
}

/// 파일 정보 오라클 막기(#856 보안 검토): t=f는 f=100(PNG)만 받고, 다른 f는 파일을 열기 전에 EINVAL이다.
/// 원시 RGB·RGBA(f=24·32)와 s·v·O·S를 쓰면 아무 파일이나 크기만 맞으면 해석되어 존재와 크기를 알아낼 수 있다
#[test]
fn only_png_format_is_accepted_for_files() {
    let dir = tempfile::tempdir().unwrap();
    let base = dir.path().canonicalize().unwrap();
    let raw = base.join("raw.bin");
    std::fs::write(&raw, [1, 2, 3, 4, 5, 6, 7, 8]).unwrap();
    let (_d, png) = tmp_png();
    let missing = base.join("none.bin");
    let mut g = Graphics::default();
    for f in [
        "f=24,s=1,v=1",
        "f=32,s=1,v=1",
        "f=24,s=1,v=1,O=3,S=3",
        "s=1,v=1",
        "f=0,s=1,v=1",
        "f=99",
    ] {
        // 있는 raw 파일, 있는 PNG 파일, 없는 파일이 모두 같은 답이고 그림은 생기지 않는다
        for p in [&raw, &png, &missing] {
            for a in ["a=t", "a=T", "a=q"] {
                let r = reply(&mut g, &path_cmd(&format!("{a},t=f,{f},i=3"), p));
                assert_eq!(r, "\x1b_Gi=3;EINVAL\x1b\\", "{a} {f} {p:?}");
            }
        }
    }
    assert!(g.image(3).is_none() && g.placements.is_empty());
    // f=100이면 그대로 받는다(a=q 포함)
    assert_eq!(
        reply(&mut g, &path_cmd("a=q,t=f,f=100,i=4", &png)),
        "\x1b_Gi=4;OK\x1b\\"
    );
}

#[cfg(unix)]
#[test]
fn symlink_to_a_regular_file_is_followed_like_ghostty() {
    let (d, p) = tmp_png();
    let link = d.path().canonicalize().unwrap().join("link.png");
    std::os::unix::fs::symlink(&p, &link).unwrap();
    let mut g = Graphics::default();
    assert_eq!(
        reply(&mut g, &path_cmd("a=t,t=f,f=100,i=3", &link)),
        "\x1b_Gi=3;OK\x1b\\"
    );
}

#[cfg(unix)]
#[test]
fn symlink_to_a_device_is_refused() {
    let dir = tempfile::tempdir().unwrap();
    let link = dir.path().canonicalize().unwrap().join("zero.png");
    std::os::unix::fs::symlink("/dev/zero", &link).unwrap();
    let mut g = Graphics::default();
    assert_eq!(
        reply(&mut g, &path_cmd("a=t,t=f,f=100,i=3", &link)),
        "\x1b_Gi=3;EINVAL\x1b\\"
    );
    assert!(g.image(3).is_none());
}

#[test]
fn refused_files_reply_einval_and_store_nothing() {
    let dir = tempfile::tempdir().unwrap();
    let base = dir.path().canonicalize().unwrap();
    let text = base.join("note.txt");
    std::fs::write(&text, b"hello not a png").unwrap();
    let empty = base.join("empty.png");
    std::fs::write(&empty, b"").unwrap();
    // 직접 전송이면 EBADPNG지만 파일 전송은 EINVAL이다: 없는 파일과 답이 같아야 그 파일이 있는지 알 수 없다
    let mut g = Graphics::default();
    let direct = reply(&mut g, &format!("a=t,f=100,i=3;{}", b64(b"hello not a png")));
    assert_eq!(direct, "\x1b_Gi=3;EBADPNG\x1b\\");
    let cases: Vec<(&str, std::path::PathBuf)> = vec![
        ("PNG가 아닌 파일", text),
        ("빈 파일", empty),
        ("폴더", base.clone()),
        ("없는 파일", base.join("none.png")),
        ("/dev/zero", "/dev/zero".into()),
        ("/dev/null", "/dev/null".into()),
        ("상대 경로", "relative/a.png".into()),
        ("빈 경로", "".into()),
    ];
    for (name, p) in cases {
        let mut g = Graphics::default();
        let r = reply(&mut g, &path_cmd("a=T,t=f,f=100,i=3", &p));
        assert_eq!(r, "\x1b_Gi=3;EINVAL\x1b\\", "{name}");
        assert!(g.image(3).is_none() && g.placements.is_empty(), "{name}");
    }
}

#[test]
fn raw_pixels_from_a_directory_or_missing_file_are_refused_too() {
    let dir = tempfile::tempdir().unwrap();
    let mut g = Graphics::default();
    assert_eq!(
        reply(&mut g, &path_cmd("a=t,t=f,f=24,s=1,v=1,i=3", dir.path())),
        "\x1b_Gi=3;EINVAL\x1b\\"
    );
}

#[test]
fn nul_in_the_path_is_refused() {
    let mut g = Graphics::default();
    let cmd = format!("a=t,t=f,f=100,i=3;{}", b64(b"/etc/pass\0wd"));
    assert_eq!(reply(&mut g, &cmd), "\x1b_Gi=3;EINVAL\x1b\\");
}

#[test]
fn file_over_the_size_cap_is_refused_without_reading_it() {
    let dir = tempfile::tempdir().unwrap();
    let p = dir.path().canonicalize().unwrap().join("big.bin");
    // 구멍 파일: 디스크를 쓰지 않고 상한보다 1바이트 큰 파일을 만든다
    let f = std::fs::File::create(&p).unwrap();
    f.set_len(MAX_FILE_BYTES as u64 + 1).unwrap();
    let mut g = Graphics::default();
    assert_eq!(
        reply(&mut g, &path_cmd("a=t,t=f,f=100,i=3", &p)),
        "\x1b_Gi=3;EINVAL\x1b\\"
    );
    // S가 상한을 넘어도 읽기 전에 거절한다
    let (_d, ok) = tmp_png();
    let cmd = path_cmd(&format!("a=t,t=f,f=100,i=4,S={}", MAX_FILE_BYTES + 1), &ok);
    assert_eq!(reply(&mut g, &cmd), "\x1b_Gi=4;EINVAL\x1b\\");
}

#[test]
fn replies_never_carry_the_path_or_file_contents() {
    let dir = tempfile::tempdir().unwrap();
    let p = dir.path().canonicalize().unwrap().join("secret-name-12345.txt");
    let mut f = std::fs::File::create(&p).unwrap();
    f.write_all(b"TOP-SECRET-CONTENT").unwrap();
    drop(f);
    let mut g = Graphics::default();
    for extra in [
        "a=t,t=f,f=100,i=3",
        "a=q,t=f,f=100,i=3",
        "a=T,t=f,f=24,s=9,v=9,i=3",
        "a=f,t=f,f=100,i=3",
    ] {
        let r = reply(&mut g, &path_cmd(extra, &p));
        assert!(
            !r.contains("secret") && !r.contains("TOP-SECRET") && !r.contains("tmp"),
            "{r:?}"
        );
        assert!(
            r.starts_with("\x1b_Gi=3;E") && r.ends_with("\x1b\\") && r.is_ascii(),
            "{r:?}"
        );
    }
}

#[test]
fn temporary_file_and_shared_memory_stay_refused() {
    let (_d, p) = tmp_png();
    let mut g = Graphics::default();
    for t in ["t", "s"] {
        let r = reply(&mut g, &path_cmd(&format!("a=T,t={t},f=100,i=3"), &p));
        assert_eq!(r, "\x1b_Gi=3;EINVAL\x1b\\", "t={t}");
        assert!(g.image(3).is_none() && g.placements.is_empty(), "t={t}");
    }
    assert!(p.exists(), "t=t가 파일을 지우면 안 된다");
}

#[test]
fn file_transmission_works_as_an_animation_frame() {
    let (_d, p) = tmp_png();
    let mut g = Graphics::default();
    reply(&mut g, &path_cmd("a=t,t=f,f=100,i=1", &p));
    let r = reply(&mut g, &path_cmd("a=f,t=f,f=100,i=1,s=2,v=1", &p));
    assert_eq!(r, "\x1b_Gi=1,r=2;OK\x1b\\");
}

#[test]
fn blocklist_matches_ghostty() {
    use std::path::Path;
    for blocked in ["/proc/self/mem", "/sys/kernel/x", "/dev/zero", "/dev/tty", "/dev/fd/3"] {
        assert!(path_is_blocked(Path::new(blocked)), "{blocked}");
    }
    for ok in [
        "/dev/shm/a.png",
        "/tmp/a.png",
        "/proc",
        "/procfoo/x",
        "/home/u/a.png",
        "/devx/a",
    ] {
        assert!(!path_is_blocked(Path::new(ok)), "{ok}");
    }
}

#[cfg(unix)]
#[test]
fn fifo_is_refused_without_blocking() {
    let dir = tempfile::tempdir().unwrap();
    let p = dir.path().canonicalize().unwrap().join("pipe.png");
    nix::unistd::mkfifo(&p, nix::sys::stat::Mode::from_bits_truncate(0o600)).unwrap();
    let mut g = Graphics::default();
    assert_eq!(
        reply(&mut g, &path_cmd("a=t,t=f,f=100,i=3", &p)),
        "\x1b_Gi=3;EINVAL\x1b\\"
    );
}

#[test]
fn relative_path_is_refused_even_when_the_file_exists_in_the_working_dir() {
    let name = "kitty_file_tests_relative_856.png";
    let here = std::env::current_dir().unwrap().join(name);
    std::fs::write(&here, png_2x1()).unwrap();
    let mut g = Graphics::default();
    let r = reply(&mut g, &format!("a=t,t=f,f=100,i=3;{}", b64(name.as_bytes())));
    std::fs::remove_file(&here).unwrap();
    assert_eq!(r, "\x1b_Gi=3;EINVAL\x1b\\");
}
