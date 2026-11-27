//! kitty 세션 파일 읽기 시험(#934). kitty `docs/sessions.rst`의 예시를 그대로 먹이고, 잘못된 줄·상한을 본다.
//! 창도 PTY도 열지 않는 순수 시험이다

use std::path::{Path, PathBuf};

use crate::session_file::*;
use crate::session_layout::PlanNode;

fn env(k: &str) -> Option<String> {
    match k {
        "PROJ" => Some("/work/proj".into()),
        _ => None,
    }
}

fn parse_text(text: &str) -> SessionPlan {
    let ctx = Ctx {
        base_dir: Path::new("/sess"),
        home: Path::new("/home/u"),
        env: &env,
    };
    parse(text, &ctx)
}

fn tab0(p: &SessionPlan) -> &TabPlan {
    &p.windows[0].tabs[0]
}

fn leaves(n: &PlanNode) -> usize {
    match n {
        PlanNode::Leaf(_) => 1,
        PlanNode::Split { first, second, .. } => leaves(first) + leaves(second),
    }
}

/// kitty `docs/sessions.rst` 첫 예시
const DOC_FIRST: &str = r#"
# Set the layout for the current tab
layout tall
# Set the working directory for windows in the current tab
cd ~/path/to/myproject
# Create the "main" window and run an editor in it to edit the project files
launch --title "Edit My Project" /usr/bin/nvim
# Create a side window to run a shell to build or test project
launch --title "Build My Project"
# Create another side window to keep an eye on some useful log file
launch --title "Log for my project" /usr/bin/tail -f /path/to/project/log/file
"#;

/// kitty `docs/sessions.rst`의 "모든 주요 키워드" 예시
const DOC_COMPLEX: &str = r#"
layout tall
cd ~
launch zsh
launch --env FOO=BAR vim
launch --title "Chat with x" irssi --profile x
launch --hold message-of-the-day

new_tab my tab
cd somewhere
enabled_layouts tall,stack
layout stack
launch zsh

new_os_window
os_window_size 80c 24c
os_window_position 100x50
os_window_title my fancy os window
os_window_class mywindow
os_window_name myname
os_window_state normal
launch sh
resize_window wider 2
focus
focus_os_window
launch emacs

new_tab logs
launch tail -f /var/log/syslog

focus_tab 0

new_tab complex tab
layout splits
launch --var window=first
launch --location=vsplit
launch --location=hsplit --bias=40
focus_matching_window var:window=first
launch --location=hsplit
"#;

#[test]
fn doc_first_example_makes_one_tab_with_three_panes() {
    let p = parse_text(DOC_FIRST);
    assert!(p.warnings.is_empty(), "{:?}", p.warnings);
    assert_eq!(p.windows.len(), 1);
    assert_eq!(p.windows[0].tabs.len(), 1);
    let t = tab0(&p);
    assert_eq!(t.panes.len(), 3);
    let home = PathBuf::from("/home/u/path/to/myproject");
    for pane in &t.panes {
        assert_eq!(pane.cwd.as_ref(), Some(&home), "cd가 이어지는 칸 모두에 적용");
    }
    assert_eq!(t.panes[0].program.as_deref(), Some("/usr/bin/nvim"));
    assert!(t.panes[0].args.is_empty());
    assert_eq!(t.panes[0].title.as_deref(), Some("Edit My Project"));
    assert_eq!(t.panes[1].program, None, "명령이 없으면 기본 셸");
    assert_eq!(t.panes[1].title.as_deref(), Some("Build My Project"));
    assert_eq!(t.panes[2].program.as_deref(), Some("/usr/bin/tail"));
    assert_eq!(t.panes[2].args, ["-f", "/path/to/project/log/file"]);
    // tall: 첫 칸이 왼쪽, 나머지는 오른쪽에 쌓는다
    assert_eq!(leaves(&t.tree), 3);
    assert!(matches!(&t.tree, PlanNode::Split { horizontal: true, first, .. } if **first == PlanNode::Leaf(0)));
    assert!(!t.zoomed);
    assert_eq!(t.focus, 2, "새 칸이 고른 칸이 되므로 마지막 칸");
}

