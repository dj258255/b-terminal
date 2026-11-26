//! 입력 함께 보내기(브로드캐스트 입력, #860, ADR-091). 번호 붙은 묶음(0~15)에 든 칸들 가운데 한 칸에 사람이 친 입력을
//! 같은 묶음의 다른 칸에도 보낸다. 모양은 Ghostty 열린 PR #13745를 따른다(`toggle_broadcast_group:N`, `clear_broadcast_groups`).
//!
//! 지키는 것
//! - 바이트를 복사하지 않는다. 키는 칸마다 그 칸의 모드(DECCKM·kitty 키보드·modifyOtherKeys)로 다시 인코딩하고,
//!   붙여넣기는 칸마다 그 칸의 괄호 붙여넣기 모드로 따로 감싼다(위험 바이트는 모드와 상관없이 공백, AGENTS.md).
//! - 읽기 전용 칸은 보내는 쪽도 받는 쪽도 아니다. 암호 입력 중인 칸에서 친 글은 퍼지지 않는다.
//! - 조합 중인 글(preedit)은 퍼지지 않는다. 확정 글만 간다.
//! - 이 모듈의 함수는 사람의 입력만 받는다. `btctl send`·`btctl run`·OSC 응답은 이 길을 지나지 않는다.
//!
//! 이 모듈은 창과 상관없는 순수 함수와 세션 쓰기만 한다. 칸 목록은 호출하는 쪽이 `Member`로 빌려 준다.

use bt_core::Rgb;
use bt_term::Session;
use winit::keyboard::ModifiersState;

use crate::keys::{self, KeyInput};

/// 묶음 수. 번호는 0~15(Ghostty PR의 `u4`)
pub const GROUPS: u8 = 16;

/// 칸 하나의 입력 쪽 모습. 호출하는 쪽이 칸 목록에서 빌려 만든다
pub struct Member<'a> {
    pub id: u64,
    pub group: Option<u8>,
    pub session: &'a Session,
    /// 프로그램이 끝났거나 끝난 칸을 남겨 둔 칸: 받지 않는다
    pub gone: bool,
    /// 지금 암호 입력 중(termios로 알아챈 값)
    pub password: bool,
}

/// 키 하나를 칸마다 다시 인코딩하는 데 필요한 것(사람의 키와 스크립트 `send key`가 같다)
pub struct KeyCtx {
    pub input: KeyInput,
    pub mods: ModifiersState,
    pub raw: ModifiersState,
    pub meta: bool,
}

/// `toggle_broadcast_group:N`을 한 뒤 칸의 묶음: 이미 그 묶음이면 빠지고, 아니면(다른 묶음이거나 없음) 그 묶음에 든다
pub fn toggled(current: Option<u8>, n: u8) -> Option<u8> {
    if current == Some(n) { None } else { Some(n) }
}

/// 묶음 번호의 색(설정 `broadcast-group-colors`로 덮은 표에서). 범위 밖 번호는 마지막 색으로 눌러 둔다
pub fn color_of(colors: &[Rgb; GROUPS as usize], group: u8) -> Rgb {
    colors[usize::from(group).min(colors.len() - 1)]
}

/// 원본 칸(`origin`)에서 친 입력이 퍼질 칸들. 원본이 묶음에 없거나 읽기 전용이거나 암호 입력 중이면 없다.
/// 받는 칸은 같은 묶음이고 끝나지 않았고 읽기 전용이 아닌 칸.
/// 읽기 전용 원본은 쓸 수 없는 칸이라 거기서 친 글이 다른 칸으로 새지 않게 하고, 암호 입력 중인 칸의 글은 다른 칸에
/// 보이는 글자로 찍히거나 셸 기록에 남지 않게 한다
pub fn targets<'a, 'm>(origin: u64, members: &'a [Member<'m>]) -> Vec<&'a Member<'m>> {
    let Some(from) = members.iter().find(|m| m.id == origin) else {
        return Vec::new();
    };
    let Some(group) = from.group else {
        return Vec::new();
    };
    if from.session.readonly() || from.session.keyboard_locked() || from.password {
        return Vec::new();
    }
    members
        .iter()
        .filter(|m| m.id != origin && m.group == Some(group) && !m.gone && !m.session.readonly())
        .collect()
}

