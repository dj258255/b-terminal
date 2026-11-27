//! kitty 세션 파일의 `layout`을 우리 칸 나누기(이진 분할 트리)로 바꾼다(#934). 창과 상관없는 순수 함수라 여기서 테스트한다.
//! 대응표는 `docs/configuration.md`의 "kitty 세션 파일" 절에 있다

/// 칸 번호(잎, 세션 파일에 나온 차례)로 만든 분할 트리
#[derive(Debug, Clone, PartialEq)]
pub enum PlanNode {
    Leaf(usize),
    Split {
        /// 좌우로 나란히(참) 또는 위아래(거짓)
        horizontal: bool,
        /// 앞 칸이 차지하는 몫
        ratio: f32,
        first: Box<PlanNode>,
        second: Box<PlanNode>,
    },
}

impl PlanNode {
    fn split(horizontal: bool, ratio: f32, first: PlanNode, second: PlanNode) -> PlanNode {
        PlanNode::Split {
            horizontal,
            ratio,
            first: Box::new(first),
            second: Box::new(second),
        }
    }

    /// 새 칸 id들(칸 번호 차례)로 앱의 분할 트리를 만든다. 번호에 맞는 id가 없으면 없다
    pub fn to_node(&self, ids: &[u64]) -> Option<crate::split::Node> {
        use crate::split::{Axis, Node};
        Some(match self {
            PlanNode::Leaf(i) => Node::Leaf(*ids.get(*i)?),
            PlanNode::Split {
                horizontal,
                ratio,
                first,
                second,
            } => Node::Split {
                axis: if *horizontal { Axis::Horizontal } else { Axis::Vertical },
                ratio: if ratio.is_finite() { ratio.clamp(0.1, 0.9) } else { 0.5 },
                first: Box::new(first.to_node(ids)?),
                second: Box::new(second.to_node(ids)?),
            },
        })
    }

    /// 트리의 잎 수
    #[cfg(test)]
    fn leaves(&self) -> usize {
        match self {
            PlanNode::Leaf(_) => 1,
            PlanNode::Split { first, second, .. } => first.leaves() + second.leaves(),
        }
    }

    /// `parent` 잎을 `new`와의 분할로 바꾼다. 못 찾으면 거짓
    fn replace_leaf(&mut self, parent: usize, with: &dyn Fn() -> PlanNode) -> bool {
        match self {
            PlanNode::Leaf(i) if *i == parent => {
                *self = with();
                true
            }
            PlanNode::Leaf(_) => false,
            PlanNode::Split { first, second, .. } => {
                first.replace_leaf(parent, with) || second.replace_leaf(parent, with)
            }
        }
    }
}

/// kitty 레이아웃 이름 가운데 우리가 받는 것
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LayoutKind {
    Tall,
    Fat,
    Grid,
    Horizontal,
    Vertical,
    Stack,
    Splits,
}

/// `layout tall:bias=60;full_size=2` 한 줄을 읽은 것
#[derive(Debug, Clone, PartialEq)]
pub struct LayoutSpec {
    pub kind: LayoutKind,
    /// 첫(주) 영역이 차지하는 몫(0.1~0.9). `tall`·`fat`만 쓴다
    pub bias: f32,
    /// 주 영역에 둘 칸 수(1~100). `tall`·`fat`만 쓴다
    pub full_size: usize,
    /// 주 영역을 반대편에 둔다. `tall`·`fat`만 쓴다
    pub mirrored: bool,
}

impl Default for LayoutSpec {
    /// 줄이 없을 때: `tall`. kitty 세션 파서도 `enabled_layouts`가 비면 `tall`을 쓴다(`session.py`의 `Tab.__init__`)
    fn default() -> Self {
        LayoutSpec {
            kind: LayoutKind::Tall,
            bias: 0.5,
            full_size: 1,
            mirrored: false,
        }
    }
}