#[test]
fn doc_complex_example_windows_tabs_and_unsupported_lines() {
    let p = parse_text(DOC_COMPLEX);
    // new_os_window 앞 정의는 첫 창, 뒤는 둘째 창
    assert_eq!(p.windows.len(), 2);
    let w0 = &p.windows[0];
    assert_eq!(w0.tabs.len(), 2);
    assert_eq!(w0.tabs[0].panes.len(), 4);
    assert_eq!(
        w0.tabs[0].panes[2].program.as_deref(),
        Some("irssi"),
        "옵션 뒤 첫 낱말이 프로그램, 그 뒤는 인자(--profile은 irssi의 인자)"
    );
    assert_eq!(w0.tabs[0].panes[2].args, ["--profile", "x"]);
    assert_eq!(w0.tabs[0].panes[1].env, [("FOO".to_string(), "BAR".to_string())]);
    assert_eq!(w0.tabs[1].title.as_deref(), Some("my tab"));
    assert_eq!(
        w0.tabs[1].panes[0].cwd,
        Some(PathBuf::from("/sess/somewhere")),
        "상대 cd는 세션 파일 폴더 기준"
    );
    assert!(!w0.tabs[1].zoomed, "layout stack이어도 칸이 하나면 확대가 아니다");
    // 둘째 창: sh, emacs 한 탭 + logs + complex tab
    let w1 = &p.windows[1];
    assert_eq!(w1.title.as_deref(), Some("my fancy os window"));
    assert_eq!(w1.tabs.len(), 3);
    assert_eq!(w1.tabs[0].panes.len(), 2);
    assert_eq!(w1.tabs[1].title.as_deref(), Some("logs"));
    assert_eq!(w1.active_tab, 0, "focus_tab 0");
    // splits: 첫 칸을 vsplit(둘째 열), 둘째 열을 hsplit(bias 40), 첫 칸으로 돌아가 hsplit
    let t = &w1.tabs[2];
    assert_eq!(t.title.as_deref(), Some("complex tab"));
    assert_eq!(t.panes.len(), 4);
    let PlanNode::Split {
        horizontal,
        first,
        second,
        ..
    } = &t.tree
    else {
        panic!("{:?}", t.tree)
    };
    assert!(horizontal);
    assert!(
        matches!(&**first, PlanNode::Split { horizontal: false, first, second, .. }
        if **first == PlanNode::Leaf(0) && **second == PlanNode::Leaf(3)),
        "첫 열: 0 위, 3 아래"
    );
    assert!(
        matches!(&**second, PlanNode::Split { horizontal: false, ratio, first, second }
        if (*ratio - 0.6).abs() < 1e-6 && **first == PlanNode::Leaf(1) && **second == PlanNode::Leaf(2)),
        "둘째 열: 1 위 60%, 2 아래 40%"
    );
    // 대응이 없는 줄은 줄 번호와 함께 경고하고 넘어갔다
    let joined = p.warnings.join("\n");
    for cmd in [
        "enabled_layouts",
        "os_window_size",
        "os_window_position",
        "os_window_class",
        "os_window_name",
        "os_window_state",
        "resize_window",
        "focus_os_window",
        "--hold",
    ] {
        assert!(joined.contains(cmd), "{cmd} 경고가 없다: {joined}");
    }
    assert!(p.warnings.iter().all(|w| w.contains("번째 줄")), "{:?}", p.warnings);
    assert_eq!(p.warnings.len(), 9, "{:?}", p.warnings);
}

#[test]
fn unknown_and_broken_lines_warn_and_continue() {
    let p = parse_text(
        "launch a\nfrobnicate now\nlaunch 'unclosed\nlayout spiral\nlaunch --type=tab x\nlaunch --title\nlaunch b\n",
    );
    // 앞뒤 정상 줄은 그대로: a, b 두 칸
    assert_eq!(tab0(&p).panes.len(), 2, "{:?}", p.warnings);
    assert_eq!(p.warnings.len(), 5, "{:?}", p.warnings);
    assert!(
        p.warnings[0].starts_with("2번째 줄") && p.warnings[0].contains("frobnicate"),
        "{:?}",
        p.warnings
    );
    assert!(
        p.warnings[1].starts_with("3번째 줄") && p.warnings[1].contains("닫히지"),
        "{:?}",
        p.warnings
    );
    assert!(p.warnings[2].contains("spiral"), "{:?}", p.warnings);
    assert!(p.warnings[3].contains("--type=tab"), "{:?}", p.warnings);
    assert!(p.warnings[4].contains("--title"), "{:?}", p.warnings);
}

