//! 창 상태 복원이 칸마다 남기는 화면·스크롤백 파일(#861, ADR-092).
//!
//! 상태 파일(`state.json`) 옆의 `scrollback/` 폴더에 칸마다 JSON 파일 하나를 둔다. 폴더는 권한 0700, 파일은 0600이다.
//! 암호화는 하지 않는다(키체인을 unsafe 없이 쓸 크레이트가 없다). 그래서 화면에 찍힌 비밀번호·토큰이 평문으로 남을 수 있고,
//! 이유로 기본은 끈다. 되살릴 때 읽고 나면 지운다. 크기 상한을 넘으면 오래된 줄부터 버린다.

use std::path::{Path, PathBuf};

use bt_term::saved_screen::SavedLine;
use serde::{Deserialize, Serialize};

/// 파일 모양의 판
pub const VERSION: u32 = 1;
/// 칸 하나의 파일 크기 상한(바이트)
pub const FILE_MAX_BYTES: usize = 4 * 1024 * 1024;
/// 모든 칸 파일의 합 상한(바이트). 칸이 많으면 칸마다 이 값을 칸 수로 나눈 몫까지만 쓴다
pub const TOTAL_MAX_BYTES: usize = 32 * 1024 * 1024;
/// 읽을 때 받는 줄 수의 상한(넘으면 끝쪽 줄만)
pub const MAX_LOAD_LINES: usize = 100_000;

#[derive(Debug, Serialize, Deserialize)]
struct SavedFile {
    version: u32,
    /// 저장한 때(Unix 초)
    saved_at: u64,
    lines: Vec<SavedLine>,
}

/// 상태 파일 옆의 스크롤백 폴더
pub fn dir_for(state_path: &Path) -> PathBuf {
    state_path.with_file_name("scrollback")
}

/// 칸 열쇠: 파일 이름이 되므로 소문자·숫자만 1~32자. 경로 문자가 끼어들 수 없게 읽기 전에 거른다
pub fn valid_key(key: &str) -> bool {
    (1..=32).contains(&key.len()) && key.bytes().all(|b| b.is_ascii_lowercase() || b.is_ascii_digit())
}

fn file_of(dir: &Path, key: &str) -> PathBuf {
    dir.join(format!("{key}.json"))
}

/// 예산 안에 드는 끝쪽 줄만 남긴다(오래된 줄부터 버린다)
pub fn trim_to_budget(lines: Vec<SavedLine>, budget: usize) -> Vec<SavedLine> {
    // 파일 머리(`version`·`saved_at`)와 줄 사이 쉼표 몫을 넉넉히 빼 둔다
    let mut left = budget.saturating_sub(128);
    let mut keep = 0usize;
    for line in lines.iter().rev() {
        let n = serde_json::to_vec(line)
            .map(|v| v.len())
            .unwrap_or(usize::MAX)
            .saturating_add(1);
        if n > left {
            break;
        }
        left -= n;
        keep += 1;
    }
    let skip = lines.len() - keep;
    lines.into_iter().skip(skip).collect()
}

#[cfg(unix)]
fn private_dir(dir: &Path) -> std::io::Result<()> {
    use std::os::unix::fs::{DirBuilderExt, PermissionsExt};
    // 이미 심볼릭 링크이거나 폴더가 아니면 쓰지 않는다(남이 만든 링크로 파일이 새지 않게)
    match std::fs::symlink_metadata(dir) {
        Ok(m) if m.is_dir() => {}
        Ok(_) => return Err(std::io::Error::other("스크롤백 자리가 폴더가 아닙니다")),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            if let Some(parent) = dir.parent() {
                std::fs::create_dir_all(parent)?;
            }
            std::fs::DirBuilder::new().mode(0o700).create(dir)?;
        }
        Err(e) => return Err(e),
    }
    std::fs::set_permissions(dir, std::fs::Permissions::from_mode(0o700))
}