impl LayoutSpec {
    /// `이름[:키=값;키=값]`. 모르는 이름은 오류, 모르는 옵션은 무시하고 알려 준다(둘째 값)
    pub fn parse(text: &str) -> Result<(LayoutSpec, Vec<String>), String> {
        let text = text.trim();
        let (name, opts) = text.split_once(':').unwrap_or((text, ""));
        let kind = match name.trim().to_ascii_lowercase().as_str() {
            "tall" => LayoutKind::Tall,
            "fat" => LayoutKind::Fat,
            "grid" => LayoutKind::Grid,
            "horizontal" => LayoutKind::Horizontal,
            "vertical" => LayoutKind::Vertical,
            "stack" => LayoutKind::Stack,
            "splits" => LayoutKind::Splits,
            other => return Err(format!("모르는 레이아웃입니다: {other}")),
        };
        let mut spec = LayoutSpec {
            kind,
            ..LayoutSpec::default()
        };
        let mut notes = Vec::new();
        for kv in opts.split(';').map(str::trim).filter(|s| !s.is_empty()) {
            let (k, v) = kv.split_once('=').unwrap_or((kv, ""));
            let (k, v) = (k.trim(), v.trim());
            let main = matches!(kind, LayoutKind::Tall | LayoutKind::Fat);
            match k {
                "bias" if main => match v.parse::<i32>() {
                    Ok(n) => spec.bias = (n as f32 / 100.0).clamp(0.1, 0.9),
                    Err(_) => notes.push(format!("레이아웃 옵션 bias 값이 숫자가 아닙니다: {v}")),
                },
                "full_size" if main => match v.parse::<i32>() {
                    Ok(n) => spec.full_size = n.clamp(1, 100) as usize,
                    Err(_) => notes.push(format!("레이아웃 옵션 full_size 값이 숫자가 아닙니다: {v}")),
                },
                "mirrored" if main => {
                    spec.mirrored = matches!(v.to_ascii_lowercase().as_str(), "y" | "yes" | "true");
                }
                _ => notes.push(format!("레이아웃 옵션은 지원하지 않아 무시했습니다: {k}")),
            }
        }
        Ok((spec, notes))
    }
}

/// `splits` 레이아웃에서 칸 하나를 어디에 붙이는가(`launch --location`·`--bias`)
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SplitReq {
    /// 이 칸을 붙일 칸 번호(이 칸보다 앞서 나온 것)
    pub parent: usize,
    /// 좌우(`vsplit`, 기본)면 참, 위아래(`hsplit`)면 거짓
    pub horizontal: bool,
    /// 새 칸이 차지할 몫(0~1). 없으면 절반
    pub bias: Option<f32>,
    /// 새 칸을 앞(왼쪽·위)에 둔다(`before`·`first`)
    pub before: bool,
}

fn chain(mut items: Vec<PlanNode>, horizontal: bool) -> PlanNode {
    // 같은 몫으로 나란히: 앞 하나가 1/n, 나머지가 (n-1)/n
    let n = items.len();
    debug_assert!(n > 0);
    if n == 1 {
        return items.remove(0);
    }
    let first = items.remove(0);
    PlanNode::split(horizontal, 1.0 / n as f32, first, chain(items, horizontal))
}

fn leaves(range: std::ops::Range<usize>) -> Vec<PlanNode> {
    range.map(PlanNode::Leaf).collect()
}

/// kitty `grid.py`의 `calc_grid_size`를 그대로: 세로줄 수와 줄마다 칸 수(왼쪽 줄부터, 위에서 아래로 채운다)
pub fn grid_columns(n: usize) -> Vec<usize> {
    if n <= 1 {
        return vec![n];
    }
    let ncols = if n <= 5 {
        2
    } else {
        let mut c = 3;
        for cand in 3..=(n / 2) {
            c = cand;
            if cand * cand >= n {
                break;
            }
        }
        c
    };
    let nrows = n / ncols;
    let special_rows = n - nrows * (ncols - 1);
    let special_col = if special_rows < nrows { 0 } else { ncols - 1 };
    (0..ncols)
        .map(|c| if c == special_col { special_rows } else { nrows })
        .collect()
}

