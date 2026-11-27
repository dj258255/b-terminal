//! 탭 묶음(#933): 탭을 이름과 색이 붙은 묶음으로 모은다(iTerm2 3.7 탭 그룹, Warp 탭 그룹, 브라우저의 탭 그룹 모양).
//! 이 파일은 창 없이 시험할 수 있는 순수한 모델이다. 탭마다 묶음 번호(`Option<Id>`)를 들고, 같은 묶음의 탭은 늘 이어져 있다.
//! 모든 연산은 "탭 번호 목록(`&mut Vec<Option<Id>>`)"을 고치고, 탭 자리가 바뀌었으면 `(from, to)`를 돌려준다.
//! 앱은 같은 `(from, to)`를 실제 탭 목록에 한 번 더 한다(`App::reorder_tab`처럼 `remove(from)` 뒤 `insert(to)`).

use bt_core::proto::printable;

use crate::tabcolor::TabColor;

/// 묶음 번호. 창마다 따로 센다
pub type Id = u32;

/// 묶음 이름의 상한(글자 수). 탭 제목(200자)보다 짧게 둔다: 머리표는 탭 줄의 한 칸이다
pub const NAME_MAX_CHARS: usize = 60;

/// 이름을 붙이지 않은 묶음을 부르는 이름
pub const DEFAULT_NAME: &str = "묶음";

/// 창이 들고 있는 묶음 하나
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Bundle {
    pub id: Id,
    pub name: String,
    pub color: TabColor,
    /// 접힘: 머리표만 보이고 안의 탭은 가려진다(고른 탭은 늘 보인다)
    pub collapsed: bool,
}

impl Bundle {
    pub fn new(id: Id) -> Self {
        Self {
            id,
            name: String::new(),
            color: TabColor::None,
            collapsed: false,
        }
    }
}

/// 제어 소켓 `list`가 내보낼 탭의 묶음 이름과 접힘(#952). 등록부에 없는 번호는 묶음 없음이다.
/// 이름은 소켓으로 나가는 글의 규칙(`export_text`)을 지난다
pub fn socket_group(bundle: Option<Id>, bundles: &[Bundle]) -> (Option<String>, bool) {
    let Some(b) = bundle.and_then(|id| bundles.iter().find(|b| b.id == id)) else {
        return (None, false);
    };
    let name = if b.name.is_empty() { DEFAULT_NAME } else { &b.name };
    (bt_core::proto::export_text(name), b.collapsed)
}

/// 탭 줄에 놓이는 것 하나: 묶음 머리표 또는 탭
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BarItem {
    Chip(Id),
    /// 탭 번호(`tabs`의 자리)
    Tab(usize),
}

/// 사람이 친 묶음 이름을 다듬는다: 제어 문자를 지우고 양끝 공백을 없애고 `NAME_MAX_CHARS`로 자른다. 비면 없다
pub fn clean_name(text: &str) -> Option<String> {
    let t: String = printable(text).trim().chars().take(NAME_MAX_CHARS).collect();
    let t = t.trim_end().to_string();
    (!t.is_empty()).then_some(t)
}

/// 탭 줄에 그릴 항목들. 묶음의 첫 탭 앞에 머리표를 놓고, 접힌 묶음의 탭은 고른 탭(`active`)만 남긴다.
/// 등록부에 없는 묶음 번호의 탭은 묶음 없는 탭으로 본다
pub fn bar_items(ids: &[Option<Id>], bundles: &[Bundle], active: usize) -> Vec<BarItem> {
    let mut out = Vec::with_capacity(ids.len());
    let mut run: Option<Id> = None;
    for (i, id) in ids.iter().enumerate() {
        let known = id.and_then(|b| bundles.iter().find(|x| x.id == b));
        match known {
            None => {
                run = None;
                out.push(BarItem::Tab(i));
            }
            Some(b) => {
                if run != Some(b.id) {
                    run = Some(b.id);
                    out.push(BarItem::Chip(b.id));
                }
                if !b.collapsed || i == active {
                    out.push(BarItem::Tab(i));
                }
            }
        }
    }
    out
}

/// 머리표에 적는 글: 이름(없으면 기본 이름), 접혔으면 뒤에 가려진 탭 수. `members`는 묶음의 탭 수, `shown`은 접혀도 보이는 탭 수
pub fn chip_title(b: &Bundle, members: usize, shown: usize) -> String {
    let name = if b.name.is_empty() { DEFAULT_NAME } else { &b.name };
    let hidden = if b.collapsed { members.saturating_sub(shown) } else { 0 };
    if hidden > 0 {
        format!("{name} +{hidden}")
    } else {
        name.to_string()
    }
}

