//! #823: 10000줄 스크롤백에서 한 번 찾는 시간을 잰다(한 번 잰 값). `--release`로 돌린다:
//! `cargo test --release -p bt-term --test search_perf823 -- --ignored --nocapture`

use bt_core::Theme;
use bt_term::{Emulator, GridSize};
use std::time::Instant;

#[test]
#[ignore = "시간을 재는 시험이라 기본으로는 돌리지 않는다"]
fn time_one_search_over_10000_lines_of_scrollback() {
    let mut e = Emulator::new(GridSize::new(100, 40), Theme::default());
    let mut text = String::new();
    for i in 0..20_000 {
        text.push_str(&format!(
            "line {i:05} the quick brown fox jumps over the lazy dog {}\r\n",
            if i % 97 == 0 { "needle" } else { "hay" }
        ));
    }
    e.feed(text.as_bytes());
    for needle in ["needle", "fox", "zzzz"] {
        let t = Instant::now();
        let st = e.search(needle).unwrap();
        let first = t.elapsed();
        let t = Instant::now();
        let st2 = e.search_step(true).unwrap();
        let step = t.elapsed();
        let t = Instant::now();
        let _ = e.snapshot();
        let snap = t.elapsed();
        println!(
            "needle={needle:?} total={} (capped {}) search={first:?} step={step:?} (index {:?}) snapshot={snap:?}",
            st.total, st.capped, st2.index
        );
    }
}