/// 칸 `n`개를 레이아웃대로 놓은 트리와 "한 칸만 크게 보이는가(stack)". `splits`는 `reqs[i]`(i번째 칸의 붙일 곳, 0번째는 비어 있음)로 만든다
pub fn build_tree(spec: &LayoutSpec, n: usize, reqs: &[Option<SplitReq>]) -> (PlanNode, bool) {
    let n = n.max(1);
    if n == 1 {
        return (PlanNode::Leaf(0), false);
    }
    match spec.kind {
        LayoutKind::Horizontal => (chain(leaves(0..n), true), false),
        LayoutKind::Vertical => (chain(leaves(0..n), false), false),
        LayoutKind::Stack => (chain(leaves(0..n), true), true),
        LayoutKind::Tall | LayoutKind::Fat => {
            let tall = spec.kind == LayoutKind::Tall;
            let k = spec.full_size.min(n);
            // 주 영역 칸은 나란히(tall) 또는 쌓고(fat), 나머지는 그 반대 방향
            let main = chain(leaves(0..k), tall);
            if k == n {
                return (main, false);
            }
            let rest = chain(leaves(k..n), !tall);
            let node = if spec.mirrored {
                PlanNode::split(tall, 1.0 - spec.bias, rest, main)
            } else {
                PlanNode::split(tall, spec.bias, main, rest)
            };
            (node, false)
        }
        LayoutKind::Grid => {
            let mut pos = 0;
            let cols: Vec<PlanNode> = grid_columns(n)
                .into_iter()
                .map(|rows| {
                    let col = chain(leaves(pos..pos + rows), false);
                    pos += rows;
                    col
                })
                .collect();
            (chain(cols, true), false)
        }
        LayoutKind::Splits => {
            let mut root = PlanNode::Leaf(0);
            for i in 1..n {
                let req = reqs.get(i).copied().flatten().unwrap_or(SplitReq {
                    parent: i - 1,
                    horizontal: true,
                    bias: None,
                    before: false,
                });
                // 새 칸이 차지할 몫. 앞 칸 몫은 그 나머지
                let new_share = req.bias.map_or(0.5, |b| b.clamp(0.0, 1.0));
                let (old, new) = (PlanNode::Leaf(req.parent), PlanNode::Leaf(i));
                let make = move || {
                    if req.before {
                        PlanNode::split(req.horizontal, new_share, new.clone(), old.clone())
                    } else {
                        PlanNode::split(req.horizontal, 1.0 - new_share, old.clone(), new.clone())
                    }
                };
                if !root.replace_leaf(req.parent, &make) {
                    // 붙일 곳이 없으면 맨 끝 칸에 붙인다(트리에는 늘 칸이 모두 들어간다)
                    let last = i - 1;
                    let (old, new) = (PlanNode::Leaf(last), PlanNode::Leaf(i));
                    root.replace_leaf(last, &|| PlanNode::split(true, 0.5, old.clone(), new.clone()));
                }
            }
            (root, false)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use PlanNode::Leaf;

    fn sp(h: bool, r: f32, a: PlanNode, b: PlanNode) -> PlanNode {
        PlanNode::split(h, r, a, b)
    }
    fn spec(text: &str) -> LayoutSpec {
        LayoutSpec::parse(text).expect(text).0
    }
    fn tree(text: &str, n: usize) -> PlanNode {
        build_tree(&spec(text), n, &[]).0
    }

    #[test]
    fn layout_names_and_options() {
        assert_eq!(spec("tall"), LayoutSpec::default());
        assert_eq!(spec("FAT").kind, LayoutKind::Fat);
        let s = spec("tall:bias=60;full_size=2;mirrored=y");
        assert_eq!((s.bias, s.full_size, s.mirrored), (0.6, 2, true));
        assert_eq!(spec("tall:bias=99").bias, 0.9, "상한 90%");
        assert_eq!(spec("tall:bias=1").bias, 0.1, "하한 10%");
        assert_eq!(spec("tall:full_size=0").full_size, 1);
        assert!(LayoutSpec::parse("spiral").is_err());
        let (s, notes) = LayoutSpec::parse("grid:bias=3").unwrap();
        assert_eq!(s.kind, LayoutKind::Grid);
        assert_eq!(notes.len(), 1, "grid에는 bias가 없어 무시: {notes:?}");
        let (_, notes) = LayoutSpec::parse("tall:bias=abc").unwrap();
        assert!(notes[0].contains("숫자"), "{notes:?}");
    }

    #[test]
    fn tall_main_left_rest_stacked_right() {
        assert_eq!(tree("tall", 1), Leaf(0));
        assert_eq!(tree("tall", 2), sp(true, 0.5, Leaf(0), Leaf(1)));
        assert_eq!(
            tree("tall", 3),
            sp(true, 0.5, Leaf(0), sp(false, 0.5, Leaf(1), Leaf(2)))
        );
        // bias는 주 영역 몫, mirrored는 주 영역을 오른쪽에
        assert_eq!(tree("tall:bias=60", 2), sp(true, 0.6, Leaf(0), Leaf(1)));
        assert_eq!(
            tree("tall:bias=60;mirrored=y", 2),
            sp(true, 1.0 - 0.6, Leaf(1), Leaf(0))
        );
    }

    #[test]
    fn tall_full_size_puts_several_in_main_area() {
        // full_size=2: 앞 둘이 주 영역에 나란히, 나머지는 오른쪽에 쌓는다
        assert_eq!(
            tree("tall:full_size=2", 4),
            sp(
                true,
                0.5,
                sp(true, 0.5, Leaf(0), Leaf(1)),
                sp(false, 0.5, Leaf(2), Leaf(3))
            )
        );
        // 칸이 full_size보다 적으면 모두 주 영역
        assert_eq!(tree("tall:full_size=3", 2), sp(true, 0.5, Leaf(0), Leaf(1)));
    }

    #[test]
    fn fat_is_tall_turned_ninety_degrees() {
        assert_eq!(tree("fat", 2), sp(false, 0.5, Leaf(0), Leaf(1)));
        assert_eq!(tree("fat", 3), sp(false, 0.5, Leaf(0), sp(true, 0.5, Leaf(1), Leaf(2))));
        assert_eq!(
            tree("fat:full_size=2", 3),
            sp(false, 0.5, sp(false, 0.5, Leaf(0), Leaf(1)), Leaf(2))
        );
    }

    #[test]
    fn horizontal_and_vertical_split_equally() {
        let third = 1.0 / 3.0;
        assert_eq!(
            tree("horizontal", 3),
            sp(true, third, Leaf(0), sp(true, 0.5, Leaf(1), Leaf(2)))
        );
        assert_eq!(
            tree("vertical", 3),
            sp(false, third, Leaf(0), sp(false, 0.5, Leaf(1), Leaf(2)))
        );
        assert_eq!(tree("vertical", 4).leaves(), 4);
    }

    #[test]
    fn stack_is_one_zoomed_pane() {
        let (t, zoomed) = build_tree(&spec("stack"), 3, &[]);
        assert!(zoomed, "stack은 한 칸만 크게 보인다");
        assert_eq!(t.leaves(), 3);
        assert!(!build_tree(&spec("stack"), 1, &[]).1, "칸이 하나면 확대가 아니다");
    }

    #[test]
    fn grid_columns_match_kitty_calc_grid_size() {
        // kitty `calc_grid_size`를 손으로 풀어 본 값(세로줄마다 칸 수). 8칸의 2·2·4는 kitty 식 그대로의 결과다
        let want: [(usize, &[usize]); 9] = [
            (1, &[1]),
            (2, &[1, 1]),
            (3, &[1, 2]),
            (4, &[2, 2]),
            (5, &[2, 3]),
            (6, &[2, 2, 2]),
            (7, &[2, 2, 3]),
            (8, &[2, 2, 4]),
            (9, &[3, 3, 3]),
        ];
        for (n, cols) in want {
            assert_eq!(grid_columns(n), cols, "n={n}");
            assert_eq!(grid_columns(n).iter().sum::<usize>(), n);
        }
        // 4칸: 2x2. 왼쪽 줄이 0·1, 오른쪽 줄이 2·3
        assert_eq!(
            tree("grid", 4),
            sp(
                true,
                0.5,
                sp(false, 0.5, Leaf(0), Leaf(1)),
                sp(false, 0.5, Leaf(2), Leaf(3))
            )
        );
        for n in 1..40 {
            assert_eq!(tree("grid", n).leaves(), n, "n={n}");
        }
    }

    #[test]
    fn splits_follow_launch_location_like_kitty_doc_example() {
        // 문서 예: 첫 칸 / vsplit(둘째 열) / hsplit --bias=40(둘째 열을 위아래로) / 첫 칸에 돌아가 hsplit
        let reqs = [
            None,
            Some(SplitReq {
                parent: 0,
                horizontal: true,
                bias: None,
                before: false,
            }),
            Some(SplitReq {
                parent: 1,
                horizontal: false,
                bias: Some(0.4),
                before: false,
            }),
            Some(SplitReq {
                parent: 0,
                horizontal: false,
                bias: None,
                before: false,
            }),
        ];
        let (t, _) = build_tree(&spec("splits"), 4, &reqs);
        assert_eq!(
            t,
            sp(
                true,
                0.5,
                sp(false, 0.5, Leaf(0), Leaf(3)),
                sp(false, 1.0 - 0.4, Leaf(1), Leaf(2))
            )
        );
    }

    #[test]
    fn splits_before_and_missing_requests() {
        let before = [
            None,
            Some(SplitReq {
                parent: 0,
                horizontal: true,
                bias: Some(0.3),
                before: true,
            }),
        ];
        assert_eq!(
            build_tree(&spec("splits"), 2, &before).0,
            sp(true, 0.3, Leaf(1), Leaf(0)),
            "before는 새 칸이 앞"
        );
        // 요청이 없으면(레이아웃을 splits로 바꿨을 때) 앞 칸에 좌우로 붙인다. 칸은 모두 들어간다
        let (t, _) = build_tree(&spec("splits"), 3, &[]);
        assert_eq!(t.leaves(), 3);
        // 붙일 곳이 트리에 없어도 칸을 잃지 않는다
        let bad = [
            None,
            Some(SplitReq {
                parent: 7,
                horizontal: true,
                bias: None,
                before: false,
            }),
        ];
        assert_eq!(build_tree(&spec("splits"), 2, &bad).0.leaves(), 2);
    }

    #[test]
    fn to_node_maps_numbers_to_ids_and_clamps() {
        let t = sp(true, 5.0, Leaf(0), Leaf(1));
        let node = t.to_node(&[10, 20]).unwrap();
        let crate::split::Node::Split { ratio, .. } = node else {
            panic!("분할이어야 한다")
        };
        assert_eq!(ratio, 0.9, "망가진 몫은 묶는다");
        assert!(t.to_node(&[10]).is_none(), "id가 모자라면 없다");
    }
}