/// 같은 묶음의 탭이 모두 이어져 있는가(시험이 모든 연산 뒤에 확인한다)
#[cfg(test)]
pub fn is_contiguous(ids: &[Option<Id>]) -> bool {
    let mut closed: Vec<Id> = Vec::new();
    let mut run: Option<Id> = None;
    for id in ids {
        if *id != run {
            if let Some(r) = run {
                closed.push(r);
            }
            if id.is_some_and(|b| closed.contains(&b)) {
                return false;
            }
            run = *id;
        }
    }
    true
}

/// 읽은 파일처럼 믿지 못하는 목록을 고친다: 한 묶음의 첫 구간만 남기고 뒤에 떨어진 구간은 묶음 없음으로 만든다
pub fn clean_runs(ids: &mut [Option<Id>]) {
    let mut seen: Vec<Id> = Vec::new();
    let mut run: Option<Id> = None;
    for id in ids.iter_mut() {
        match *id {
            None => run = None,
            Some(b) if run == Some(b) => {}
            Some(b) if seen.contains(&b) => {
                *id = None;
                run = None;
            }
            Some(b) => {
                seen.push(b);
                run = Some(b);
            }
        }
    }
}

/// 탭 `i`가 든 묶음의 이어진 구간 [처음, 끝]
fn run_of(ids: &[Option<Id>], i: usize) -> Option<(usize, usize)> {
    let b = ids.get(i).copied().flatten()?;
    let mut s = i;
    while s > 0 && ids[s - 1] == Some(b) {
        s -= 1;
    }
    let mut e = i;
    while e + 1 < ids.len() && ids[e + 1] == Some(b) {
        e += 1;
    }
    Some((s, e))
}

/// 묶음 `b`의 첫 구간 [처음, 끝]
fn first_run(ids: &[Option<Id>], b: Id) -> Option<(usize, usize)> {
    let s = ids.iter().position(|x| *x == Some(b))?;
    run_of(ids, s)
}

/// 탭 `i`가 자기 묶음의 유일한 탭인가(그러면 "새 묶음으로 묶기"는 할 일이 없다)
pub fn is_sole_member(ids: &[Option<Id>], i: usize) -> bool {
    run_of(ids, i).is_some_and(|(s, e)| s == e)
}

fn move_to(ids: &mut Vec<Option<Id>>, from: usize, to: usize, bundle: Option<Id>) {
    ids.remove(from);
    ids.insert(to.min(ids.len()), bundle);
}

/// 묶음에서 뺀다. 가운데 탭은 묶음 바로 뒤로 나가고(그 `(from, to)`를 돌려준다) 가장자리 탭은 제자리에서 묶음만 벗는다
pub fn leave(ids: &mut Vec<Option<Id>>, i: usize) -> Option<(usize, usize)> {
    let (s, e) = run_of(ids, i)?;
    if i == s || i == e {
        ids[i] = None;
        return None;
    }
    move_to(ids, i, e, None);
    Some((i, e))
}

/// 탭 `i`를 새 묶음 `id`에 혼자 넣는다. 다른 묶음의 가운데 탭이면 먼저 그 묶음 밖으로 나간다
pub fn new_bundle(ids: &mut Vec<Option<Id>>, i: usize, id: Id) -> Option<(usize, usize)> {
    let mv = leave(ids, i);
    let at = mv.map_or(i, |m| m.1);
    ids[at] = Some(id);
    mv
}

/// 탭 `i`를 옆 탭의 묶음에 넣는다(왼쪽 이웃 먼저, 없으면 오른쪽). 이웃이 자기와 같은 묶음이거나 묶음이 없으면 없다.
/// 이웃의 묶음과 맞닿은 가장자리일 때만 일어나므로 자리를 옮길 일이 없다
pub fn join_neighbor(ids: &mut [Option<Id>], i: usize) -> Option<Id> {
    let own = *ids.get(i)?;
    let left = i.checked_sub(1).and_then(|l| ids[l]);
    let right = ids.get(i + 1).copied().flatten();
    let joined = [left, right].into_iter().flatten().find(|b| Some(*b) != own)?;
    ids[i] = Some(joined);
    Some(joined)
}

/// 탭 `from`을 탭 `to` 자리로 옮기고 그 탭의 묶음을 이어받는다: 묶음의 탭 위에 놓으면 그 묶음에 들어가고,
/// 묶음 없는 탭 위에 놓으면 묶음에서 나온다. 키로 옮기기(`move_tab`)와 끌어 놓기가 같이 쓴다
pub fn reorder(ids: &mut Vec<Option<Id>>, from: usize, to: usize) {
    let bundle = ids.get(to).copied().flatten();
    move_to(ids, from, to, bundle);
}