#[test]
fn empty_or_comment_only_file_is_one_default_shell() {
    for text in ["", "\n\n  # 주석만\n"] {
        let p = parse_text(text);
        assert!(p.warnings.is_empty());
        assert_eq!(p.windows.len(), 1);
        assert_eq!(p.windows[0].tabs.len(), 1);
        assert_eq!(
            tab0(&p).panes,
            [PanePlan::default()],
            "칸이 없는 탭은 기본 셸 하나(kitty finalize_session)"
        );
    }
}

#[test]
fn new_tab_drops_trailing_empty_tab_and_names_tabs() {
    // 처음 암묵 탭이 비어 있으면 new_tab이 지운다(그 앞의 layout은 버려진다)
    let p = parse_text("layout fat\nnew_tab  a b \nlaunch x\nnew_tab\nlaunch y\nnew_tab last");
    let tabs = &p.windows[0].tabs;
    assert_eq!(tabs.len(), 3);
    assert_eq!(tabs[0].title.as_deref(), Some("a b"), "앞뒤 공백은 지운다");
    assert_eq!(tabs[1].title, None, "제목이 없으면 칸 제목을 따른다");
    assert_eq!(tabs[2].panes, [PanePlan::default()], "칸이 없는 마지막 탭도 셸이 뜬다");
    assert!(matches!(tabs[0].tree, PlanNode::Leaf(0)));
}

#[test]
fn cd_resolution_and_env_expansion() {
    let p = parse_text(
        "cd $PROJ/x\nlaunch a\ncd rel\nlaunch b\ncd\nlaunch c\ncd ~/h\nlaunch --cwd=$PROJ/y d $PROJ/z\ncd /abs\nlaunch --cwd sub e",
    );
    let panes = &tab0(&p).panes;
    assert_eq!(panes[0].cwd, Some(PathBuf::from("/work/proj/x")), "$VAR 풀기");
    assert_eq!(
        panes[1].cwd,
        Some(PathBuf::from("/sess/rel")),
        "상대 경로는 세션 파일 폴더 기준"
    );
    assert_eq!(
        panes[2].cwd,
        Some(PathBuf::from("/sess")),
        "인자 없는 cd는 세션 파일 폴더"
    );
    assert_eq!(
        panes[3].cwd,
        Some(PathBuf::from("/work/proj/y")),
        "--cwd가 cd보다 앞선다"
    );
    assert_eq!(panes[3].args, ["$PROJ/z"], "인자의 변수는 풀지 않는다(kitty 문서)");
    assert_eq!(panes[4].cwd, Some(PathBuf::from("/sess/sub")));
    assert!(p.warnings.is_empty(), "{:?}", p.warnings);
}

#[test]
fn title_line_applies_to_next_window_only() {
    let p = parse_text("title  First one \nlaunch a\nlaunch b\ntitle X\nlaunch --title Y c");
    let panes = &tab0(&p).panes;
    assert_eq!(panes[0].title.as_deref(), Some("First one"));
    assert_eq!(panes[1].title, None);
    assert_eq!(
        panes[2].title.as_deref(),
        Some("Y"),
        "launch --title이 title 줄보다 앞선다"
    );
}