#[cfg(not(unix))]
fn private_dir(dir: &Path) -> std::io::Result<()> {
    std::fs::create_dir_all(dir)
}

#[cfg(unix)]
fn write_private(path: &Path, bytes: &[u8]) -> std::io::Result<()> {
    use std::io::Write;
    use std::os::unix::fs::OpenOptionsExt;
    let tmp = path.with_extension("tmp");
    let _ = std::fs::remove_file(&tmp);
    // create_new: 남이 미리 만들어 둔 파일·링크를 따라가지 않는다. 처음부터 0600
    let mut f = std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(0o600)
        .open(&tmp)?;
    f.write_all(bytes)?;
    f.sync_all()?;
    std::fs::rename(&tmp, path)
}

#[cfg(not(unix))]
fn write_private(path: &Path, bytes: &[u8]) -> std::io::Result<()> {
    let tmp = path.with_extension("tmp");
    std::fs::write(&tmp, bytes)?;
    std::fs::rename(&tmp, path)
}

/// 칸들의 줄을 파일로 적는다. 이번에 적지 않는 옛 파일은 지운다. `now`는 Unix 초
pub fn write_all(dir: &Path, panes: &[(String, Vec<SavedLine>)], now: u64) -> std::io::Result<()> {
    if panes.is_empty() {
        remove_all(dir);
        return Ok(());
    }
    private_dir(dir)?;
    let budget = FILE_MAX_BYTES.min(TOTAL_MAX_BYTES / panes.len());
    let mut keep: Vec<String> = Vec::new();
    for (key, lines) in panes {
        if !valid_key(key) {
            continue;
        }
        let file = SavedFile {
            version: VERSION,
            saved_at: now,
            lines: trim_to_budget(lines.clone(), budget),
        };
        write_private(
            &file_of(dir, key),
            &serde_json::to_vec(&file).map_err(std::io::Error::other)?,
        )?;
        keep.push(format!("{key}.json"));
    }
    // 이번에 적지 않은 옛 파일(닫은 칸)은 남기지 않는다
    if let Ok(rd) = std::fs::read_dir(dir) {
        for e in rd.flatten() {
            let name = e.file_name().to_string_lossy().into_owned();
            if !keep.contains(&name) {
                let _ = std::fs::remove_file(e.path());
            }
        }
    }
    Ok(())
}

/// 칸 하나의 지난 화면을 읽는다: (줄들, 저장한 때). 없거나 망가졌거나 다른 판이거나 너무 크면 없다.
/// 파일이 일반 파일이 아니면(심볼릭 링크 등) 읽지 않는다
pub fn read(dir: &Path, key: &str) -> Option<(Vec<SavedLine>, u64)> {
    if !valid_key(key) {
        return None;
    }
    let path = file_of(dir, key);
    let meta = std::fs::symlink_metadata(&path).ok()?;
    if !meta.is_file() || meta.len() as usize > FILE_MAX_BYTES + 4096 {
        return None;
    }
    let file: SavedFile = serde_json::from_slice(&std::fs::read(&path).ok()?).ok()?;
    if file.version != VERSION {
        return None;
    }
    let mut lines = file.lines;
    if lines.len() > MAX_LOAD_LINES {
        lines.drain(..lines.len() - MAX_LOAD_LINES);
    }
    Some((lines, file.saved_at))
}

/// 폴더 안의 파일과 폴더를 지운다. 없으면 아무것도 하지 않는다. 폴더가 심볼릭 링크면 건드리지 않는다
pub fn remove_all(dir: &Path) {
    let Ok(meta) = std::fs::symlink_metadata(dir) else {
        return;
    };
    if !meta.is_dir() {
        return;
    }
    if let Ok(rd) = std::fs::read_dir(dir) {
        for e in rd.flatten() {
            if e.file_type().is_ok_and(|t| t.is_file()) {
                let _ = std::fs::remove_file(e.path());
            }
        }
    }
    let _ = std::fs::remove_dir(dir);
}