/// 키 하나를 같은 묶음의 다른 칸에 그 칸의 모드로 다시 인코딩해 쓴다. 키 뗌을 받고 싶은 칸(kitty 2)이 있으면 참
pub fn send_key(
    origin: u64,
    members: &[Member],
    key: &KeyCtx,
    scroll: bool,
    clears_selection: &dyn Fn(&[u8]) -> bool,
) -> bool {
    let mut wants_release = false;
    for t in targets(origin, members) {
        let modes = t.session.modes();
        wants_release |= modes.kitty.report_events;
        let bytes = keys::encode_input(&key.input, key.mods, key.raw, modes, key.meta);
        if bytes.is_empty() {
            continue;
        }
        if clears_selection(&bytes) {
            t.session.select_clear();
        }
        t.session.write_input(&bytes, scroll);
    }
    wants_release
}

/// 키 뗌: kitty 키보드 프로토콜의 이벤트 종류 보고(2)를 켠 칸에만 그 칸의 모드로 인코딩해 쓴다
pub fn send_release(origin: u64, members: &[Member], key: &KeyCtx) {
    for t in targets(origin, members) {
        let modes = t.session.modes();
        if !modes.kitty.report_events {
            continue;
        }
        let bytes = keys::encode_input(&key.input, key.mods, key.raw, modes, key.meta);
        t.session.write_input(&bytes, false);
    }
}

/// 모드와 상관없는 바이트(IME 확정 글, `text:` 묶음의 글)를 같은 묶음의 다른 칸에 그대로 쓴다
pub fn send_bytes(
    origin: u64,
    members: &[Member],
    bytes: &[u8],
    scroll: bool,
    clears_selection: &dyn Fn(&[u8]) -> bool,
) {
    for t in targets(origin, members) {
        if clears_selection(bytes) {
            t.session.select_clear();
        }
        t.session.write_input(bytes, scroll);
    }
}

/// 이 붙여넣기를 물어야 하는가: 원본과 받을 칸 가운데 하나라도 붙여넣기 보호가 막는 글이면 참.
/// 칸마다 모드가 달라 한 칸에서는 안전해도 다른 칸에서는 줄바꿈이 바로 실행될 수 있다
pub fn paste_needs_confirm(origin: u64, members: &[Member], text: &str, bracketed_safe: bool) -> bool {
    let from = members.iter().find(|m| m.id == origin);
    from.into_iter()
        .chain(targets(origin, members))
        .any(|m| !bt_term::paste_is_safe(text, m.session.modes().bracketed_paste, bracketed_safe))
}

