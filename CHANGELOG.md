# 변경 기록

이 파일은 [Keep a Changelog](https://keepachangelog.com/ko/1.1.0/) 형식을 따르고, 버전은 [유의적 버전](https://semver.org/lang/ko/)을 따릅니다.

## [아직 릴리스하지 않음]

- 색 이모지 글꼴은 파일에서 그 꼴과 셀 크기에 맞는 비트맵 한 벌만 읽습니다(Apple Color Emoji 180MiB 중 약 7~10MiB). 이모지를 그린 뒤 메모리가 약 160MiB 줄었습니다.

## [0.2.0] - 2026-11-06

Ghostty 없이 혼자 서는 우리 터미널. AI CLI가 쓰는 표준 신호(kitty 키보드, 진행 표시, 클립보드, 셸 통합)를 받고, 화면 분할·창 여러 개·링크·찾기·색 이모지를 더했습니다. 추적 이슈 #32.

- 설정을 처음 다시 읽거나 글자 크기를 처음 바꿀 때 메모리가 약 250MiB 뛰던 것을 고쳤습니다(풀어 둔 글꼴을 다시 씀).
- 창 여러 개: Cmd-N(`new_window`)으로 새 창을 엽니다. 창마다 탭·분할·찾기를 따로 두고, 셸이 끝나 창의 마지막 칸이 닫히면 그 창이, 마지막 창이 닫히면 앱이 끝납니다. `btctl new --window`. 창 밖에서 부른 btctl은 마지막으로 보던 창을 씁니다. 전에는 `new_window`가 새 탭을 열었습니다.
- 색 이모지: 😀🎉✅ 같은 두 칸 이모지를 색 글꼴(macOS는 Apple Color Emoji, Linux는 Noto Color Emoji)에서 두 칸 가운데에 그립니다(swash). Claude Code의 `⏺`처럼 한 칸 기호는 글자색을 따르는 흑백 글꼴(STIX Two Math·Noto Sans Symbols 2)에서 그립니다. 변형 선택자(❤️)·ZWJ 묶음(👨‍👩‍👧)·피부색은 아직 합치지 않고 글자마다 그립니다.
- 커서 깜빡임: `cursor-style-blink = true`이거나 프로그램이 깜빡이는 커서(DECSCUSR)를 고르면 0.6초마다 깜빡입니다. 키를 치면 다시 보이고, 창을 보지 않을 때는 멈춥니다.
- 바뀐 줄만 다시 그리기: 앞 프레임을 들고 있다가 바뀐 줄만 다시 그립니다. 한 줄만 바뀐 프레임의 그리기가 약 6ms에서 0.1~0.2ms로 줄었습니다(실험 E2, 한 기기). 바뀐 것이 없으면 창에 올리지 않습니다.
- 찾기: Cmd-F로 칸 오른쪽 위에 찾기 막대를 열고, 치는 대로 스크롤백 전체에서 찾습니다(글자 그대로, 대문자가 없으면 대소문자 무시). 맞은 곳은 노랑으로 칠하고 지금 것은 진하게, "2/5"처럼 몇 번째인지 보입니다. Enter·Cmd-G는 위로, Shift-Enter·Cmd-Shift-G는 아래로, Esc로 닫습니다. `start_search`·`end_search`·`navigate_search`.
- 링크 열기: Cmd를 누른 채 마우스를 올리면 화면의 URL(여러 줄로 접힌 긴 주소 포함)과 OSC 8 링크에 밑줄을 긋고 실제 주소를 창 왼쪽 아래에 보입니다. Cmd-클릭으로 엽니다. `http`·`https`와 이 컴퓨터의 파일만 엽니다. 웹 주소는 사용자 정보·보이지 않는 서식 문자·비ASCII 호스트가 있으면 열지 않고, 주소 띠는 호스트를 다 보입니다. 파일은 폴더와 알려진 문서만 열고 그 밖에는 Finder에서 보이기만 합니다(ADR-007).
- 화면 분할: 탭 안을 칸으로 나눕니다(Cmd-D 오른쪽, Cmd-Shift-D 아래). 칸 옮기기(Cmd-[ / Cmd-], Cmd-Opt-방향키, 누르기), 경계 밀기(Cmd-Ctrl-방향키), 크기 고르게(Cmd-Ctrl-=), 크게 보기(Cmd-Shift-Enter), 칸 닫기(Cmd-W). 고르지 않은 칸은 흐리게, 커서는 속 빈 네모로 보입니다. Ghostty와 같은 동작 이름과 기본 키이고, `unfocused-split-opacity`·`unfocused-split-fill`·`split-divider-color`를 읽습니다. `btctl new --split 방향`으로 칸을 나눠 엽니다. **Cmd-[ / Cmd-]는 이제 칸 옮기기입니다**(Ghostty 기본과 같게). 탭 옮기기는 Cmd-Shift-[ / Cmd-Shift-]입니다.
- macOS 앱 묶음: `scripts/bundle-macos.sh [--install]`이 `b-terminal.app`(아이콘, btctl 포함, 임시 서명)을 만듭니다. 메뉴 막대(앱·셸·편집·보기·윈도우)와 "설정…"(예시 설정 파일을 만들어 엶). 아이콘은 `scripts/make-icon.swift`로 그립니다.
- 셸 통합: zsh·bash에 자동으로 실어 명령의 시작·끝·종료 코드를 압니다(OSC 133·633). Cmd-↑/↓로 프롬프트 사이 이동, 창을 보지 않을 때 오래 걸린 명령이 끝나면 알림, `btctl run`(다른 탭에서 명령을 돌리고 출력·종료 코드를 받음)과 `btctl last`. `shell-integration`, `notify-command-seconds`.
- 진행 표시(OSC 9;4): 탭 줄의 그 탭 아래(탭이 하나면 창 맨 위)에 막대를 그립니다. 보통·오류·끝 모름·멈춤을 색으로 가르고, 15초 소식이 없으면 지웁니다.
- 클립보드 쓰기(OSC 52)를 기본으로 허용합니다(AI CLI의 `/copy`). 보고 있는 창의 고른 칸에서 온 것만 받고, 받으면 창 안 알림 띠로 보입니다. 읽기는 기본 금지. `clipboard-write`·`clipboard-read`.
- kitty 키보드 프로토콜(1단계 모호한 키 구분, 8단계 모든 키를 이스케이프로): Claude Code에서 Shift+Enter가 줄바꿈이 됩니다. 프로그램의 `CSI ? u` 질의에 답합니다.
- Ghostty 없이 혼자 섭니다: 테마 905개(iTerm2-Color-Schemes)를 앱에 넣었고, 테마 이름은 띄어쓰기·대소문자를 무시합니다. Ghostty 설정 읽기는 기본으로 끕니다(`import-ghostty = true`로 켬). `--list-themes`(ADR-006).

### 알려진 한계

- 키보드·마우스로 직접 쳐 보는 확인이 대부분 빠져 있습니다(한글 IME, Cmd 단축키, 분할·찾기·링크의 키와 마우스, 창 닫기 단추). 기능은 btctl과 캡처로, 키 → 동작은 테스트로 확인했습니다. 각 PR의 "돌리지 않은 검증"에 적었습니다.
- 변형 선택자(❤️)·ZWJ 묶음(👨‍👩‍👧)·피부색 이모지는 글자마다 따로 그립니다.
- 이모지를 한 번 그리면 메모리가 약 180MiB 늡니다(색 글꼴을 통째로 읽음, #59). 처음 메모리도 약 380MiB입니다(fontdue가 대체 글꼴의 글리프를 모두 풀어 둠).
- 창에 올리는 비용이 macOS에서 프레임마다 약 10ms 남습니다(softbuffer, 실험 E2).
- 앱은 임시 서명이라 다른 Mac에서는 Gatekeeper가 막을 수 있습니다(서명·공증 전).
- 화면 배율이 다른 두 모니터에 창을 나눠 두면 한쪽 창의 글자 크기가 맞지 않을 수 있습니다.

## [0.1.0] - 2026-11-06

첫 판. Ghostty 같은 Rust 터미널 에뮬레이터입니다(macOS 1순위).

- 저장소 기여 방식(이슈 양식, PR 템플릿과 본문 검사, 에이전트 작업 규칙)을 b-studio에서 옮겨 왔습니다.
- 범위를 순수 터미널로 정했습니다. 한때 넣었던 에이전트 상태 사이드바·에이전트 훅·구독 사용량 막대는 걷어냈습니다. AI 작업 화면은 b-studio가 맡습니다(ADR-005).
- Cargo 작업 공간(크레이트 다섯 개: bt-core·bt-term·bt-render·bt-ctl·bt-app)과 CI(fmt·clippy·테스트)를 더했습니다.
- 터미널 코어(bt-term): PTY로 셸을 띄우고 alacritty_terminal로 화면을 유지합니다. 키·붙여넣기를 바이트로 바꾸고, 괄호 붙여넣기 안의 끝 표시를 지웁니다.
- CPU 렌더러(bt-render): 시스템 글꼴(대체 글꼴로 한글)로 셀 격자를 그립니다. 상자 선·블록·막대 글자는 직접 그려 줄 사이가 끊기지 않습니다.
- 창 앱(b-terminal): winit 창에서 셸을 띄우고 키·마우스·붙여넣기·스크롤백·글자 크기를 다룹니다. 한글 IME 확정 직후의 스페이스·Enter가 사라지지 않게 winit을 고쳐 씁니다(ADR-004).
- 가로 탭 줄(Ghostty처럼): 탭이 둘 이상일 때만 창 위쪽에 탭 이름과 안 읽음 점을 보입니다. 셸이 제목을 보내지 않으면 전경 프로세스 이름을 씁니다.
- 알림: PTY 출력에서 OSC 9·99·777 알림을 뽑아 보고 있지 않은 탭에 안 읽음 점과 알림 한 줄을 보이고, 창을 보고 있지 않으면 데스크톱 알림을 띄웁니다. OSC 9;4(진행 표시)는 알림으로 세지 않습니다. 셸이 OSC 7로 알린 폴더를 새 탭이 물려받습니다.
- Ghostty 설정 호환: Ghostty 문법의 설정 파일(`~/.config/b-terminal/config`)을 읽고, Ghostty 설정이 있으면 먼저 읽습니다. 테마는 Ghostty 테마 폴더에서 이름으로 불러옵니다. 기본값을 Ghostty 1.3과 같게 바꿨습니다(JetBrains Mono 13, 바탕 #282c34, bold-is-bright 끔, Option은 Alt가 아님). JetBrains Mono와 Symbols Nerd Font Mono를 앱에 넣었고, 사설 영역 글자(프롬프트 아이콘)는 기호 글꼴에서만 찾습니다. `--show-config`, `--config-file`.
- 키 묶음: Ghostty의 `keybind = 트리거=동작`(unbind·clear·text: 포함)으로 모든 단축키를 바꿉니다. `reload_config`(Cmd-Shift-,)와 `btctl reload-config`가 설정을 다시 읽어 이미 열린 탭의 색까지 바꿉니다.
- 프로그램이 마우스 모드를 켠 채 끝나 셸로 돌아오면 마우스 모드를 끕니다. 셸에 `35;15;35M…`가 쏟아지는 것을 막습니다(`mouse-reset-on-shell`).
- 제어 소켓과 btctl: 같은 사용자 전용 Unix 소켓(줄 단위 JSON)으로 탭 목록·새 탭·옮기기·글 넣기·화면 읽기·알림을 다룹니다(kitty·WezTerm의 원격 조종과 같은 성격). 탭 안의 셸에 `BT_SOCKET`·`BT_TAB_ID`를 넣습니다.

### 알려진 한계

- 색 이모지가 빈 상자로 보입니다(fontdue가 비트맵 글꼴을 그리지 못함).
- Kitty 그래픽·Sixel, OSC 8 링크, OSC 52 클립보드(보안상 끔), 화면 분할이 없습니다.
- 한글 IME 패치(ADR-004)와 키보드·마우스 동작은 밤사이 사람 없이 만들어 실제로 쳐 보지 못했습니다. 각 PR의 "돌리지 않은 검증"에 적었습니다.
