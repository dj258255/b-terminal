//! assets/themes의 테마 파일들을 실행 파일에 묶는다(Ghostty를 깔지 않아도 테마를 쓰게, #33)

use std::fmt::Write as _;
use std::path::Path;

fn main() {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../assets/themes");
    println!("cargo:rerun-if-changed={}", dir.display());
    let mut entries: Vec<_> = std::fs::read_dir(&dir)
        .map(|rd| {
            rd.filter_map(|e| e.ok())
                .map(|e| e.path())
                .filter(|p| p.is_file())
                .collect()
        })
        .unwrap_or_default();
    entries.sort();
    let mut out = String::from(
        "/// (테마 이름, 내용). build.rs가 assets/themes에서 만든다\npub static BUNDLED_THEMES: &[(&str, &str)] = &[\n",
    );
    for p in entries {
        let name = p.file_name().and_then(|n| n.to_str()).unwrap_or_default();
        if name.starts_with('.') {
            continue;
        }
        let path = p.canonicalize().unwrap_or(p.clone());
        writeln!(out, "    ({name:?}, include_str!({:?})),", path.display().to_string()).unwrap();
    }
    out.push_str("];\n");
    let dest = Path::new(&std::env::var("OUT_DIR").unwrap()).join("themes.rs");
    std::fs::write(dest, out).unwrap();
}
