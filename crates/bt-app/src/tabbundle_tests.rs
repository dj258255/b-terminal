//! 탭 묶음 모델의 순수 시험(#933): 막대에 그릴 항목, 빼기·묶기, 끌어 놓기가 묶음 경계를 지키는 규칙, 끼워 넣을 자리

use crate::tabbundle::*;
use crate::tabcolor::TabColor;

/// 글자 하나가 탭 하나: 같은 글자는 같은 묶음(a=1, b=2, c=3), `.`은 묶음 없음
fn ids(s: &str) -> Vec<Option<Id>> {
    s.chars()
        .map(|c| match c {
            '.' => None,
            c => Some(c as Id - 'a' as Id + 1),
        })
        .collect()
}

fn show(v: &[Option<Id>]) -> String {
    v.iter()
        .map(|i| i.map_or('.', |i| (b'a' + (i - 1) as u8) as char))
        .collect()
}

fn bundle(id: Id, collapsed: bool) -> Bundle {
    Bundle {
        id,
        name: format!("묶음{id}"),
        color: TabColor::Blue,
        collapsed,
    }
}

/// 어떤 시작이든 규칙을 지킨 결과는 같은 묶음의 탭이 이어져 있다
fn all_layouts() -> Vec<Vec<Option<Id>>> {
    ["aab.c", ".aab", "aabb", "abc", "a", "a.b", "..aa..", "abbbc.", "aaa"]
        .iter()
        .map(|s| ids(s))
        .collect()
}

#[test]
fn bar_items_put_a_chip_before_each_run_and_hide_collapsed_members_except_the_active_tab() {
    let v = ids("a.bb");
    let open = [bundle(1, false), bundle(2, false)];
    assert_eq!(
        bar_items(&v, &open, 0),
        [
            BarItem::Chip(1),
            BarItem::Tab(0),
            BarItem::Tab(1),
            BarItem::Chip(2),
            BarItem::Tab(2),
            BarItem::Tab(3)
        ]
    );
    // 접으면 머리표만. 고른 탭은 보인다
    let shut = [bundle(1, true), bundle(2, true)];
    assert_eq!(
        bar_items(&v, &shut, 1),
        [BarItem::Chip(1), BarItem::Tab(1), BarItem::Chip(2)]
    );
    assert_eq!(
        bar_items(&v, &shut, 3),
        [BarItem::Chip(1), BarItem::Tab(1), BarItem::Chip(2), BarItem::Tab(3)]
    );
    // 등록부에 없는 묶음 번호는 묶음 없는 탭으로 본다
    assert_eq!(bar_items(&ids("aa"), &[], 0), [BarItem::Tab(0), BarItem::Tab(1)]);
}

#[test]
fn chip_title_adds_the_hidden_count_only_when_collapsed() {
    let mut b = bundle(1, false);
    assert_eq!(chip_title(&b, 3, 0), "묶음1");
    b.collapsed = true;
    assert_eq!(chip_title(&b, 3, 0), "묶음1 +3", "고른 탭이 밖이면 셋 다 가려졌다");
    assert_eq!(chip_title(&b, 3, 1), "묶음1 +2", "고른 탭이 안에 있으면 그 탭은 보인다");
    b.name.clear();
    assert_eq!(chip_title(&b, 2, 0), "묶음 +2", "이름이 없으면 기본 이름");
}

#[test]
fn leaving_from_the_middle_moves_the_tab_just_after_the_bundle() {
    let mut v = ids("aaa.");
    assert_eq!(leave(&mut v, 1), Some((1, 2)));
    assert_eq!(show(&v), "aa..");
    // 가장자리는 자리를 옮기지 않는다
    let mut v = ids("aaa.");
    assert_eq!(leave(&mut v, 0), None);
    assert_eq!(show(&v), ".aa.");
    let mut v = ids("aaa.");
    assert_eq!(leave(&mut v, 2), None);
    assert_eq!(show(&v), "aa..");
    // 묶음 없는 탭은 그대로
    let mut v = ids("aa.");
    assert_eq!(leave(&mut v, 2), None);
    assert_eq!(show(&v), "aa.");
}

#[test]
fn a_new_bundle_takes_the_tab_out_of_the_old_one_and_a_lone_member_is_left_alone() {
    let mut v = ids("..a.");
    // 묶음 없는 탭을 새 묶음으로
    assert!(!is_sole_member(&v, 0));
    assert_eq!(new_bundle(&mut v, 0, 9), None);
    assert_eq!(v[0], Some(9));
    // 묶음 가운데 탭을 새 묶음으로: 묶음 밖으로 나가 홀로 선다
    let mut v = ids("aaa.");
    assert_eq!(new_bundle(&mut v, 1, 9), Some((1, 2)));
    assert_eq!(v, [Some(1), Some(1), Some(9), None]);
    assert!(is_contiguous(&v));
    // 혼자뿐인 묶음의 탭은 새 묶음을 만들 이유가 없다
    let v = ids("a.");
    assert!(is_sole_member(&v, 0));
    assert!(!is_sole_member(&ids("aa"), 0));
}

