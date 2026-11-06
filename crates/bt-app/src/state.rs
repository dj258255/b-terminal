//! 창 상태 복원(#94, Ghostty `window-save-state`): 끌 때 창 크기·자리, 탭, 분할 트리(비율), 칸의 작업 폴더를
//! 파일로 적고, 켤 때 새 셸로 되살린다. 돌던 프로그램·화면 내용은 되살리지 않는다

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::split::{Axis, Node};

/// 상태 파일의 판. 모양이 바뀌면 올리고, 다른 판은 읽지 않는다
pub const VERSION: u32 = 1;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SavedState {
    pub version: u32,
    pub windows: Vec<SavedWindow>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SavedWindow {
    /// 안쪽 크기(물리 픽셀)
    pub size: (u32, u32),
    /// 바깥 자리(물리 픽셀). 모르면 없다
    pub position: Option<(i32, i32)>,
    pub tabs: Vec<SavedTab>,
    pub active: usize,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SavedTab {
    pub layout: SavedNode,
    /// 고른 칸: 잎 차례(왼쪽·위부터)
    pub focus: usize,
    pub zoomed: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum SavedNode {
    Pane {
        cwd: Option<PathBuf>,
    },
    Split {
        /// 좌우로 나란히(참) 또는 위아래(거짓)
        horizontal: bool,
        ratio: f32,
        first: Box<SavedNode>,
        second: Box<SavedNode>,
    },
}

impl SavedNode {
    /// 분할 트리를 적는다. `cwd(칸 id)`가 칸의 폴더를 준다
    pub fn from_node(node: &Node, cwd: &dyn Fn(u64) -> Option<PathBuf>) -> Self {
        match node {
            Node::Leaf(id) => SavedNode::Pane { cwd: cwd(*id) },
            Node::Split {
                axis,
                ratio,
                first,
                second,
            } => SavedNode::Split {
                horizontal: *axis == Axis::Horizontal,
                ratio: *ratio,
                first: Box::new(Self::from_node(first, cwd)),
                second: Box::new(Self::from_node(second, cwd)),
            },
        }
    }

    /// 칸들의 폴더(잎 차례)
    pub fn panes(&self) -> Vec<Option<PathBuf>> {
        match self {
            SavedNode::Pane { cwd } => vec![cwd.clone()],
            SavedNode::Split { first, second, .. } => {
                let mut v = first.panes();
                v.extend(second.panes());
                v
            }
        }
    }

    /// 새 칸 id들(잎 차례)로 분할 트리를 다시 만든다. id가 모자라면 없다
    pub fn to_node(&self, ids: &mut impl Iterator<Item = u64>) -> Option<Node> {
        Some(match self {
            SavedNode::Pane { .. } => Node::Leaf(ids.next()?),
            SavedNode::Split {
                horizontal,
                ratio,
                first,
                second,
            } => Node::Split {
                axis: if *horizontal { Axis::Horizontal } else { Axis::Vertical },
                // 망가진 값이 칸을 없애지 않게 묶는다
                ratio: if ratio.is_finite() { ratio.clamp(0.1, 0.9) } else { 0.5 },
                first: Box::new(first.to_node(ids)?),
                second: Box::new(second.to_node(ids)?),
            },
        })
    }
}

/// 상태 파일 자리: macOS는 ~/Library/Application Support/b-terminal, 그 밖에는 $XDG_STATE_HOME(없으면 ~/.local/state)/b-terminal
pub fn path() -> Option<PathBuf> {
    let home = std::env::var_os("HOME").map(PathBuf::from)?;
    let dir = if cfg!(target_os = "macos") {
        home.join("Library/Application Support/b-terminal")
    } else {
        std::env::var_os("XDG_STATE_HOME")
            .map(PathBuf::from)
            .unwrap_or_else(|| home.join(".local/state"))
            .join("b-terminal")
    };
    Some(dir.join("state.json"))
}

/// 적는다. 창이 없으면 파일을 지운다(마지막 창을 닫고 끈 것은 되살리지 않는다)
pub fn save(path: &Path, state: &SavedState) -> std::io::Result<()> {
    if state.windows.is_empty() {
        return match std::fs::remove_file(path) {
            Err(e) if e.kind() != std::io::ErrorKind::NotFound => Err(e),
            _ => Ok(()),
        };
    }
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir)?;
    }
    // 반쯤 적힌 파일을 남기지 않게 임시 파일에 적고 바꾼다
    let tmp = path.with_extension("json.tmp");
    std::fs::write(&tmp, serde_json::to_vec_pretty(state).map_err(std::io::Error::other)?)?;
    std::fs::rename(tmp, path)
}

/// 읽는다. 없거나 다른 판이거나 망가졌으면 없다. 너무 큰 파일은 읽지 않는다
pub fn load(path: &Path) -> Option<SavedState> {
    let meta = std::fs::metadata(path).ok()?;
    if meta.len() > 4 * 1024 * 1024 {
        return None;
    }
    let state: SavedState = serde_json::from_slice(&std::fs::read(path).ok()?).ok()?;
    (state.version == VERSION).then_some(state)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::split::Direction;

    #[test]
    fn layout_round_trips_with_cwds_and_ratios() {
        // 1 | (2 / 3), 경계를 민 뒤
        let mut t = Node::Leaf(1);
        t.split(1, 2, Direction::Right);
        t.split(2, 3, Direction::Down);
        if let Node::Split { ratio, .. } = &mut t {
            *ratio = 0.3;
        }
        let cwd = |id: u64| Some(PathBuf::from(format!("/tmp/p{id}")));
        let saved = SavedNode::from_node(&t, &cwd);
        assert_eq!(saved.panes(), vec![cwd(1), cwd(2), cwd(3)]);
        let mut ids = [10u64, 20, 30].into_iter();
        let back = saved.to_node(&mut ids).unwrap();
        assert_eq!(back.leaves(), vec![10, 20, 30]);
        match back {
            Node::Split { axis, ratio, .. } => assert_eq!((axis, ratio), (Axis::Horizontal, 0.3)),
            _ => panic!("나뉘어야 한다"),
        }
        // id가 모자라면 없다
        assert!(saved.to_node(&mut [1u64].into_iter()).is_none());
    }

    #[test]
    fn save_load_and_remove_when_empty() {
        let dir = std::env::temp_dir().join(format!("bt-state-{}", std::process::id()));
        let p = dir.join("state.json");
        let state = SavedState {
            version: VERSION,
            windows: vec![SavedWindow {
                size: (2000, 1360),
                position: Some((10, 20)),
                tabs: vec![SavedTab {
                    layout: SavedNode::Pane {
                        cwd: Some("/tmp".into()),
                    },
                    focus: 0,
                    zoomed: false,
                }],
                active: 0,
            }],
        };
        save(&p, &state).unwrap();
        assert_eq!(load(&p), Some(state.clone()));
        // 다른 판은 읽지 않는다
        save(&p, &SavedState { version: 99, ..state }).unwrap();
        assert_eq!(load(&p), None);
        // 창이 없으면 지운다
        save(
            &p,
            &SavedState {
                version: VERSION,
                windows: vec![],
            },
        )
        .unwrap();
        assert!(!p.exists());
        std::fs::write(&p, b"{not json").ok();
        let _ = std::fs::create_dir_all(&dir);
        std::fs::write(&p, b"{not json").unwrap();
        assert_eq!(load(&p), None, "망가진 파일");
        std::fs::remove_dir_all(&dir).unwrap();
    }
}
