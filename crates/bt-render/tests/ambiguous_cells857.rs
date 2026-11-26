//! 셀 밖에서 폭을 세는 곳(조합 글, 제목·상태 줄)이 모호 폭 설정을 따른다(#857). 전역 값을 바꾸므로 이 파일 안의 시험 하나만 둔다

use bt_render::{char_cells, set_ambiguous_wide, text_cells, truncate};

#[test]
fn char_cells_follows_the_ambiguous_setting() {
    set_ambiguous_wide(false);
    assert_eq!(
        (char_cells('\u{2460}'), char_cells('\u{D55C}'), char_cells('a')),
        (1, 2, 1)
    );
    assert_eq!(text_cells("\u{2460}x"), 2);
    set_ambiguous_wide(true);
    assert_eq!(
        (char_cells('\u{2460}'), char_cells('\u{D55C}'), char_cells('a')),
        (2, 2, 1)
    );
    assert_eq!(char_cells('\u{301}'), 0);
    assert_eq!(text_cells("\u{2460}x"), 3);
    // 줄여 자를 때도 같은 폭을 쓴다: 3칸에는 ①(2)와 …(1)
    assert_eq!(truncate("\u{2460}\u{2460}\u{2460}", 3), "\u{2460}\u{2026}");
    set_ambiguous_wide(false);
    assert_eq!(
        truncate("\u{2460}\u{2460}\u{2460}\u{2460}", 3),
        "\u{2460}\u{2460}\u{2026}"
    );
}