#[test]
fn joining_the_neighbor_prefers_the_left_and_only_works_across_a_boundary() {
    let mut v = ids("a.b");
    assert_eq!(join_neighbor(&mut v, 1), Some(1));
    assert_eq!(show(&v), "aab");
    // 왼쪽 이웃에 묶음이 없으면 오른쪽
    let mut v = ids(".bb");
    assert_eq!(join_neighbor(&mut v, 0), Some(2));
    assert_eq!(show(&v), "bbb");
    // 묶음 가운데 탭은 이웃이 같은 묶음이라 할 일이 없다
    let mut v = ids("aaa");
    assert_eq!(join_neighbor(&mut v, 1), None);
    // 두 묶음 사이의 가장자리 탭은 건너편 묶음으로
    let mut v = ids("aabb");
    assert_eq!(join_neighbor(&mut v, 1), Some(2));
    assert_eq!(show(&v), "abbb");
    // 이웃이 모두 묶음 없음이면 아무 일도 없다
    let mut v = ids("...");
    assert_eq!(join_neighbor(&mut v, 1), None);
}

#[test]
fn dropping_on_a_tab_takes_that_tabs_bundle() {
    // 묶음 없는 탭을 묶음의 탭 위에 놓으면 들어간다(가운데든 가장자리든)
    let mut v = ids(".aa");
    reorder(&mut v, 0, 1);
    assert_eq!(show(&v), "aaa");
    // 묶음 탭을 묶음 없는 탭 위에 놓으면 빠져나온다
    let mut v = ids("aa..");
    reorder(&mut v, 0, 3);
    assert_eq!(show(&v), "a...");
    // 같은 묶음 안에서 옮기면 그대로
    let mut v = ids("aab");
    reorder(&mut v, 0, 1);
    assert_eq!(show(&v), "aab");
    // 다른 묶음의 탭 위에 놓으면 그 묶음으로
    let mut v = ids("aabb");
    reorder(&mut v, 1, 2);
    assert_eq!(show(&v), "abbb");
}

#[test]
fn dropping_on_a_chip_joins_at_the_front_of_that_bundle() {
    let mut v = ids(".bb");
    assert_eq!(reorder_to_chip(&mut v, 0, 2), (0, 0));
    assert_eq!(show(&v), "bbb");
    let mut v = ids("bb.");
    assert_eq!(reorder_to_chip(&mut v, 2, 2), (2, 0));
    assert_eq!(show(&v), "bbb");
    let mut v = ids("a.bb");
    assert_eq!(
        reorder_to_chip(&mut v, 1, 2),
        (1, 1),
        "오른쪽에서 왼쪽 묶음 앞으로가 아니라 제자리 앞"
    );
    assert_eq!(show(&v), "abbb");
}

#[test]
fn every_reorder_and_chip_drop_keeps_each_bundle_in_one_run() {
    for start in all_layouts() {
        let n = start.len();
        for from in 0..n {
            for to in 0..n {
                let mut v = start.clone();
                reorder(&mut v, from, to);
                assert!(is_contiguous(&v), "{} {from}->{to} = {}", show(&start), show(&v));
            }
            for b in start
                .iter()
                .flatten()
                .copied()
                .collect::<std::collections::BTreeSet<_>>()
            {
                let mut v = start.clone();
                reorder_to_chip(&mut v, from, b);
                assert!(is_contiguous(&v), "{} {from}->chip {b} = {}", show(&start), show(&v));
            }
        }
    }
}

#[test]
fn every_leave_new_bundle_and_join_keeps_each_bundle_in_one_run() {
    for start in all_layouts() {
        for i in 0..start.len() {
            let mut v = start.clone();
            leave(&mut v, i);
            assert!(is_contiguous(&v), "leave {} {i} = {}", show(&start), show(&v));
            let mut v = start.clone();
            new_bundle(&mut v, i, 9);
            assert!(is_contiguous(&v), "new {} {i} = {}", show(&start), show(&v));
            let mut v = start.clone();
            join_neighbor(&mut v, i);
            assert!(is_contiguous(&v), "join {} {i} = {}", show(&start), show(&v));
        }
    }
}

