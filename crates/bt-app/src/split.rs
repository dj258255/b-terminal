//! 탭 안의 화면 분할(#35). 칸(pane) id를 잎으로 둔 이진 트리와 배치 계산.
//!
//! 창 그리기와 상관없는 순수 계산이라 여기서 테스트한다. 앱은 트리를 들고 있다가
//! `layout`으로 칸마다 사각형을 받아 그 크기로 PTY를 맞추고 그린다

/// 픽셀 사각형
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Rect {
    pub x: i32,
    pub y: i32,
    pub w: u32,
    pub h: u32,
}

impl Rect {
    pub fn new(x: i32, y: i32, w: u32, h: u32) -> Self {
        Self { x, y, w, h }
    }

    pub fn contains(&self, px: f64, py: f64) -> bool {
        px >= self.x as f64 && py >= self.y as f64 && px < self.right() as f64 && py < self.bottom() as f64
    }

    pub fn right(&self) -> i32 {
        self.x + self.w as i32
    }

    pub fn bottom(&self) -> i32 {
        self.y + self.h as i32
    }
}

/// 나눈 방향. `Horizontal`은 좌우로 나란히, `Vertical`은 위아래로 쌓는다
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Axis {
    Horizontal,
    Vertical,
}

/// 새 칸을 놓을 쪽, 옮겨 갈 쪽, 경계를 밀 쪽
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Direction {
    Left,
    Right,
    Up,
    Down,
}

impl Direction {
    fn axis(self) -> Axis {
        match self {
            Direction::Left | Direction::Right => Axis::Horizontal,
            Direction::Up | Direction::Down => Axis::Vertical,
        }
    }

    /// 새 칸이 뒤쪽(오른쪽·아래)에 붙는가
    fn after(self) -> bool {
        matches!(self, Direction::Right | Direction::Down)
    }
}

/// 비율의 한계. 경계를 끝까지 밀어도 칸이 사라지지 않게
const MIN_RATIO: f32 = 0.1;
const MAX_RATIO: f32 = 0.9;

/// 칸 하나가 트리에서 놓인 자리(#164): 붙어 있던 이웃 묶음의 잎들(`sibling`), 그 묶음의 가까운 칸(`anchor`),
/// 나눈 방향과 비율, 앞쪽(왼쪽·위)이었는가
#[derive(Debug, Clone, PartialEq)]
pub struct Slot {
    pub sibling: Vec<u64>,
    pub anchor: u64,
    pub axis: Axis,
    pub ratio: f32,
    pub before: bool,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Node {
    Leaf(u64),
    Split {
        axis: Axis,
        /// 앞 칸이 차지하는 몫(0~1)
        ratio: f32,
        first: Box<Node>,
        second: Box<Node>,
    },
}

/// 사각형을 비율로 나눈다. 가운데 `divider` 픽셀은 경계선 자리
fn divide(rect: Rect, axis: Axis, ratio: f32, divider: u32) -> (Rect, Rect, Rect) {
    match axis {
        Axis::Horizontal => {
            let room = rect.w.saturating_sub(divider);
            let a = (room as f32 * ratio).round() as u32;
            let b = room - a;
            let first = Rect::new(rect.x, rect.y, a, rect.h);
            let line = Rect::new(rect.x + a as i32, rect.y, rect.w - a - b, rect.h);
            let second = Rect::new(line.right(), rect.y, b, rect.h);
            (first, line, second)
        }
        Axis::Vertical => {
            let room = rect.h.saturating_sub(divider);
            let a = (room as f32 * ratio).round() as u32;
            let b = room - a;
            let first = Rect::new(rect.x, rect.y, rect.w, a);
            let line = Rect::new(rect.x, rect.y + a as i32, rect.w, rect.h - a - b);
            let second = Rect::new(rect.x, line.bottom(), rect.w, b);
            (first, line, second)
        }
    }
}

impl Node {
    /// 잎을 왼쪽·위부터 차례로
    pub fn leaves(&self) -> Vec<u64> {
        let mut out = Vec::new();
        self.collect(&mut out);
        out
    }

    fn collect(&self, out: &mut Vec<u64>) {
        match self {
            Node::Leaf(id) => out.push(*id),
            Node::Split { first, second, .. } => {
                first.collect(out);
                second.collect(out);
            }
        }
    }

    pub fn contains(&self, id: u64) -> bool {
        match self {
            Node::Leaf(x) => *x == id,
            Node::Split { first, second, .. } => first.contains(id) || second.contains(id),
        }
    }

