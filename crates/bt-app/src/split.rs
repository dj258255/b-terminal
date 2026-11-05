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
}
