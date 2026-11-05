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

- 값의 큰따옴표는 벗깁니다. 같은 키를 여러 번 쓰면 마지막 값이 이깁니다(`font-family`·`palette`·`keybind`는 쌓입니다).
- 모르는 키와 읽지 못한 값은 버리고 경고로 남깁니다. 앱 로그와 `--show-config`에 보입니다.

## 지원하는 키

| 키 | 값 | 기본 | 메모 |
|---|---|---|---|
| `font-family` | 글꼴 이름(여러 번) | 넣어 둔 JetBrains Mono | 첫째가 기본, 나머지는 대체 글꼴(그 뒤에 한글·기호 기본 대체 글꼴이 붙음) |
| `font-family-bold` / `-italic` / `-bold-italic` | 글꼴 이름 | 기본 글꼴의 그 꼴 | |
| `font-size` | 포인트 | `13` | Cmd-= / Cmd-- / Cmd-0 |
| `adjust-cell-height` / `adjust-cell-width` | `10%` 또는 포인트(`2`, `-1`) | 없음 | 높이를 바꾸면 글자를 가운데에 둡니다 |
| `theme` | 테마 이름, 절대 경로, `light:…,dark:…` | 없음 | 찾는 곳: `~/.config/b-terminal/themes`, 그다음 앱에 넣어 둔 905개([assets/themes-README.md](../assets/themes-README.md), `b-terminal --list-themes`). 이름은 띄어쓰기·대소문자를 무시합니다(`Tokyo Night` = `TokyoNight`) |
| `background` / `foreground` | `#rrggbb` | Ghostty 기본 | 테마 위에 덮습니다 |
| `palette` | `N=#rrggbb`(0~15) | Ghostty 기본 | 16~255번은 xterm 표 |
| `cursor-color` / `cursor-text` | `#rrggbb` | 글자색 / 바탕색 | |
| `selection-background` / `selection-foreground` | `#rrggbb` | 색 뒤집기 | |
| `bold-is-bright` | `true` / `false` | `false` | Ghostty 기본과 같게 끔 |
| `cursor-style` | `block` `bar` `underline` `block_hollow` | `block` | 프로그램이 바꾸면 그쪽이 이깁니다 |
| `cursor-style-blink` | `true` / `false` | 끔 | |
| `window-padding-x` / `window-padding-y` | `2` 또는 `왼,오른`(`위,아래`) | `2` | 포인트 |
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
| `unfocused-split-opacity` | `0.15`~`1` | `0.7` | 화면을 나눴을 때 고르지 않은 칸의 불투명도. 낮을수록 흐립니다 |
| `unfocused-split-fill` | `#rrggbb` | 바탕색 | 고르지 않은 칸을 덮는 색 |
| `split-divider-color` | `#rrggbb` | 바탕색과 글자색 사이 | 칸 사이 경계선 색 |
| `mouse-reset-on-shell` | `true` / `false` | `true` | b-terminal 키. 프로그램이 마우스 모드를 켠 채 끝나 셸로 돌아오면 끈다([문제 해결 1](troubleshooting.md#1-셸에-3515353583235가-쏟아지고-command-not-found가-줄줄이-뜬다)) |

### 받기만 하고 쓰지 않는 Ghostty 키

경고 없이 넘깁니다: `font-feature`, `font-thicken`, `background-opacity`, `background-blur`, `window-decoration`, `window-theme`, `window-save-state`, `macos-titlebar-style`, `macos-icon`, `confirm-close-surface`, `auto-update`, `minimum-contrast` 등. 그 밖의 키는 경고합니다.

## 키 묶음

Ghostty와 같은 문법입니다. 기본 묶음 위에 차례로 얹고, 같은 트리거는 나중 것이 이깁니다.

```
keybind = super+shift+n=new_tab
keybind = super+t=unbind              # 기본 묶음 지우기
keybind = ctrl+alt+g=text:git status\r # 글 보내기(\n \r \t \e \xHH)
keybind = clear                       # 기본 묶음을 모두 지우기
```

- 수식 키: `super`(cmd) `ctrl` `alt`(opt) `shift`. 키 이름은 Ghostty 이름: `a`~`z`, `digit_1`(또는 `1`), `equal`, `minus`, `comma`, `bracket_left`, `bracket_right`, `arrow_up`, `page_up`, `home`, `enter`, `f1` …
- 키 순서(`ctrl+a>n`)와 분할 동작은 아직 받지 않습니다(경고).
- macOS가 아니면 기본 묶음의 `super`는 `ctrl+shift`로 바뀝니다.

| 동작 | 하는 일 | macOS 기본 |
|---|---|---|
| `copy_to_clipboard` / `paste_from_clipboard` | 복사 / 붙여넣기 | `super+c` / `super+v` |
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
| `clear_screen` | 화면 지우기(셸에 Ctrl-L) | `super+k` |
| `select_all` | 보이는 화면 모두 고르기 | `super+a` |
| `reload_config` | 설정 다시 읽기 | `super+shift+comma` |
| `text:글` / `csi:…` / `esc:…` | 글·이스케이프 보내기 | |
| `unbind` | 그 트리거 지우기 | |

설정 다시 읽기는 글꼴·색(이미 열린 탭 포함)·키 묶음·여백·Option 키를 바꿉니다. 바깥에서 부르려면 `btctl reload-config`.

## 셸 통합

탭에서 띄우는 zsh·bash에 셸 통합을 자동으로 싣습니다(Ghostty와 같은 방식, 사용자 설정은 그대로 읽힘. [assets/shell-integration](../assets/shell-integration/README.md)). 셸이 명령의 시작·끝·종료 코드(OSC 133)와 명령 글(OSC 633;E)을 알려 주면 터미널이 이렇게 씁니다.

- Cmd-↑ / Cmd-↓: 앞·뒤 프롬프트로 스크롤
- 창을 보지 않을 때 10초(`notify-command-seconds`) 넘게 걸린 명령이 끝나면 데스크톱 알림. 실패하면 "명령이 실패했습니다"
- `btctl run -- <명령>`: 그 탭에서 명령을 돌리고 끝날 때까지 기다려 출력을 찍고 그 종료 코드로 끝납니다. 에이전트가 다른 탭을 도구로 쓰는 길입니다. `btctl last`는 마지막으로 끝난 명령

fish·elvish는 아직 스크립트가 없습니다. 끄려면 `shell-integration = none`.