    /// `target` 칸을 둘로 나눠 `dir` 쪽에 `new` 칸을 놓는다. 반씩 나눈다
    pub fn split(&mut self, target: u64, new: u64, dir: Direction) -> bool {
        match self {
            Node::Leaf(id) if *id == target => {
                let (first, second) = if dir.after() {
                    (Node::Leaf(target), Node::Leaf(new))
                } else {
                    (Node::Leaf(new), Node::Leaf(target))
                };
                *self = Node::Split {
                    axis: dir.axis(),
                    ratio: 0.5,
                    first: Box::new(first),
                    second: Box::new(second),
                };
                true
            }
            Node::Leaf(_) => false,
            Node::Split { first, second, .. } => first.split(target, new, dir) || second.split(target, new, dir),
        }
    }

    /// `target` 칸을 뺀다. 남은 형제가 부모 자리를 채운다.
    /// 돌려주는 값: (남은 트리, 대신 고를 칸). 트리가 비면 `None`
    pub fn remove(self, target: u64) -> (Option<Node>, Option<u64>) {
        match self {
            Node::Leaf(id) if id == target => (None, None),
            leaf @ Node::Leaf(_) => (Some(leaf), None),
            Node::Split {
                axis,
                ratio,
                first,
                second,
            } => {
                if *first == Node::Leaf(target) {
                    // 뒤 칸이 앞으로 당겨진다: 뺀 칸과 붙어 있던 쪽(뒤 칸의 첫 잎)을 고른다
                    let focus = second.leaves().first().copied();
                    return (Some(*second), focus);
                }
                if *second == Node::Leaf(target) {
                    let focus = first.leaves().last().copied();
                    return (Some(*first), focus);
                }
                let (first, f1) = if first.contains(target) {
                    first.remove(target)
                } else {
                    (Some(*first), None)
                };
                let (second, f2) = if second.contains(target) {
                    second.remove(target)
                } else {
                    (Some(*second), None)
                };
                match (first, second) {
                    (Some(a), Some(b)) => (
                        Some(Node::Split {
                            axis,
                            ratio,
                            first: Box::new(a),
                            second: Box::new(b),
                        }),
                        f1.or(f2),
                    ),
                    (Some(n), None) | (None, Some(n)) => (Some(n), f1.or(f2)),
                    (None, None) => (None, None),
                }
            }
        }
    }

    /// `from` 칸을 떼어 `to` 칸의 `dir` 쪽으로 나눠 붙인다(칸 끌어 옮기기). 비율은 반반이다.
    /// 칸이 하나이거나 `from == to`이거나 어느 칸이 없으면, 또는 옮겨도 트리가 그대로면 아무것도 하지 않고 거짓
    pub fn move_pane(&mut self, from: u64, to: u64, dir: Direction) -> bool {
        if from == to || !self.contains(from) || !self.contains(to) {
            return false;
        }
        // 칸이 둘 이상이니 하나를 빼도 트리가 남는다. 실패하면 원래 트리를 건드리지 않게 복사본에서 한다
        let (rest, _) = self.clone().remove(from);
        let Some(mut next) = rest else { return false };
        if !next.split(to, from, dir) || next == *self {
            return false;
        }
        *self = next;
        true
    }

    /// `target` 칸이 트리에서 놓인 자리(되살릴 때 쓴다). 칸이 하나뿐이면 `None`
    pub fn slot_of(&self, target: u64) -> Option<Slot> {
        let Node::Split {
            axis,
            ratio,
            first,
            second,
        } = self
        else {
            return None;
        };
        let slot = |sibling: &Node, anchor: Option<u64>, before: bool| {
            anchor.map(|anchor| Slot {
                sibling: sibling.leaves(),
                anchor,
                axis: *axis,
                ratio: *ratio,
                before,
            })
        };
        if **first == Node::Leaf(target) {
            return slot(second, second.leaves().first().copied(), true);
        }
        if **second == Node::Leaf(target) {
            return slot(first, first.leaves().last().copied(), false);
        }
        first.slot_of(target).or_else(|| second.slot_of(target))
    }

    /// `slot`이 가리키는 자리에 `new` 칸을 되살린다. 기준 칸이 없으면 거짓
    /// 이웃 묶음이 그대로 있으면 그 묶음 곁에, 달라졌으면 가까운 칸 하나를 나눠 그 곁에 놓는다
    pub fn insert_at(&mut self, slot: &Slot, new: u64) -> bool {
        self.wrap_group(slot, new) || self.wrap_leaf(slot, new)
    }

    fn joined(slot: &Slot, old: Node, new: u64) -> Node {
        let (first, second) = if slot.before {
            (Node::Leaf(new), old)
        } else {
            (old, Node::Leaf(new))
        };
        Node::Split {
            axis: slot.axis,
            ratio: slot.ratio,
            first: Box::new(first),
            second: Box::new(second),
        }
    }

    fn wrap_group(&mut self, slot: &Slot, new: u64) -> bool {
        if self.leaves() == slot.sibling {
            let old = std::mem::replace(self, Node::Leaf(new));
            *self = Self::joined(slot, old, new);
            return true;
        }
        match self {
            Node::Leaf(_) => false,
            Node::Split { first, second, .. } => first.wrap_group(slot, new) || second.wrap_group(slot, new),
        }
    }