#[test]
fn focus_and_focus_tab() {
    let p =
        parse_text("launch a\nlaunch b\nfocus\nlaunch c\nnew_tab t2\nlaunch d\nnew_tab t3\nfocus\nlaunch e\nlaunch f");
    let w = &p.windows[0];
    assert_eq!(
        w.tabs[0].focus, 1,
        "focus는 그때까지 만든 마지막 칸(이후 칸이 생겨도 그대로)"
    );
    assert_eq!(w.tabs[1].focus, 0, "focus가 없으면 마지막 칸");
    assert_eq!(w.tabs[2].focus, 0, "칸이 없을 때의 focus는 첫 칸");
    assert_eq!(w.active_tab, 2, "focus가 있는 탭이 처음 탭");
    let p = parse_text("new_tab a\nlaunch x\nnew_tab b\nlaunch y\nfocus_tab 1");
    assert_eq!(p.windows[0].active_tab, 1);
    let p = parse_text("new_tab a\nlaunch x\nnew_tab logs\nlaunch y\nfocus_tab title:logs\nfocus_tab 9");
    assert_eq!(p.windows[0].active_tab, 0);
    let p2 = parse_text("new_tab a\nlaunch x\nnew_tab logs\nlaunch y\nfocus_tab title:logs");
    assert_eq!(p2.windows[0].active_tab, 1);
    assert!(
        p.warnings
            .iter()
            .any(|w| w.starts_with("6번째 줄") && w.contains("focus_tab 9")),
        "{:?}",
        p.warnings
    );
}

#[test]
fn focus_matching_window_by_var() {
    // 변수로 첫 칸을 되찾아 거기에 hsplit
    let p = parse_text(
        "layout splits\nlaunch --var window=first\nlaunch --location=vsplit\nfocus_matching_window var:window=first\nlaunch --location=hsplit\nfocus_matching_window var:window=nope",
    );
    assert_eq!(leaves(&tab0(&p).tree), 3);
    assert!(
        p.warnings.iter().any(|w| w.contains("var:window=nope")),
        "{:?}",
        p.warnings
    );
    let p = parse_text("focus_matching_window title:x\n");
    assert!(p.warnings[0].contains("var:"), "{:?}", p.warnings);
}