/// 붙여넣기를 같은 묶음의 다른 칸에 칸마다 그 칸의 괄호 붙여넣기 모드로 감싸 쓴다. 보낸 칸 수.
/// 물어야 하는지는 호출하는 쪽이 `paste_needs_confirm`으로 먼저 정한다. 감싸기(`encode_paste`)가 위험 바이트를 늘 공백으로 바꾼다
pub fn send_paste(origin: u64, members: &[Member], text: &str, scroll: bool) -> usize {
    let mut n = 0;
    for t in targets(origin, members) {
        let bytes = bt_term::encode_paste(text, t.session.modes().bracketed_paste);
        t.session.select_clear();
        t.session.write_input(&bytes, scroll);
        n += 1;
    }
    n
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;
    use std::time::{Duration, Instant};

    use bt_core::Theme;
    use bt_term::{GridSize, SessionEvent, SpawnOptions};
    use winit::keyboard::{Key, KeyCode, KeyLocation, NamedKey, PhysicalKey};

    /// 받은 바이트 `count`개를 16진수로 화면에 찍고 기다리는 /bin/sh 칸. 줄 처리·되울림을 끄고(raw) 한 바이트씩 읽는다
    fn pane(count: usize) -> Session {
        let script = format!("stty raw -echo; dd bs=1 count={count} 2>/dev/null | od -An -tx1; sleep 30");
        let opts = SpawnOptions {
            program: Some("/bin/sh".into()),
            args: vec!["-c".into(), script],
            cwd: Some(std::env::temp_dir()),
            ..Default::default()
        };
        let sink = Arc::new(|_e: SessionEvent| {});
        let s = Session::spawn(opts, GridSize::new(80, 10), Theme::default(), sink).expect("spawn");
        // stty가 raw로 바꾸기 전에 쓴 바이트가 되울림으로 섞이지 않게 잠깐 기다린다
        std::thread::sleep(Duration::from_millis(300));
        s
    }

    fn screen_of(s: &Session, want: &str) -> String {
        let deadline = Instant::now() + Duration::from_secs(5);
        loop {
            // od가 칸 사이를 두 칸 띄우고 16바이트마다 줄을 바꾸므로 공백을 하나로 모아 견준다
            let text = s.snapshot().text().split_whitespace().collect::<Vec<_>>().join(" ");
            if text.contains(want) {
                return text;
            }
            assert!(Instant::now() < deadline, "화면에 {want:?}이 나오지 않았다:\n{text}");
            std::thread::sleep(Duration::from_millis(20));
        }
    }

    fn quiet(s: &Session) -> String {
        std::thread::sleep(Duration::from_millis(500));
        s.snapshot().text().split_whitespace().collect::<Vec<_>>().join(" ")
    }

    fn m(id: u64, group: Option<u8>, s: &Session) -> Member<'_> {
        Member {
            id,
            group,
            session: s,
            gone: false,
            password: false,
        }
    }

    fn up_arrow() -> KeyCtx {
        KeyCtx {
            input: KeyInput {
                pressed: true,
                repeat: false,
                logical: Key::Named(NamedKey::ArrowUp),
                without_modifiers: Key::Named(NamedKey::ArrowUp),
                location: KeyLocation::Standard,
                physical: PhysicalKey::Code(KeyCode::ArrowUp),
                text: None,
            },
            mods: ModifiersState::empty(),
            raw: ModifiersState::empty(),
            meta: false,
        }
    }

    fn letter_a(pressed: bool) -> KeyCtx {
        KeyCtx {
            input: KeyInput {
                pressed,
                repeat: false,
                logical: Key::Character("a".into()),
                without_modifiers: Key::Character("a".into()),
                location: KeyLocation::Standard,
                physical: PhysicalKey::Code(KeyCode::KeyA),
                text: pressed.then(|| "a".into()),
            },
            mods: ModifiersState::empty(),
            raw: ModifiersState::empty(),
            meta: false,
        }
    }

    fn never(_: &[u8]) -> bool {
        false
    }

    #[test]
    fn toggled_leaves_the_same_group_and_moves_to_another() {
        assert_eq!(toggled(None, 3), Some(3));
        assert_eq!(toggled(Some(3), 3), None);
        assert_eq!(toggled(Some(2), 3), Some(3));
    }

    #[test]
    fn colors_follow_the_table() {
        let mut t = bt_core::config::BROADCAST_GROUP_COLORS;
        assert_eq!(
            color_of(&t, 0),
            Rgb {
                r: 0xff,
                g: 0x78,
                b: 0x00
            }
        );
        t[5] = Rgb { r: 1, g: 2, b: 3 };
        assert_eq!(color_of(&t, 5), Rgb { r: 1, g: 2, b: 3 });
        // 범위 밖 번호는 마지막 색으로 눌러 둔다(패닉하지 않는다)
        assert_eq!(color_of(&t, 200), t[15]);
    }

    #[test]
    fn targets_are_the_other_live_writable_members_of_the_group() {
        let s: Vec<Session> = (0..6).map(|_| pane(1)).collect();
        s[3].set_readonly(true);
        let mut v: Vec<Member> = vec![
            m(10, Some(0), &s[0]), // 원본
            m(11, Some(0), &s[1]), // 받음
            m(12, Some(1), &s[2]), // 다른 묶음
            m(13, Some(0), &s[3]), // 읽기 전용
            m(14, None, &s[4]),    // 묶음 없음
            m(15, Some(0), &s[5]), // 끝난 칸
        ];
        v[5].gone = true;
        let ids: Vec<u64> = targets(10, &v).iter().map(|t| t.id).collect();
        assert_eq!(ids, vec![11]);
        // 원본이 묶음에 없으면 아무 데도 가지 않는다
        assert!(targets(14, &v).is_empty());
        // 원본이 읽기 전용이면(쓸 수 없는 칸에서 친 글이 새지 않게) 가지 않는다
        assert!(targets(13, &v).is_empty());
        // 원본이 암호 입력 중이면 가지 않는다
        v[0].password = true;
        assert!(targets(10, &v).is_empty());
    }

    #[test]
    fn key_is_reencoded_with_each_peers_own_mode() {
        let (origin, normal, app, other) = (pane(3), pane(3), pane(3), pane(3));
        // 한 칸은 프로그램이 커서 키 모드(DECCKM)를 켰다
        app.feed_local(b"\x1b[?1h");
        let v = vec![
            m(1, Some(0), &origin),
            m(2, Some(0), &normal),
            m(3, Some(0), &app),
            m(4, Some(1), &other),
        ];
        send_key(1, &v, &up_arrow(), false, &never);
        // 보통 모드: ESC [ A, 커서 키 모드: ESC O A
        screen_of(&normal, "1b 5b 41");
        screen_of(&app, "1b 4f 41");
        assert!(!quiet(&other).contains("1b"), "다른 묶음에는 가지 않는다");
    }

    #[test]
    fn readonly_peer_gets_nothing() {
        let (origin, ro, ok) = (pane(1), pane(1), pane(1));
        ro.set_readonly(true);
        let v = vec![m(1, Some(2), &origin), m(2, Some(2), &ro), m(3, Some(2), &ok)];
        send_bytes(1, &v, b"x", false, &never);
        screen_of(&ok, "78");
        assert!(!quiet(&ro).contains("78"));
    }

    #[test]
    fn committed_text_goes_to_peers_unchanged() {
        let bytes = "한".as_bytes();
        let (origin, peer) = (pane(bytes.len()), pane(bytes.len()));
        let v = vec![m(1, Some(0), &origin), m(2, Some(0), &peer)];
        send_bytes(1, &v, bytes, false, &never);
        screen_of(&peer, "ed 95 9c");
    }

    #[test]
    fn paste_is_wrapped_per_peer_and_dangerous_bytes_become_spaces() {
        let text = "a\x1bb\x03c\n";
        // 괄호 붙여넣기 칸: ESC[200~ + 글(ESC·Ctrl-C는 공백, 줄바꿈은 그대로) + ESC[201~ = 6 + 6 + 6
        let bracketed = pane(18);
        bracketed.feed_local(b"\x1b[?2004h");
        // 괄호 붙여넣기가 꺼진 칸: 글만, 줄바꿈은 CR
        let plain = pane(6);
        let origin = pane(1);
        let v = vec![m(1, Some(0), &origin), m(2, Some(0), &bracketed), m(3, Some(0), &plain)];
        assert_eq!(send_paste(1, &v, text, false), 2);
        screen_of(&bracketed, "1b 5b 32 30 30 7e 61 20 62 20 63 0a 1b 5b 32 30 31 7e");
        screen_of(&plain, "61 20 62 20 63 0d");
    }

    #[test]
    fn paste_asks_when_any_target_would_run_a_newline() {
        let (origin, bracketed, plain) = (pane(1), pane(1), pane(1));
        origin.feed_local(b"\x1b[?2004h");
        bracketed.feed_local(b"\x1b[?2004h");
        let text = "ls\nrm x";
        // 모두 괄호 붙여넣기 칸이면 `paste-bracketed-safe`일 때 묻지 않는다
        let all = vec![m(1, Some(0), &origin), m(2, Some(0), &bracketed)];
        assert!(!paste_needs_confirm(1, &all, text, true));
        // 한 칸이 괄호 붙여넣기를 쓰지 않으면 그 칸에서는 줄바꿈이 곧바로 실행된다: 묻는다
        let mixed = vec![m(1, Some(0), &origin), m(2, Some(0), &bracketed), m(3, Some(0), &plain)];
        assert!(paste_needs_confirm(1, &mixed, text, true));
        // 다른 묶음의 칸은 보지 않는다
        let other = vec![m(1, Some(0), &origin), m(3, Some(1), &plain)];
        assert!(!paste_needs_confirm(1, &other, text, true));
    }

    #[test]
    fn release_goes_only_to_peers_that_report_events() {
        // kitty 키보드 프로토콜: 1(모호한 키 구분)|2(이벤트 종류 보고)
        let kitty = pane(9);
        kitty.feed_local(b"\x1b[>3u");
        let plain = pane(1);
        let origin = pane(1);
        let v = vec![m(1, Some(0), &origin), m(2, Some(0), &kitty), m(3, Some(0), &plain)];
        send_release(1, &v, &letter_a(false));
        // ESC [ 9 7 ; 1 : 3 u
        screen_of(&kitty, "1b 5b 39 37 3b 31 3a 33 75");
        assert!(!quiet(&plain).contains("1b"));
        // 누름을 보낼 때 뗌을 받고 싶은 칸이 있다고 알려 준다
        assert!(send_key(1, &v, &letter_a(true), false, &never));
    }
}