    fn wrap_leaf(&mut self, slot: &Slot, new: u64) -> bool {
        match self {
            Node::Leaf(id) if *id == slot.anchor => {
                let old = Node::Leaf(*id);
                *self = Self::joined(slot, old, new);
                true
            }
            Node::Leaf(_) => false,
            Node::Split { first, second, .. } => first.wrap_leaf(slot, new) || second.wrap_leaf(slot, new),
        }
    }

    /// 칸마다 사각형(경계선 자리를 뺀). 잎 순서와 같다
    pub fn layout(&self, rect: Rect, divider: u32) -> Vec<(u64, Rect)> {
        let mut out = Vec::new();
        self.place(rect, divider, &mut out, &mut Vec::new());
        out
    }

    /// 경계선 사각형들(그릴 자리)
    pub fn dividers(&self, rect: Rect, divider: u32) -> Vec<Rect> {
        let mut lines = Vec::new();
        self.place(rect, divider, &mut Vec::new(), &mut lines);
        lines
    }

    fn place(&self, rect: Rect, divider: u32, out: &mut Vec<(u64, Rect)>, lines: &mut Vec<Rect>) {
        match self {
            Node::Leaf(id) => out.push((*id, rect)),
            Node::Split {
                axis,
                ratio,
                first,
                second,
            } => {
                let (a, line, b) = divide(rect, *axis, *ratio, divider);
                lines.push(line);
                first.place(a, divider, out, lines);
                second.place(b, divider, out, lines);
            }
        }
    }

    /// `from` 칸에서 `dir` 쪽으로 붙어 있는 칸. 그쪽에 여럿이면 가장 가깝고, 같으면 겹치는 길이가 긴 칸
    pub fn neighbor(&self, rect: Rect, divider: u32, from: u64, dir: Direction) -> Option<u64> {
        let rects = self.layout(rect, divider);
        let (_, f) = *rects.iter().find(|(id, _)| *id == from)?;
        let overlap = |a0: i32, a1: i32, b0: i32, b1: i32| (a1.min(b1) - a0.max(b0)).max(0);
        rects
            .iter()
            .filter(|(id, _)| *id != from)
            .filter_map(|(id, r)| {
                let (gap, shared) = match dir {
                    Direction::Right => (r.x - f.right(), overlap(f.y, f.bottom(), r.y, r.bottom())),
                    Direction::Left => (f.x - r.right(), overlap(f.y, f.bottom(), r.y, r.bottom())),
                    Direction::Down => (r.y - f.bottom(), overlap(f.x, f.right(), r.x, r.right())),
                    Direction::Up => (f.y - r.bottom(), overlap(f.x, f.right(), r.x, r.right())),
                };
                (gap >= 0 && shared > 0).then_some((gap, -shared, *id))
            })
            .min()
            .map(|(_, _, id)| id)
    }

    /// 잎 순서로 다음(+1)·앞(-1) 칸. 끝에서 처음으로 돈다
    pub fn cycle(&self, from: u64, step: i32) -> Option<u64> {
        let leaves = self.leaves();
        let i = leaves.iter().position(|x| *x == from)? as i32;
        let n = leaves.len() as i32;
        Some(leaves[(i + step).rem_euclid(n) as usize])
    }

    /// `target` 칸을 감싼 경계 중 `dir`과 같은 축의 가장 가까운 경계를 `px` 픽셀 민다
    pub fn resize(&mut self, rect: Rect, divider: u32, target: u64, dir: Direction, px: u32) -> bool {
        match self {
            Node::Leaf(_) => false,
            Node::Split {
                axis,
                ratio,
                first,
                second,
            } => {
                let (a, _, b) = divide(rect, *axis, *ratio, divider);
                // 더 안쪽 경계가 먼저 받는다
                let inner = if first.contains(target) {
                    first.resize(a, divider, target, dir, px)
                } else if second.contains(target) {
                    second.resize(b, divider, target, dir, px)
                } else {
                    return false;
                };
                if inner || *axis != dir.axis() {
                    return inner;
                }
                let span = match axis {
                    Axis::Horizontal => rect.w.saturating_sub(divider),
                    Axis::Vertical => rect.h.saturating_sub(divider),
                };
                if span == 0 {
                    return false;
                }
                let delta = px as f32 / span as f32;
                let next = if dir.after() { *ratio + delta } else { *ratio - delta };
                *ratio = next.clamp(MIN_RATIO, MAX_RATIO);
                true
            }
        }
    }

    /// 경계선마다 정보(그리는 순서와 같다)
    pub fn divider_infos(&self, rect: Rect, divider: u32) -> Vec<DividerInfo> {
        let mut out = Vec::new();
        self.collect_dividers(rect, divider, &mut Vec::new(), &mut out);
        out
    }