#[test]
fn launch_command_is_argv_not_shell_string() {
    // 따옴표 안 공백은 한 인자, 셸 메타 문자는 글자 그대로
    let p = parse_text(r#"launch sh -c "echo a; rm -rf /tmp/x && echo 'b c' | cat""#);
    let pane = &tab0(&p).panes[0];
    assert_eq!(pane.program.as_deref(), Some("sh"));
    assert_eq!(pane.args, ["-c", "echo a; rm -rf /tmp/x && echo 'b c' | cat"]);
    // 옵션 값에 따옴표
    let p = parse_text(r#"launch --title "a b" --env 'K=v w' prog -x --title"#);
    let pane = &tab0(&p).panes[0];
    assert_eq!(pane.title.as_deref(), Some("a b"));
    assert_eq!(pane.env, [("K".to_string(), "v w".to_string())]);
    assert_eq!(pane.args, ["-x", "--title"], "프로그램 뒤는 모두 인자");
    // `--` 뒤는 모두 프로그램과 인자
    let p = parse_text("launch -- --weird arg");
    assert_eq!(tab0(&p).panes[0].program.as_deref(), Some("--weird"));
    // 별칭 --window-title, 값을 받는 -w, 값 없는 깃발은 프로그램 앞에서 건너뛴다
    let p = parse_text("launch --window-title T -w watcher.py --hold --keep-focus prog arg");
    let pane = &tab0(&p).panes[0];
    assert_eq!(pane.title.as_deref(), Some("T"));
    assert_eq!(pane.program.as_deref(), Some("prog"));
    assert_eq!(pane.args, ["arg"]);
    assert_eq!(p.warnings.len(), 3, "-w·--hold·--keep-focus: {:?}", p.warnings);
    // 잘못된 --env는 칸은 만들고 그 옵션만 버린다
    let p = parse_text("launch --env NOEQ prog");
    assert_eq!(tab0(&p).panes[0].env, []);
    assert!(p.warnings[0].contains("--env"), "{:?}", p.warnings);
    assert_eq!(tab0(&p).panes[0].program.as_deref(), Some("prog"));
}

#[test]
fn split_words_quotes_and_escapes() {
    let w = |s: &str| split_words(s).unwrap();
    assert_eq!(w("a  b\tc"), ["a", "b", "c"]);
    assert_eq!(w(r#"a "b c" 'd e'"#), ["a", "b c", "d e"]);
    assert_eq!(w(r#""a\"b" 'a\b' a\ b"#), ["a\"b", r"a\b", "a b"]);
    assert_eq!(w("''"), [""], "빈 따옴표는 빈 낱말");
    assert_eq!(w(""), Vec::<String>::new());
    assert_eq!(w(r#"x"y"'z'"#), ["xyz"], "붙은 따옴표는 한 낱말");
    assert_eq!(w(r"$'a\nb\tc\x41é\101\\'"), ["a\nb\tcAé".to_string() + "A\\"]);
    assert_eq!(w(r"$'nul\x00end'"), ["nulend"], "NUL은 넣지 않는다");
    assert_eq!(w(r"$'\q'"), [r"\q"], "모르는 이스케이프는 그대로");
    assert!(split_words("a 'b").is_err());
    assert!(split_words("a \"b").is_err());
    assert!(split_words("a $'b").is_err());
}

#[test]
fn caps_on_tabs_panes_and_windows_do_not_open_thousands() {
    // 탭: 64개에서 멈추고 한 번만 경고
    let text: String = (0..500).map(|i| format!("new_tab t{i}\nlaunch x\n")).collect();
    let p = parse_text(&text);
    assert_eq!(p.windows[0].tabs.len(), MAX_TABS);
    assert_eq!(p.warnings.len(), 1, "{:?}", p.warnings.len());
    assert!(p.warnings[0].contains("64"), "{:?}", p.warnings);
    // 탭 하나의 칸: 32개
    let text: String = (0..100).map(|_| "launch x\n").collect();
    let p = parse_text(&text);
    assert_eq!(tab0(&p).panes.len(), MAX_PANES_PER_TAB);
    assert_eq!(leaves(&tab0(&p).tree), MAX_PANES_PER_TAB);
    // 전체 칸: 256개
    let text: String = (0..60)
        .map(|i| format!("new_tab t{i}\n{}", "launch x\n".repeat(30)))
        .collect();
    let p = parse_text(&text);
    let total: usize = p.windows.iter().flat_map(|w| &w.tabs).map(|t| t.panes.len()).sum();
    assert_eq!(total, MAX_PANES);
    // 창: 8개
    let text: String = (0..50).map(|_| "new_os_window\nlaunch x\n").collect();
    let p = parse_text(&text);
    assert_eq!(p.windows.len(), MAX_WINDOWS);
    assert!(p.warnings.iter().any(|w| w.contains("창이 8개")), "{:?}", p.warnings);
    // 경고도 상한: 모르는 줄이 수만 개여도 경고는 101개 이하
    let text = "bogus\n".repeat(20_000);
    let p = parse_text(&text);
    assert!(p.warnings.len() <= 101, "{}", p.warnings.len());
}

#[test]
fn line_breaks_like_python_splitlines() {
    // kitty는 str.splitlines()로 나눈다: 한 줄 안의 \v·\f·U+2028도 줄바꿈이라 줄을 몰래 끼워 넣을 수 없다
    let p = parse_text("launch a\u{2028}launch b\x0b\x0claunch c\r\nlaunch d\rlaunch e");
    assert_eq!(tab0(&p).panes.len(), 5);
}

fn tmp(name: &str) -> PathBuf {
    std::env::temp_dir().join(format!("bt-session-{}-{name}", std::process::id()))
}

#[test]
fn read_file_limits_and_kinds() {
    let ok = tmp("ok");
    std::fs::write(&ok, "\u{feff}launch a\n").unwrap();
    assert_eq!(read_file(&ok).unwrap(), "launch a\n", "BOM은 벗긴다");
    // 상한 이하는 읽고 초과는 거절
    let big = tmp("big");
    std::fs::write(&big, vec![b'#'; MAX_FILE_BYTES as usize]).unwrap();
    assert!(read_file(&big).is_ok());
    std::fs::write(&big, vec![b'#'; MAX_FILE_BYTES as usize + 1]).unwrap();
    assert!(read_file(&big).unwrap_err().contains("상한"));
    // UTF-8이 아니면 거절
    let bad = tmp("bad");
    std::fs::write(&bad, [0xff, 0xfe, b'a']).unwrap();
    assert!(read_file(&bad).unwrap_err().contains("UTF-8"));
    // 폴더·없는 파일
    assert!(read_file(&std::env::temp_dir()).unwrap_err().contains("보통 파일"));
    assert!(read_file(&tmp("missing")).is_err());
    for f in [ok, big, bad] {
        let _ = std::fs::remove_file(f);
    }
}
