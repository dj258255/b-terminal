# 변경 기록

이 파일은 [Keep a Changelog](https://keepachangelog.com/ko/1.1.0/) 형식을 따르고, 버전은 [유의적 버전](https://semver.org/lang/ko/)을 따릅니다.

## [아직 릴리스하지 않음]

- 비례폭 `window-title-font-family`에서 색 이모지가 제목 글꼴 두 칸 폭 대신 그려지는 글리프 폭만큼 나아가, `한글제목 😀`처럼 이모지 앞뒤 여백이 넓지 않습니다. 고정폭 제목 글꼴과 터미널 칸은 그대로 두 칸입니다. Ghostty macOS는 제목 글꼴 크기를 터미널 글꼴 크기와 무관하게 `NSFont.systemFontSize`로 정하는데(요약으로 확인), b-terminal은 터미널과 같은 크기를 그대로 둡니다(차이로 문서에 적음). 폭 판단은 테스트(Helvetica가 있는 시스템에서만 도는 테스트)로 확인했고 실제 창에서는 아직 보지 않았습니다. #252
- `+show-config`(과 `--show-config`)가 `theme = 값` 줄을 찍고(테마가 정한 색도 바뀐 키로 함께 나옴, Ghostty와 같음), `--changes-only`가 줄 비교에서 키 단위 비교로 바뀝니다. 키의 줄 묶음이 기본과 다르면 그 키의 줄을 모두 보이고, 기본에 있다가 지워진 키는 `키 = `로 드러납니다. **기본 동작이 바뀝니다**: `palette`처럼 여러 줄 키는 하나만 바꿔도 키 전체가 나옵니다. `keybind`는 Ghostty와 달리 기본 묶음을 풀지 않고 쓴 줄만 보입니다. 판단은 테스트로, 동작은 `--config-file`로 돌려 확인했고 창은 띄우지 않았습니다. #251

## [0.17.0] - 2026-11-10

Ghostty 받기만 하던 키와 시퀀스 채우기. 0.16까지 Ghostty 최상위 설정 키를 모두 받게 된 뒤(Config.zig 전체를 넣어 모르는 키 0개), 받기만 하던 키와 빠진 시퀀스를 채웠습니다: `background-blur`, OSC 22, `+show-config`·`+list-themes` 인자, `window-title-font-family`. 하위 이슈마다 에이전트 하나가 구현하고, 주 세션이 Ghostty 원본과 대조·검토·실제 확인·병합했습니다. 추적 이슈 #237.

- `background-blur`(옛 이름 `background-blur-radius`)를 받아 macOS에서 투명 창(`background-opacity` < 1) 뒤를 흐립니다. `false`·`true`(20)·0~255 정수(세기가 그대로 반지름)이고, 잘못된 값은 경고합니다. 창을 만들 때(퀵 터미널 포함)와 설정을 다시 읽을 때(끄기 포함) 적용하고, `background-opacity`가 1 이상이면 걸지 않습니다. macOS 26 유리 값은 받기만 하고 `true`로 봅니다. 기본(`false`)은 그대로입니다. 판단은 테스트로 확인했고, 실제 창에서는 흐림 0과 40을 견주고 `false`로 다시 읽어 꺼지는 것과 틀린 값의 경고를 봤습니다(퀵 터미널과 유리 값은 보지 않음). #238
- 프로그램이 `OSC 22 ; 이름`으로 마우스 포인터 모양을 정할 수 있습니다(Ghostty와 같은 이름 표: W3C 34가지와 xterm 별칭). 모양은 칸마다 들고 마우스가 그 칸 위에 있을 때 적용하며, 링크 위 손가락이 앞서고, 빈 이름이나 초기화(RIS)는 화살표로 되돌립니다. 모르는 이름·긴 이름·제어 문자가 든 이름은 무시합니다. 정한 모양이 없으면 지금까지와 같습니다. Ghostty의 기본 I자 모양, 마우스 보고 모드에 따른 되돌림, Shift·Alt 누름에 따른 바뀜은 아직 없습니다. 판단은 테스트로 확인했고, 실제 창에서는 시퀀스를 보내도 앱이 멀쩡한 것만 봤습니다(마우스를 움직여 포인터가 바뀌는 것은 보지 않음). #239
- 명령줄 `+show-config`가 `--changes-only`(Ghostty처럼 기본 true)와 `--docs`를, `+list-themes`가 `--path`와 `--color=dark|light|all`을 받습니다. **기본 동작이 바뀝니다**: `+show-config`는 이제 기본과 다른 키만 보이고, 전부 보려면 `--changes-only=false`를 줍니다. `--docs`의 설명은 `+explain-config`와 같은 문서 표에서 옵니다. `--path`는 사용자 테마만 경로가 있고 넣어 둔 테마는 경로 없이 `(resources)`만 보입니다. 판단은 테스트로, 동작은 임시 HOME에서 직접 돌려 확인했고 창은 띄우지 않았습니다. #240
- `window-title-font-family = Helvetica`로 탭 줄 제목 글꼴을 고릅니다(지금까지는 받기만 했음). 고정폭이 아니어도 글자마다 폭으로 자리를 잡아 겹치지 않고, 탭 안 잘림(`…`)도 실제 폭으로 셉니다. 제목 글꼴에 없는 글자는 터미널 기본 글꼴 다음 터미널과 같은 대체 순서이고, 없는 이름이면 경고하고 터미널 글꼴로 그립니다. 키가 없으면 지금처럼 터미널 글꼴입니다(Ghostty는 시스템 글꼴이 기본이라 다릅니다). macOS 창 제목 막대는 그대로입니다. 판단은 테스트로 확인했고, 실제 창에서는 Helvetica로 비례폭 제목·한글·잘림과 없는 글꼴의 경고를 봤습니다. 색 이모지는 앞뒤 여백이 조금 넓습니다. #241
- 모드 2031 테스트가 병렬 실행에서 둘째 답을 놓쳐 가끔 실패하던 것을 고쳤습니다(테스트만 바뀜). #244

### 알려진 한계

- 기본 동작이 바뀐 것: `+show-config`가 기본과 다른 키만 보입니다(`--changes-only=false`로 전부).
- `window-title-font-family`를 비우면 터미널 글꼴입니다(Ghostty는 시스템 글꼴). 바꿀지는 정하지 않았습니다.
- OSC 22: Ghostty의 기본 I자 모양, 마우스 보고 모드·Shift·Alt에 따른 모양은 없습니다.
- `background-blur`: macOS 26 유리 효과는 흐림으로 대신합니다. 투명도를 1 이상으로 다시 읽으면 걸린 흐림이 남습니다(보이지 않음).
- `+show-config`는 `theme` 줄을 찍지 않고 테마가 정한 색으로 풀어 찍습니다(0.16 전부터). 기본 묶음에서 지운 키는 바꾼 키로 드러나지 않습니다.
- 실제로 보지 않은 것: 포인터 모양이 바뀌는 모습, 퀵 터미널의 흐림, Linux.

## [0.16.0] - 2026-11-10

Ghostty 명령줄 동작과 색 구성표 보고. Ghostty의 `ghostty +동작` 열두 개와 색 구성표 보고(`CSI ? 996 n`, 모드 2031)를 더했습니다. 하위 이슈마다 에이전트 하나가 구현하고, 주 세션이 Ghostty 소스와 대조·검토·병합했습니다. 두 에이전트가 따로 만든 `+동작` 길은 주 세션이 하나로 모았습니다. 추적 이슈 #229.

- 명령줄 동작 `b-terminal +동작`을 받습니다(Ghostty의 `ghostty +동작`): `+version`, `+help`, `+show-config`, `+list-themes`, `+validate-config`(경고가 있으면 종료 코드 1), `+edit-config`(`$VISUAL`/`$EDITOR`를 셸 없이 인자 배열로 열고, 설정 파일이 없으면 만듭니다). 모르는 `+동작`은 오류(1)입니다. 기존 `--` 인자는 그대로입니다. Ghostty와 다른 점: 편집기 값을 셸이 아니라 빈칸·따옴표로 갈라 실행하고, `+show-config --changes-only/--docs`·`+list-themes --path/--color`는 아직 없습니다. 판단은 테스트로 확인했고, 실제 명령은 `+edit-config`를 `cat`을 편집기로 삼아, 나머지는 직접 돌려 봤습니다(실제 편집기는 띄우지 않음). #230
- 명령줄 `+list-fonts`·`+list-keybinds`·`+list-actions`·`+list-colors`·`+show-face`·`+explain-config`를 더했습니다(Ghostty와 같은 이름·인자). 창을 띄우지 않고 글만 찍고 끝납니다. 판단은 테스트로, 동작은 임시 HOME에서 직접 돌려 확인했습니다. `+list-keybinds`는 설정을 반영한 표, `+explain-config`는 설정 문서 표에서 설명을 뽑습니다. `+list-colors`의 X11 색 676개는 Ghostty 원본 `rgb.txt`와 한 줄씩 대조했습니다. #231
- 색 구성표 보고를 받습니다(Ghostty와 같음): 프로그램이 `CSI ? 996 n`으로 물으면 `CSI ? 997 ; 1 n`(어두움)·`; 2 n`(밝음)으로 답하고, DEC 모드 2031을 켜 두면 시스템 밝음·어두움이 바뀔 때 같은 보고를 보냅니다. 기준은 테마 바탕색이 아니라 시스템 모양이고(`window-theme = light|dark`로 고정한 창은 그 값), 테마가 하나뿐이어도 알리며, 켠 직후에는 보내지 않고 같은 값은 다시 보내지 않으며 초기화(RIS)는 모드를 끕니다. 읽기 전용 칸에서도 답합니다. 판단과 PTY 답은 테스트로 확인했고 실제 창에서 시스템 모양을 바꿔 보지는 않았습니다. #232

### 알려진 한계

- 기본 동작이 바뀐 것:
  - `+`로 시작하는 인자는 이제 동작으로 읽힙니다(전에는 "모르는 인자" 오류). `-e` 뒤는 그대로입니다.
  - 프로그램이 `CSI ? 996 n`으로 물으면 이제 답합니다.
- `+edit-config`는 편집기 값을 셸로 풀지 않습니다(`;`, `$(…)`, `~`, 환경 변수가 풀리지 않음). Ghostty는 `/bin/sh -c`로 엽니다.
- 받기만 하는 인자: `+list-keybinds`·`+list-actions`의 `--docs`, `+list-themes`의 `--plain`. 아직 없는 인자: `+show-config --changes-only/--docs`, `+list-themes --path/--color`.
- `+list-keybinds`·`+list-fonts`·`+show-face`·`+explain-config`는 `--config-file`을 받지 않고 기본 설정 파일만 읽습니다.
- `+동작`은 로그를 남기지 않습니다(출력에 섞이지 않게 로거보다 먼저 돎).
- `set_window_theme`으로 창 테마를 실행 중에 바꾸면 열린 칸에 모드 2031 알림을 보내지 않습니다.
- 실제로 보지 않은 것: 시스템 모양을 바꿔 모드 2031 알림을 받는 것, nvim 같은 실제 프로그램의 색 구성표 질문, 실제 편집기(`code -w` 등)로 `+edit-config`, `+show-face`가 고른 글꼴과 창에 그려지는 글꼴의 비교, Linux.

## [0.15.0] - 2026-11-10

Ghostty 동작 맞추기. 0.13·0.14에서 받기만 하거나 알려진 한계로 남긴 것 가운데 Ghostty에 있는 동작을 채웠습니다: OSC 52 클립보드 `ask`, 시스템 밝음·어두움 따라가기, 스크롤 막대 끌기, 창 상태의 제목과 탭 옮기기 뒤 정리. 하위 이슈마다 에이전트 하나가 구현하고, 주 세션이 Ghostty 소스와 대조·검토·병합했습니다. 추적 이슈 #219.

- `clipboard-read = ask`·`clipboard-write = ask`를 받습니다(전에는 read의 ask를 deny로 보고 write는 몰랐음). 프로그램이 OSC 52로 읽거나 쓰려 하면 창 안 확인 막대가 읽어 갈 글·쓸 글 앞부분을 보이며 묻습니다. 출력이 띄운 막대라 Enter로는 허용되지 않고 `y`만(뜬 지 0.6초 뒤부터), `n`·Esc는 거절하며, 막대는 하나만 띄웁니다. 기본값(읽기 deny, 쓰기 allow)은 그대로입니다. 판단은 테스트로 확인했고, 실제 창에서는 쓰기 막대가 뜨는 것과 막대가 떠 있을 때 온 두 번째 요청이 거절되는 것을 봤습니다(키를 눌러 답하거나 읽기 막대를 띄우지는 않음). 입력·클립보드 보안 경계라 ADR-019. #220
- 시스템 밝음·어두움이 바뀌면 `theme = light:…,dark:…`를 열린 모든 창·칸에서 다시 고르고 창 테마도 맞춥니다(Ghostty `changeConditionalState`처럼 설정 파일은 다시 읽지 않음). 테마가 하나뿐이거나 `window-theme = light|dark`(또는 `set_window_theme`)로 고정한 창에서는 하지 않고, 창마다 같은 모양이 두 번 와도 한 번만 처리합니다. 판단은 테스트로 확인했고, 실제 창에서는 밝은 시스템에서 밝은 테마로 뜨는 것만 봤습니다(시스템 모양을 실제로 바꿔 보지 않음). #221
- 스크롤 막대(`scrollbar = system`)를 끌고 누를 수 있습니다: 오른쪽 끝 띠에서 손잡이를 끌면 끄는 만큼 스크롤하고(손잡이를 쥔 자리의 차이를 유지), 손잡이 밖 빈 곳을 누르면 그쪽으로 한 쪽(보이는 줄 수에서 한 줄을 뺀 만큼)씩 옮깁니다. 누르고 있는 동안은 마우스를 프로그램에 보고하지 않고 글을 고르지 않으며 막대를 계속 보이고, 놓으면 1.2초 뒤 사라집니다. 스크롤백이 없거나 대체 화면이거나 `scrollbar = never`면 막대가 없으니 마우스는 평소와 같습니다(마우스 보고 중인 프로그램이라도 막대가 보이는 칸의 오른쪽 끝 띠를 누르면 프로그램이 아니라 막대가 받습니다). 빈 곳을 꾹 눌러도 반복하지 않고 한 번만 옮깁니다. 시스템 설정 "스크롤 막대 클릭 시"는 읽지 않습니다. 실제 창에서는 스크롤백을 채운 칸이 경고 없이 뜨는 것만 봤고 막대를 끌어 보지는 않았습니다 #222
- 창 상태 복원(`window-save-state`)이 사람이 붙인 탭·창·칸 제목도 적고 되살립니다(프로그램이 정한 제목은 적지 않음, 제목 없는 옛 상태 파일도 읽음). `move_tab_to_new_window` 뒤 옮긴 칸의 초점 보고(DEC 1004)가 중복·누락 없이 한 번만 가고, 옮긴 칸을 가리키던 확인 막대는 남는 창에서 닫힙니다. 판단은 테스트로 확인했고, 실제 창에서는 제목을 넣은 상태 파일로 띄워 탭·창·칸 제목이 되살아나는 것(제어 문자는 빈칸으로)을 봤습니다. 키로 제목을 붙인 뒤 끄고 저장하는 것과 초점 보고는 보지 않았습니다. #223

### 알려진 한계

- 기본 동작이 바뀐 것:
  - 막대가 보이는 칸에서 오른쪽 끝 띠를 누르면 스크롤 막대가 받습니다(전에는 글 고르기·프로그램 마우스 보고). `scrollbar = never`로 되돌립니다.
  - `theme = light:…,dark:…`면 시스템 모양을 바꿀 때 색이 바로 바뀝니다.
  - 창 상태에 사람이 붙인 제목이 함께 저장됩니다.
- `clipboard-read` 기본은 `deny` 그대로입니다(Ghostty는 `ask`). 바꿀지는 정하지 않았습니다.
- 클립보드 막대: 0.6초가 지난 뒤 우연히 친 `y`는 허용이 됩니다. 쓰기 막대는 앞 40글자만 보입니다. "세션 동안 기억"은 없습니다.
- 모드 2031(색 구성표 변경 보고)은 없습니다.
- 실제 창에서 보지 않은 것: 스크롤 막대 끌기·누르기, 시스템 모양 전환, 클립보드 막대에서 키로 답하기와 읽기 막대, 탭을 옮긴 뒤 초점 보고.

## [0.14.0] - 2026-11-10

Ghostty 키 묶음 동작 채우기. 0.13까지 Ghostty 동작 89개 중 37개를 몰랐던 것(`keybind`에 쓰면 경고)을 여섯 묶음으로 채웠습니다: 창·탭, 제목, 화면·찾기, 토글(읽기 전용 포함), 키 테이블, 받기만 하는 동작과 `--config-default-files`. 이제 Ghostty 동작 89개를 모두 받습니다(`cursor_key`는 Ghostty처럼 설정에서 막음, 89개 이름을 해석기에 넣어 잼). 하위 이슈마다 에이전트 하나가 구현하고, 주 세션이 Ghostty 소스와 대조·검토·병합했습니다. 추적 이슈 #205.

- 창·탭 동작(Ghostty): `close_window`·`close_all_windows`(닫기 확인을 거침), `goto_window:previous|next`, `move_tab:N`(끝을 넘으면 돌아감), `move_tab_to_new_window`(탭이 하나면 아무것도 안 함), `reset_window_size`, `toggle_window_decorations`, `toggle_visibility`(macOS)와 팔레트 항목을 더했습니다. 기본 동작은 그대로입니다. 판단 부분(돌아가기·창 고르기·크기·숨김 계획)과 해석은 테스트로 확인했고, 실제 창에서는 여덟 동작을 키에 묶어 경고 없이 뜨는 것만 봤습니다(눌러 보지 않음). `close_all_windows`는 Ghostty 문서가 효과 없다고 한 동작이지만 b-terminal은 닫습니다. #206
- 제목 동작(Ghostty): `prompt_tab_title`·`prompt_window_title`(입력 막대로 묻기), `set_tab_title:글`·`set_window_title:글`(빈 글이면 덮어쓴 제목을 지움), `copy_title_to_clipboard`(지금 칸의 터미널 제목을 복사, 제목이 비면 하지 않음). 제목은 제어 문자를 지우고 200자로 자릅니다. 순서는 탭 줄·탭: 탭 제목 → 칸 제목(사람이 붙인 것 → 프로그램이 정한 것), 창 제목: 설정 `title` → 창 제목 → 지금 탭 제목. 메뉴 "셸"에 탭·창 제목 항목과 팔레트 항목을 더했습니다. 판단은 테스트로 확인했고, 실제 창에서는 덮어쓴 제목이 없을 때 기존 제목 경로가 그대로인 것만 봤습니다. #207
- 화면·찾기 동작 6개(Ghostty 키 묶음): `reset`(VT의 RIS), `scroll_to_row:N`(스크롤백 맨 위가 0), `scroll_to_selection`, `search:글`(빈 글이면 찾기만 취소, 막대는 둠), `search_selection`, `set_font_size:N`(6~72pt). `cursor_key`는 Ghostty처럼 설정에 쓸 수 없다고 경고합니다. 기본 묶음은 그대로입니다. 판단·해석은 테스트로 확인했고, 실제 창에서는 키에 묶어 `cursor_key` 경고 하나만 나는 것을 봤습니다(눌러 보지 않음). #208
- 키 묶음 동작 `toggle_background_opacity`(창마다 바탕 투명도를 설정값과 불투명 사이에서, `background-opacity`가 1 이상이면 하는 일 없음), `toggle_mouse_reporting`(칸의 마우스 보고 켬·끔), `toggle_readonly`(칸을 읽기 전용으로: 키·IME·붙여넣기·마우스·초점·`text:`·`btctl send`/`run`이 PTY에 가지 않고, 고르기·복사·스크롤은 그대로, 닫을 때는 늘 묻고, 탭 제목 앞에 🔒), `set_window_theme:config|system|light|dark`(이번 실행 동안의 창 테마, 설정을 다시 읽어도 남음), `open_config`(설정 파일을 기본 편집기로)를 받습니다. 기본 동작은 그대로이고(키를 묶어야 동작합니다), 읽기 전용은 입력 경로를 바꾸는 보안 경계입니다(ADR-018). 쓰는 도중 읽기 전용을 켜면 칸을 띄울 때의 시작 입력(`input`)도 남은 부분을 버립니다. 판단은 테스트로 확인했고, 실제 창에서는 다섯 동작을 키에 묶어 경고 없이 뜨는 것만 봤습니다(읽기 전용은 키로만 켤 수 있어 실창에서 켜 보지 않음). #209
- 키 테이블(Ghostty 1.3): `keybind = 이름/트리거=동작`, `이름/`(비우기), `activate_key_table:이름`, `activate_key_table_once:이름`, `deactivate_key_table`, `deactivate_all_key_tables`, `end_key_sequence`. 키를 찾는 순서는 맨 위 테이블부터 기본 묶음까지이고 키 이어 누르기와 함께 동작합니다. 새 동작이라 기본 동작은 그대로입니다. 한 번에 켤 수 있는 테이블은 Ghostty처럼 8개까지입니다. 판단은 테스트로 확인했고, 실제 창에서는 테이블 줄을 넣어 경고 없이 뜨는 것만 봤습니다(눌러 보지 않음). 켜진 테이블을 화면에 보이지는 않습니다. #210
- 키 묶음 동작 `check_for_updates`, `inspector:toggle|show|hide`, `show_gtk_inspector`, `show_on_screen_keyboard`, `toggle_tab_overview`, `toggle_secure_input`, `crash:main|io|render`를 `keybind`에 써도 경고가 나지 않습니다(받기만 하고 키를 먹으며 하는 일은 없음. 이유는 docs/configuration.md). 명령줄 `--config-default-files=false`를 더해 기본 설정 파일(b-terminal·Ghostty)을 읽지 않고 `--config-file`만 읽습니다. 기본 동작은 그대로이고, 판단·해석은 테스트로 확인했고, 실제 실행에서는 `--show-config`의 읽은 파일 목록과 받기만 동작 세 줄이 경고 없이 받아지는 것을 봤습니다(눌러 보지 않음). #211

### 알려진 한계

- 기본 동작은 바뀌지 않았습니다. 새 동작은 모두 `keybind`에 묶어야 동작합니다.
- Ghostty와 다른 것:
  - `close_all_windows`는 Ghostty 문서상 효과가 없지만 b-terminal은 모든 창을 닫습니다(닫기 확인을 거침).
  - `cursor_key`는 Ghostty처럼 설정에 쓸 수 없습니다(경고하고 묶지 않음).
  - 읽기 전용 칸은 `btctl send`·`run`도 받지 않습니다. `btctl`로 읽기 전용을 켜고 끄지는 못합니다(ADR-018).
- 켜진 키 테이블을 화면에 보이지 않습니다.
- 받기만 하는 동작(`check_for_updates`, `inspector`, `toggle_secure_input` 등)은 키를 먹을 뿐 하는 일이 없습니다.
- 실제 창에서 보지 않은 것(테스트로만 확인): 모든 새 동작을 실제 키로 누르기(키 입력 흉내는 다른 앱에 들어갈 수 있어서). 🔒 표시, 읽기 전용에서 입력이 막히는 모습, 창 옮기기·숨기기, 투명도·창 테마 전환, 제목 입력 막대.

## [0.13.0] - 2026-11-10

Ghostty 키 채우기 6. 환경·시작 입력, 링크, 창 모양, 글꼴 합성, 셸 통합 기능, 스크롤바·진행 표시, 수정 키 바꾸기와 플랫폼 전용 키. 0.12처럼 하위 이슈마다 에이전트 하나가 구현하고, 주 세션이 Ghostty 소스와 대조·검토·병합했습니다. 추적 이슈 #189.

- 환경·시작 입력 키(Ghostty): `term`(기본은 **`xterm-256color` 그대로**, Ghostty의 `xterm-ghostty`는 terminfo가 없어 따르지 않음), `env = KEY=VALUE`(여러 번, `KEY=`는 지움, 빈 값은 전체 초기화, b-terminal이 넣는 변수와 겹치면 `env`가 이김), `input`(`raw:`·`path:`, 새 칸을 띄운 직후 PTY에 보냄, 합쳐서 10MiB 상한, 못 읽거나 넘으면 경고하고 보내지 않음). 기본 동작은 그대로입니다. 실제 창에서는 `env`와 `input = raw:`가 셸에 닿는 것을 봤고, `path:` 입력은 테스트로만 확인했습니다(ADR-016). #190
- 링크 키 `link-url`·`link-osc8`·`link-previews`를 Ghostty와 같은 뜻·기본(모두 켬)으로 받습니다. `link-previews = false`는 주소 띠를 꺼서 열기 전에 실제 주소를 보는 보장을 약하게 합니다(`osc8`은 화면 글의 URL에만 띠를 뺍니다). `link`(정규식)는 Ghostty도 아직 쓸 수 없어 받고 경고만 합니다. 기본 동작은 그대로이고, 판단·해석은 테스트로 확인했으나 실제 창에서는 보지 않았습니다. #191
- 창 모양 키(Ghostty): `window-decoration`(`none`이면 제목 막대·테두리 없음), `macos-titlebar-style`(`native` `transparent` `tabs` `hidden`), `macos-window-shadow`, `window-theme`. `macos-titlebar-proxy-icon`은 받기만 합니다(safe API로 `NSWindow`를 얻을 수 없음). **기본 동작이 달라집니다: macOS 기본이 Ghostty 릴리스 판처럼 `transparent`라서 제목 막대가 투명해지고 탭 줄·격자가 그 밑에서 시작합니다**(전과 같게 하려면 `macos-titlebar-style = native`). 제목 막대 높이는 28pt로 가정한 값입니다(실제 창에서 단추와 격자 사이를 눈으로만 봄). `hidden`은 단추와 함께 창 제목 글도 숨깁니다. `tabs`는 `transparent`와 같게 동작합니다. 시스템 모양이 바뀔 때 `theme = light:…,dark:…`를 다시 고르는 일은 아직 없습니다(설정을 다시 읽을 때만 고름) #192
- 글꼴 키 `font-feature`(따옴표·`on`/`off`·공백 값 문법 추가), `font-thicken`·`font-thicken-strength`(글자 두껍게, 윤곽을 부풀리는 근사), `font-synthetic-style`(없는 굵게·기울임 꼴 합성)을 Ghostty와 같은 키로 받습니다. **기본 동작이 바뀝니다: 굵은·기울임 꼴이 없는 글꼴은 이제 굵게는 윤곽을 부풀리고 기울임은 10도 비틀어 그립니다**(전에는 보통 꼴 그대로). 넣어 둔 JetBrains Mono와 굵은 꼴이 있는 글꼴은 그대로입니다. 전처럼 보려면 `font-synthetic-style = false`. 합성 정도와 두껍게 그리기 강도는 JetBrains Mono 한 글자로 재어 맞춘 값이고 macOS의 두껍게 그리기와 견주지 않았습니다. 실제 창에서는 Monaco로 합성 켬·끔을 봤고, `font-thicken`은 보지 않았습니다. #193
- `shell-integration-features`(Ghostty 키)로 셸 통합 기능을 켜고 끕니다: `cursor`, `title`, `path`(기본 켬), `sudo`, `ssh-env`, `ssh-terminfo`(기본 끔), `true`/`false`/`no-이름` 문법. **기본 동작이 바뀝니다**: zsh·bash 탭의 프롬프트에서 커서가 깜박이는 막대가 되고(명령이 도는 동안은 원래 모양), 창 제목을 셸이 폴더·명령으로 정하며, 실행 파일 폴더(`btctl`이 있는 곳)가 PATH 끝에 더해집니다. 끄려면 `shell-integration-features = no-cursor,no-title,no-path`. `sudo`·`ssh-env`·`ssh-terminfo`는 b-terminal이 TERM을 `xterm-256color`로 쓰므로 받기만 하고 하는 일이 없습니다. 원격 호스트에는 아무것도 설치하지 않습니다. 셸 스크립트는 `zsh -f`·`bash --norc` 테스트로 확인했고, 실제 창에서는 zsh 프롬프트의 막대 커서·폴더 제목·PATH를 봤습니다(bash와 명령 중 바뀜은 보지 않음). #194
- Ghostty 키 `scrollbar`(스크롤 중·뒤 잠깐·오른쪽 끝에 마우스를 올렸을 때 반투명 막대, `never`로 끔), `progress-style`(`false`면 OSC 9;4 진행 표시를 버림), `split-preserve-zoom = navigation`(크게 보는 중 `goto_split`이 크게 보기를 풀지 않음)을 받습니다. `scrollbar` 기본이 `system`이라 스크롤하면 막대가 새로 보입니다(나머지 둘은 기본 동작 그대로). 실제 창에서는 진행 표시 켬·끔만 봤고 스크롤 막대는 보지 않았습니다. 막대 끌기와 시스템의 "스크롤 막대 항상 보기" 설정은 지원하지 않습니다 #195
- `key-remap = ctrl=super`처럼 앱 안에서만 수정 키를 바꿉니다(좌우 구분 `left_ctrl` 가능, 키 묶음 판단과 PTY 인코딩 모두 바뀐 키 사용). `quick-terminal-space-behavior = move|remain`(기본 `move`: 퀵 터미널이 열린 Space를 따라옴 - 지금까지는 macOS 기본 동작이라 보던 Space에서 열면 원래 Space로 끌려갈 수 있었습니다). `macos-icon = custom`과 `macos-custom-icon`은 Dock 아이콘만 바꿉니다(`setApplicationIconImage`가 unsafe라 Cmd-Tab 아이콘은 그대로). `macos-dock-drop-behavior`, `vt-xt-checksum-*`(DECRQCRA는 구현하지 않음, 기본 끔 그대로), `macos-icon-frame`·`-ghost-color`·`-screen-color`와 GTK·리눅스·GPU 전용 키(`class`, `gtk-*`, `linux-cgroup*`, `custom-shader*`, `alpha-blending`, `freetype-load-flags`, `window-vsync`, `async-backend`, `window-title-font-family`, `quick-terminal-keyboard-interactivity`, `scrollback-compression`, `macos-applescript`, `macos-shortcuts`, `macos-auto-secure-input`, `macos-secure-input-indication`, `drag-handle`, `language` 등)는 경고 없이 받기만 합니다. 실제 창에서는 새 키들이 경고 없이 받아지는 것만 봤고, 키 바꾸기·Dock 아이콘·Space 동작은 보지 않았습니다 #196

### 알려진 한계

- 기본 동작이 바뀐 것:
  - macOS 제목 막대가 `transparent`입니다(`macos-titlebar-style = native`로 되돌림).
  - 스크롤하면 스크롤 막대가 잠깐 보입니다(`scrollbar = never`로 끔).
  - zsh·bash 탭에서 프롬프트 커서가 막대가 되고, 셸이 창 제목을 정하고, PATH 끝에 실행 파일 폴더가 붙습니다(`shell-integration-features = no-cursor,no-title,no-path`).
  - 굵은·기울임 꼴이 없는 글꼴은 합성해서 그립니다(`font-synthetic-style = false`).
  - 퀵 터미널이 지금 Space로 따라옵니다(`quick-terminal-space-behavior = remain`).
- `term` 기본은 `xterm-256color` 그대로입니다. Ghostty의 `xterm-ghostty`는 terminfo가 없어 따르지 않았습니다.
- unsafe 없이 할 수 없어 받기만 하는 키: `macos-titlebar-proxy-icon`, `background-blur`, `macos-auto-secure-input`. Dock 아이콘은 Dock 타일만 바뀌고 Cmd-Tab 아이콘은 그대로입니다.
- 실제 창에서 보지 않은 것(테스트로만 확인):
  - 링크 키 셋(Cmd를 누른 채 마우스를 올려야 함)
  - 스크롤 막대, 분할 확대 유지
  - `font-thicken`
  - `key-remap`을 실제 키로 누르기, Dock 아이콘, Space 동작
  - `hidden` 제목 막대에서 창 끌기, 창 그림자, `window-theme`

## [0.12.0] - 2026-11-09

Ghostty 키 채우기 5. 글꼴 고르기, 마우스, VT 제한, 스크롤백·팔레트, 명령 실행·창 수명, macOS 창, 글자 묶음 폭(mode 2027). 0.11처럼 하위 이슈마다 에이전트 하나가 구현하고, 주 세션이 Ghostty 소스와 대조·검토·병합했습니다. 추적 이슈 #173.

- 글꼴 고르기 키(Ghostty): `font-codepoint-map`(`U+3000-U+9FFF=글꼴`, 여러 번 쓰면 쌓임), `font-style`·`-bold`·`-italic`·`-bold-italic`(스타일 이름으로 꼴 고르기, `false`면 그 꼴을 끄고 보통 꼴로), `font-shaping-break`(`no-cursor`면 커서 밑에서도 합자 유지), `adjust-icon-height`(Nerd Font 아이콘 높이). 기본 동작은 그대로입니다. 실제 창에서는 `font-codepoint-map`(한글을 다른 글꼴로)과 `font-style-bold = false`만 확인했습니다. #174
- 마우스 키 `mouse-reporting`·`mouse-shift-capture`·`middle-click-action`·`click-repeat-interval`을 Ghostty와 같은 뜻으로 받습니다. 두 번 누르기 기본 간격이 400ms에서 OS 값(macOS)·500ms로 바뀝니다 #175
- VT 제한 키(Ghostty): `enquiry-response`(ENQ에 돌려보낼 글, 기본 빈 값이라 전처럼 답하지 않음), `vt-kam-allowed`·`vt-window-resize-allowed`(프로그램의 키보드 잠금·창 크기 바꾸기 요청, 기본 끔이라 전처럼 무시), `image-storage-limit`(kitty 그림 저장 상한, **기본이 256MiB에서 320000000바이트로 늘어남**, `0`이면 그림 프로토콜을 쓰지 않음), `clipboard-write-limit-bytes`(클립보드 쓰기 상한, 기본 64MiB라 전의 OSC 52 1MiB 고정이 그대로 이김), `clipboard-codepoint-map`(복사할 때 글자 바꾸기). 창 크기 바꾸기는 실제 창에서 아직 확인하지 않았고 나머지는 PTY 테스트와 순수 함수 테스트로만 확인했습니다. #176
- 스크롤백·팔레트 키(Ghostty): `scrollback-limit-bytes`(기본 1000만, 스크롤백 기본은 전과 같은 1만 줄), `scrollback-limit-lines`(먼저 닿는 쪽), `palette-generate`·`palette-harmonious`(16~255번 색 만들기, 기본 끔), `palette = 100=#...`(16번 이후), `background-opacity-cells`(색 바탕 칸도 비침, 기본 끔). 옛 `scrollback-limit`·`scrollback-lines`는 그대로 받습니다. 실제 창에서는 아직 확인하지 않았습니다. #177
- 명령 실행·창 수명 키(Ghostty): `initial-command`(첫 칸만), `wait-after-command`(끝나도 칸을 남기고 아무 키나 누르면 닫음), `abnormal-command-exit-runtime`(기본 250ms, 이보다 짧게 끝나면 칸을 남기고 안내), `initial-window`, `quit-after-last-window-closed`, `-delay`. 기본 동작이 달라집니다: macOS에서 마지막 창을 닫아도 앱이 남고(Ghostty 기본, 메뉴·btctl·전역 단축키로 새 창), 너무 빨리 끝난 칸은 닫히지 않고 안내가 남습니다. 실제 창에서는 마지막 창을 닫은 뒤 앱이 남고 `btctl new`로 창이 열리는 것, 빨리 끝난 칸의 안내를 확인했습니다. 키 입력이 필요한 것은 테스트로만 확인했습니다. #178
- macOS 창 키 추가: `macos-non-native-fullscreen`(새 Space 없는 전체 화면), `window-step-resize`, `macos-window-buttons`, `macos-hidden`. `window-subtitle`·`window-titlebar-*`는 받기만 한다 #179
- 글자 묶음 폭(Ghostty `grapheme-width-method`, mode 2027): 이모지 ZWJ 묶음(👨‍👩‍👧)·U+FE0F(❤️)·국기(🇰🇷)·피부색(👍🏽)이 이제 묶음 하나로 두 칸을 차지합니다(전에는 6·1·2·4칸으로 글자마다 셈). 기본은 Ghostty처럼 `unicode`이고 `legacy`면 옛 폭입니다. 프로그램은 DECSET/DECRST 2027로 바꾸고 DECRQM으로 물을 수 있습니다. 이를 위해 VT 엔진(alacritty_terminal 0.26)을 `vendor/`로 들여 고쳤습니다(ADR-014). 실제 창에서 네 묶음이 두 칸에 그려지고 `ESC[?2027l` 뒤 옛 폭으로 돌아가는 것을 확인했습니다. #180

### 알려진 한계

- 기본 동작이 바뀐 것:
  - 이모지 묶음이 두 칸입니다(`grapheme-width-method = legacy`로 되돌림).
  - macOS에서 마지막 창을 닫아도 앱이 남습니다(`quit-after-last-window-closed = true`로 되돌림).
  - 250ms 안에 끝난 칸은 안내를 남깁니다(`abnormal-command-exit-runtime = 0`으로 끔).
  - 두 번 누르기 간격이 OS 값입니다.
  - 그림 저장 상한이 320000000바이트입니다.
- 스크롤백 기본은 그대로 1만 줄입니다. Ghostty 1.4 소스의 50MB 기본은 스크롤백 압축과 함께 올린 값이라 따르지 않았습니다.
- 실제 창에서 보지 않은 것(테스트로만 확인):
  - 창 크기 바꾸기 요청(`vt-window-resize-allowed`)
  - 비네이티브 전체 화면 키
  - 칸 단위 크기 바꾸기
  - 팔레트 만들기, 색 칸 투명도
  - 커서 밑 합자(`font-shaping-break`)
  - tmux·vim 안의 이모지 묶음
- 글자 묶음 폭은 alacritty_terminal을 `vendor/`로 고쳐 씁니다. 엔진 판을 올릴 때마다 패치를 다시 얹어야 합니다. U+FE0E 좁히기, 결합 한글 자모, 인도계 묶음 폭은 하지 않습니다.

## [0.11.0] - 2026-11-09

Ghostty 키 채우기 4. 창·탭 이어받기, 명령 끝 알림, 선택·색, 글꼴 미세 조정, 닫은 것 되살리기, 소리 벨. 0.10처럼 하위 이슈마다 에이전트 하나가 구현하고 주 세션이 Ghostty 소스와 대조·검토·병합했습니다. 추적 이슈 #159.

- 이어받기·탭 키(Ghostty): `window-`·`tab-`·`split-inherit-working-directory`(새 창·탭·분할이 폴더를 이어받을지, 기본 켬이고 이제 `working-directory`가 `home`·경로여도 이어받음), `window-new-tab-position`(`current` 기본은 지금 탭 바로 뒤라 새 탭 자리가 전의 맨 끝에서 달라짐, `end`), `window-show-tab-bar`(`auto` 기본·`always`·`never`), `window-inherit-font-size`(글자 크기가 앱 전체 하나라 값만 받음). 실제 창에서는 아직 확인하지 않았습니다. #160
- 명령 끝 알림과 알림 허용 키(Ghostty): `notify-on-command-finish`(`never`·`unfocused`·`always`), `-action`(`bell`·`notify`), `-after`(기간, 기본 5초), `desktop-notifications`(프로그램의 OSC 9·777 알림 허용, 켬), `app-notifications`(`clipboard-copy`·`config-reload`, 켬). `notify-on-command-finish`를 적지 않으면 옛 `notify-command-seconds` 동작 그대로입니다(기본 동작 변화 없음). 실제 창에서는 아직 확인하지 않았고 순수 함수 테스트로만 확인했습니다. #161
- 선택·색 키(Ghostty): `selection-clear-on-typing`(기본 켬)·`selection-clear-on-copy`(기본 끔), 찾기 강조 색 `search-foreground`·`search-background`·`search-selected-foreground`·`search-selected-background`(`cell-foreground`·`cell-background` 포함), `bold-color`(색 또는 `bright`), `faint-opacity`(기본 0.5). 기본 동작이 바뀝니다: 찾기에 맞은 칸이 검정 글자에 노랑(#ffe082) 바탕, 지금 고른 곳이 살구색(#f2a57e) 바탕으로 칠해지고(전에는 팔레트 노랑을 섞은 색/팔레트 노랑), 흐린 글자가 전보다 조금 더 흐립니다(불투명도 0.6에서 0.5). 실제 창에서는 아직 확인하지 않았고 단위·픽셀 테스트로만 확인했습니다. #162
- 글꼴 미세 조정 키(Ghostty): `adjust-underline-position`·`-thickness`, `adjust-strikethrough-position`·`-thickness`, `adjust-overline-position`·`-thickness`(윗줄 SGR 53은 아직 그리지 않아 키만 받음), `adjust-cursor-height`·`-thickness`, `adjust-font-baseline`, `adjust-box-thickness`와 가변 글꼴 축 `font-variation`·`-bold`·`-italic`·`-bold-italic`(예 `wght=500`). 실제 창에서는 아직 확인하지 않았고 픽셀 테스트로만 확인했습니다. #163
- 닫은 것 되살리기(Ghostty): `undo-timeout`(기본 `5s`, `0s`면 끔) 동안 직접 닫은 칸·탭의 셸을 끊지 않고 들고 있다가 `undo`(⌘Z, ⇧⌘T)로 원래 자리에 되살리고 `redo`(⇧⌘Z)로 다시 닫습니다. 되살릴 것이 없으면 ⌘Z는 전처럼 셸로 갑니다. 창 닫기·셸이 스스로 끝난 칸은 되살리지 않습니다. 실제 창에서는 아직 확인하지 않았습니다. #164
- 소리 벨(Ghostty 키): `bell-features`의 `audio`가 `bell-audio-path` 소리 파일을 `bell-audio-volume`(0.0~1.0, 기본 0.5)으로 재생합니다(macOS `NSSound`, 앞 소리가 나는 중이거나 0.1초 안이면 건너뜀). 이전에는 `audio`를 받기만 했습니다. 실제 소리는 아직 들어 보지 않았고 해석·판단만 테스트로 확인했습니다. #165

### 알려진 한계

- 실제 창에서 본 것은 탭 막대 보이기·새 탭 자리·굵은/흐린 글자·밑줄·상자 선·커서 미세 조정입니다. 되살리기(⌘Z), 명령 끝 알림, 소리 벨, 찾기 강조 색, 고르기 지우기는 키·소리·알림이 필요해 테스트로만 확인했습니다.
- 기본 동작이 바뀐 것: 새 탭이 지금 탭 바로 뒤에 열립니다(`window-new-tab-position = current`). `working-directory = home`이어도 이어받기가 우선합니다. 찾기 강조 색과 흐린 글자(0.5)가 Ghostty 기본으로 바뀌었습니다. 닫은 칸의 셸은 5초 동안 더 삽니다(`undo-timeout`).
- `window-inherit-font-size = false`는 동작하지 않습니다(글자 크기가 앱 전체 하나). 창 되살리기는 하지 않습니다. 윗줄(SGR 53)은 그리지 않습니다.

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