    fn collect_dividers(&self, rect: Rect, divider: u32, path: &mut Vec<bool>, out: &mut Vec<DividerInfo>) {
        if let Node::Split {
            axis,
            ratio,
            first,
            second,
        } = self
        {
            let (a, line, b) = divide(rect, *axis, *ratio, divider);
            out.push(DividerInfo {
                path: path.clone(),
                axis: *axis,
                parent: rect,
                line,
            });
            path.push(false);
            first.collect_dividers(a, divider, path, out);
            path.pop();
            path.push(true);
            second.collect_dividers(b, divider, path, out);
            path.pop();
        }
    }

    /// `path`가 가리키는 분할의 비율을 바꾼다. 길이 없으면(트리가 바뀜) `false`
    pub fn set_ratio_at(&mut self, path: &[bool], new: f32) -> bool {
        match (self, path.split_first()) {
            (Node::Split { ratio, .. }, None) => {
                *ratio = new;
                true
            }
            (Node::Split { first, second, .. }, Some((&to_second, rest))) => {
                if to_second {
                    second.set_ratio_at(rest, new)
                } else {
                    first.set_ratio_at(rest, new)
                }
            }
            _ => false,
        }
    }

    /// 같은 축으로 이어진 칸들이 같은 폭(높이)을 갖게 비율을 맞춘다(Ghostty의 equalize_splits)
    pub fn equalize(&mut self) {
        if let Node::Split {
            axis,
            ratio,
            first,
            second,
        } = self
        {
            first.equalize();
            second.equalize();
            let (a, b) = (first.weight(*axis), second.weight(*axis));
            *ratio = a as f32 / (a + b) as f32;
        }
    }