#[test]
fn a_new_tab_is_never_inserted_inside_a_bundle_and_a_restored_tab_returns_to_its_bundle() {
    // 묶음 없는 새 탭: 묶음 가운데 자리면 묶음 뒤로 민다
    assert_eq!(fit_insert(&ids("aaa."), 1, None), 3);
    assert_eq!(fit_insert(&ids("aaa."), 2, None), 3);
    assert_eq!(fit_insert(&ids("aaa."), 0, None), 0, "묶음 앞 가장자리는 괜찮다");
    assert_eq!(fit_insert(&ids("aaa."), 3, None), 3);
    assert_eq!(fit_insert(&ids("aaa."), 4, None), 4);
    // 되살리는 탭: 그 묶음의 이어진 구간 안(양 끝 포함)이면 그 자리, 밖이면 구간 끝
    assert_eq!(
        fit_insert(&ids("aa.a"), 1, Some(1)),
        1,
        "이 입력은 이미 끊겼지만 구간은 첫 구간"
    );
    assert_eq!(fit_insert(&ids("aab"), 1, Some(1)), 1);
    assert_eq!(fit_insert(&ids("aab"), 2, Some(1)), 2);
    assert_eq!(fit_insert(&ids("aab"), 3, Some(1)), 2, "묶음 구간 밖은 구간 끝으로");
    assert_eq!(fit_insert(&ids("baa"), 0, Some(1)), 1);
    // 묶음이 남아 있지 않으면 새로 시작하지만 다른 묶음 한가운데는 피한다
    assert_eq!(fit_insert(&ids("bb"), 1, Some(1)), 2);
    for start in all_layouts() {
        for at in 0..=start.len() {
            for b in [None, Some(1), Some(2), Some(9)] {
                let mut v = start.clone();
                let at = fit_insert(&v, at, b);
                v.insert(at, b);
                assert!(is_contiguous(&v), "{} at {at} {b:?} = {}", show(&start), show(&v));
            }
        }
    }
}

#[test]
fn clean_runs_keeps_the_first_run_and_ungroups_later_ones() {
    let mut v = ids("a.a.b");
    clean_runs(&mut v);
    assert_eq!(show(&v), "a...b");
    let mut v = ids("aabaa");
    clean_runs(&mut v);
    assert_eq!(show(&v), "aab..");
    let mut v = ids("abc");
    clean_runs(&mut v);
    assert_eq!(show(&v), "abc");
}

#[test]
fn registry_is_pruned_to_used_bundles_and_ids_are_fresh() {
    let mut reg = vec![bundle(1, false), bundle(2, true), bundle(5, false)];
    prune(&mut reg, &ids("bb..."));
    assert_eq!(reg.iter().map(|b| b.id).collect::<Vec<_>>(), [2]);
    assert_eq!(next_id(&[bundle(3, false), bundle(7, false)]), 8);
    assert_eq!(next_id(&[]), 1);
}

#[test]
fn bundle_names_are_cleaned_like_other_titles() {
    assert_eq!(clean_name("  에이전트 \u{1b}[31m "), Some("에이전트  [31m".to_string()));
    assert_eq!(clean_name(""), None);
    assert_eq!(clean_name("   "), None);
    assert_eq!(clean_name(&"가".repeat(500)).unwrap().chars().count(), NAME_MAX_CHARS);
}

#[test]
fn default_colors_cycle_through_nine_and_never_none() {
    let cs: Vec<_> = (1..=10).map(default_color).collect();
    assert!(cs.iter().all(|c| *c != TabColor::None));
    assert_eq!(cs[0], cs[9], "아홉 가지를 돈다");
    assert_ne!(cs[0], cs[1]);
}

#[test]
fn saved_bundles_keep_only_used_ones_and_load_distrusts_the_file() {
    use crate::state::SavedBundle;
    let mut b = bundle(2, true);
    b.color = TabColor::Teal;
    let reg = [bundle(1, false), b];
    let saved = saved_bundles(&reg, &ids("bb."));
    assert_eq!(saved.len(), 1);
    assert_eq!(
        (saved[0].id, saved[0].color.as_deref(), saved[0].collapsed),
        (2, Some("teal"), true)
    );
    // 읽기: 같은 번호 둘째는 버리고, 모르는 색은 없음, 이름의 제어 문자는 지운다
    let file = vec![
        SavedBundle {
            id: 4,
            name: "일\u{7}꾼".into(),
            color: Some("magenta".into()),
            collapsed: false,
        },
        SavedBundle {
            id: 4,
            name: "둘째".into(),
            color: None,
            collapsed: true,
        },
    ];
    let loaded = load_bundles(&file);
    assert_eq!(loaded.len(), 1);
    assert_eq!(loaded[0].color, TabColor::None);
    assert!(!loaded[0].name.contains('\u{7}'));
    // 탭: 등록부에 없는 번호와 떨어진 구간은 묶음 없음, 쓰이지 않는 묶음은 지운다
    let mut tabs = vec![Some(4), None, Some(4), Some(7)];
    let mut reg = load_bundles(&file);
    reg.push(bundle(9, false));
    settle_loaded(&mut tabs, &mut reg);
    assert_eq!(tabs, [Some(4), None, None, None]);
    assert_eq!(reg.iter().map(|b| b.id).collect::<Vec<_>>(), [4]);
}
