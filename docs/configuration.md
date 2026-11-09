# 설정

b-terminal은 Ghostty와 같은 문법으로 설정합니다. Ghostty를 깔지 않아도 됩니다. 글꼴과 테마를 앱에 넣어 두었고, Ghostty 설정은 원할 때만 가져옵니다(ADR-006).

## 파일과 읽는 순서

뒤에 읽은 것이 앞을 덮습니다.

1. 기본값: JetBrains Mono 13, 바탕 `#282c34`, 글자 `#ffffff`, Tomorrow Night 팔레트, Nerd Font 기호(Ghostty 1.3 기본과 같은 값). 두 글꼴은 앱에 넣어 두었습니다([assets/fonts](../assets/fonts/README.md)).
2. Ghostty 설정 파일 — b-terminal 설정에 `import-ghostty = true`를 쓴 경우만(기본 끔): `$XDG_CONFIG_HOME/ghostty/config`(`config.ghostty`), macOS는 `~/Library/Application Support/com.mitchellh.ghostty/config`(`config.ghostty`) 중 처음 찾은 하나.
3. b-terminal 설정: `~/.config/b-terminal/config`(`$XDG_CONFIG_HOME` 따름)
4. 명령줄 `--config-file <파일>`

`b-terminal --show-config`는 읽은 파일, 경고, 지금 적용된 값을 Ghostty 문법으로 보입니다.

## 문법

```
# 주석
font-family = D2Coding
font-family = Apple SD Gothic Neo   # 여러 번 쓰면 뒤는 대체 글꼴
font-size = 15
theme = Dracula                     # 또는 light:GitHub Light Default,dark:GitHub Dark
background = #1e1e2e
palette = 1=#f38ba8
config-file = ?~/.config/b-terminal/local   # ?가 붙으면 없어도 된다
```

- 값의 큰따옴표는 벗깁니다. 같은 키를 여러 번 쓰면 마지막 값이 이깁니다(`font-family`·`font-feature`·`palette`·`keybind`는 쌓입니다).
- 모르는 키와 읽지 못한 값은 버리고 경고로 남깁니다. 앱 로그와 `--show-config`에 보입니다.

## 지원하는 키