    /// 이 축으로 늘어선 칸 수
    fn weight(&self, along: Axis) -> u32 {
        match self {
            Node::Split {
                axis, first, second, ..
            } if *axis == along => first.weight(along) + second.weight(along),
            _ => 1,
        }
    }
}

/// 경계선 하나(마우스로 맞히고 끌 때 쓴다). `path`는 루트에서 그 분할까지 가는 길(앞 칸 쪽이 `false`, 뒤 칸 쪽이 `true`)이라
/// 끄는 동안 트리가 그대로면 같은 노드를 다시 찾는다
#[derive(Debug, Clone, PartialEq)]
pub struct DividerInfo {
    pub path: Vec<bool>,
    pub axis: Axis,
    /// 이 분할이 차지한 사각형(경계선 자리 포함)
    pub parent: Rect,
    /// 경계선 사각형(그리는 자리)
    pub line: Rect,
}

/// 마우스가 경계선 띠 위인가. 띠는 경계선을 나눈 방향으로 양쪽 `reach` 픽셀 넓힌 것이다. 여러 개가 걸리면 가운데가 가장 가까운 것
pub fn hit_divider(infos: &[DividerInfo], x: f64, y: f64, reach: u32) -> Option<&DividerInfo> {
    let reach = reach as f64;
    infos
        .iter()
        .filter_map(|d| {
            let l = d.line;
            // 나눈 방향의 폭만 넓힌다. 선의 길이 쪽은 그대로
            let (inside, off) = match d.axis {
                Axis::Horizontal => {
                    let c = l.x as f64 + l.w as f64 / 2.0;
                    (
                        x >= l.x as f64 - reach
                            && x < l.right() as f64 + reach
                            && y >= l.y as f64
                            && y < l.bottom() as f64,
                        (x - c).abs(),
                    )
                }
                Axis::Vertical => {
                    let c = l.y as f64 + l.h as f64 / 2.0;
                    (
                        y >= l.y as f64 - reach
                            && y < l.bottom() as f64 + reach
                            && x >= l.x as f64
                            && x < l.right() as f64,
                        (y - c).abs(),
                    )
                }
            };
            inside.then_some((off, d))
        })
        .min_by(|a, b| a.0.total_cmp(&b.0))
        .map(|(_, d)| d)
}

/// 마우스를 `x`·`y`로 끌 때 경계선 `info`의 새 비율. 양쪽 칸이 각각 `min_px`보다 작아지지 않게 자른다. 둘을 못 담으면 `None`
pub fn ratio_for_pointer(info: &DividerInfo, divider: u32, x: f64, y: f64, min_px: u32) -> Option<f32> {
    let (origin, span, pos) = match info.axis {
        Axis::Horizontal => (info.parent.x, info.parent.w, x),
        Axis::Vertical => (info.parent.y, info.parent.h, y),
    };
    let room = span.saturating_sub(divider);
    if room < min_px.saturating_mul(2) || room == 0 {
        return None;
    }
    // 앞 칸의 몫 = 마우스까지의 거리에서 경계선 두께의 절반을 뺀 것(Ghostty는 마우스 자리를 그대로 비율로 쓴다)
    let front = (pos - origin as f64 - divider as f64 / 2.0).clamp(min_px as f64, (room - min_px) as f64);
    Some((front / room as f64) as f32)
}

/// 칸을 끌어다 놓을 때 칸 안의 한 점이 가리키는 쪽. 가장 가까운 변이다(변까지의 거리를 칸 크기로 나눠 견준다: 대각선이 칸을 네 삼각형으로 나눈다).
/// 같으면 왼쪽, 오른쪽, 위, 아래 차례(Ghostty `TerminalSplitDropZone.calculate`와 같다). 칸 밖이거나 크기가 0이면 `None`
pub fn drop_direction(rect: Rect, x: f64, y: f64) -> Option<Direction> {
    if rect.w == 0 || rect.h == 0 || !rect.contains(x, y) {
        return None;
    }
    let rel_x = (x - rect.x as f64) / rect.w as f64;
    let rel_y = (y - rect.y as f64) / rect.h as f64;
    let dists = [
        (rel_x, Direction::Left),
        (1.0 - rel_x, Direction::Right),
        (rel_y, Direction::Up),
        (1.0 - rel_y, Direction::Down),
    ];
    let min = dists.iter().map(|d| d.0).fold(f64::INFINITY, f64::min);
    dists.iter().find(|d| d.0 == min).map(|d| d.1)
}

/// 놓을 쪽의 미리 보기 사각형: 칸의 그쪽 절반
pub fn drop_preview(rect: Rect, dir: Direction) -> Rect {
    let (half_w, half_h) = (rect.w / 2, rect.h / 2);
    match dir {
        Direction::Left => Rect::new(rect.x, rect.y, half_w, rect.h),
        Direction::Right => Rect::new(rect.x + half_w as i32, rect.y, rect.w - half_w, rect.h),
        Direction::Up => Rect::new(rect.x, rect.y, rect.w, half_h),
        Direction::Down => Rect::new(rect.x, rect.y + half_h as i32, rect.w, rect.h - half_h),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const WIN: Rect = Rect {
        x: 0,
        y: 0,
        w: 801,
        h: 601,
    };

    #[test]
    fn divider_band_is_wider_than_the_line_and_picks_the_nearest() {
        let mut t = Node::Leaf(1);
        t.split(1, 2, Direction::Right);
        let infos = t.divider_infos(WIN, 1);
        assert_eq!(infos.len(), 1);
        assert_eq!(infos[0].line, Rect::new(400, 0, 1, 601));
        assert_eq!(infos[0].axis, Axis::Horizontal);
        // 선 자리와 양쪽 3픽셀까지는 맞는다
        assert!(hit_divider(&infos, 400.5, 10.0, 3).is_some());
        assert!(hit_divider(&infos, 397.0, 10.0, 3).is_some());
        assert!(hit_divider(&infos, 403.9, 10.0, 3).is_some());
        assert!(hit_divider(&infos, 396.9, 10.0, 3).is_none());
        assert!(hit_divider(&infos, 404.0, 10.0, 3).is_none());
        // 선의 길이 밖은 아니다
        assert!(hit_divider(&infos, 400.5, 602.0, 3).is_none());
    }

    #[test]
    fn nested_dividers_are_found_by_path_and_the_closest_wins() {
        // [1 | (2 / 3)]: 가운데 세로 경계선과 오른쪽 가로 경계선
        let mut t = Node::Leaf(1);
        t.split(1, 2, Direction::Right);
        t.split(2, 3, Direction::Down);
        let infos = t.divider_infos(WIN, 1);
        assert_eq!(infos.len(), 2);
        assert_eq!(infos[0].path, Vec::<bool>::new());
        assert_eq!(infos[1].path, vec![true]);
        assert_eq!(infos[1].axis, Axis::Vertical);
        let h = infos[1].line;
        // 가로 경계선의 한가운데에서 가까운 위쪽은 가로 경계선이 맞는다
        let hit = hit_divider(&infos, 600.0, h.y as f64 + 0.5, 3).unwrap();
        assert_eq!(hit.path, vec![true]);
        // 세로 경계선과 가로 경계선의 이음매: 가운데가 더 가까운 쪽
        let hit = hit_divider(&infos, 401.0, h.y as f64 + 2.0, 3).unwrap();
        assert_eq!(hit.path, Vec::<bool>::new());
    }

    #[test]
    fn dragging_sets_the_ratio_of_only_that_node_and_keeps_the_minimum() {
        let mut t = Node::Leaf(1);
        t.split(1, 2, Direction::Right);
        t.split(2, 3, Direction::Down);
        let infos = t.divider_infos(WIN, 1);
        let outer = &infos[0];
        // 800 중 200에 놓는다
        let r = ratio_for_pointer(outer, 1, 200.5, 10.0, 10).unwrap();
        assert!((r - 0.25).abs() < 0.01, "{r}");
        // 끝으로 끌어도 각 칸 10픽셀은 남는다
        let lo = ratio_for_pointer(outer, 1, -50.0, 10.0, 10).unwrap();
        let hi = ratio_for_pointer(outer, 1, 5000.0, 10.0, 10).unwrap();
        assert!((lo - 10.0 / 800.0).abs() < 1e-4, "{lo}");
        assert!((hi - 790.0 / 800.0).abs() < 1e-4, "{hi}");
        // 안쪽(가로) 경계선은 오른쪽 사각형 안의 세로 위치로 잰다
        let inner = &infos[1];
        let r = ratio_for_pointer(inner, 1, 600.0, 150.0, 10).unwrap();
        assert!((r - 150.0 / 600.0).abs() < 0.01, "{r}");
        // 두 칸이 최소를 못 담으면 바꾸지 않는다
        let tiny = DividerInfo {
            path: vec![],
            axis: Axis::Horizontal,
            parent: Rect::new(0, 0, 15, 10),
            line: Rect::new(7, 0, 1, 10),
        };
        assert_eq!(ratio_for_pointer(&tiny, 1, 7.0, 5.0, 10), None);
        // 길을 따라 그 노드의 비율만 바꾼다
        let before = t.layout(WIN, 1);
        assert!(t.set_ratio_at(&[true], 0.25));
        let after = t.layout(WIN, 1);
        assert_eq!(before[0], after[0]);
        assert_ne!(before[1], after[1]);
        assert!(!t.set_ratio_at(&[true, false], 0.5));
        assert!(t.set_ratio_at(&[], 0.3));
        assert!(!Node::Leaf(1).set_ratio_at(&[], 0.5));
    }

    #[test]
    fn split_right_puts_new_pane_after_and_splits_width() {
        let mut t = Node::Leaf(1);
        assert!(t.split(1, 2, Direction::Right));
        assert_eq!(t.leaves(), vec![1, 2]);
        let l = t.layout(WIN, 1);
        // 801 - 경계 1 = 800을 반씩
        assert_eq!(l[0], (1, Rect::new(0, 0, 400, 601)));
        assert_eq!(l[1], (2, Rect::new(401, 0, 400, 601)));
        assert_eq!(t.dividers(WIN, 1), vec![Rect::new(400, 0, 1, 601)]);
    }

    #[test]
    fn split_left_and_up_put_new_pane_first() {
        let mut t = Node::Leaf(1);
        t.split(1, 2, Direction::Up);
        assert_eq!(t.leaves(), vec![2, 1]);
        let l = t.layout(WIN, 1);
        assert_eq!(l[0], (2, Rect::new(0, 0, 801, 300)));
        assert_eq!(l[1], (1, Rect::new(0, 301, 801, 300)));
        assert!(!t.split(9, 3, Direction::Left), "없는 칸은 나누지 않는다");
    }

    #[test]
    fn remove_collapses_parent_and_picks_adjacent_pane() {
        // 1 | (2 / 3)
        let mut t = Node::Leaf(1);
        t.split(1, 2, Direction::Right);
        t.split(2, 3, Direction::Down);
        let (t2, focus) = t.clone().remove(2);
        let t2 = t2.unwrap();
        assert_eq!(t2.leaves(), vec![1, 3]);
        assert_eq!(focus, Some(3), "2가 빠지면 아래 칸 3이 그 자리를 채운다");
        // 3이 빠지면 같은 묶음의 2가 고르게 된다
        let (_, focus) = t.clone().remove(3);
        assert_eq!(focus, Some(2));
        // 1(앞 칸)이 빠지면 뒤 묶음의 첫 칸
        let (t3, focus) = t.clone().remove(1);
        assert_eq!(t3.unwrap().leaves(), vec![2, 3]);
        assert_eq!(focus, Some(2));
        // 마지막 칸이 빠지면 트리가 빈다
        assert_eq!(Node::Leaf(7).remove(7), (None, None));
    }

    #[test]
    fn closed_pane_returns_to_its_slot() {
        // 1 | (2 / 3) 에서 비율을 바꿔 둔 뒤 각 칸을 빼고 되살리면 같은 트리가 된다
        let mut t = Node::Leaf(1);
        t.split(1, 2, Direction::Right);
        t.split(2, 3, Direction::Down);
        if let Node::Split { ratio, .. } = &mut t {
            *ratio = 0.3;
        }
        for target in [1, 2, 3] {
            let slot = t.slot_of(target).expect("칸이 둘 이상이면 자리가 있다");
            let (rest, _) = t.clone().remove(target);
            let mut rest = rest.unwrap();
            assert!(rest.insert_at(&slot, target), "{target}");
            assert_eq!(rest, t, "{target}을 되살리면 원래 트리");
        }
    }

    #[test]
    fn slot_of_single_pane_or_unknown_is_none() {
        assert_eq!(Node::Leaf(1).slot_of(1), None);
        let mut t = Node::Leaf(1);
        t.split(1, 2, Direction::Right);
        assert_eq!(t.slot_of(9), None);
        let mut gone = Node::Leaf(5);
        assert!(
            !gone.insert_at(&t.slot_of(2).unwrap(), 2),
            "기준 칸 1이 없으면 되살리지 못한다"
        );
    }

    #[test]
    fn neighbor_follows_geometry_not_tree_order() {
        // 1 | (2 / 3): 3에서 왼쪽은 1, 위는 2, 오른쪽은 없음
        let mut t = Node::Leaf(1);
        t.split(1, 2, Direction::Right);
        t.split(2, 3, Direction::Down);
        assert_eq!(t.neighbor(WIN, 1, 3, Direction::Left), Some(1));
        assert_eq!(t.neighbor(WIN, 1, 3, Direction::Up), Some(2));
        assert_eq!(t.neighbor(WIN, 1, 3, Direction::Right), None);
        assert_eq!(
            t.neighbor(WIN, 1, 1, Direction::Right),
            Some(2),
            "같은 거리면 위쪽부터(겹침이 같다)"
        );
        assert_eq!(t.cycle(3, 1), Some(1));
        assert_eq!(t.cycle(1, -1), Some(3));
    }

    #[test]
    fn resize_moves_the_nearest_divider_on_the_same_axis() {
        // 1 | (2 / 3). 3에서 위로 밀면 안쪽(위아래) 경계가, 왼쪽으로 밀면 바깥(좌우) 경계가 움직인다
        let mut t = Node::Leaf(1);
        t.split(1, 2, Direction::Right);
        t.split(2, 3, Direction::Down);
        assert!(t.resize(WIN, 1, 3, Direction::Up, 60));
        let l = t.layout(WIN, 1);
        assert_eq!(l[1].1.h, 240, "2는 300에서 60 줄어든다");
        assert_eq!(l[2].1.h, 360);
        assert!(t.resize(WIN, 1, 3, Direction::Left, 80));
        let l = t.layout(WIN, 1);
        assert_eq!(l[0].1.w, 320, "1은 400에서 80 줄어든다");
        // 끝까지 밀어도 칸이 사라지지 않는다
        for _ in 0..50 {
            t.resize(WIN, 1, 1, Direction::Left, 100);
        }
        assert!(t.layout(WIN, 1)[0].1.w >= 80);
        // 나눈 적 없는 칸은 밀 경계가 없다
        assert!(!Node::Leaf(1).resize(WIN, 1, 1, Direction::Left, 10));
        // 같은 축으로 겹쳐 나눈 1 | (2 | 3): 3을 왼쪽으로 밀면 2와 3 사이 경계만 움직이고 1은 그대로
        let mut t = Node::Leaf(1);
        t.split(1, 2, Direction::Right);
        t.split(2, 3, Direction::Right);
        // 803 - 1 = 802 → 401 | 401, 오른쪽 401 - 1 = 400 → 200 | 200(반올림 없이 떨어지는 폭)
        let win = Rect::new(0, 0, 803, 100);
        let before = t.layout(win, 1)[0].1.w;
        assert!(t.resize(win, 1, 3, Direction::Left, 40));
        let l = t.layout(win, 1);
        assert_eq!(l[0].1.w, before, "바깥 경계는 움직이지 않는다");
        assert_eq!(l[2].1.w, l[1].1.w + 80);
    }

    #[test]
    fn equalize_gives_each_pane_on_an_axis_the_same_width() {
        // 1 | 2 를 만든 뒤 2를 다시 좌우로: 1 | (2 | 3). 맞추면 셋이 같은 폭
        let mut t = Node::Leaf(1);
        t.split(1, 2, Direction::Right);
        t.split(2, 3, Direction::Right);
        t.equalize();
        let win = Rect::new(0, 0, 302, 100);
        let widths: Vec<u32> = t.layout(win, 1).iter().map(|(_, r)| r.w).collect();
        assert_eq!(widths, vec![100, 100, 100]);
    }

    fn tree_1_2_3() -> Node {
        // 1 | (2 / 3)
        let mut t = Node::Leaf(1);
        t.split(1, 2, Direction::Right);
        t.split(2, 3, Direction::Down);
        t
    }

    #[test]
    fn move_pane_detaches_and_splits_target_in_half() {
        let mut t = tree_1_2_3();
        // 1을 3의 오른쪽으로: (2 / (3 | 1))
        assert!(t.move_pane(1, 3, Direction::Right));
        assert_eq!(t.leaves(), vec![2, 3, 1]);
        let l = t.layout(WIN, 1);
        assert_eq!(l[0].1, Rect::new(0, 0, 801, 300), "2는 위 전체");
        assert_eq!(l[1].1.w, l[2].1.w, "3과 1은 반반");
        assert!(l[1].1.x < l[2].1.x);
        assert_eq!(l[1].1.y, 301);
    }

    #[test]
    fn move_pane_resets_ratio_and_keeps_every_pane_once() {
        let mut t = tree_1_2_3();
        t.resize(WIN, 1, 1, Direction::Right, 100);
        assert!(t.move_pane(3, 1, Direction::Up));
        let mut leaves = t.leaves();
        leaves.sort();
        assert_eq!(leaves, vec![1, 2, 3]);
        // 3이 1의 위: (3 / 1) | 2
        assert_eq!(t.leaves(), vec![3, 1, 2]);
        let l = t.layout(WIN, 1);
        assert_eq!(l[0].1.h, l[1].1.h, "새로 나눈 쪽은 반반");
    }

    #[test]
    fn move_pane_does_nothing_for_self_missing_or_single() {
        let mut t = tree_1_2_3();
        let before = t.clone();
        assert!(!t.move_pane(2, 2, Direction::Left), "자기 자신 곁으로는 옮기지 않는다");
        assert!(!t.move_pane(9, 1, Direction::Left), "없는 칸");
        assert!(!t.move_pane(1, 9, Direction::Left), "없는 대상");
        assert_eq!(t, before);
        let mut one = Node::Leaf(1);
        assert!(!one.move_pane(1, 1, Direction::Right));
        assert!(!one.move_pane(1, 2, Direction::Right));
        assert_eq!(one, Node::Leaf(1));
    }

    #[test]
    fn move_pane_that_changes_nothing_reports_false() {
        // 1 | 2 에서 1을 2의 왼쪽으로 놓아도 그대로
        let mut t = Node::Leaf(1);
        t.split(1, 2, Direction::Right);
        let before = t.clone();
        assert!(!t.move_pane(1, 2, Direction::Left));
        assert_eq!(t, before);
        // 2의 위로 놓으면 달라진다
        assert!(t.move_pane(1, 2, Direction::Up));
        assert_eq!(t.leaves(), vec![1, 2]);
        assert_ne!(t, before);
    }

    #[test]
    fn move_pane_two_panes_swaps_axis() {
        let mut t = Node::Leaf(1);
        t.split(1, 2, Direction::Right);
        assert!(t.move_pane(2, 1, Direction::Left));
        assert_eq!(t.leaves(), vec![2, 1]);
    }

    #[test]
    fn drop_direction_picks_the_nearest_edge() {
        let r = Rect::new(100, 50, 400, 200);
        assert_eq!(drop_direction(r, 110.0, 150.0), Some(Direction::Left));
        assert_eq!(drop_direction(r, 490.0, 150.0), Some(Direction::Right));
        assert_eq!(drop_direction(r, 300.0, 55.0), Some(Direction::Up));
        assert_eq!(drop_direction(r, 300.0, 245.0), Some(Direction::Down));
        // 칸 크기로 나눠 견준다: 가로로 긴 칸에서 위 변은 가깝고(10/200) 왼쪽 변은 멀다(60/400 = 0.15 > 0.05)
        assert_eq!(drop_direction(r, 160.0, 60.0), Some(Direction::Up));
        // 정확히 가운데는 같은 거리: 왼쪽이 먼저
        assert_eq!(drop_direction(r, 300.0, 150.0), Some(Direction::Left));
        // 왼쪽 위 대각선(상대 거리가 같다)도 왼쪽이 먼저
        assert_eq!(drop_direction(r, 140.0, 70.0), Some(Direction::Left));
    }

    #[test]
    fn drop_direction_is_none_outside_or_empty() {
        let r = Rect::new(100, 50, 400, 200);
        assert_eq!(drop_direction(r, 99.0, 100.0), None);
        assert_eq!(drop_direction(r, 500.0, 100.0), None, "오른쪽 변은 칸 밖");
        assert_eq!(drop_direction(r, 200.0, 250.0), None);
        assert_eq!(drop_direction(Rect::new(0, 0, 0, 10), 0.0, 1.0), None);
    }

    #[test]
    fn drop_preview_is_the_half_on_that_side() {
        let r = Rect::new(100, 50, 401, 201);
        assert_eq!(drop_preview(r, Direction::Left), Rect::new(100, 50, 200, 201));
        assert_eq!(drop_preview(r, Direction::Right), Rect::new(300, 50, 201, 201));
        assert_eq!(drop_preview(r, Direction::Up), Rect::new(100, 50, 401, 100));
        assert_eq!(drop_preview(r, Direction::Down), Rect::new(100, 150, 401, 101));
    }
}