/// 저장한 때를 사람이 읽는 글로: `2026-11-26 05:03 UTC`
pub fn format_saved_at(secs: u64) -> String {
    let days = (secs / 86400) as i64;
    let rem = secs % 86400;
    // 민간 날짜(Howard Hinnant의 civil_from_days)
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = yoe + era * 400 + i64::from(m <= 2);
    format!("{y:04}-{m:02}-{d:02} {:02}:{:02} UTC", rem / 3600, rem % 3600 / 60)
}

/// 되살린 줄들 끝에 붙일 안내 줄
pub fn note_for(saved_at: u64) -> String {
    format!("── {}에 저장한 화면 ──", format_saved_at(saved_at))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tmp(name: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("bt-sb-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    fn lines(n: usize) -> Vec<SavedLine> {
        (0..n).map(|i| SavedLine::plain(&format!("줄 {i}"))).collect()
    }

    #[test]
    fn key_validation_blocks_paths() {
        assert!(valid_key("w0t1p2"));
        for bad in ["", "../x", "a/b", "A", "a.b", "a b", &"a".repeat(33), "한"] {
            assert!(!valid_key(bad), "{bad:?}");
        }
        let d = tmp("key");
        assert!(read(&d, "../state").is_none());
        std::fs::remove_dir_all(&d).unwrap();
    }

    #[test]
    fn write_read_round_trip_and_removes_stale_files() {
        let d = tmp("rt").join("scrollback");
        write_all(
            &d,
            &[("w0t0p0".into(), lines(3)), ("w0t0p1".into(), lines(2))],
            1_700_000_000,
        )
        .unwrap();
        let (l, at) = read(&d, "w0t0p0").unwrap();
        assert_eq!((l.len(), at), (3, 1_700_000_000));
        assert_eq!(l[2].text(), "줄 2");
        // 다음 저장에 없는 칸의 파일은 지운다
        write_all(&d, &[("w0t0p0".into(), lines(1))], 1_700_000_001).unwrap();
        assert!(read(&d, "w0t0p1").is_none());
        assert_eq!(read(&d, "w0t0p0").unwrap().0.len(), 1);
        remove_all(&d);
        assert!(!d.exists());
        std::fs::remove_dir_all(d.parent().unwrap()).unwrap();
    }

    #[cfg(unix)]
    #[test]
    fn dir_is_0700_and_files_are_0600() {
        use std::os::unix::fs::PermissionsExt;
        let root = tmp("perm");
        let d = root.join("scrollback");
        // 이미 느슨한 권한으로 있던 폴더도 조인다
        std::fs::create_dir_all(&d).unwrap();
        std::fs::set_permissions(&d, std::fs::Permissions::from_mode(0o755)).unwrap();
        write_all(&d, &[("w0t0p0".into(), lines(2))], 1).unwrap();
        let mode = |p: &Path| std::fs::metadata(p).unwrap().permissions().mode() & 0o777;
        assert_eq!(mode(&d), 0o700);
        assert_eq!(mode(&d.join("w0t0p0.json")), 0o600);
        std::fs::remove_dir_all(&root).unwrap();
    }

    #[cfg(unix)]
    #[test]
    fn symlinked_dir_and_files_are_refused() {
        let root = tmp("link");
        let target = root.join("elsewhere");
        std::fs::create_dir_all(&target).unwrap();
        let d = root.join("scrollback");
        std::os::unix::fs::symlink(&target, &d).unwrap();
        assert!(write_all(&d, &[("w0t0p0".into(), lines(1))], 1).is_err());
        assert!(
            std::fs::read_dir(&target).unwrap().next().is_none(),
            "링크 너머에 파일이 새면 안 된다"
        );
        remove_all(&d);
        assert!(d.exists(), "링크 폴더는 건드리지 않는다");
        std::fs::remove_file(&d).unwrap();
        // 파일이 링크면 읽지 않는다
        let real = root.join("real.json");
        std::fs::write(&real, br#"{"version":1,"saved_at":1,"lines":[]}"#).unwrap();
        let d2 = root.join("s2");
        std::fs::create_dir(&d2).unwrap();
        std::os::unix::fs::symlink(&real, d2.join("w0t0p0.json")).unwrap();
        assert!(read(&d2, "w0t0p0").is_none());
        std::fs::remove_dir_all(&root).unwrap();
    }

    #[test]
    fn oversize_drops_oldest_lines_first() {
        let l = lines(1000);
        let last10: usize = l[990..].iter().map(|x| serde_json::to_vec(x).unwrap().len() + 1).sum();
        let kept = trim_to_budget(l, 128 + last10 + 1);
        assert_eq!(kept.len(), 10);
        assert_eq!(kept[0].text(), "줄 990", "끝쪽 줄을 남긴다");
        assert_eq!(kept[9].text(), "줄 999");
        assert!(trim_to_budget(lines(5), 0).is_empty());
    }

    #[test]
    fn budget_is_shared_by_pane_count() {
        let d = tmp("budget").join("scrollback");
        // 칸이 많으면 칸마다 상한이 줄어 총합이 TOTAL_MAX_BYTES를 넘지 않는다
        let big: Vec<SavedLine> = (0..2000).map(|_| SavedLine::plain(&"가".repeat(4000))).collect();
        let panes: Vec<(String, Vec<SavedLine>)> = (0..16).map(|i| (format!("p{i}"), big.clone())).collect();
        write_all(&d, &panes, 1).unwrap();
        let total: u64 = std::fs::read_dir(&d)
            .unwrap()
            .flatten()
            .map(|e| e.metadata().unwrap().len())
            .sum();
        assert!(total as usize <= TOTAL_MAX_BYTES, "{total}");
        assert!(total > 0);
        remove_all(&d);
        std::fs::remove_dir_all(d.parent().unwrap()).unwrap();
    }

    #[test]
    fn broken_big_or_other_version_files_are_ignored() {
        let d = tmp("bad");
        std::fs::write(d.join("a.json"), b"{nope").unwrap();
        assert!(read(&d, "a").is_none());
        std::fs::write(d.join("b.json"), br#"{"version":99,"saved_at":1,"lines":[]}"#).unwrap();
        assert!(read(&d, "b").is_none());
        std::fs::write(d.join("c.json"), vec![b' '; FILE_MAX_BYTES + 5000]).unwrap();
        assert!(read(&d, "c").is_none(), "상한을 넘는 파일은 읽지 않는다");
        std::fs::remove_dir_all(&d).unwrap();
    }

    #[test]
    fn hostile_file_loads_only_as_data() {
        // 파일에 OSC 52·DCS·BEL을 심어도 읽기는 줄 데이터로만 받는다(되살릴 때 격자에 직접 놓이며 해석되지 않는다)
        let d = tmp("evil");
        let json = r#"{"version":1,"saved_at":1,"lines":[[{"t":"\u001b]52;c;ZXZpbA==\u0007\u001bPq\u001b\\"}]]}"#;
        std::fs::write(d.join("w0t0p0.json"), json).unwrap();
        let (l, _) = read(&d, "w0t0p0").unwrap();
        assert_eq!(l.len(), 1);
        assert!(
            l[0].text().contains('\u{1b}'),
            "읽기는 글을 그대로 데이터로 둔다. 거르는 곳은 격자에 놓는 쪽이다"
        );
        std::fs::remove_dir_all(&d).unwrap();
    }

    #[test]
    fn saved_at_format() {
        assert_eq!(format_saved_at(0), "1970-01-01 00:00 UTC");
        assert_eq!(format_saved_at(1795669380), "2026-11-26 05:03 UTC");
        assert_eq!(format_saved_at(1_709_164_800), "2024-02-29 00:00 UTC");
    }
}
