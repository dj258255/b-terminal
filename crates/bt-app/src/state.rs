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
    /// 사람이 붙인 창 제목(`set_window_title`). 옛 판 파일에는 없다
    #[serde(default)]
    pub title: Option<String>,
    /// 탭 묶음(#933)의 이름·색·접힘. 묶음이 없으면 적지 않고, 옛 판 파일에는 없다(묶음 없음으로 읽힌다)
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub bundles: Vec<SavedBundle>,
}

/// 저장한 탭 묶음 하나(#933). 파일은 믿지 않는다: 이름은 읽을 때 다시 다듬고, 모르는 색 이름은 색 없음으로 본다
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SavedBundle {
    pub id: u32,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub color: Option<String>,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub collapsed: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SavedTab {
    pub layout: SavedNode,
    /// 고른 칸: 잎 차례(왼쪽·위부터)
    pub focus: usize,
    pub zoomed: bool,
    /// 사람이 붙인 탭 제목(`set_tab_title`). 옛 판 파일에는 없다
    #[serde(default)]
    pub title: Option<String>,
    /// 사람이 고른 탭 색의 이름(`tabcolor::TabColor::key`, #852). 옛 판 파일에는 없고, 모르는 이름은 읽을 때 없는 것으로 본다
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub color: Option<String>,
    /// 든 탭 묶음의 번호(`SavedWindow::bundles`의 `id`, #933). 묶음이 없으면 적지 않는다. 옛 판 파일에는 없다
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub bundle: Option<u32>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum SavedNode {
    Pane {
        cwd: Option<PathBuf>,
        /// 사람이 붙인 칸 제목(`set_surface_title`). 옛 판 파일에는 없다
        #[serde(default)]
        title: Option<String>,
        /// 지난 화면 파일의 열쇠(`scrollback/<열쇠>.json`, #861). `window-save-state-scrollback`을 켜고 끈 경우에만 있다. 옛 판 파일에는 없다
        #[serde(default)]
        scrollback: Option<String>,
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
    pub fn from_node(node: &Node, cwd: &dyn Fn(u64) -> Option<PathBuf>, title: &dyn Fn(u64) -> Option<String>) -> Self {
        match node {
            Node::Leaf(id) => SavedNode::Pane {
                cwd: cwd(*id),
                title: title(*id),
                scrollback: None,
            },
            Node::Split {
                axis,
                ratio,
                first,
                second,
            } => SavedNode::Split {
                horizontal: *axis == Axis::Horizontal,
                ratio: *ratio,
                first: Box::new(Self::from_node(first, cwd, title)),
                second: Box::new(Self::from_node(second, cwd, title)),
            },
        }
    }

    /// 칸들의 폴더(잎 차례)
    pub fn panes(&self) -> Vec<Option<PathBuf>> {
        match self {
            SavedNode::Pane { cwd, .. } => vec![cwd.clone()],
            SavedNode::Split { first, second, .. } => {
                let mut v = first.panes();
                v.extend(second.panes());
                v
            }
        }
    }

    /// 칸들의 사람이 붙인 제목(잎 차례). 파일에서 읽은 글이라 쓰기 전에 다듬어야 한다
    pub fn pane_titles(&self) -> Vec<Option<String>> {
        match self {
            SavedNode::Pane { title, .. } => vec![title.clone()],
            SavedNode::Split { first, second, .. } => {
                let mut v = first.pane_titles();
                v.extend(second.pane_titles());
                v
            }
        }
    }

    /// 칸들의 지난 화면 열쇠(잎 차례). 파일에서 읽은 글이라 쓰기 전에 `scrollback_store::valid_key`로 거른다
    pub fn pane_scrollback_keys(&self) -> Vec<Option<String>> {
        match self {
            SavedNode::Pane { scrollback, .. } => vec![scrollback.clone()],
            SavedNode::Split { first, second, .. } => {
                let mut v = first.pane_scrollback_keys();
                v.extend(second.pane_scrollback_keys());
                v
            }
        }
    }

    /// 칸들(잎 차례)에 지난 화면 열쇠를 단다. 열쇠가 모자라면 나머지 칸은 없다
    pub fn set_scrollback_keys(&mut self, keys: &mut impl Iterator<Item = Option<String>>) {
        match self {
            SavedNode::Pane { scrollback, .. } => *scrollback = keys.next().flatten(),
            SavedNode::Split { first, second, .. } => {
                first.set_scrollback_keys(keys);
                second.set_scrollback_keys(keys);
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
        let title = |id: u64| (id != 2).then(|| format!("제목{id}"));
        let saved = SavedNode::from_node(&t, &cwd, &title);
        assert_eq!(
            saved.pane_titles(),
            vec![Some("제목1".to_string()), None, Some("제목3".to_string())]
        );
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
        // 지난 화면 열쇠는 잎 차례로 달린다
        let mut saved = saved;
        saved.set_scrollback_keys(&mut [Some("a".to_string()), None, Some("c".to_string())].into_iter());
        assert_eq!(
            saved.pane_scrollback_keys(),
            vec![Some("a".to_string()), None, Some("c".to_string())]
        );
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
                        title: Some("서버".into()),
                        scrollback: Some("w0t0p0".into()),
                    },
                    focus: 0,
                    zoomed: false,
                    title: Some("빌드".into()),
                    color: Some("teal".into()),
                    bundle: Some(3),
                }],
                active: 0,
                title: Some("작업".into()),
                bundles: vec![SavedBundle {
                    id: 3,
                    name: "에이전트".into(),
                    color: Some("blue".into()),
                    collapsed: true,
                }],
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

    #[test]
    fn old_state_file_without_titles_still_loads() {
        // 제목 필드가 생기기 전 판(#94)이 적은 모양. 판 번호는 그대로 1이다
        let old = r#"{"version":1,"windows":[{"size":[800,600],"position":null,
            "tabs":[{"layout":{"Split":{"horizontal":true,"ratio":0.5,
              "first":{"Pane":{"cwd":"/tmp"}},"second":{"Pane":{"cwd":null}}}},
              "focus":0,"zoomed":false}],"active":0}]}"#;
        let state: SavedState = serde_json::from_str(old).expect("옛 판도 읽혀야 한다");
        assert_eq!(state.version, VERSION);
        let w = &state.windows[0];
        assert_eq!(w.title, None);
        assert_eq!(w.tabs[0].title, None);
        assert_eq!(w.tabs[0].color, None, "탭 색이 생기기 전 판(#852)");
        assert_eq!(w.tabs[0].layout.pane_titles(), vec![None, None]);
        assert_eq!(
            w.tabs[0].layout.pane_scrollback_keys(),
            vec![None, None],
            "지난 화면 열쇠가 생기기 전 판"
        );
        assert_eq!(w.tabs[0].layout.panes()[0], Some(PathBuf::from("/tmp")));
    }

    /// 탭 색(#852): 이름으로 적고 되살린다. 모르는 이름이 든 파일도 통째로 버리지 않고 그 탭만 색 없이 연다. 색이 없으면 키를 적지 않는다
    #[test]
    fn tab_color_round_trips_and_unknown_names_do_not_drop_the_file() {
        use crate::tabcolor::TabColor;
        let tab = |color: Option<&str>| SavedTab {
            layout: SavedNode::Pane {
                cwd: None,
                title: None,
                scrollback: None,
            },
            focus: 0,
            zoomed: false,
            title: None,
            color: color.map(str::to_string),
            bundle: None,
        };
        let state = SavedState {
            version: VERSION,
            windows: vec![SavedWindow {
                size: (800, 600),
                position: None,
                tabs: vec![tab(Some("teal")), tab(None)],
                active: 0,
                title: None,
                bundles: Vec::new(),
            }],
        };
        let json = serde_json::to_string(&state).unwrap();
        assert_eq!(
            json.matches("\"color\"").count(),
            1,
            "색 없는 탭은 키를 적지 않는다: {json}"
        );
        let back: SavedState = serde_json::from_str(&json).unwrap();
        assert_eq!(back, state);
        let colors: Vec<_> = back.windows[0]
            .tabs
            .iter()
            .map(|t| TabColor::from_saved(t.color.as_deref()))
            .collect();
        assert_eq!(colors, [TabColor::Teal, TabColor::None]);
        // 모르는 이름·빈 글은 색 없음
        let odd = json.replace("teal", "magenta");
        let back: SavedState = serde_json::from_str(&odd).expect("모르는 색 이름이어도 파일은 읽힌다");
        assert_eq!(
            TabColor::from_saved(back.windows[0].tabs[0].color.as_deref()),
            TabColor::None
        );
        assert_eq!(TabColor::from_saved(Some("")), TabColor::None);
    }

    /// 탭 묶음(#933): 묶음 목록과 탭의 묶음 번호를 적고 되살린다. 묶음이 없으면 키를 적지 않고, 묶음이 없던 옛 파일도 그대로 읽힌다
    #[test]
    fn bundles_round_trip_and_old_files_read_as_no_bundles() {
        let tab = |bundle: Option<u32>| SavedTab {
            layout: SavedNode::Pane {
                cwd: None,
                title: None,
                scrollback: None,
            },
            focus: 0,
            zoomed: false,
            title: None,
            color: None,
            bundle,
        };
        let state = SavedState {
            version: VERSION,
            windows: vec![SavedWindow {
                size: (800, 600),
                position: None,
                tabs: vec![tab(Some(2)), tab(Some(2)), tab(None)],
                active: 1,
                title: None,
                bundles: vec![SavedBundle {
                    id: 2,
                    name: "서버".into(),
                    color: Some("green".into()),
                    collapsed: true,
                }],
            }],
        };
        let json = serde_json::to_string(&state).unwrap();
        assert_eq!(serde_json::from_str::<SavedState>(&json).unwrap(), state);
        // 묶음이 없는 창은 두 키를 모두 적지 않는다
        let plain = SavedState {
            version: VERSION,
            windows: vec![SavedWindow {
                bundles: Vec::new(),
                tabs: vec![tab(None)],
                ..state.windows[0].clone()
            }],
        };
        let json = serde_json::to_string(&plain).unwrap();
        assert!(!json.contains("bundle"), "{json}");
        // 묶음 키가 없는 옛 파일
        let old = r#"{"version":1,"windows":[{"size":[800,600],"position":null,"tabs":[{"layout":{"Pane":{"cwd":null}},"focus":0,"zoomed":false}],"active":0}]}"#;
        let back: SavedState = serde_json::from_str(old).unwrap();
        assert!(back.windows[0].bundles.is_empty());
        assert_eq!(back.windows[0].tabs[0].bundle, None);
    }
}
