# 설정

b-terminal은 Ghostty와 같은 문법으로 설정합니다. Ghostty에 맞춰 둔 설정을 그대로 가져오고, 그 위에 b-terminal 설정을 덮습니다.

## 파일과 읽는 순서

뒤에 읽은 것이 앞을 덮습니다.

1. 기본값: Ghostty 1.3 기본과 같습니다(JetBrains Mono 13, 바탕 `#282c34`, 글자 `#ffffff`, Tomorrow Night 팔레트, Nerd Font 기호). 두 글꼴은 앱에 넣어 두었습니다([assets/fonts](../assets/fonts/README.md)).
2. Ghostty 설정 파일(처음 찾은 하나): `$XDG_CONFIG_HOME/ghostty/config`(`config.ghostty`), macOS는 `~/Library/Application Support/com.mitchellh.ghostty/config`(`config.ghostty`). b-terminal 설정에 `import-ghostty = false`를 쓰면 읽지 않습니다.
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
| `theme` | 테마 이름, 절대 경로, `light:…,dark:…` | 없음 | 찾는 곳: `~/.config/b-terminal/themes`, `~/.config/ghostty/themes`, `/Applications/Ghostty.app/Contents/Resources/ghostty/themes`(Ghostty를 깔았으면 463개) |
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
| `keybind` | `트리거=동작` | Ghostty 기본 일부 | [#27](https://github.com/dj258255/b-terminal/issues/27)에서 해석 |
| `config-file` | 경로(`?` 붙이면 선택) | | 그 자리에서 읽어 넣음. 8단계까지 |
| `import-ghostty` | `true` / `false` | `true` | b-terminal 키 |

### 받기만 하고 쓰지 않는 Ghostty 키

경고 없이 넘깁니다: `font-feature`, `font-thicken`, `background-opacity`, `background-blur`, `window-decoration`, `window-theme`, `window-save-state`, `macos-titlebar-style`, `macos-icon`, `confirm-close-surface`, `shell-integration`, `auto-update`, `minimum-contrast`, `clipboard-read`·`clipboard-write`, `unfocused-split-opacity` 등. 그 밖의 키는 경고합니다.