/// 탭 `from`을 묶음 `b`의 머리표 위에 놓았다: 그 묶음의 맨 앞에 들어간다. 옮긴 `(from, to)`를 돌려준다(묶음이 없으면 제자리)
pub fn reorder_to_chip(ids: &mut Vec<Option<Id>>, from: usize, b: Id) -> (usize, usize) {
    let Some((t, _)) = first_run(ids, b) else {
        return (from, from);
    };
    let to = if from < t { t - 1 } else { t };
    move_to(ids, from, to, Some(b));
    (from, to)
}

/// 새 탭(또는 되살린 탭)을 `at`에 끼우려 할 때 묶음 구간을 깨지 않는 자리. 묶음이 있고 구간이 남아 있으면 그 구간 안(양 끝 포함)으로,
/// 아니면 다른 묶음 한가운데를 피해 그 묶음 뒤로 민다
pub fn fit_insert(ids: &[Option<Id>], at: usize, bundle: Option<Id>) -> usize {
    let at = at.min(ids.len());
    if let Some((s, e)) = bundle.and_then(|b| first_run(ids, b)) {
        return at.clamp(s, e + 1);
    }
    if at > 0 && at < ids.len() && ids[at - 1].is_some() && ids[at - 1] == ids[at] {
        if let Some((_, e)) = run_of(ids, at) {
            return e + 1;
        }
    }
    at
}

/// 쓰이지 않는 묶음을 등록부에서 지운다
pub fn prune(bundles: &mut Vec<Bundle>, ids: &[Option<Id>]) {
    bundles.retain(|b| ids.contains(&Some(b.id)));
}

/// 새 묶음 번호
pub fn next_id(bundles: &[Bundle]) -> Id {
    bundles.iter().map(|b| b.id).max().map_or(1, |m| m.saturating_add(1))
}

/// 묶음 하나에 하는 일(키 동작·메뉴가 같이 쓴다)
#[derive(Debug, Clone, PartialEq)]
pub enum BundleOp {
    /// 접기·펴기
    Toggle,
    /// 이름을 입력 막대로 묻는다(탭 제목 바꾸기와 같은 길)
    Rename,
    /// 이름을 정한다. 빈 글이면 기본 이름으로 돌아간다
    SetTitle(String),
    Color(TabColor),
    /// 묶음을 풀어 모든 탭을 묶음 없음으로 만든다(탭은 그대로 남는다)
    Dissolve,
}

/// 새 묶음의 처음 색: 번호마다 달라 나란한 묶음이 구별된다(색 없음을 뺀 아홉 가지를 돈다)
pub fn default_color(id: Id) -> TabColor {
    TabColor::ALL[1 + (id.saturating_sub(1) as usize) % (TabColor::ALL.len() - 1)]
}

/// 창 상태 파일에 적을 묶음들: 탭이 든 묶음만, 등록 순서대로
pub fn saved_bundles(bundles: &[Bundle], ids: &[Option<Id>]) -> Vec<crate::state::SavedBundle> {
    bundles
        .iter()
        .filter(|b| ids.contains(&Some(b.id)))
        .map(|b| crate::state::SavedBundle {
            id: b.id,
            name: b.name.clone(),
            color: (b.color != TabColor::None).then(|| b.color.key().to_string()),
            collapsed: b.collapsed,
        })
        .collect()
}

/// 파일에서 읽은 묶음들을 믿을 수 있는 등록부로: 같은 번호는 첫 것만, 이름은 다시 다듬고, 모르는 색 이름은 색 없음
pub fn load_bundles(saved: &[crate::state::SavedBundle]) -> Vec<Bundle> {
    let mut out: Vec<Bundle> = Vec::new();
    for s in saved {
        if out.iter().any(|b| b.id == s.id) {
            continue;
        }
        out.push(Bundle {
            id: s.id,
            name: clean_name(&s.name).unwrap_or_default(),
            color: TabColor::from_saved(s.color.as_deref()),
            collapsed: s.collapsed,
        });
    }
    out
}

/// 읽은 탭들의 묶음 번호를 등록부에 맞춘다: 등록부에 없는 번호는 묶음 없음, 떨어진 구간도 묶음 없음
pub fn settle_loaded(ids: &mut [Option<Id>], bundles: &mut Vec<Bundle>) {
    for id in ids.iter_mut() {
        if id.is_some_and(|b| !bundles.iter().any(|x| x.id == b)) {
            *id = None;
        }
    }
    clean_runs(ids);
    prune(bundles, ids);
}