| 키 | 값 | 기본 | 메모 |
|---|---|---|---|
| `font-family` | 글꼴 이름(여러 번) | 넣어 둔 JetBrains Mono | 첫째가 기본, 나머지는 대체 글꼴(그 뒤에 한글·기호 기본 대체 글꼴이 붙음) |
| `font-family-bold` / `-italic` / `-bold-italic` | 글꼴 이름 | 기본 글꼴의 그 꼴 | |
| `font-feature` | 글꼴 기능(여러 번, 쉼표로 여럿): `-calt` 끔, `ss01` 켬, `cv01=2` 값 | 글꼴의 기본(calt·liga 켬) | 합자(`->` `!=` `===`)를 끄려면 `-calt`(JetBrains Mono·Fira Code). 커서 칸에서는 합자를 풀어 보입니다. 빈 값이면 지움 |
| `font-size` | 포인트 | `13` | Cmd-= / Cmd-- / Cmd-0 |
| `adjust-cell-height` / `adjust-cell-width` | `10%` 또는 포인트(`2`, `-1`) | 없음 | 높이를 바꾸면 글자를 가운데에 둡니다 |
| `theme` | 테마 이름, 절대 경로, `light:…,dark:…` | 없음 | 찾는 곳: `~/.config/b-terminal/themes`, 그다음 앱에 넣어 둔 905개([assets/themes-README.md](../assets/themes-README.md), `b-terminal --list-themes`). 이름은 띄어쓰기·대소문자를 무시합니다(`Tokyo Night` = `TokyoNight`) |
| `background` / `foreground` | `#rrggbb` | Ghostty 기본 | 테마 위에 덮습니다 |
| `palette` | `N=#rrggbb`(0~15) | Ghostty 기본 | 16~255번은 xterm 표 |
| `cursor-color` / `cursor-text` | `#rrggbb`, `cell-foreground`, `cell-background` | 글자색 / 바탕색 | `cell-*`는 커서 칸의 글자색·바탕색을 씁니다(예: `cursor-color = cell-foreground`, `cursor-text = cell-background`면 칸을 뒤집은 모양) |
| `cursor-opacity` | `0`~`1` | `1` | 커서 불투명도. 1보다 작으면 칸 위에 섞어 그리고, 블록 커서 위 글자색도 그만큼 섞습니다 |
| `cursor-click-to-move` | `true` `false` | `true` | 셸 통합이 있는 칸의 프롬프트에서 커서 줄을 한 번 누르면 그 자리까지 ←·→를 보내 커서를 옮깁니다(넓은 글자는 한 글자로 셈, 글 끝 뒤를 누르면 글 끝까지). 명령이 도는 중·vim 같은 전체 화면 프로그램·스크롤해 올라가 있을 때는 하지 않습니다. 커서 줄 밖(여러 줄 입력의 다른 줄)은 옮기지 않습니다 |
| `scroll-to-bottom` | `keystroke` `output` (쉼표로 이음, 앞에 `no-`를 붙이면 끔), `true` `false` | `keystroke, no-output` | Ghostty 값. 스크롤해 올라가 있을 때 언제 맨 아래로 돌아갈지. `keystroke`는 키를 치거나 붙여넣어 칸에 글을 보낼 때(앱 단축키·수식 키만 누른 것은 아님), `output`은 새 출력이 올 때. `output`을 켜면 로그를 거슬러 읽는 중에도 출력이 오면 맨 아래로 끌려갑니다. 적지 않은 쪽은 기본값, 모르는 이름이 있으면 경고하고 기본값 |
| `selection-background` / `selection-foreground` | `#rrggbb` | 색 뒤집기 | |
| `bold-is-bright` | `true` / `false` | `false` | Ghostty 기본과 같게 끔 |
| `cursor-style` | `block` `bar` `underline` `block_hollow` | `block` | 프로그램이 바꾸면 그쪽이 이깁니다 |
| `cursor-style-blink` | `true` / `false` | 끔 | 0.6초마다 깜빡입니다. 프로그램이 DECSCUSR(`ESC[1 q` 등)로 바꾸면 그쪽을 따르고, 키를 치는 동안과 창을 보지 않을 때는 깜빡이지 않습니다 |
| `window-padding-x` / `window-padding-y` | `2` 또는 `왼,오른`(`위,아래`) | `2` | 포인트 |
| `window-padding-balance` | `true` `false` | `false` | 창 크기가 칸 크기로 나누어떨어지지 않아 남는 자리를 네 변에 고르게 나눕니다(끄면 오른쪽·아래에 몰림) |
| `window-padding-color` | `background` `extend` `extend-always` | `background` | 여백 색. `extend`는 가장 가까운 칸의 바탕색으로 늘이되, 주 화면에서 가장 가까운 줄에 기본 바탕색 칸·프롬프트·powerline 꼭 맞춤 글자가 있으면 위아래로는 늘이지 않습니다(Ghostty와 같은 규칙). `extend-always`는 따지지 않고 늘입니다 |
| `window-width` / `window-height` | 셀 수 | 없음 | 둘 다 있을 때 처음 창 크기 |
| `macos-option-as-alt` | `true` `false` `left` `right` | `false` | Ghostty 기본과 같게 끔 |
| `scrollback-limit` | 바이트 | 1000만 | Ghostty 값. 줄 하나를 약 1KB로 보고 줄 수로 바꿉니다(100~20만 줄) |
| `scrollback-lines` | 줄 수 | | b-terminal 키. 줄 수로 바로 |
| `copy-on-select` | `true` `false` `clipboard` | `true` | 고르기를 마치면 클립보드로 |
| `mouse-hide-while-typing` | `true` / `false` | `false` | |
| `command` | 명령 | 로그인 셸 | `/bin/sh -c "exec <명령>"`으로 띄웁니다 |
| `working-directory` | `home` `inherit` 경로 | `inherit` | 새 탭은 `inherit`일 때 지금 탭의 폴더 |
| `title` | 글 | 셸이 보낸 제목 | 창 제목을 고정 |
| `keybind` | `트리거=동작` | 아래 표 | 아래 "키 묶음" |
| `config-file` | 경로(`?` 붙이면 선택) | | 그 자리에서 읽어 넣음. 8단계까지 |
| `import-ghostty` | `true` / `false` | `false` | b-terminal 키. Ghostty 설정을 옮겨 올 때 |
| `clipboard-write` | `allow` / `deny` | `allow` | 프로그램이 OSC 52로 클립보드에 쓰기(AI CLI의 `/copy`). 보고 있는 창의 고른 칸(화면을 나눴으면 키 입력을 받는 칸)에서 온 것만 받습니다 |
| `clipboard-read` | `allow` / `deny` / `ask` | `deny` | 프로그램이 OSC 52로 클립보드 읽기. `ask`는 아직 묻는 창이 없어 `deny`로 봅니다 |
| `shell-integration` | `detect` `none` `zsh` `bash` | `detect` | 탭의 셸에 셸 통합을 싣습니다(아래 "셸 통합") |
| `notify-command-seconds` | 초 | `10` | b-terminal 키. 셸 통합으로 안 명령이 이만큼 넘게 걸려 끝나면 창을 보지 않을 때 알림. `0`이면 끔 |
| `bell-features` | `system` `attention` `title`(쉼표로, 앞에 `no-`면 끔), `true` `false` | `attention,title` | 프로그램이 벨(BEL)을 울리면: `title`은 보지 않는 칸의 탭·창 제목 앞에 🔔를 붙이고 그 칸을 보면 뗍니다. `attention`은 창이 앞에 없을 때 Dock 아이콘을 튀깁니다. `system`은 시스템 경고음입니다(0.1초에 한 번까지). Ghostty의 `audio`·`border`는 받지만 하는 일이 없습니다 |
| `clipboard-paste-protection` | `true` `false` | `true` | 괄호 붙여넣기를 쓰지 않는 프로그램에 줄바꿈(LF·CR)이 든 글을 붙여넣으면 먼저 묻습니다(줄바꿈이 바로 Enter가 되어 보지 못한 명령이 돌 수 있어서). ↩·`y` 붙여넣기, esc·`n` 버리기 |
| `clipboard-paste-bracketed-safe` | `true` `false` | `true` | 괄호 붙여넣기 모드의 붙여넣기는 안전으로 봅니다. `false`면 그때도 줄바꿈이 있으면 묻습니다 |
| `confirm-close-surface` | `true` `false` `always` | `true` | 칸·탭·창을 닫거나 끝낼 때(⌘W, ⌥⌘W, 창 닫기 단추, ⌘Q) 프로그램이 도는 칸이 있으면 창 위쪽에 확인 막대를 띄웁니다. ↩·`y`면 닫고 esc·`n`이면 그만둡니다. 셸 통합이 있으면 명령이 도는 중인지로, 없으면 전경 프로세스가 셸 자신인지로 봅니다. `always`는 셸이 살아 있으면 늘 묻습니다. Dock 메뉴나 로그아웃으로 끝낼 때는 묻지 못합니다 |
| `resize-overlay` | `always` `never` `after-first` | `after-first` | 칸의 칸·줄 수가 바뀌면 칸 안에 `80 × 24`를 보입니다. `after-first`는 칸을 만든 직후(0.5초 안)의 크기 맞추기에는 보이지 않습니다 |
| `resize-overlay-position` | `center` `top-left` `top-center` `top-right` `bottom-left` `bottom-center` `bottom-right` | `center` | 크기 표시 자리 |
| `resize-overlay-duration` | 기간(`750ms`, `2s`, `1m30s`; 단위 `y` `d` `h` `m` `s` `ms` `us` `ns`) | `750ms` | 크기 표시가 보이는 시간 |
| `minimum-contrast` | `1`~`21` | `1` | 글자색과 바탕의 명암비(WCAG)가 이보다 낮으면 글자를 흰색이나 검은색(바탕과 명암비가 큰 쪽)으로 그립니다. `1`이면 끔. 3이면 대개 읽힙니다 |
| `focus-follows-mouse` | `true` `false` | `false` | 화면을 나눴을 때 마우스가 올라간 칸을 고릅니다(끌어 고르는 중에는 바꾸지 않음) |
| `mouse-scroll-multiplier` | 수, `discrete:수`, `precision:수`(쉼표로 둘) | `discrete:3,precision:1` | 휠 배율. `discrete`는 줄 단위 기기(마우스 휠), `precision`은 픽셀 단위 기기(트랙패드). 접두 없는 수는 둘 다. 0.01~10000 |
| `selection-word-chars` | 글자들(`\t`·`\\` 풂) | `` \t'"│`\|:;,()[]{}<>$``(맨 앞 공백 포함) | 두 번 눌러 단어를 고를 때 끊는 글자. 공백을 넣으려면 큰따옴표로 감쌉니다 |
| `quick-terminal-position` | `top` `bottom` `left` `right` `center` | `top` | 퀵 터미널이 붙는 가장자리. 메뉴 막대와 Dock을 뺀 영역에 놓습니다 |
| `quick-terminal-size` | `40%`, `300px`, 둘을 쉼표로(`50%,800px`) | `40%` | 첫 값은 주축(위·아래면 높이, 왼쪽·오른쪽·가운데면 폭), 둘째 값은 다른 축(없으면 위·아래·왼쪽·오른쪽은 끝까지, 가운데는 첫 값과 같게). 단위가 없으면 잘못된 값입니다. px는 화면의 점 단위입니다 |
| `quick-terminal-screen` | `main` `mouse` `macos-menu-bar` | `main` | 퀵 터미널이 뜨는 화면: 앞에 있는 창의 화면, 마우스가 있는 화면, 메뉴 막대가 있는 화면 |
| `quick-terminal-autohide` | `true` `false` | `true` | 포커스를 잃으면 숨습니다 |
| `quick-terminal-animation-duration` | 초 | `0.2` | 들어오고 나가는 움직임의 길이. `0`이면 움직이지 않습니다. 가운데(`center`)는 움직이지 않습니다 |
| `window-save-state` | `default` `always` `never` | `default` | 앱을 끌 때(Cmd-Q) 창 크기·자리, 탭, 분할(비율), 칸의 작업 폴더를 적어 두고 다음에 켤 때 새 셸로 되살립니다. 돌던 프로그램·화면 내용은 되살리지 않습니다. 마지막 창을 닫아 꺼진 경우와 `-e`·`--cwd`로 띄운 경우는 되살리지 않습니다 |
| `background-opacity` | `0`~`1` | `1` | 기본 바탕색 칸으로 창 뒤가 비칩니다(글자·색 칸은 불투명). 새 창부터 따릅니다 |
| `background-image` | PNG·JPEG 파일 경로(`~/`, 상대 경로는 설정 파일 폴더 기준), 빈 값이면 없음 | (없음) | 창 바탕에 그림을 깝니다. 기본 바탕색 칸과 여백에만 보이고, 색 바탕 칸은 그 색으로 덮습니다. 한 변 10000px·파일 64MiB까지 |
| `background-image-opacity` | 0 이상 | `1` | 그림 불투명도. 1보다 크면 `background-opacity`보다 진하게 보입니다(그림 알파 = 값 × 바탕 불투명도, 1에서 자름) |
| `background-image-position` | `top-left` `top-center` `top-right` `center-left` `center` `center-right` `bottom-left` `bottom-center` `bottom-right` | `center` | 그림 자리 |
| `background-image-fit` | `contain` `cover` `stretch` `none` | `contain` | 맞추기: 다 보이게 / 빈 곳 없이 덮게 / 늘려서 / 원래 크기 |
| `background-image-repeat` | `true` `false` | `false` | 놓은 자리에서 사방으로 이어 깝니다 |
| `window-colorspace` | `display-p3` / `srgb` | `display-p3` | macOS. `display-p3`는 화면 색 공간으로 그대로 넘겨 빠르고(색은 맞추지 않음), `srgb`는 테마 색을 정확히 맞추지만 큰 창에서 프레임마다 약 10ms가 더 듭니다(실험 E3). 새 창부터 따릅니다 |
| `unfocused-split-opacity` | `0.15`~`1` | `0.7` | 화면을 나눴을 때 고르지 않은 칸의 불투명도. 낮을수록 흐립니다 |
| `unfocused-split-fill` | `#rrggbb` | 바탕색 | 고르지 않은 칸을 덮는 색 |
| `split-divider-color` | `#rrggbb` | 바탕색과 글자색 사이 | 칸 사이 경계선 색 |
| `mouse-reset-on-shell` | `true` / `false` | `true` | b-terminal 키. 프로그램이 마우스 모드를 켠 채 끝나 셸로 돌아오면 끈다([문제 해결 1](troubleshooting.md#1-셸에-3515353583235가-쏟아지고-command-not-found가-줄줄이-뜬다)) |

### 받기만 하고 쓰지 않는 Ghostty 키

경고 없이 넘깁니다: `font-feature`, `font-thicken`, `background-blur`, `window-decoration`, `window-theme`, `macos-titlebar-style`, `macos-icon`, `confirm-close-surface`, `auto-update`, `minimum-contrast` 등. 그 밖의 키는 경고합니다.

## 키 묶음

Ghostty와 같은 문법입니다. 기본 묶음 위에 차례로 얹고, 같은 트리거는 나중 것이 이깁니다.

```
keybind = super+shift+n=new_tab
keybind = super+t=unbind              # 기본 묶음 지우기
keybind = ctrl+alt+g=text:git status\r # 글 보내기(\n \r \t \e \xHH)
keybind = clear                       # 기본 묶음을 모두 지우기
keybind = global:ctrl+backquote=toggle_quick_terminal  # 앱이 뒤에 있어도 듣는 전역 단축키
keybind = ctrl+a>n=new_window         # 키 순서: ctrl+a를 누르고 n
keybind = ctrl+a>ctrl+a=text:\x01     # 앞 키 자체를 보내고 싶을 때
```

- 수식 키: `super`(cmd) `ctrl` `alt`(opt) `shift`. 키 이름은 Ghostty 이름: `a`~`z`, `digit_1`(또는 `1`), `equal`, `minus`, `comma`, `bracket_left`, `bracket_right`, `arrow_up`, `page_up`, `home`, `enter`, `f1` …
- 키 순서(`ctrl+a>n`, Ghostty와 같은 문법): 앞 키를 누르면 다음 키를 기다립니다. 시간 제한은 없습니다. 순서에 묶이지 않은 키를 누르면 그때까지 누른 키를 모두 프로그램에 그대로 보냅니다(`ctrl+a` `x` → `^A` `x`). 겹치는 묶음은 나중 것이 이깁니다(`ctrl+a=…` 뒤에 `ctrl+a>n=…`을 쓰면 `ctrl+a`는 앞 키가 됩니다). 명령 팔레트에는 키 순서를 보이지 않습니다. `catch_all`과 `performable:`은 받지 않습니다.
- macOS가 아니면 기본 묶음의 `super`는 `ctrl+shift`로 바뀝니다.
- `global:` 접두사를 붙이면 시스템 전역 단축키가 됩니다(Ghostty와 같은 문법). 다른 앱이 앞에 있어도 듣습니다. 기본 전역 묶음은 없습니다. 다른 앱이 이미 잡은 조합이면 경고를 남기고 건너뜁니다.

| 동작 | 하는 일 | macOS 기본 |
|---|---|---|
| `copy_to_clipboard` / `paste_from_clipboard` | 복사 / 붙여넣기 | `super+c` / `super+v` |
| `new_window` | 새 창(마지막 창이 닫히면 앱이 끝남) | `super+n` |
| `new_tab` / `close_surface` / `close_tab` | 새 탭 / 칸 닫기(칸이 하나면 탭) / 탭을 칸째 닫기 | `super+t` / `super+w` / `super+alt+w` |
| `goto_tab:N` / `last_tab` | N번째 / 마지막 탭 | `super+1`~`8` / `super+9` |
| `next_tab` / `previous_tab` | 다음 / 앞 탭 | `super+shift+]` / `super+shift+[` |
| `new_split:right` `down` `left` `up` `auto` | 고른 칸을 나눠 그쪽에 새 칸(`auto`는 긴 쪽) | `super+d`(right) / `super+shift+d`(down) |
| `goto_split:previous` `next` `left` `right` `up` `down` | 앞·다음 칸, 그쪽에 붙은 칸으로(`top`·`bottom`도 받음) | `super+[` / `super+]` / `super+alt+방향키` |
| `resize_split:방향,포인트` | 고른 칸을 감싼 경계를 그쪽으로 민다 | `super+ctrl+방향키`(10) |
| `equalize_splits` | 같은 줄에 선 칸들을 같은 크기로 | `super+ctrl+equal` |
| `toggle_split_zoom` | 고른 칸만 탭 전체로 크게 보기 / 되돌리기 | `super+shift+enter` |
| `increase_font_size:N` / `decrease_font_size:N` / `reset_font_size` | 글자 크기 | `super+=` / `super+-` / `super+0` |
| `scroll_page_up` / `scroll_page_down` | 스크롤백 한 쪽 | `shift+page_up` `super+page_up` / 아래도 같음 |
| `jump_to_prompt:-1` / `jump_to_prompt:1` | 앞·뒤 프롬프트로(셸 통합) | `super+arrow_up` / `super+arrow_down` |
| `scroll_to_top` / `scroll_to_bottom` | 맨 위 / 맨 아래 | `shift+home` `super+home` / `shift+end` `super+end` |
| `scroll_page_lines:N` / `scroll_page_fractional:F` | N줄 / 화면의 F배만큼 스크롤(양수면 아래로, 음수면 위로) | (없음) |
| `toggle_fullscreen` | 전체 화면 켜고 끄기 | `super+ctrl+f` |
| `start_search` / `end_search` | 찾기 막대 열기 / 닫기. 막대에서 글을 치면 바로 찾고, Enter는 위로·Shift-Enter는 아래로, Esc로 닫습니다 | `super+f` / (Esc) |
| `navigate_search:next` / `navigate_search:previous` | 위(오래된 쪽) / 아래 맞은 곳으로 | `super+g` / `super+shift+g` |
| `toggle_command_palette` | 명령 팔레트 열기·닫기 | `super+shift+p` |
| `clear_screen` | 화면 지우기(셸에 Ctrl-L) | `super+k` |
| `select_all` | 보이는 화면 모두 고르기 | `super+a` |
| `reload_config` | 설정 다시 읽기 | `super+shift+comma` |
| `quit` | 앱 끝내기(프로그램이 도는 칸이 있으면 묻습니다) | `super+q` |
| `prompt_surface_title` / `set_surface_title:글` | 지금 칸의 제목을 입력 막대로 묻기 / 정하기. 사람이 붙인 제목은 프로그램이 정한 제목보다 먼저 보이고, 빈 글이면 지워져 프로그램 제목으로 돌아갑니다 | (없음, 메뉴 "셸 › 제목 바꾸기…"와 팔레트) |
| `write_scrollback_file:할일` / `write_screen_file:할일` / `write_selection_file:할일` | 스크롤백 전체 / 화면 / 고른 글을 파일로 쓰고, 할일이 `copy`면 경로를 클립보드에, `paste`면 칸에 붙여넣고, `open`이면 기본 앱으로 엽니다. 파일은 이 사용자 전용 임시 폴더에 두고 앱이 끝날 때 지웁니다. 고른 글이 없으면 아무것도 하지 않습니다 | (없음, 팔레트에 열기·복사) |
| `adjust_selection:방향` | 고른 것의 끝점을 옮깁니다. 방향은 `left` `right`(줄 끝에서 앞·뒤 줄로) `up` `down` `page_up` `page_down` `home` `end` `beginning_of_line` `end_of_line`. 고른 것이 없으면 아무것도 하지 않습니다 | (없음) |
| `copy_url_to_clipboard` | 마우스 아래 링크(화면의 URL·OSC 8) 주소를 클립보드에 | (없음) |
| `toggle_quick_terminal` | 퀵 터미널(화면 가장자리에서 미끄러져 나오는 창) 보이기·숨기기. 자리·크기·화면·움직임은 `quick-terminal-*` 설정. 창 상태 복원에서 빠집니다 | (없음, `global:`로 묶으세요) |
| `text:글` / `csi:…` / `esc:…` | 글·이스케이프 보내기 | |
| `unbind` | 그 트리거 지우기 | |

설정 다시 읽기는 글꼴·색(이미 열린 탭 포함)·키 묶음(전역 단축키 포함)·여백·Option 키를 바꿉니다. 바깥에서 부르려면 `btctl reload-config`. 퀵 터미널은 `btctl quick`.

## 셸 통합

탭에서 띄우는 zsh·bash에 셸 통합을 자동으로 싣습니다(Ghostty와 같은 방식, 사용자 설정은 그대로 읽힘. [assets/shell-integration](../assets/shell-integration/README.md)). 셸이 명령의 시작·끝·종료 코드(OSC 133)와 명령 글(OSC 633;E)을 알려 주면 터미널이 이렇게 씁니다.

- Cmd-↑ / Cmd-↓: 앞·뒤 프롬프트로 스크롤
- 창을 보지 않을 때 10초(`notify-command-seconds`) 넘게 걸린 명령이 끝나면 데스크톱 알림. 실패하면 "명령이 실패했습니다"
- `btctl run -- <명령>`: 그 탭에서 명령을 돌리고 끝날 때까지 기다려 출력을 찍고 그 종료 코드로 끝납니다. 에이전트가 다른 탭을 도구로 쓰는 길입니다. `btctl last`는 마지막으로 끝난 명령

fish·elvish는 아직 스크립트가 없습니다. 끄려면 `shell-integration = none`.
