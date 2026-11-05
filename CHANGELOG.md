# 변경 기록

이 파일은 [Keep a Changelog](https://keepachangelog.com/ko/1.1.0/) 형식을 따르고, 버전은 [유의적 버전](https://semver.org/lang/ko/)을 따릅니다.

## [아직 릴리스하지 않음]

- 진행 표시(OSC 9;4): 탭 줄의 그 탭 아래(탭이 하나면 창 맨 위)에 막대를 그립니다. 보통·오류·끝 모름·멈춤을 색으로 가르고, 15초 소식이 없으면 지웁니다.
- 클립보드 쓰기(OSC 52)를 기본으로 허용합니다(AI CLI의 `/copy`). 보고 있는 창의 보고 있는 탭에서 온 것만 받고, 받으면 창 안 알림 띠로 보입니다. 읽기는 기본 금지. `clipboard-write`·`clipboard-read`.

- kitty 키보드 프로토콜(1단계 모호한 키 구분, 8단계 모든 키를 이스케이프로): Claude Code에서 Shift+Enter가 줄바꿈이 됩니다. 프로그램의 `CSI ? u` 질의에 답합니다.

- Ghostty 없이 혼자 섭니다: 테마 905개(iTerm2-Color-Schemes)를 앱에 넣었고, 테마 이름은 띄어쓰기·대소문자를 무시합니다. Ghostty 설정 읽기는 기본으로 끕니다(`import-ghostty = true`로 켬). `--list-themes`(ADR-006).

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
