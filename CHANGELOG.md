# 변경 기록

이 파일은 [Keep a Changelog](https://keepachangelog.com/ko/1.1.0/) 형식을 따르고, 버전은 [유의적 버전](https://semver.org/lang/ko/)을 따릅니다.

## [아직 릴리스하지 않음]

- 닫은 것 되살리기(Ghostty): `undo-timeout`(기본 `5s`, `0s`면 끔) 동안 직접 닫은 칸·탭의 셸을 끊지 않고 들고 있다가 `undo`(⌘Z, ⇧⌘T)로 원래 자리에 되살리고 `redo`(⇧⌘Z)로 다시 닫습니다. 되살릴 것이 없으면 ⌘Z는 전처럼 셸로 갑니다. 창 닫기·셸이 스스로 끝난 칸은 되살리지 않습니다. 실제 창에서는 아직 확인하지 않았습니다. #164
- 글꼴 미세 조정 키(Ghostty): `adjust-underline-position`·`-thickness`, `adjust-strikethrough-position`·`-thickness`, `adjust-overline-position`·`-thickness`(윗줄 SGR 53은 아직 그리지 않아 키만 받음), `adjust-cursor-height`·`-thickness`, `adjust-font-baseline`, `adjust-box-thickness`와 가변 글꼴 축 `font-variation`·`-bold`·`-italic`·`-bold-italic`(예 `wght=500`). 실제 창에서는 아직 확인하지 않았고 픽셀 테스트로만 확인했습니다. #163
- 선택·색 키(Ghostty): `selection-clear-on-typing`(기본 켬)·`selection-clear-on-copy`(기본 끔), 찾기 강조 색 `search-foreground`·`search-background`·`search-selected-foreground`·`search-selected-background`(`cell-foreground`·`cell-background` 포함), `bold-color`(색 또는 `bright`), `faint-opacity`(기본 0.5). 기본 동작이 바뀝니다: 찾기에 맞은 칸이 검정 글자에 노랑(#ffe082) 바탕, 지금 고른 곳이 살구색(#f2a57e) 바탕으로 칠해지고(전에는 팔레트 노랑을 섞은 색/팔레트 노랑), 흐린 글자가 전보다 조금 더 흐립니다(불투명도 0.6에서 0.5). 실제 창에서는 아직 확인하지 않았고 단위·픽셀 테스트로만 확인했습니다. #162
- 명령 끝 알림과 알림 허용 키(Ghostty): `notify-on-command-finish`(`never`·`unfocused`·`always`), `-action`(`bell`·`notify`), `-after`(기간, 기본 5초), `desktop-notifications`(프로그램의 OSC 9·777 알림 허용, 켬), `app-notifications`(`clipboard-copy`·`config-reload`, 켬). `notify-on-command-finish`를 적지 않으면 옛 `notify-command-seconds` 동작 그대로입니다(기본 동작 변화 없음). 실제 창에서는 아직 확인하지 않았고 순수 함수 테스트로만 확인했습니다. #161
- 소리 벨(Ghostty 키): `bell-features`의 `audio`가 `bell-audio-path` 소리 파일을 `bell-audio-volume`(0.0~1.0, 기본 0.5)으로 재생합니다(macOS `NSSound`, 앞 소리가 나는 중이거나 0.1초 안이면 건너뜀). 이전에는 `audio`를 받기만 했습니다. 실제 소리는 아직 들어 보지 않았고 해석·판단만 테스트로 확인했습니다. #165
- 이어받기·탭 키(Ghostty): `window-`·`tab-`·`split-inherit-working-directory`(새 창·탭·분할이 폴더를 이어받을지, 기본 켬이고 이제 `working-directory`가 `home`·경로여도 이어받음), `window-new-tab-position`(`current` 기본은 지금 탭 바로 뒤라 새 탭 자리가 전의 맨 끝에서 달라짐, `end`), `window-show-tab-bar`(`auto` 기본·`always`·`never`), `window-inherit-font-size`(글자 크기가 앱 전체 하나라 값만 받음). 실제 창에서는 아직 확인하지 않았습니다. #160

## [0.10.0] - 2026-11-09

Ghostty 키 채우기 3. 클립보드·오른쪽 클릭, 맨 아래로 스크롤, 시작 창, 키 묶음 접두사, 팔레트 사용자 항목, 제목·색 보고. 하위 이슈마다 에이전트 하나가 따로 구현하고 주 세션이 검토·병합했습니다. 추적 이슈 #145.

- 클립보드·오른쪽 클릭 키(Ghostty): `clipboard-trim-trailing-spaces`(복사하는 글의 줄 끝 공백 지움, 켬), `copy-on-select`가 `clipboard` 값을 구분해 받음(macOS는 `true`와 같음), `right-click-action`(`context-menu` 기본은 하는 일 없음, 이전에는 오른쪽 클릭이 아무 일도 하지 않던 것과 같음). 주의: 복사한 글의 줄 끝 공백이 이제 기본으로 지워집니다. #146
- 맨 아래로 스크롤(`scroll-to-bottom`, Ghostty 키, 기본 `keystroke, no-output`): 키·붙여넣기 때(`keystroke`)와 새 출력이 올 때(`output`) 맨 아래로 돌아갈지 정합니다. 전에는 `output` 없이 글을 보낼 때(응답·초점 알림·btctl send 포함) 항상 돌아갔는데, 이제 사용자가 친 키·붙여넣기 때만 돌아갑니다. 실제 창에서는 아직 확인하지 않았고 PTY 테스트로만 확인했습니다. #147
- 시작 창 키(Ghostty): `maximize`·`fullscreen`으로 새 창을 최대화·전체 화면으로 띄우고, `window-position-x`·`-y`로 첫 창 자리를 정하며(둘 다 있어야 함, 복원한 창은 복원 자리가 이김), `toggle_maximize`·`toggle_window_float_on_top` 동작과 팔레트 항목을 더했습니다. 실제 창에서는 아직 확인하지 않았습니다(#148).
- 키 묶음 접두사 `all:`·`unconsumed:`·`performable:`(겹쳐 쓰기 가능)와 `catch_all` 트리거, `ignore` 동작(Ghostty, #149): `performable:`은 동작을 할 수 없으면(고른 글이 없는데 copy 등) 키를 프로그램에 보내고, `all:`은 글 보내기를 모든 칸에 합니다. 앱 창에서는 확인하지 않았고 keymap 단위 테스트로 확인했습니다.
- 팔레트 사용자 항목(`command-palette-entry`, Ghostty 1.2 문법, #150): `title:…,action:…,description:…`을 여러 줄 쌓아 명령 팔레트에 더하고, 빈 값은 기본 항목을 지웁니다. 설명으로도 찾습니다.
- 제목·색 보고 키(Ghostty): `title-report`(기본 끔, CSI 21 t에 제목으로 답함)와 `osc-color-report-format`(`none`·`8-bit`·`16-bit`, 기본 `16-bit`). 제목 보고는 출력이 정한 제목이 셸 입력으로 돌아와 명령이 되는 길이라 기본으로 끕니다(ADR-012).

### 알려진 한계

- 이번 판의 키·마우스 동작(오른쪽 클릭, 줄 끝 공백 다듬기, 키 묶음 접두사, 팔레트 사용자 항목, 창 최대화·항상 위 토글)은 실제 키·마우스로 해 보지 않았고 테스트로만 확인했습니다. 실제 창에서 본 것은 `window-position`과 `maximize`입니다.
- 맨 아래로 스크롤: 이제 사용자 키·붙여넣기 때만 돌아갑니다(전에는 프로그램 응답·btctl send에도 돌아감).
- 복사한 글의 줄 끝 공백이 기본으로 지워집니다(`clipboard-trim-trailing-spaces`, Ghostty 기본과 같음).
- `copy-on-select`는 지금 나온 Ghostty 판대로 기본 켬입니다(Ghostty main은 macOS 기본을 `none`으로 바꿈).
- `toggle_maximize`가 macOS에서 무엇을 하는지 확인하지 않았습니다(Ghostty 문서는 macOS에서 효과가 없다고 함).

## [0.9.0] - 2026-11-09

그림·커서·여백. 배경 그림, 커서 키, 여백 키, 프롬프트를 눌러 커서 옮기기, 키보드로 고르기 늘리기·링크 복사. 추적 이슈 #133.

- 배경 그림(`background-image`·`-opacity`·`-position`·`-fit`·`-repeat`, Ghostty 키, PNG·JPEG): 기본 바탕색 칸과 여백에 그림을 깝니다. 창 크기가 바뀔 때마다 그림 층을 다시 합성합니다(3200×2100 창에서 약 17ms, 릴리스 빌드에서 잰 값).
- 커서 키(Ghostty): `cursor-opacity`(0~1)로 커서를 칸 위에 섞어 그리고, `cursor-color`·`cursor-text`에 `cell-foreground`·`cell-background`를 주면 커서 칸의 색을 씁니다.
- 여백 키(Ghostty): `window-padding-balance`로 남는 자리를 네 변에 고르게 나누고, `window-padding-color = extend`로 여백을 가장자리 칸의 바탕색으로 칠합니다(vim 상태 줄 같은 색 줄이 창 끝까지 이어짐).
- 프롬프트를 눌러 커서 옮기기(`cursor-click-to-move`, Ghostty 키, 켬): 셸 통합이 있는 프롬프트에서 커서 줄을 누르면 그 자리로 커서가 갑니다.
- 키보드로 고르기 늘리기(`adjust_selection`)와 링크 주소 복사(`copy_url_to_clipboard`), Ghostty 동작.

### 알려진 한계

- 프롬프트 클릭으로 커서 옮기기, 키보드로 고르기 늘리기, 링크 복사는 키·마우스로 실제로 해 보지 않았고 테스트로만 확인했습니다. 실제 창에서 본 것은 배경 그림·커서 불투명도·여백 색입니다.
- 배경 그림은 창 크기가 바뀔 때마다 층을 다시 합성합니다(3200×2100에서 약 17ms). 투명 창과 함께 쓰는 경우는 실제로 보지 않았습니다.
- 프롬프트 클릭은 커서 줄 안에서만 옮깁니다(여러 줄 입력의 다른 줄로는 가지 않음).
- `cursor-click-to-move`·`cursor-opacity`·여백 키의 기본값은 Ghostty 문서에서 확인하지 못했습니다.

## [0.8.0] - 2026-11-09

Ghostty 키 채우기. 크기 표시, 최소 명암비, 칸 제목 바꾸기, 화면을 파일로, 마우스·창의 작은 키. 추적 이슈 #121.

- 크기 표시(`resize-overlay`·`-position`·`-duration`, Ghostty 키, 기본 `after-first`·`center`·`750ms`): 창이나 분할 칸의 크기가 바뀌면 칸 안에 `80 × 24`를 잠깐 보입니다.
- 최소 명암비(`minimum-contrast`, Ghostty 키, 기본 `1`=끔): 바탕과 너무 비슷한 글자색을 흰색이나 검은색으로 그립니다. 켜면 화면 전체 다시 그리기가 약 0.6ms 늘었습니다(200×60 레티나, 한 번 잰 값).
- 칸 제목 바꾸기(`prompt_surface_title`·`set_surface_title`, Ghostty 동작): 메뉴 "셸 › 제목 바꾸기…"나 팔레트로 입력 막대를 열어 탭 제목을 붙입니다. 붙인 제목은 프로그램이 정한 제목보다 먼저 보이고, 비우면 원래대로 돌아갑니다.
- 화면을 파일로(`write_scrollback_file`·`write_screen_file`·`write_selection_file`, Ghostty 동작, `copy`·`paste`·`open`): 긴 출력을 편집기로 열거나 경로를 다른 프로그램에 넘깁니다. 파일은 이 사용자 전용 폴더에 0600으로 쓰고 앱이 끝날 때 지웁니다.
- 마우스·창의 작은 Ghostty 키: `focus-follows-mouse`(마우스가 올라간 칸 고르기), `mouse-scroll-multiplier`(휠 배율, 기본 3·트랙패드 1), `selection-word-chars`(두 번 눌러 고르는 단어의 경계 글자), `toggle_fullscreen` 동작(⌃⌘F), `scroll_page_lines`·`scroll_page_fractional` 동작.

### 알려진 한계

- 이번 판에서 새로 들어온 동작 대부분(칸 제목 입력 막대, 화면을 파일로, 마우스 따라 고르기, 휠 배율, 단어 고르기 경계, 전체 화면)은 키·마우스로 실제로 해 보지 않았고 테스트로만 확인했습니다. 실제 창에서 본 것은 크기 표시뿐입니다.
- 최소 명암비를 켜면 화면 전체 다시 그리기가 약 0.6ms 늘어납니다(한 번 잰 값).
- 크기 표시의 `after-first`는 칸을 만든 직후 0.5초를 시간으로 셉니다(Ghostty는 칸의 첫 크기 바뀜을 셉니다).
- 화면을 파일로 쓴 파일은 앱이 정상으로 끝날 때만 지웁니다.

## [0.7.0] - 2026-11-09

확인과 알림. 실수로 잃는 것을 막는 확인(닫기 확인, 붙여넣기 보호), 놓치지 않게 하는 벨, 키 순서, 퀵 터미널의 자리·움직임. 추적 이슈 #105.

- 키 순서(`keybind = ctrl+a>n=new_window`, Ghostty 문법): 앞 키를 누르면 다음 키를 기다리고, 묶이지 않은 키가 오면 누른 키를 모두 프로그램에 그대로 보냅니다.
- 벨(`bell-features`, Ghostty 키, 기본 `attention,title`): 보지 않는 칸에서 벨이 울리면 탭 제목 앞에 🔔를 붙이고, 창이 앞에 없으면 Dock 아이콘을 튀깁니다. `system`을 켜면 경고음도 냅니다.
- 닫기 확인(`confirm-close-surface`, Ghostty 키, 기본 `true`): 프로그램이 도는 칸을 ⌘W·⌥⌘W·창 닫기 단추·⌘Q로 닫으려 하면 창 위쪽 확인 막대로 묻습니다(↩ 닫기, esc 취소). ⌘Q는 이제 키 묶음의 `quit` 동작입니다.
- 붙여넣기 보호(`clipboard-paste-protection`·`clipboard-paste-bracketed-safe`, Ghostty 키, 기본 `true`): 괄호 붙여넣기를 쓰지 않는 프로그램에 줄바꿈이 든 글을 붙여넣으면 줄 수와 첫 줄을 보이고 먼저 묻습니다(ADR-011).
- 퀵 터미널의 자리·크기·화면·움직임(`quick-terminal-position`·`-size`·`-screen`·`-autohide`·`-animation-duration`, Ghostty 키): 위·아래·왼쪽·오른쪽·가운데에, 앞에 있는 창의 화면이나 마우스가 있는 화면에 띄우고, 가장자리 바깥에서 0.2초 동안 미끄러져 들어오고 나갑니다. 메뉴 막대와 Dock을 뺀 영역에 놓습니다.
- 탭 제목·찾기 막대·명령 팔레트·알림 토스트의 글에 색 이모지가 있으면 앱이 꺼지던 것을 고쳤습니다. 프로그램이 탭 제목(OSC 2)에 😀를 넣는 것만으로 모든 창이 꺼질 수 있었습니다.
- 명령을 주지 않은 `btctl new`가 설정의 `command`·`working-directory`를 무시하고 로그인 셸로 띄우던 것을 고쳤습니다(Cmd-T와 같게).

### 알려진 한계

- 키보드·마우스로 직접 쳐 보는 확인은 여전히 대부분 빠져 있습니다. 이번 판에서는 키 순서, 닫기 확인 막대(⌘W·⌘Q), 붙여넣기 보호(⌘V, 실제 클립보드가 필요함)를 실제 키로 눌러 보지 않았고 단위 테스트로만 확인했습니다.
- 벨의 Dock 튀기기와 경고음(`system`)은 실제로 확인하지 않았습니다.
- 닫기 확인은 Dock 메뉴의 종료나 로그아웃으로 끝낼 때는 묻지 못합니다.
- 퀵 터미널의 `main`·`mouse` 화면 고르기는 외부 모니터가 없어 화면이 둘일 때 해 보지 못했습니다.

## [0.6.0] - 2026-11-06

매일 쓰는 터미널. 창 상태 복원, 퀵 터미널, 글꼴 합자, 창이 있는 화면의 색 공간. 추적 이슈 #93.

- `btctl read --lines N`이 화면 아래쪽 빈 줄까지 세어 글이 위쪽에만 있으면 빈 글을 돌려주던 것을 고쳤습니다(마지막 글 줄부터 셈).
- 창 상태 복원(`window-save-state`, Ghostty 키): Cmd-Q로 끌 때 창·탭·분할·작업 폴더를 적어 두고 다음에 켤 때 되살립니다.
- 퀵 터미널(`toggle_quick_terminal`, Ghostty 동작): 화면 위쪽에 떠 있는 창을 전역 단축키(`keybind = global:…`)나 `btctl quick`으로 보였다 숨깁니다. 포커스를 잃으면 숨습니다.
- 글꼴 합자: JetBrains Mono의 `->` `!=` `===` 같은 합자를 그립니다(Ghostty처럼 기본으로 켬). `font-feature = -calt`로 끄고, `ss01`·`cv01=2` 같은 다른 글꼴 기능도 줍니다. 칸 폭은 그대로입니다.
- `window-colorspace = display-p3`(기본)에서 주 화면이 아니라 창이 있는 화면의 색 공간을 씁니다. 창을 다른 화면으로 옮기면 그 화면의 색 공간으로 바꿉니다(외부 모니터에서 색 맞춤 변환이 다시 생기지 않게, 실제로 옮겨 보지는 못함).

### 알려진 한계

- 키보드·마우스로 직접 쳐 보는 확인은 여전히 대부분 빠져 있습니다(퀵 터미널의 전역 단축키를 실제 키로 눌러 보지 않음, `btctl quick`으로만 확인).
- 퀵 터미널은 주 화면 위쪽에만 뜨고, 미끄러져 내려오는 움직임이 없습니다.
- 합자는 글자 수와 글리프 수가 같은 글꼴(JetBrains Mono·Fira Code식)만 그립니다. 다른 합자 글꼴로는 그려 보지 않았습니다. 화면 전체가 새 글이면 다시 그리기가 약 2.4ms 늘어납니다(한 번 잰 값).
- 창을 다른 화면으로 옮길 때의 색 공간 바꾸기는 외부 모니터가 없어 실제로 해 보지 않았습니다.
- 창 상태 복원은 탭의 작업 폴더까지만 되살리고, 화면 내용(스크롤백)은 되살리지 않습니다.

## [0.5.0] - 2026-11-06

다듬기. 투명 창의 글자 가장자리, 이모지 묶음, 서명·공증 준비. 추적 이슈 #85.

- 서명·공증 준비: `scripts/bundle-macos.sh --sign … --notarize … --dmg`가 Developer ID 서명(하드닝 런타임)·공증·스테이플·DMG까지 합니다. 계정이 없으면 하드닝 런타임으로 임시 서명합니다(`docs/release-macos.md`).
- 이모지 묶음을 한 글리프로 그립니다: 피부색(👍🏽), ZWJ(👨‍👩‍👧 🏳️‍🌈 🧑‍💻), 국기(🇰🇷). 색 글꼴의 다듬기(morx)로 만들고, 묶음이 차지한 칸(VT 엔진 폭) 가운데에 둡니다.
- 투명 창에서 글자 가장자리를 바탕과 알파째 섞습니다. 밝은 배경 위에서 글자 둘레가 어둡게 보이던 것을 고쳤습니다(캔버스가 곱한 알파로 그림).

### 알려진 한계

- 키보드·마우스로 직접 쳐 보는 확인은 여전히 대부분 빠져 있습니다.
- 이모지 묶음은 VT 엔진 폭대로 칸을 차지해 사이가 넓게 보입니다(글자 묶음 폭 mode 2027은 하지 않음).
- 서명·공증은 Developer ID 계정이 있어야 합니다. 스크립트는 준비됐지만 실제 공증은 해 보지 않았습니다.
- 창에 올리는 시간을 더 줄이려면 IOSurface·GPU가 필요합니다(실험 E4).
- `window-colorspace` 기본(`display-p3`)은 색을 맞추지 않습니다(ADR-009).

## [0.4.0] - 2026-11-06

그림과 손에 잡히는 것. 터미널 안의 그림(kitty 그래픽), 명령 팔레트, 투명 창을 더하고, macOS 창 버퍼를 다시 씁니다. 추적 이슈 #75.

- macOS에서 창 버퍼를 프레임마다 새로 잡지 않고 다시 씁니다. 올리기가 조금 짧아졌습니다(다섯 쌍 모두, 중앙값 차이 약 0.2ms, 실험 E4).
- 투명 창(`background-opacity`, Ghostty 키): 기본 바탕색 칸으로 창 뒤가 비칩니다. 글자와 색 칸은 불투명합니다.
- 명령 팔레트(Cmd-Shift-P, `toggle_command_palette`): 동작을 이름이나 Ghostty 동작 이름으로 찾아 부릅니다. 걸린 단축키가 함께 보입니다.
- kitty 그래픽 프로토콜: 터미널 안에 그림을 보입니다(직접 전송, PNG·RGB·RGBA, zlib, 조각, 놓기·지우기·질의). 파일 전송은 받지 않습니다(ADR-010).
- PTY에 알리는 셀 픽셀 크기가 8×16으로 잘못 가던 것을 고쳤습니다(그림 도구가 그림 크기를 잡는 값).

### 알려진 한계

- 키보드·마우스로 직접 쳐 보는 확인은 여전히 대부분 빠져 있습니다(명령 팔레트의 키 입력 포함).
- kitty 그래픽: 파일·공유 메모리 전송, 애니메이션, 유니코드 자리표시는 받지 않습니다. 스크롤백이 상한에 닿으면 그림 자리가 어긋날 수 있습니다. 실제 도구(icat·yazi)로는 해 보지 않았습니다.
- 투명 창에서 글자 가장자리가 밝은 배경 위에서 조금 어둡게 보일 수 있습니다.
- `window-colorspace` 기본(`display-p3`)은 색을 맞추지 않습니다(ADR-009).
- ZWJ 묶음·피부색 이모지는 합치지 않습니다. 앱은 임시 서명입니다.

## [0.3.0] - 2026-11-06

가볍고 정확하게. 글꼴을 필요한 만큼만 읽어 메모리를 줄이고(unsafe 없이), macOS에서 창에 올리는 시간을 줄이고, 에이전트용 btctl을 늘렸습니다. 추적 이슈 #64.

- macOS에서 창에 올리는 시간이 약 15ms에서 2.7ms로 줄었습니다. 그림을 화면 자신의 색 공간으로 넘겨 프레임마다의 색 맞춤 변환을 없앴습니다. 정확한 sRGB 색이 필요하면 `window-colorspace = srgb`(ADR-009, 실험 E3).
- U+FE0F가 붙은 이모지(❤️ ⚠️ ☀️ 🏳️)를 그 칸 안에 색으로 그립니다. 칸 폭은 VT 엔진대로라 한 칸짜리는 작게 보입니다.
- `btctl read --lines N`: 맨 아래에서 위로 N줄(스크롤백 포함)을 읽습니다. `btctl wait -- 글`: 탭에 그 글이 나올 때까지 기다립니다(기한이 지나면 종료 코드 1).
- 글자 래스터를 swash로 바꾸고(fontdue 뺌) 글꼴 파일에서 쓰는 꼴만 읽습니다. 처음 메모리가 약 380MiB에서 160MiB로 줄었습니다(ADR-008).
- 색 이모지 글꼴은 파일에서 그 꼴과 셀 크기에 맞는 비트맵 한 벌만 읽습니다(Apple Color Emoji 180MiB 중 약 7~10MiB). 이모지를 그린 뒤 메모리가 약 160MiB 줄었습니다.

### 알려진 한계

- 키보드·마우스로 직접 쳐 보는 확인은 여전히 대부분 빠져 있습니다(0.2.0과 같음).
- `window-colorspace` 기본(`display-p3`)은 색을 맞추지 않아 P3 화면에서 테마 색이 조금 진하게 보입니다. 정확한 색은 `srgb`(큰 창에서 프레임마다 약 10ms 더).
- ZWJ 묶음(👨‍👩‍👧)과 피부색은 아직 합치지 않습니다. U+FE0F 이모지는 VT 엔진 폭대로 한 칸에 작게 보입니다.
- softbuffer가 프레임마다 새 버퍼를 잡는 약 2ms가 남았습니다(실험 E3).
- 앱은 임시 서명입니다(서명·공증 전).

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
