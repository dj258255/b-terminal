# 설정

b-terminal은 Ghostty와 같은 문법으로 설정합니다. Ghostty를 깔지 않아도 됩니다. 글꼴과 테마를 앱에 넣어 두었고, Ghostty 설정은 원할 때만 가져옵니다(ADR-006).

## 파일과 읽는 순서

뒤에 읽은 것이 앞을 덮습니다.

1. 기본값: JetBrains Mono 13, 바탕 `#282c34`, 글자 `#ffffff`, Tomorrow Night 팔레트, Nerd Font 기호(Ghostty 1.3 기본과 같은 값). 두 글꼴은 앱에 넣어 두었습니다([assets/fonts](../assets/fonts/README.md)).
2. Ghostty 설정 파일 — b-terminal 설정에 `import-ghostty = true`를 쓴 경우만(기본 끔): `$XDG_CONFIG_HOME/ghostty/config`(`config.ghostty`), macOS는 `~/Library/Application Support/com.mitchellh.ghostty/config`(`config.ghostty`) 중 처음 찾은 하나.
3. b-terminal 설정: `~/.config/b-terminal/config`(`$XDG_CONFIG_HOME` 따름)
4. 명령줄 `--config-file <파일>`

`b-terminal --config-default-files=false`(Ghostty와 같은 명령줄 전용 키)를 주면 2·3번(기본 설정 파일)을 읽지 않고, 4번으로 준 파일만 읽습니다. 설정을 다시 읽을 때도 같습니다. 설정 파일 안에 이 키를 써도 경고 없이 아무 일도 하지 않습니다(Ghostty와 같음). 값 없이 `--config-default-files`만 쓰면 켬(기본)입니다.

`b-terminal --show-config`는 읽은 파일, 경고, 지금 적용된 값을 Ghostty 문법으로 보입니다. `+show-config --changes-only=false`와 같은 글(`theme` 줄 포함)이고 기본과의 비교는 없습니다.

## 명령줄 동작 `+동작`

`b-terminal +동작 [인자]`는 창을 띄우지 않고 일을 하고 끝납니다(Ghostty의 `ghostty +동작`). `+`로 시작하는 인자가 동작이고 자리는 가리지 않지만(`-e` 뒤는 프로그램의 인자), 동작은 하나만 줄 수 있습니다. 모르는 동작은 오류(종료 코드 1)입니다. `+help`가 목록을, `+동작 --help`가 그 동작의 설명을 보입니다. 기존 `--` 인자는 그대로입니다.

| 동작 | 하는 일 |
|---|---|
| `+version` | `b-terminal <버전>` 한 줄(`--version`과 같음). Ghostty처럼 빌드 정보를 늘어놓지는 않습니다 |
| `+help` | 동작 목록 |
| `+show-config` `[--default] [--changes-only[=false]] [--docs]` | Ghostty처럼 **기본과 다른 키만** 보입니다(`--changes-only`는 기본 true, `--changes-only=false`면 `--show-config`처럼 전부. 키 하나를 한 덩어리로 기본과 견주어, 줄 묶음이 다르면 그 키의 줄을 모두 보입니다(Ghostty `Config.changed`처럼. 그래서 `palette`를 하나만 바꿔도 256줄이 나옵니다). 기본에 있는데 지금 없는 키는 `키 = `로 드러납니다. `theme`을 썼으면 쓴 값 그대로(`light:…,dark:…` 포함) `theme = 값` 줄이 나오고, 테마가 정한 색도 바뀐 키라서 함께 나옵니다(Ghostty도 테마를 설정에 적용한 뒤 견줍니다). 한계: 키 묶음은 Ghostty처럼 기본 묶음까지 풀어 찍지 않고 쓴 `keybind` 줄(`keybind = clear` 포함)을 그대로 보입니다). `--default`는 파일을 읽지 않은 기본값 전체를 보이고 `--changes-only`는 거기에는 효과가 없습니다. `--docs`는 키 위에 이 문서 "지원하는 키" 표의 값·기본·메모를 `# ` 주석으로 답니다(`+explain-config`와 같은 표, 같은 키가 여러 줄이면 첫 줄 위에 한 번, 표에 없는 키는 설명 없이). `--config-file`, `--config-default-files`도 받습니다 |
| `+list-themes` `[--path] [--color=all\|dark\|light] [--plain]` | `이름 (resources)`(넣어 둔 것)·`이름 (user)`(`~/.config/b-terminal/themes`)를 이름순으로 한 줄씩. 사용자 테마가 이름이 같으면 이깁니다. `--path`는 사용자 테마 줄 끝에 파일 경로를 붙입니다(`이름 (user) /경로`). 넣어 둔 테마는 파일이 없어 경로를 붙이지 않고 `이름 (resources)` 그대로입니다. `--color`는 배경색의 밝기(0.2126 R + 0.7152 G + 0.0722 B, 0.5 미만이면 어두움)로 거릅니다. 배경을 `#rrggbb`로 읽지 못한 테마는 어둡다고 칩니다. `--plain`은 받기만 합니다(늘 목록, 대화형 미리보기 없음) |
| `+validate-config` | 설정을 읽어 경고를 한 줄씩 표준 출력에 보이고, 경고가 있으면 종료 코드 1, 없으면 0. `--config-file`, `--config-default-files`와 함께 씁니다. `--config-file`로 준 파일이 없으면 오류(1)입니다 |
| `+edit-config` | `~/.config/b-terminal/config`(없으면 주석 한 줄로 만들고)를 `$VISUAL`(없거나 비면 `$EDITOR`)로 열고 편집기가 끝나길 기다려 그 종료 코드를 따릅니다. 편집기 값은 셸을 거치지 않고 빈칸·따옴표로 갈라 인자 배열로 넘깁니다(`code -w`는 됩니다. 셸 문법인 `;`·`$(…)`·`~`·환경 변수 풀이는 되지 않습니다). 둘 다 없으면 오류(1)입니다 |
| `+list-keybinds` `[--default] [--docs] [--plain]` | 지금 쓰는 키 묶음 표를 `keybind = 트리거=동작` 줄로 보입니다(기본 묶음 위에 설정을 얹은 결과, `--default`면 기본만). 표는 늘 평문이라 `--plain`은 받기만 하고, 동작 설명이 없어 `--docs`도 받기만 합니다. |
| `+list-actions` `[--docs] [--plain]` | 키 묶음이 받는 동작 이름을 한 줄에 하나씩. `--docs`(설명)는 받기만 합니다. |
| `+list-fonts` `[--family=이름] [--style=이름] [--bold] [--italic]` | 시스템 글꼴과 넣어 둔 글꼴을 집안별로. `--family`가 없으면 고정폭 글꼴만 보입니다. 얼굴 이름은 PostScript 이름입니다. |
| `+show-face` `--cp=번호 \| --string=글 [--style=regular\|bold\|italic\|bold_italic] [--presentation=text\|emoji]` | 글자가 어느 글꼴로 그려지는지(앱이 그릴 때의 대체 순서와 같고 설정 파일을 읽습니다). 번호는 10·`0x`·`0o`·`0b`진수. |
| `+list-colors` `[--plain]` | X11 색 이름과 `#rrggbb`를 이름순으로(소문자). 설정의 색 값이 이 이름을 모두 받지는 않습니다. |
| `+explain-config` `<키>` (또는 `--option=<키>`, `--keybind=<동작>`) | 이 문서의 "지원하는 키" 표에서 그 키의 값·기본·메모를 보입니다. 모르는 이름이면 `Unknown: '이름'.`과 종료 코드 1. |

Linux 전용인 `+new-window`·`+ssh-cache`·`+crash-report`·`+boo`는 두지 않습니다.

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

- 값의 큰따옴표는 벗깁니다. 같은 키를 여러 번 쓰면 마지막 값이 이깁니다(`font-family`·`font-feature`·`palette`·`keybind`·`command-palette-entry`는 쌓입니다).
- 모르는 키와 읽지 못한 값은 버리고 경고로 남깁니다. 앱 로그와 `--show-config`에 보입니다.

## 지원하는 키

| 키 | 값 | 기본 | 메모 |
|---|---|---|---|
| `font-family` | 글꼴 이름(여러 번) | 넣어 둔 JetBrains Mono | 첫째가 기본, 나머지는 대체 글꼴(그 뒤에 한글·기호 기본 대체 글꼴이 붙음) |
| `font-family-bold` / `-italic` / `-bold-italic` | 글꼴 이름 | 기본 글꼴의 그 꼴 | |
| `font-feature` | 글꼴 기능(여러 번, 쉼표로 여럿): `-calt` 끔, `ss01` 켬, `cv01=2` 값 | 글꼴의 기본(calt·liga 켬) | 합자(`->` `!=` `===`)를 끄려면 `-calt`(JetBrains Mono·Fira Code). 커서 칸에서는 합자를 풀어 보입니다. 쓸 수 있는 모양: `calt`·`+calt`·`calt on`·`calt=1`(켬), `-calt`·`calt off`·`calt=0`(끔), `ss01=2`·`ss01 2`(값), 이름을 따옴표로 감싸도 됩니다(`"calt" off`). 읽지 못하는 설정은 경고하고 넘깁니다. 빈 값이면 지움 |
| `font-size` | 포인트 | `13` | Cmd-= / Cmd-- / Cmd-0 |
| `adjust-cell-height` / `adjust-cell-width` | `10%` 또는 포인트(`2`, `-1`) | 없음 | 높이를 바꾸면 글자를 가운데에 둡니다 |
| `adjust-font-baseline` | `10%` 또는 포인트 | 없음 | 셀 아래에서 기준선까지의 거리를 바꿉니다. 늘리면 글자가 위로 올라갑니다 |
| `adjust-underline-position` / `adjust-strikethrough-position` / `adjust-overline-position` | `10%` 또는 포인트 | 없음 | 셀 위에서 그 줄 위 가장자리까지의 거리. 늘리면 줄이 아래로 내려가고, 줄이 셀 안에 들어가게 자릅니다. 윗줄(SGR 53)은 아직 그리지 않아서 키만 받습니다 |
| `adjust-underline-thickness` / `adjust-strikethrough-thickness` / `adjust-overline-thickness` | `10%` 또는 포인트 | 없음 | 줄 두께. 1픽셀 아래로는 내려가지 않습니다. 윗줄은 키만 받습니다 |
| `adjust-cursor-height` | `10%` 또는 포인트 | 없음 | 모든 커서 꼴의 높이. 셀 가운데에 두고 셀 높이를 넘기지 않습니다. 밑줄 커서는 커서 아래에 붙습니다 |
| `adjust-cursor-thickness` | `10%` 또는 포인트 | 없음 | 막대 커서와 속 빈 네모 커서의 선 두께. 밑줄 커서 두께는 바꾸지 않습니다 |
| `adjust-box-thickness` | `10%` 또는 포인트 | 없음 | 상자 선(`─` `│` 등)의 선 두께 |
| `font-variation` / `-bold` / `-italic` / `-bold-italic` | `축=값`(여러 번, 예 `wght=500`) | 없음 | 가변 글꼴의 축. 축 이름은 네 글자입니다. `font-variation`은 모든 꼴에, 나머지는 그 꼴에만 더합니다(같은 축이면 꼴별 값이 이김). 글꼴이 지원하지 않는 축이나 범위 밖 값은 글꼴이 알아서 무시하거나 자릅니다. 기본 글꼴의 네 꼴에만 적용하고 대체 글꼴에는 적용하지 않습니다. 칸 크기를 잴 때는 반영하지 않습니다. 빈 값이면 그 목록을 비웁니다 |
| `font-style` / `-bold` / `-italic` / `-bold-italic` | 글꼴이 알리는 스타일 이름(예 `Heavy`), `false`, `default` | `default` | 그 꼴에 쓸 스타일을 이름으로 고릅니다(대소문자 무시). 찾는 글꼴은 그 꼴의 `font-family-*`, 없으면 기본 글꼴이고, 못 찾으면 `default`처럼 알아서 고릅니다. `false`는 그 꼴을 끄고 보통 꼴로 그립니다(`font-style = false`는 보통 꼴을 끌 수 없어 무시). 굵게·기울임 꼴이 없을 때의 합성은 `font-synthetic-style`입니다. 빈 값이면 `default` |
| `font-codepoint-map` | `U+3000-U+9FFF=글꼴 이름`(여러 번, 한 글꼴에 여러 범위는 쉼표 `U+AC00,U+E0A0-U+E0B3=…`) | 없음 | 그 범위 글자를 정한 글꼴에서 먼저 찾습니다. 글자는 큰 `U+` 다음 16진수입니다. 여러 번 쓰면 쌓이고 겹치면 뒤에 쓴 것이 이깁니다. 정한 글꼴이 없거나 그 글자가 없으면 보통 차례로 찾고, 이 글자는 합자로 묶지 않습니다. 굵게·기울임은 그 글꼴의 해당 꼴을 씁니다. 읽지 못하는 줄은 경고하고 건너뜁니다. 빈 값이면 지움 |
| `font-synthetic-style` | `true`, `false`, 쉼표 목록 `bold`·`italic`·`bold-italic`(앞에 `no-`면 끔) | 모두 켬 | 글꼴에 굵은·기울임 꼴이 **없을 때** 합성합니다: 굵게는 윤곽을 부풀리고(글자 크기의 1/90px, 넣어 둔 JetBrains Mono로 보통 꼴보다 먹이 약 27% 늘게 맞춤), 기울임은 10도 비틉니다. 굵은 기울임은 둘 다. 글꼴에 그 꼴이 있으면(굵기 600 이상, 기울임·비스듬함 표시, 또는 가변 글꼴이면 그 축에 `font-variation-*`로 값을 준 경우) 합성하지 않습니다. `font-family-bold` 등으로 꼴의 글꼴을 따로 정했거나 `font-style-*`에 이름을 줬으면 그 꼴로 보고 합성하지 않으며, `font-style-* = false`로 끈 꼴은 보통 꼴 그대로입니다. 끄면 없는 꼴은 보통 꼴로 그립니다. `bold-italic`은 따로 켜고 끕니다(`no-bold`만으로는 굵은 기울임이 안 꺼집니다). 대체 글꼴·`font-codepoint-map` 글꼴의 글자에는 합성하지 않습니다 |
| `font-thicken` | `true`, `false` | `false` | 글자를 두껍게 그립니다. Ghostty에서는 macOS 전용이지만 b-terminal은 윤곽을 부풀리는 방식(swash `embolden`)이라 어느 OS에서나 됩니다. macOS의 두껍게 그리기(CoreText)와 알고리즘이 달라 모양이 똑같지는 않습니다 |
| `font-thicken-strength` | 0~255 | `255` | `font-thicken` 강도. Ghostty처럼 `0`도 두껍게 하는 가장 약한 값이고(끄는 값이 아님), `font-thicken = false`면 효과가 없습니다. 부풀리는 양은 `(0.015 + 0.05 × 강도/255) × 화면 배율`px입니다. 배율 2에서 JetBrains Mono 'a'의 먹이 `0`은 약 3%, `255`는 약 12.5% 늘었습니다(한 글자로 잰 값, macOS 두껍게 그리기와 견준 값 아님). 범위 밖이면 경고하고 앞 값을 둡니다 |
| `window-title-font-family` | 글꼴 이름 | 비움(터미널 글꼴) | 탭 줄 제목을 그릴 글꼴입니다. 고정폭이 아니어도 되고, 글자마다 글꼴이 정한 폭으로 자리를 잡아 탭 너비·잘림(`…`)도 그 실제 폭으로 셉니다. 글꼴 크기는 터미널과 같고(Ghostty macOS는 터미널 글꼴 크기와 무관하게 시스템 기본 크기 `NSFont.systemFontSize`로 정합니다. 요약으로 확인한 것이고 b-terminal은 아직 따르지 않습니다), 제목 글꼴에 없는 글자(한글·기호·이모지)는 터미널 기본 글꼴 다음 터미널과 같은 대체 순서로 찾고, 제목 글꼴이 비례폭이면 한글은 그 대체 글꼴의 폭, 색 이모지는 그려지는 글리프 폭만큼 나아갑니다(고정폭이면 터미널처럼 두 칸). 없는 이름이면 경고하고 터미널 글꼴로 그립니다. 설정을 다시 읽으면 바뀝니다. **Ghostty와 다른 점**: Ghostty는 비우면 시스템 기본 글꼴을 쓰지만(macOS) b-terminal은 비우면 지금까지처럼 터미널 글꼴을 씁니다. macOS 창 제목 막대(AppKit이 그리는 제목)와 확인 막대·팔레트 같은 다른 화면 글은 바뀌지 않습니다. 탭 줄 높이는 터미널 글꼴 기준 그대로라 키 큰 글꼴이면 잘릴 수 있습니다 |
| `font-shaping-break` | `cursor`, `no-cursor`, `true`, `false` | `cursor` | 합자를 끊는 자리. `cursor`는 커서 칸에서 합자를 풀어 보입니다. `no-cursor`면 커서 밑에서도 합자를 유지합니다 |
| `adjust-icon-height` | `10%` 또는 포인트 | 없음 | Nerd Font 아이콘(사설 영역 글자, 파워라인 도형 U+E0B0~U+E0D7 제외)이 넘지 않을 높이를 셀 높이 기준으로 바꿉니다. 양수는 키우고 음수는 줄이며 원래 자리의 가운데에 둡니다. 키울 때는 칸 폭에서 멈추고, 이미 칸보다 넓은 아이콘은 더 키우지 않습니다(Ghostty의 아이콘 맞춤 표를 그대로 옮기지는 않았습니다) |
| `theme` | 테마 이름, 절대 경로, `light:…,dark:…` | 없음 | 찾는 곳: `~/.config/b-terminal/themes`, 그다음 앱에 넣어 둔 905개([assets/themes-README.md](../assets/themes-README.md), `b-terminal --list-themes`). 이름은 띄어쓰기·대소문자를 무시합니다(`Tokyo Night` = `TokyoNight`)`light:…,dark:…`는 시스템이 밝음·어두움을 바꾸면 열려 있는 모든 창·칸에서 바로 다시 골라집니다(설정 파일은 다시 읽지 않고, 테마 파일만 다시 읽음). 테마가 하나뿐이면 색은 그대로입니다(모드 2031을 켠 칸에는 시스템 모양이 바뀌었다고 알립니다, 아래 "색 구성표 보고") |
| `background` / `foreground` | `#rrggbb` | Ghostty 기본 | 테마 위에 덮습니다 |
| `palette` | `N=#rrggbb`(0~255) | Ghostty 기본 | 16~255번은 xterm 표이고, `palette-generate`를 켜면 만든 색입니다. 직접 정한 번호가 늘 이깁니다 |
| `cursor-color` / `cursor-text` | `#rrggbb`, `cell-foreground`, `cell-background` | 글자색 / 바탕색 | `cell-*`는 커서 칸의 글자색·바탕색을 씁니다(예: `cursor-color = cell-foreground`, `cursor-text = cell-background`면 칸을 뒤집은 모양) |
| `cursor-opacity` | `0`~`1` | `1` | 커서 불투명도. 1보다 작으면 칸 위에 섞어 그리고, 블록 커서 위 글자색도 그만큼 섞습니다 |
| `cursor-click-to-move` | `true` `false` | `true` | 셸 통합이 있는 칸의 프롬프트에서 커서 줄을 한 번 누르면 그 자리까지 ←·→를 보내 커서를 옮깁니다(넓은 글자는 한 글자로 셈, 글 끝 뒤를 누르면 글 끝까지). 명령이 도는 중·vim 같은 전체 화면 프로그램·스크롤해 올라가 있을 때는 하지 않습니다. 커서 줄 밖(여러 줄 입력의 다른 줄)은 옮기지 않습니다 |
| `scroll-to-bottom` | `keystroke` `output` (쉼표로 이음, 앞에 `no-`를 붙이면 끔), `true` `false` | `keystroke, no-output` | Ghostty 값. 스크롤해 올라가 있을 때 언제 맨 아래로 돌아갈지. `keystroke`는 키를 치거나 붙여넣어 칸에 글을 보낼 때(앱 단축키·수식 키만 누른 것은 아님), `output`은 새 출력이 올 때. `output`을 켜면 로그를 거슬러 읽는 중에도 출력이 오면 맨 아래로 끌려갑니다. 적지 않은 쪽은 기본값, 모르는 이름이 있으면 경고하고 기본값 |
| `title-report` | `true` `false` | `false` | 프로그램이 창 제목을 묻는 질의(CSI 21 t)에 제목으로 답합니다(제어 문자는 지움). 제목은 출력이 정하므로 답이 셸 입력이 되어 명령이 될 수 있어 기본으로 끕니다. 새로 연 칸부터 적용 |
| `grapheme-width-method` | `unicode` `legacy` | `unicode` | Ghostty 값. 이모지 묶음(👨‍👩‍👧 ZWJ, ❤️ U+FE0F, 🇰🇷 국기, 👍🏽 피부색)의 폭을 어떻게 셀지. `unicode`는 묶음 하나를 한 글자처럼 두 칸으로 세고, `legacy`는 글자(코드포인트)마다 센 옛 폭입니다. 프로그램이 DECSET 2027(`ESC[?2027h`)로 켜고 DECRST로 끌 수 있고 DECRQM(`ESC[?2027$p`)으로 상태를 물을 수 있으며, 이 값은 그 처음 상태를 정합니다. 새로 연 칸부터 적용 |
| `osc-color-report-format` | `none` `8-bit` `16-bit` | `16-bit` | 프로그램이 색을 묻는 질의(OSC 4·10·11)에 답하는 꼴. `none`은 답하지 않고, `8-bit`는 `rgb:RR/GG/BB`, `16-bit`는 `rgb:RRRR/GGGG/BBBB`입니다. 새로 연 칸부터 적용 |
| `enquiry-response` | 글(따옴표로 감싸도 됨) | (빈 값) | 프로그램이 ENQ(0x05)를 보내면 PTY로 돌려보낼 글. 비어 있으면 답하지 않습니다. 글은 설정이 정하고 출력이 정하지 못합니다. 줄바꿈 같은 이스케이프는 풀지 않고 적은 그대로 보냅니다. 새로 연 칸부터 적용 |
| `vt-kam-allowed` | `true` `false` | `false` | 프로그램이 키보드 잠금 모드(KAM, `CSI 2 h`)로 사람의 키 입력을 막을 수 있게 합니다. `CSI 2 l`이나 초기화(RIS)로 풀리고, 잠가 놓고 죽은 프로그램이 있으면 그 탭은 입력을 받지 못하니(탭 닫기·앱 단축키는 됩니다) 필요한 사람만 켜세요. 새로 연 칸부터 적용 |
| `vt-window-resize-allowed` | `true` `false` | `false` | 프로그램이 `CSI 8 ; 행 ; 열 t`로 창 크기를 바꾸게 합니다. 0이나 빠진 쪽은 그대로 두고, 40열 × 10행보다 작게는 줄이지 않고 화면보다 크게는 키우지 않습니다. 창에 탭이 하나이고 분할이 없고 퀵 터미널·전체 화면·최대화가 아닐 때만 따릅니다. 새로 연 칸부터 적용 |
| `term` | 글 | `xterm-256color` | 칸의 `TERM` 값. **Ghostty의 기본은 `xterm-ghostty`지만 b-terminal에는 그 terminfo가 없어** 지금 값(`xterm-256color`)을 지킵니다. 많은 시스템이 `xterm-ghostty`를 몰라 `tput`·`vim`이 깨질 수 있으니, 그 terminfo를 직접 깔았을 때만 바꾸세요. 빈 값은 기본으로 되돌립니다. `env = TERM=…`가 이 값보다 이깁니다. 새로 연 칸부터 적용 |
| `env` | `KEY=VALUE` (여러 번) | (없음) | 칸을 띄울 때 환경에 더할 변수. 같은 키를 또 쓰면 덮고, `KEY=`는 맵에서 그 키를 지우고(앱이 물려받은 환경 변수를 지우지는 않습니다), 빈 값(`env =`)은 맵 전체를 비웁니다. **b-terminal이 넣는 변수(`TERM`·`COLORTERM`·`TERM_PROGRAM`·`BT_SOCKET`·`BT_TAB_ID`·셸 통합의 `ZDOTDIR`)와 겹치면 `env`가 이깁니다**(Ghostty와 같음). `BT_SOCKET`·`BT_TAB_ID`·`ZDOTDIR`을 덮으면 `btctl`이나 셸 통합이 안 될 수 있습니다. 새로 연 칸부터 적용 |
| `input` | `raw:글` `path:파일` 또는 접두사 없는 글 (여러 번) | (없음) | 새 칸을 띄운 직후 PTY에 보낼 데이터. 모든 새 칸(탭·창·분할)에 보내고, 여러 줄은 사이에 줄바꿈 없이 이어 붙입니다. `raw:`는 Zig 문자열 이스케이프(`\n \r \t \\ \' \"`, `\xNN` 바이트, `\u{코드포인트}`)를 풀고 그 밖의 `\`는 경고합니다. `path:`는 보통 파일만 읽고(상대 경로는 설정 파일 폴더 기준, `~/` 지원) 칸을 띄울 때마다 읽습니다. **`raw:`와 파일을 합친 크기는 10MiB까지**이고, 파일이 없거나 못 읽거나 상한을 넘으면 경고(로그)하고 하나도 보내지 않습니다. 제어 문자가 셸에서 명령을 실행시킬 수 있으니 조심하세요. 값은 설정만 정하고 터미널 출력은 바꾸지 못합니다. 빈 값은 목록을 비웁니다. 새로 연 칸부터 적용 |
| `image-storage-limit` | 바이트(0 이상 4294967295 이하) | `320000000` | kitty 그림을 저장하는 총량. 넘으면 오래된 그림부터 버리고, 그림 한 장이 이보다 크면 `ENOSPC`로 거절합니다. `0`이면 그림 프로토콜을 쓰지 않고 질의에도 답하지 않습니다. Ghostty는 주 화면·대체 화면에 따로 적용하지만 b-terminal은 칸 하나에 하나입니다. 한 장 64MiB·한 변 10000px·그림 4096개 같은 다른 상한은 그대로입니다. 새로 연 칸부터 적용 |
| `clipboard-write-limit-bytes` | 바이트 또는 `unlimited` | `67108864` | 프로그램의 클립보드 쓰기 한 번의 크기 상한(`0`은 빈 글 말고 모두 거절). Ghostty에서는 kitty 클립보드(OSC 5522)에만 걸리지만 b-terminal은 OSC 52에 겁니다. OSC 52에는 1MiB 고정 상한이 따로 있어 이 값은 그보다 줄이기만 하고(`unlimited`여도 1MiB), 설정을 다시 읽으면 바로 반영됩니다 |
| `clipboard-codepoint-map` | `범위=바꿀 글`(여러 번). 범위는 `U+2500`, `U+2500-U+257F`, 쉼표로 이은 목록. 바꿀 글은 `U+002D` 하나이거나 아무 글 | 없음 | 복사할 때 글자를 바꿉니다(예 `U+2500=U+002D`, `U+03A3=SUM`). 겹치면 나중에 적은 것이 이기고, 바꿀 글이 비어 있으면 그 글자를 지우고, 값이 비면 목록을 비웁니다. 글을 복사하는 동작에만 적용하고 링크 복사와 프로그램의 OSC 52 쓰기에는 적용하지 않습니다 |
| `selection-background` / `selection-foreground` | `#rrggbb` | 색 뒤집기 | |
| `bold-is-bright` | `true` / `false` | `false` | Ghostty 기본과 같게 끔 |
| `bold-color` | `#rrggbb`, `bright` | 없음 | 굵은 글자의 색(Ghostty 1.2). 색을 주면 굵은 기본 글자색이 그 색이 되고, 나머지 굵은 0~7번 색은 밝은 8~15번 색이 됩니다. `bright`는 색 없이 굵은 0~7번 색만 밝게 올립니다(`bold-is-bright = true`와 같음). 값을 비우면 기본 |
| `faint-opacity` | `0`~`1` | `0.5` | 흐린 글자(SGR 2)의 불투명도(Ghostty 1.2). 1이면 그대로, 0이면 바탕에 묻혀 보이지 않습니다. 범위 밖은 가까운 값으로 묶습니다. 전에는 0.6 고정이었습니다 |
| `search-background` / `search-foreground` | `#rrggbb`, `cell-foreground`, `cell-background` | `#ffe082` / `#000000` | 찾기에 맞은 칸의 바탕·글자색(지금 고른 곳 말고 나머지). `cell-*`는 그 칸의 글자색·바탕색 |
| `search-selected-background` / `search-selected-foreground` | `#rrggbb`, `cell-foreground`, `cell-background` | `#f2a57e` / `#000000` | 지금 고른 맞은 곳(Enter로 옮겨 다니는 것)의 바탕·글자색 |
| `selection-clear-on-typing` | `true` / `false` | `true` | 글을 치면 고른 영역을 지웁니다(IME 조합을 시작할 때도). 끄면 지우지 않지만 Esc는 지웁니다. 붙여넣기는 이 값과 상관없이 지웁니다 |
| `selection-clear-on-copy` | `true` / `false` | `false` | 복사 명령(단축키·메뉴·`right-click-action`의 복사)으로 복사한 뒤 고른 영역을 지웁니다. `copy-on-select`의 자동 복사는 지우지 않습니다 |
| `macos-auto-secure-input` | `true` / `false` | `true` | macOS에서 암호 입력을 알아채면(PTY의 termios가 줄 단위 입력이고 되울림이 꺼짐, 200ms마다 살핌) 그 칸이 초점일 때 보안 키보드 입력을 켭니다. 앱이 뒤로 가면 끄고 앞으로 오면 다시 켭니다. 끄면 칸 단위로는 켜지 않고 `toggle_secure_input`도 아무 일이 없습니다. 자물쇠 커서는 이 값과 상관없이 그려집니다. ssh 너머의 암호는 알아채지 못합니다(Ghostty도 같음). 앱 메뉴 `보안 키보드 입력`(앱 전체로 켬, 다음 실행에도 남음)은 이 값과 상관없습니다. macOS 밖에서는 자물쇠 커서만 있습니다 |
| `macos-secure-input-indication` | `true` / `false` | `true` | 보안 입력이 켜져 있는 동안 창 오른쪽 위에 자물쇠 표시를 그립니다(누르면 설명이 뜨는 것은 없음) |
| `cursor-style` | `block` `bar` `underline` `block_hollow` | `block` | 프로그램이 바꾸면 그쪽이 이깁니다. 암호 입력 중에는 자물쇠입니다 |
| `cursor-style-blink` | `true` / `false` | 끔 | 0.6초마다 깜빡입니다. 프로그램이 DECSCUSR(`ESC[1 q` 등)로 바꾸면 그쪽을 따르고, 키를 치는 동안과 창을 보지 않을 때는 깜빡이지 않습니다 |
| `window-padding-x` / `window-padding-y` | `2` 또는 `왼,오른`(`위,아래`) | `2` | 포인트 |
| `window-padding-balance` | `true` `false` | `false` | 창 크기가 칸 크기로 나누어떨어지지 않아 남는 자리를 네 변에 고르게 나눕니다(끄면 오른쪽·아래에 몰림) |
| `window-padding-color` | `background` `extend` `extend-always` | `background` | 여백 색. `extend`는 가장 가까운 칸의 바탕색으로 늘이되, 주 화면에서 가장 가까운 줄에 기본 바탕색 칸·프롬프트·powerline 꼭 맞춤 글자가 있으면 위아래로는 늘이지 않습니다(Ghostty와 같은 규칙). `extend-always`는 따지지 않고 늘입니다 |
| `window-width` / `window-height` | 셀 수 | 없음 | 둘 다 있을 때 처음 창 크기 |
| `maximize` | `true` `false` | `false` | 새 창을 최대화로 띄웁니다(Ghostty처럼 첫 창만이 아니라 새 창 모두. 탭·분할·퀵 터미널 창은 해당 없음). 창 상태 복원으로 되살린 창은 복원한 크기가 이깁니다 |
| `fullscreen` | `true` `false` `non-native` `non-native-visible-menu` `non-native-padded-notch` | `false` | 새 창을 전체 화면으로 띄웁니다. `maximize`와 같이 켜면 전체 화면이 이깁니다. `non-native*` 값은 전체 화면을 켜면서 `macos-non-native-fullscreen`을 그 값으로 정합니다. 퀵 터미널 창과 되살린 창에는 적용하지 않습니다 |
| `macos-non-native-fullscreen` | `false` `true` `visible-menu` `padded-notch` | `false` | macOS에서 전체 화면을 새 Space 없이 합니다(winit simple fullscreen). 켜면 `toggle_fullscreen`과 `fullscreen`이 이 방식을 씁니다. 메뉴 막대와 Dock은 마우스를 대면 나타나는 자동 숨김입니다. `visible-menu`와 `padded-notch`는 `true`와 구분하지 못합니다(메뉴 막대를 늘 보이게 하거나 노치를 피하는 일은 안전한 API만으로는 못 합니다). 이미 전체 화면인 창은 끌 때까지 들어간 방식을 따릅니다. macOS 밖에서는 하는 일이 없습니다 |
| `window-step-resize` | `true` `false` | `false` | 창 크기를 칸 크기 단위로 바꿉니다(`NSWindow` 내용 크기 증분). 끌어서 바꿀 때만 적용되고 여백·탭 줄 높이는 단위에 넣지 않아 끝 칸이 조금 어긋날 수 있습니다. 글꼴 크기를 바꾸면 다음 크기 변경 때 새 칸 크기로 맞춥니다 |
| `macos-window-buttons` | `visible` `hidden` | `visible` | 제목 막대의 닫기·최소화·확대 단추를 숨깁니다. 새 창부터 적용합니다(퀵 터미널 창은 테두리가 없어 해당 없음) |
| `window-decoration` | `auto` `client` `server` `none` `true` `false` | `auto` | `none`(또는 `false`)이면 제목 막대·테두리 없는 창을 만들고, 켜 둔 창도 설정을 다시 읽을 때 바뀝니다(퀵 터미널 창은 늘 테두리가 없음). `auto`·`client`·`server`(`true`)는 모두 기본 꾸밈입니다. 제목 막대만 감추고 틀은 두려면 `macos-titlebar-style = hidden`을 씁니다 |
| `macos-titlebar-style` | `native` `transparent` `tabs` `hidden` | `transparent` | `native`는 보통 제목 막대, `transparent`는 제목 막대가 투명해 바탕색이 비치고 탭 줄·격자는 그 밑에서 시작합니다. `tabs`는 b-terminal이 탭 막대를 직접 그리므로 `transparent`와 같습니다(Ghostty의 제목 막대 속 탭 막대는 아님). `hidden`은 둥근 틀은 두고 제목 막대와 신호등 단추를 감춥니다(창 끌기는 되지 않을 수 있음). 새 창부터 적용합니다. 제목 막대 높이는 28pt로 가정한 재지 않은 값입니다 |
| `macos-window-shadow` | `true` `false` | `true` | macOS 창 그림자. 설정을 다시 읽을 때 열린 창에도 적용합니다 |
| `window-theme` | `auto` `system` `light` `dark` `ghostty` | `auto` | 창(제목 막대·신호등 단추)의 밝음·어두움. `auto`는 바탕색 밝기로 정하되(macOS에서는 `transparent`·`tabs`일 때만) `theme = light:…,dark:…`이면 `system`과 같습니다. `ghostty`는 Linux 전용 값이라 `auto`처럼 봅니다`system`이거나 `light:…,dark:…`와 함께인 `auto`는 시스템 모양을 따라 바뀝니다. `light`·`dark`(또는 `set_window_theme`)로 고정한 창은 시스템 모양을 따르지 않으므로 그때는 색 테마도 다시 고르지 않습니다 |
| `macos-titlebar-proxy-icon` | `visible` `hidden` | `visible` | 받기만 합니다. 제목 막대 폴더 아이콘은 `NSWindow`를 얻어야 해서(winit이 노출하지 않음) `unsafe`가 필요합니다 |
| `macos-hidden` | `never` `always` | `never` | `always`면 Dock과 앱 전환기에서 앱을 숨깁니다(`NSApplication` 활성화 정책 Accessory). 다시 읽으면 바로 적용합니다. 메뉴 막대는 앱이 앞에 있을 때만 보입니다 |
| `window-subtitle` | `false` `working-directory` | `false` | 받기만 합니다. Ghostty에서도 GTK 전용 키이고, macOS의 창 부제목은 winit이 노출하지 않아 safe로 정할 수 없습니다 |
| `window-titlebar-background` `window-titlebar-foreground` | 색 | (없음) | 받기만 합니다. Ghostty에서도 GTK 앱 런타임 전용이고, macOS 제목 막대 색은 winit·objc2의 safe 함수로 정할 수 없습니다 |
| `window-position-x` / `window-position-y` | 정수(점) | 없음 | 첫 창의 자리. 주 화면의 보이는 영역(macOS는 메뉴 막대 아래) 왼쪽 위에서 잰 값이고, 둘 다 있어야 쓰며 하나만 있으면 무시합니다(Ghostty와 같음). 창 상태 복원으로 되살린 창은 복원한 자리가 이깁니다 |
| `macos-option-as-alt` | `true` `false` `left` `right` | `false` | Ghostty 기본과 같게 끔 |
| `key-remap` | `from=to` (예 `ctrl=super`, `left_alt=right_ctrl`), 여러 번 쓸 수 있음 | 없음 | 앱 안에서만 수정 키를 바꿉니다. 이름은 `shift` `ctrl`/`control` `alt`/`option` `super`/`cmd`/`command`, 앞에 `left_`·`right_`를 붙이면 그쪽만입니다(좌우 없는 이름은 양쪽). 키 묶음 판단과 PTY로 보내는 인코딩이 모두 바뀐 키를 씁니다. 한 방향이고 이어서 바뀌지 않습니다(`ctrl=super`, `alt=ctrl`이면 Alt는 Ctrl이 될 뿐 Super가 아닙니다). 키보드 배치와 OS가 만드는 글자는 그대로라 `option+a`가 `å`를 만드는 것은 달라지지 않습니다. 메뉴 막대의 Cmd 단축키는 macOS가 먼저 처리해 바뀌지 않습니다. 좌우는 macOS의 winit이 장치별 비트로 알려 주는 것을 씁니다(그 밖의 OS는 좌우를 모르면 왼쪽으로 봅니다). 빈 값은 지금까지 쌓은 것을 지웁니다. `option-as-alt`는 진짜 Option 키에만 걸리고, 바꾸기로 생긴 Alt는 늘 Alt입니다 |
| `macos-icon` | `official` `custom` `custom-style`, 내장 이름(`blueprint` `chalkboard` `microchip` `glass` `holographic` `paper` `retro` `xray`) | `official` | `custom`일 때만 `macos-custom-icon`의 그림으로 바꿉니다. 내장 이름과 `custom-style`은 b-terminal에 그 그림이 없어 받기만 합니다. `NSApplication.setApplicationIconImage`가 objc2에서 `unsafe`라 쓰지 못해 Dock 아이콘(`NSDockTile`)에만 올립니다. Cmd-Tab과 Finder의 아이콘은 그대로입니다. 실제 Dock에서는 아직 확인하지 않았습니다 |
| `macos-custom-icon` | 그림 파일 경로(png·jpeg·icns) | 없음 | `macos-icon = custom`일 때 쓸 그림. `~/`는 홈, 상대 경로는 설정 파일이 있는 폴더 기준입니다(Ghostty의 `~/.config/ghostty/Ghostty.icns` 기본값은 없습니다). 읽지 못하면 로그에 경고하고 아이콘을 그대로 둡니다. 빈 값은 지움 |
| `scrollback-limit-bytes` | 바이트, `unlimited` | 1000만 | Ghostty 1.4 이름. VT 엔진은 줄 수 상한만 있어 줄 하나를 약 1KB로 보고 줄 수로 바꿉니다(1000만이면 1만 줄, 100줄 이상). 옛 이름 `scrollback-limit`도 같은 뜻으로 받습니다. Ghostty 1.4 소스의 기본은 5000만이지만 스크롤백 압축과 함께 올린 값이라 따르지 않습니다 |
| `scrollback-limit-lines` | 줄 수, `unlimited` | `unlimited` | Ghostty 1.4 값. 바이트 상한과 둘 다 있으면 먼저 닿는 쪽(작은 쪽)이 이깁니다. 둘 다 `unlimited`여도 100만 줄에서 멈춥니다 |
| `scrollback-lines` | 줄 수 | | b-terminal 옛 키. 정확히 그 줄 수로 두고 바이트 상한은 풉니다(뒤에 쓴 `scrollback-limit-*`가 이김) |
| `copy-on-select` | `true` `false` `clipboard`(Ghostty 1.4 이름 `none` `primary` `both`도 받음) | `true` | 고르기를 마치면 클립보드로. macOS에는 선택 클립보드가 없어 `true`·`clipboard`·`both`는 같고(시스템 클립보드), `false`·`none`·`primary`면 복사하지 않습니다. Ghostty 1.4(아직 나오지 않은 판)는 macOS 기본을 `none`으로 바꿨지만, b-terminal은 지금 나온 판의 기본(켬)을 따릅니다 |
| `clipboard-trim-trailing-spaces` | `true` / `false` | `true` | 복사하는 글(⌘C·`copy_to_clipboard`·고르면 복사)의 줄 끝 공백과 탭을 지웁니다. 줄 가운데 공백은 그대로이고, 프로그램이 OSC 52로 쓰는 글에는 적용하지 않습니다 |
| `right-click-action` | `context-menu` `paste` `copy` `copy-or-paste` `ignore` | `context-menu` | 오른쪽 클릭 동작. `copy-or-paste`는 고른 글이 있으면 복사, 없으면 붙여넣기(붙여넣기 보호를 거칩니다). `context-menu`는 값만 받고 하는 일이 없습니다(메뉴가 없음). 마우스 보고 모드의 프로그램에는 이 키와 상관없이 클릭을 보냅니다 |
| `mouse-reporting` | `true` / `false` | `true` | 끄면 프로그램이 마우스 보고를 켜도 클릭·움직임·휠을 보내지 않고 고르기·스크롤이 받습니다 |
| `mouse-shift-capture` | `false` `true` `always` `never` | `false` | Shift를 누른 클릭을 마우스 보고 프로그램이 받을지 정합니다. `false`·`never`는 고르기가 받고, `true`·`always`는 프로그램이 받습니다. 프로그램이 바꾸는 XTSHIFTESCAPE는 받지 않아 `true`는 `always`, `false`는 `never`와 같게 동작합니다 |
| `middle-click-action` | `primary-paste` `clipboard-paste` `ignore` | `primary-paste` | 가운데 클릭 동작. `primary-paste`는 선택 클립보드 붙여넣기인데 macOS에는 없어 하는 일이 없습니다. `clipboard-paste`는 시스템 클립보드를 붙여넣습니다(붙여넣기 보호를 거칩니다). 마우스 보고 모드의 프로그램에는 이 키와 상관없이 클릭을 보냅니다 |
| `click-repeat-interval` | 밀리초(정수) | `0` | 두 번·세 번 누르기로 치는 간격. `0`이면 macOS는 OS 설정(`NSEvent.doubleClickInterval`), 그 밖에는 500ms |
| `window-inherit-working-directory` / `tab-inherit-working-directory` / `split-inherit-working-directory` | `true` `false` | `true` | 새 창·탭·분할이 고른 칸의 작업 폴더를 이어받습니다(Ghostty와 같음). 켜져 있으면 `working-directory`보다 앞서고, 끄면 `working-directory`를 씁니다. 이어받을 폴더를 알 수 없으면 `working-directory`입니다. 전에는 `working-directory = inherit`일 때만 이어받았는데, 이제 `home`·경로여도 이어받습니다 |
| `window-inherit-font-size` | `true` `false` | `true` | 값만 받고 하는 일이 없습니다. b-terminal은 글자 크기가 앱 전체 하나라 새 창은 늘 지금 크기(⌘+/⌘-로 바꾼 값)를 씁니다. `false`(새 창은 `font-size`로)를 지키려면 창마다 글자 크기를 두어야 해서 이번에는 하지 않았습니다 |
| `window-new-tab-position` | `current` `end` | `current` | 새 탭을 둘 자리. `current`는 지금 탭 바로 뒤, `end`는 맨 끝. 전에는 늘 맨 끝이었습니다 |
| `window-show-tab-bar` | `auto` `always` `never` | `auto` | 탭 막대를 언제 보일지. `auto`는 탭이 둘 이상일 때만, `always`는 하나여도, `never`는 안 보임(탭은 키·팔레트로 다룸). 막대가 없는 동안 진행 막대는 칸 맨 위에 그립니다 |
| `mouse-hide-while-typing` | `true` / `false` | `false` | |
| `command` | 명령 | 로그인 셸 | `/bin/sh -c "exec <명령>"`으로 띄웁니다. 모든 새 칸에 쓰고, 첫 칸은 아래 `initial-command`가 이깁니다 |
| `initial-command` | 명령 | 없음 | 앱을 켤 때 처음 만드는 칸에만 쓰는 명령(Ghostty와 같음, 형식은 `command`와 같음). 이후 칸은 `command`를 씁니다. 명령줄 `-e`가 있으면 `-e`가 이기고, 창 상태 복원(`window-save-state`)은 하지 않습니다 |
| `wait-after-command` | `true` / `false` | `false` | 명령이 끝나도 칸을 닫지 않고 안내를 그린 채 남깁니다. 수정 키가 아닌 키를 아무거나 누르면 닫습니다. 스크립트·디버깅용입니다 |
| `abnormal-command-exit-runtime` | 밀리초 정수 | `250` | 칸을 띄운 지 이 시간보다 짧게 끝나면 비정상으로 보고, `wait-after-command`가 꺼져 있어도 칸을 남겨 안내를 그립니다(명령을 못 찾았을 때 칸이 깜빡 사라지지 않게). Ghostty처럼 macOS는 어떤 종료 코드든, Linux는 0이 아닐 때만 봅니다. `0`이면 끕니다. `btctl new --command`로 띄운 칸에도 같습니다 |
| `initial-window` | `true` / `false` | `true` | `false`면 켤 때 창을 만들지 않습니다. 앱은 메뉴 막대에 남고(Dock 아이콘을 눌러도 창이 열리지는 않습니다) 메뉴(새 창)·`btctl new`·전역 단축키(퀵 터미널)로 창을 엽니다. 명령줄 `-e`로 켠 경우는 창을 만듭니다. `window-save-state`로 되살리는 것도 하지 않습니다 |
| `quit-after-last-window-closed` | `true` / `false` | macOS `false`, Linux `true` | 마지막 창을 닫으면 앱을 끝냅니다. macOS 기본이 `false`라 마지막 창을 닫아도 앱이 남고(메뉴로 새 창), ⌘Q로 끝냅니다. 명령줄 `-e`로 켜면 이 값을 `true`로 두고 `quit-after-last-window-closed-delay`는 지웁니다 |
| `quit-after-last-window-closed-delay` | 기간(`30s`, `1m` 등) | 없음 | 위가 `true`일 때 마지막 창을 닫고 이만큼 기다렸다가 끝냅니다. 그 사이 창을 열면 취소합니다. 1초보다 짧으면 1초로 올립니다. 없으면 바로 끝냅니다. `initial-window = false`로 켠 뒤 창이 하나도 안 열려도 이 시간이 지나면 끝냅니다 |
| `working-directory` | `home` `inherit` 경로 | `inherit` | 첫 창의 폴더. 새 창·탭·분할은 위의 `*-inherit-working-directory`가 켜져 있으면 지금 칸의 폴더를 이어받음 |
| `title` | 글 | 셸이 보낸 제목 | 창 제목을 고정 |
| `keybind` | `트리거=동작` | 아래 표 | 아래 "키 묶음" |
| `command-palette-entry` | `title:…,action:…,description:…` | 기본 항목 | 명령 팔레트(Ghostty 1.2)에 항목을 더합니다. 여러 줄이 쌓이고, 빈 값(`command-palette-entry =`)은 기본 항목과 그 앞에 쓴 항목을 지웁니다. 아래 "팔레트 항목" |
| `config-file` | 경로(`?` 붙이면 선택) | | 그 자리에서 읽어 넣음. 8단계까지 |
| `import-ghostty` | `true` / `false` | `false` | b-terminal 키. Ghostty 설정을 옮겨 올 때 |
| `clipboard-write` | `allow` / `deny` / `ask` | `allow` | 프로그램이 OSC 52로 클립보드에 쓰기(AI CLI의 `/copy`). 보고 있는 창의 고른 칸(화면을 나눴으면 키 입력을 받는 칸)에서 온 것만 받습니다. `ask`는 쓸 글 앞부분을 창 안 막대로 보이고 묻습니다(`y` 허용, `n`·Esc 거절, Enter는 허용 아님) |
| `clipboard-read` | `allow` / `deny` / `ask` | `deny` | 프로그램이 OSC 52로 클립보드 읽기. `ask`는 읽어 갈 글 앞부분을 창 안 막대로 보이고 묻습니다(`y` 허용, `n`·Esc 거절, Enter는 허용 아님). Ghostty 기본은 `ask`지만 b-terminal 기본은 `deny`입니다 |
| `shell-integration` | `detect` `none` `zsh` `bash` | `detect` | 탭의 셸에 셸 통합을 싣습니다(아래 "셸 통합") |
| `shell-integration-features` | `cursor` `sudo` `title` `ssh-env` `ssh-terminfo` `path`(쉼표로, 앞에 `no-`면 끔), `true` `false` | `cursor,title,path` | Ghostty 키. 셸 통합의 기능별 켜고 끄기. 적지 않은 기능은 기본값을 따르고, `true`·`false`는 모두 켜고 끕니다. 아래 "셸 통합 기능" |
| `notify-command-seconds` | 초 | `10` | b-terminal 키(옛 키). 셸 통합으로 안 명령이 이만큼 넘게 걸려 끝나면 창을 보지 않을 때 데스크톱 알림. `0`이면 끔. `notify-on-command-finish`를 적으면 무시합니다 |
| `notify-on-command-finish` | `never` `unfocused` `always` | 적지 않음 | Ghostty 키. 명령이 끝났을 때 알릴 때. `unfocused`는 그 칸을 보고 있지 않을 때만. 적으면 아래 두 키를 따르고 `notify-command-seconds`는 무시합니다. 적지 않으면 옛 `notify-command-seconds` 동작(10초 넘게, 창을 보지 않을 때 데스크톱 알림만)입니다. Ghostty 기본은 `never`인데 b-terminal은 이미 있던 동작을 지키려고 이렇게 둡니다. 셸 통합(OSC 133)이 있어야 합니다 |
| `notify-on-command-finish-action` | `bell` `notify`(쉼표로, 앞에 `no-`면 끔), `true` `false` | `bell` | Ghostty 키. `bell`은 벨이 울린 것처럼(`bell-features`를 따름), `notify`는 데스크톱 알림 |
| `notify-on-command-finish-after` | 기간(`5s`, `1m30s`) | `5s` | Ghostty 키. 명령이 이만큼 이상 걸려야 알립니다 |
| `desktop-notifications` | `true` `false` | `true` | Ghostty 키. 프로그램의 OSC 9·777 데스크톱 알림을 받을지. `false`면 버립니다(명령 끝 알림과 `btctl notify`는 그대로) |
| `progress-style` | `true` `false` | `true` | Ghostty 키. 프로그램의 OSC 9;4 진행 표시를 그릴지. `false`면 조용히 버립니다(탭 줄·칸 위 막대 모두 안 나옵니다) |
| `scrollbar` | `system` `never` | `system` | Ghostty 키. `system`은 스크롤하는 동안과 그 뒤 1.2초 동안, 또는 마우스가 칸 오른쪽 끝 띠 위에 있을 때 오른쪽에 얇은 반투명 막대를 그립니다(스크롤백이 있을 때만). `never`는 그리지 않습니다. 오른쪽 끝 띠에서 손잡이를 끌면 끄는 만큼 스크롤하고, 손잡이 밖 빈 곳을 누르면 그쪽으로 한 쪽(보이는 줄 수에서 한 줄을 뺀 만큼)씩 옮깁니다(누르고 있는 동안은 마우스를 프로그램에 보고하지 않고 글도 고르지 않으며, 막대가 있을 때만: 스크롤백이 없거나 대체 화면이면 평소 마우스). 분할 칸이면 칸마다 오른쪽 끝에 막대가 있고, 마우스로 분할 경계를 끄는 기능은 없어서 겹치는 일이 없습니다. 시스템 설정 "스크롤 막대 클릭 시"와 "스크롤 막대 항상 보기"는 읽지 않습니다(읽으려면 unsafe가 필요해서) |
| `split-preserve-zoom` | `navigation`, `no-navigation` | `no-navigation` | Ghostty 키(1.3). `navigation`이면 칸을 크게 보는 중에 `goto_split`으로 옮겨도 크게 보기를 풀지 않고 새 칸을 크게 봅니다. 마우스로 칸을 누르거나 새로 나누기·닫기는 여전히 풉니다 |
| `app-notifications` | `clipboard-copy` `config-reload`(쉼표로, 앞에 `no-`면 끔), `true` `false` | `true` | Ghostty 키. 앱 자신의 알림 띠: 클립보드에 복사했을 때(OSC 52 쓰기·경로·링크 주소 복사), 설정을 다시 읽었을 때. 설정 경고가 있으면 `config-reload`를 꺼도 알립니다. Ghostty 문서는 GTK 전용이라고 하지만 b-terminal은 같은 띠를 macOS에서 씁니다 |
| `bell-features` | `system` `attention` `title` `audio`(쉼표로, 앞에 `no-`면 끔), `true` `false` | `attention,title` | 프로그램이 벨(BEL)을 울리면: `title`은 보지 않는 칸의 탭·창 제목 앞에 🔔를 붙이고 그 칸을 보면 뗍니다. `attention`은 창이 앞에 없을 때 Dock 아이콘을 튀깁니다. `system`은 시스템 경고음입니다(0.1초에 한 번까지). `audio`는 `bell-audio-path`의 소리 파일을 재생합니다(macOS만, 보고 있어도 납니다. 앞 소리가 아직 나는 중이거나 0.1초 안이면 건너뜁니다). Ghostty의 `border`는 받지만 하는 일이 없습니다 |
| `bell-audio-path` | 파일 경로 | 없음 | `bell-features`에 `audio`가 있을 때 재생할 소리 파일(macOS가 읽을 수 있는 wav·aiff·mp3 등). `~/`는 홈, 상대 경로는 설정 파일이 있는 폴더 기준입니다. 파일을 읽지 못하면 소리 없이 지나갑니다. 빈 값은 지움 |
| `bell-audio-volume` | 0.0~1.0 | `0.5` | `audio`로 재생할 크기(시스템 볼륨 대비). 범위 밖은 0.0~1.0으로 자릅니다 |
| `clipboard-paste-protection` | `true` `false` | `true` | 괄호 붙여넣기를 쓰지 않는 프로그램에 줄바꿈(LF·CR)이 든 글을 붙여넣으면 먼저 묻습니다(줄바꿈이 바로 Enter가 되어 보지 못한 명령이 돌 수 있어서). ↩·`y` 붙여넣기, esc·`n` 버리기 |
| `clipboard-paste-bracketed-safe` | `true` `false` | `true` | 괄호 붙여넣기 모드의 붙여넣기는 안전으로 봅니다. `false`면 그때도 줄바꿈이 있으면 묻습니다 |
| `confirm-close-surface` | `true` `false` `always` | `true` | 칸·탭·창을 닫거나 끝낼 때(⌘W, ⌥⌘W, 창 닫기 단추, ⌘Q) 프로그램이 도는 칸이 있으면 창 위쪽에 확인 막대를 띄웁니다. ↩·`y`면 닫고 esc·`n`이면 그만둡니다. 셸 통합이 있으면 명령이 도는 중인지로, 없으면 전경 프로세스가 셸 자신인지로 봅니다. `always`는 셸이 살아 있으면 늘 묻습니다. Dock 메뉴나 로그아웃으로 끝낼 때는 묻지 못합니다 |
| `resize-overlay` | `always` `never` `after-first` | `after-first` | 칸의 칸·줄 수가 바뀌면 칸 안에 `80 × 24`를 보입니다. `after-first`는 칸을 만든 직후(0.5초 안)의 크기 맞추기에는 보이지 않습니다 |
| `resize-overlay-position` | `center` `top-left` `top-center` `top-right` `bottom-left` `bottom-center` `bottom-right` | `center` | 크기 표시 자리 |
| `resize-overlay-duration` | 기간(`750ms`, `2s`, `1m30s`; 단위 `y` `d` `h` `m` `s` `ms` `us` `ns`) | `750ms` | 크기 표시가 보이는 시간 |
| `undo-timeout` | 기간(`5s`, `1m30s` 등 `resize-overlay-duration`과 같은 문법) | `5s` | 닫은 칸·탭을 되살릴 수 있는 시간(Ghostty와 같은 뜻). 칸·탭을 직접 닫으면(⌘W, ⌥⌘W) 셸을 바로 끝내지 않고 이 시간 동안 들고 있다가 `undo`(⌘Z, ⇧⌘T)로 원래 자리에 되살립니다. 시간이 지나면 그때 셸이 끊깁니다. 일마다 따로 셉니다. `0s`면 끕니다(바로 끊음). 셸이 스스로 끝나 닫힌 칸·창 닫기·앱 끝내기는 되살리지 않습니다. 들고 있는 동안 셸은 계속 돌며 출력을 받습니다 |
| `minimum-contrast` | `1`~`21` | `1` | 글자색과 바탕의 명암비(WCAG)가 이보다 낮으면 글자를 흰색이나 검은색(바탕과 명암비가 큰 쪽)으로 그립니다. `1`이면 끔. 3이면 대개 읽힙니다 |
| `focus-follows-mouse` | `true` `false` | `false` | 화면을 나눴을 때 마우스가 올라간 칸을 고릅니다(끌어 고르는 중에는 바꾸지 않음) |
| `mouse-scroll-multiplier` | 수, `discrete:수`, `precision:수`(쉼표로 둘) | `discrete:3,precision:1` | 휠 배율. `discrete`는 줄 단위 기기(마우스 휠), `precision`은 픽셀 단위 기기(트랙패드). 접두 없는 수는 둘 다. 0.01~10000 |
| `selection-word-chars` | 글자들(`\t`·`\\` 풂) | `` \t'"│`\|:;,()[]{}<>$``(맨 앞 공백 포함) | 두 번 눌러 단어를 고를 때 끊는 글자. 공백을 넣으려면 큰따옴표로 감쌉니다 |
| `quick-terminal-position` | `top` `bottom` `left` `right` `center` | `top` | 퀵 터미널이 붙는 가장자리. 메뉴 막대와 Dock을 뺀 영역에 놓습니다 |
| `quick-terminal-size` | `40%`, `300px`, 둘을 쉼표로(`50%,800px`) | `40%` | 첫 값은 주축(위·아래면 높이, 왼쪽·오른쪽·가운데면 폭), 둘째 값은 다른 축(없으면 위·아래·왼쪽·오른쪽은 끝까지, 가운데는 첫 값과 같게). 단위가 없으면 잘못된 값입니다. px는 화면의 점 단위입니다 |
| `quick-terminal-screen` | `main` `mouse` `macos-menu-bar` | `main` | 퀵 터미널이 뜨는 화면: 앞에 있는 창의 화면, 마우스가 있는 화면, 메뉴 막대가 있는 화면 |
| `quick-terminal-autohide` | `true` `false` | `true` | 포커스를 잃으면 숨습니다 |
| `quick-terminal-animation-duration` | 초 | `0.2` | 들어오고 나가는 움직임의 길이. `0`이면 움직이지 않습니다. 가운데(`center`)는 움직이지 않습니다 |
| `quick-terminal-space-behavior` | `move` `remain` | `move` | Space(데스크톱)를 옮길 때 퀵 터미널이 따라오는가(`move`: 지금 Space로 따라옴, `remain`: 처음 연 Space에 남음). 퀵 창이 포커스를 받을 때 `NSWindow.collectionBehavior`를 정합니다(macOS). 실제 창에서는 아직 확인하지 않았습니다 |
| `window-save-state` | `default` `always` `never` | `default` | 앱을 끌 때(Cmd-Q) 창 크기·자리, 탭, 분할(비율), 칸의 작업 폴더, 사람이 붙인 탭·창·칸 제목(`set_tab_title`·`set_window_title`·`set_surface_title`과 `prompt_*`)을 적어 두고 다음에 켤 때 새 셸로 되살립니다. 프로그램이 OSC로 정한 제목, 돌던 프로그램, 화면 내용은 되살리지 않습니다. 제목이 없던 옛 상태 파일도 읽고, 읽은 제목은 제어 문자를 지우고 200자로 자릅니다. 마지막 창을 닫아 꺼진 경우와 `-e`·`--cwd`로 띄운 경우는 되살리지 않습니다 |
| `background-opacity` | `0`~`1` | `1` | 기본 바탕색 칸으로 창 뒤가 비칩니다(글자·색 칸은 불투명). 새 창부터 따릅니다 |
| `background-blur` | `false` / `true` / `0`~`255` | `false` | macOS만. `background-opacity`가 1보다 작을 때 창 뒤를 흐립니다. `true`는 20, 정수는 세기이고 그대로 흐림 반지름이 됩니다(Ghostty와 같음). 높을수록 흐리지만 너무 높으면 어색하게 그려질 수 있습니다. `background-opacity`가 1 이상이면 흐림이 보이지 않아 창에 걸지 않습니다. 새 창과 설정을 다시 읽을 때 따르고 끄기(`false`·`0`)도 다시 읽을 때 적용됩니다. 퀵 터미널 창도 같습니다. macOS 26 유리 값 `macos-glass-regular`·`macos-glass-clear`는 받되 유리 효과는 없고 `true`(20)로 봅니다. 옛 이름 `background-blur-radius`도 같은 키로 받습니다 |
| `background-opacity-cells` | `true` / `false` | `false` | Ghostty 1.2 값. 켜면 `background-opacity`를 색이 있는 바탕 칸(프로그램이 바탕색을 칠한 칸)에도 적용합니다. 끄면 기본 바탕색 칸만 비칩니다. 고른 영역·커서·탭 막대는 그대로입니다 |
| `link-url` | `true` / `false` | `true` | 화면 글의 URL을 Cmd(Linux는 Ctrl)를 누른 채 위에 있으면 링크로 찾습니다. `false`면 글의 URL은 링크가 아닙니다. OSC 8 링크는 `link-osc8`이 따로 정합니다 |
| `link-osc8` | `true` / `false` | `true` | Ghostty 1.4 키. `false`면 OSC 8 하이퍼링크를 강조·주소 띠·복사·열기 모두 하지 않고 보통 글로 둡니다 |
| `link-previews` | `true` / `false` / `osc8` | `true` | Cmd를 누른 채 링크 위에 있을 때 창 왼쪽 아래에 실제 주소 띠를 보입니다. `false`면 띠가 없고(밑줄과 열기는 그대로), `osc8`이면 OSC 8 링크에만 보입니다. `false`는 열기 전에 주소를 보는 보장을 약하게 합니다(SECURITY.md) |
| `link` | 정규식 | 없음 | Ghostty에서도 아직 쓸 수 없는 키입니다. 받기만 하고 경고를 남기며 링크를 더하지 않습니다 |
| `palette-generate` | `true` / `false` | `false` | Ghostty 1.3 값. 16~255번 색을 기본 16색과 바탕·글자색에서 만듭니다(CIELAB 3선형 보간 6x6x6 정육면체와 회색 24단계). `palette = 100=#...`로 직접 정한 번호는 덮어쓰지 않습니다. 끄면 xterm 표 그대로입니다 |
| `palette-harmonious` | `true` / `false` | `false` | Ghostty 1.3 값. `palette-generate`일 때, 밝은 테마(글자색이 바탕보다 어두움)에서 바탕·글자색을 맞바꾸지 않습니다. 기본(끔)은 맞바꿔서 16번이 어둡고 231번이 밝은 옛 순서를 지킵니다. 어두운 테마에서는 차이가 없습니다. `palette-generate`가 꺼져 있으면 아무 일도 하지 않습니다 |
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

경고 없이 넘깁니다: `auto-update`, `gtk-titlebar` 등. 그 밖의 키는 경고합니다. 값은 확인하지 않고 그대로 넘깁니다. 0.13(#196)에서 더한 것과 이유:

| 키 | 이유 |
|---|---|
| `macos-applescript` `macos-shortcuts` | AppleScript 사전과 단축어(App Intents)는 앱 꾸러미와 Objective-C 클래스 정의가 필요한데 지금 b-terminal에는 그 기능이 없습니다 |
| `macos-dock-drop-behavior` | Dock 아이콘에 파일·폴더를 놓으면 앱 델리게이트의 `application:openURLs:`가 불리는데 winit 0.30은 그 이벤트를 주지 않습니다. 창에 놓은 파일(`DroppedFile`)만 옵니다 |
| `macos-icon-frame` `macos-icon-ghost-color` `macos-icon-screen-color` | `macos-icon = custom-style` 전용입니다. 그 그림 틀(유령·화면 그라데이션)이 b-terminal에 없습니다 |
| `vt-xt-checksum-report` `vt-xt-checksum-extension` | DECRQCRA(화면 영역 체크섬 보고)는 구현하지 않았습니다. 프로그램이 칸 하나씩 체크섬을 물어 화면 전체(다른 프로그램의 출력 포함)를 읽어 갈 수 있는 길이라, 구현할 때는 기본을 끄고 ADR·SECURITY.md에 적어야 합니다. 구현하지 않으니 지금은 어떤 값을 줘도 체크섬 요청에 답하지 않습니다(Ghostty의 기본과 같습니다) |
| `class` `x11-instance-name` `language` | GTK·X11 전용입니다 |
| `gtk-custom-css` `gtk-horizontal-tab-scroll` `gtk-opengl-debug` `gtk-quick-terminal-layer` `gtk-quick-terminal-namespace` `gtk-tabs-inverted` `gtk-tabs-location` `gtk-titlebar-hide-when-maximized` `gtk-titlebar-style` `gtk-toolbar-style` `gtk-wide-tabs` | GTK 전용입니다(`gtk-titlebar`·`gtk-single-instance`는 앞서 받기만 하던 것) |
| `linux-cgroup` `linux-cgroup-memory-limit` `linux-cgroup-processes-limit` `linux-cgroup-hard-fail` `async-backend` | 리눅스 전용입니다(cgroup, epoll·io_uring 고르기). macOS는 kqueue 하나입니다 |
| `quick-terminal-keyboard-interactivity` | Wayland 레이어 셸 전용입니다 |
| `custom-shader` `custom-shader-animation` `alpha-blending` `freetype-load-flags` `window-vsync` | Ghostty의 GPU 그리기(셰이더, 선형 알파, FreeType 옵션, vsync)에 대한 것입니다. b-terminal은 CPU로 그려 그 단계가 없습니다 |
| `scrollback-compression` | Ghostty 화면 엔진(페이지 메모리 압축)의 옵션입니다. b-terminal의 VT 엔진에는 그 단계가 없습니다 |
| `drag-handle` | 마우스로 칸을 끌어 옮기는 손잡이입니다. b-terminal에는 그 기능이 없습니다 |

## 팔레트 항목

`command-palette-entry`는 Ghostty 1.2와 같은 문법입니다. `title`과 `action`은 꼭 있어야 하고 `description`은 고릅니다. 필드는 쉼표로 나누고 필드 사이 공백은 무시합니다. `action`은 `keybind`의 동작과 같은 글입니다. 값에 쉼표가 있으면 큰따옴표로 감쌉니다(`\"` `\\` `\n` `\t` 이스케이프).

```
command-palette-entry = title:Reset Font Style, action:reset_font_size
command-palette-entry = title:"Git, 상태", action:"text:git status\r", description:셸에 보내기
command-palette-entry =                # 기본 항목을 모두 지우기(이 줄 뒤에 쓴 항목만 남음)
```

- 사용자 항목은 기본 항목 뒤에 붙습니다. 팔레트에서는 `title`로 찾고, 동작 이름과 `description`으로도 찾습니다. 기본 항목의 이름은 바꾸지 않습니다.
- 읽지 못하는 줄과 풀지 못하는 동작은 경고를 남기고 그 항목만 건너뜁니다. `unbind`는 항목의 동작이 될 수 없습니다.

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
keybind = ctrl+b=activate_key_table:nav  # 키 테이블: ctrl+b를 누르면 nav 테이블을 켬
keybind = nav/h=goto_split:left
keybind = nav/escape=deactivate_key_table
```

- 수식 키: `super`(cmd) `ctrl` `alt`(opt) `shift`. 키 이름은 Ghostty 이름: `a`~`z`, `digit_1`(또는 `1`), `equal`, `minus`, `comma`, `bracket_left`, `bracket_right`, `arrow_up`, `page_up`, `home`, `enter`, `f1` …
- 키 순서(`ctrl+a>n`, Ghostty와 같은 문법): 앞 키를 누르면 다음 키를 기다립니다. 시간 제한은 없습니다. 순서에 묶이지 않은 키를 누르면 그때까지 누른 키를 모두 프로그램에 그대로 보냅니다(`ctrl+a` `x` → `^A` `x`). 겹치는 묶음은 나중 것이 이깁니다(`ctrl+a=…` 뒤에 `ctrl+a>n=…`을 쓰면 `ctrl+a`는 앞 키가 됩니다). 명령 팔레트에는 키 순서를 보이지 않습니다.
- 접두사(Ghostty와 같은 문법, 겹쳐 쓸 수 있고 순서는 상관없음: `unconsumed:performable:ctrl+c=copy_to_clipboard`):
  - `unconsumed:` 동작을 하고 그 키도 프로그램에 보냅니다(기본은 키를 먹음). 키 순서의 마지막 키에서는 그 키만 보내고, 앞 키는 이미 순서에 먹힌 것으로 봅니다.
  - `performable:` 동작을 할 수 있을 때만 키를 먹습니다. 할 수 없으면 묶이지 않은 키처럼 프로그램에 보냅니다. 할 수 없는 때: `copy_to_clipboard`·`adjust_selection`·`write_selection_file`은 고른 글이 없을 때, `paste_from_clipboard`는 클립보드에 글이 없을 때, `copy_url_to_clipboard`는 마우스 아래 링크가 없을 때, `end_search`는 찾기 막대가 닫혀 있을 때. 다른 동작은 늘 할 수 있는 것으로 봅니다.
  - `all:` 모든 칸에 겁니다. b-terminal에서는 `text:`·`csi:`·`esc:`만 열린 모든 칸에 보내고, 나머지 동작은 지금 칸·창에만 합니다. `all:` 묶음은 늘 키를 먹고(`unconsumed:`가 있어도), `performable:`은 효과가 없으며, 키 순서를 쓸 수 없습니다(Ghostty와 같음).
  - `global:`은 위와 같고, 함께 쓴 다른 접두사는 받기만 하고 쓰지 않습니다.
- `catch_all`: 다른 묶음에 걸리지 않은 모든 키에 맞는 트리거입니다(`catch_all=ignore`). `ctrl+catch_all`처럼 수식 키를 붙이면 그 수식 키가 같은 키에 맞고(없으면 수식 키가 붙은 키도 수식 없는 `catch_all`로 넘어감), 키 순서 안에서도 쓸 수 있습니다(`ctrl+a>catch_all=ignore`: `ctrl+a` 다음에 `n` 같은 묶인 키가 아닌 것을 누르면 아무 일도 없이 먹음). 정확히 묶인 키가 늘 먼저이고, `ctrl+a>n` 같은 순서가 있으면 `catch_all`이 있어도 `ctrl+a`는 순서의 앞 키입니다. 순서 중에 묶이지 않은 키가 `catch_all=ignore`에 맞으면 앞 키까지 순서 전체를 버립니다. 수식 키만 누른 것에는 맞지 않습니다.
- `ignore`: 키를 먹고 아무것도 하지 않습니다(프로그램에도 보내지 않음).
- 키 테이블(Ghostty 1.3과 같은 문법): `keybind = 이름/트리거=동작`은 `이름` 테이블에 묶습니다. 테이블은 `activate_key_table:이름`으로 켜기 전에는 쓰지 않습니다(복사 모드·vim 모드 같은 모달 입력용). 이름에는 `/` `=` `+` `>`를 쓸 수 없습니다. 이름 앞의 `/`는 `=` 앞에 있을 때만 나눔 자리라서 `ctrl+/=…`, `/=…`, `x=text:/a`는 그대로 키 묶음입니다. 테이블 안에서도 접두사(`performable:` 등), `catch_all`, 키 순서(`nav/g>g=scroll_to_top`)를 씁니다. `global:`은 테이블 안에서 경고하고 건너뜁니다. `이름/`(묶음 없이)는 그 테이블을 만들고 비웁니다. `keybind = clear`는 테이블도 지웁니다. 한 번에 켜 둘 수 있는 테이블은 Ghostty처럼 8개까지입니다.
  - 키를 찾는 순서는 맨 위 테이블 → 그 아래 테이블 → … → 기본 묶음입니다. 안쪽 테이블에 없는 키는 바깥으로 내려가므로 기본 묶음은 그대로 쓸 수 있고, 안쪽에서 `catch_all=ignore`로 막을 수 있습니다. 키 순서의 앞 키가 잡힌 층에서만 다음 키를 찾고, 순서가 깨져도 테이블은 켜진 채입니다(쌓은 키는 프로그램에 보냅니다).
  - `activate_key_table:이름`: 테이블을 쌓습니다. 없는 테이블이거나 이미 맨 위 테이블이면 아무 일도 없습니다(없는 이름이면 로그에 남김). `A -> B -> A`는 되지만 `A -> B -> B`는 안 됩니다.
  - `activate_key_table_once:이름`: 그 테이블의 묶음을 처음 쓰면(`catch_all` 포함) 저절로 빠집니다. 맨 위일 때만 빠지고, 맨 위가 아니면 맨 위가 될 때까지 그대로입니다. 테이블에 없어 기본 묶음으로 내려간 키는 쓴 것으로 치지 않습니다.
  - `deactivate_key_table`: 맨 위 테이블 하나를 내립니다. `deactivate_all_key_tables`: 모두 내립니다.
  - `end_key_sequence`: 진행 중인 키 순서를 끝내고, 이 키를 뺀 앞 키들만 프로그램에 보냅니다(`ctrl+w>escape=end_key_sequence`는 `^W`를 보내고 끝). 순서 밖에서는 키만 먹습니다.
  - `performable:`을 붙인 테이블 동작은 할 수 있을 때만 키를 먹습니다(없는 테이블, 이미 맨 위인 테이블, 켜진 테이블이 없을 때의 내리기, 진행 중인 순서가 없을 때의 `end_key_sequence`는 할 수 없음).
  - 지금 켜진 테이블을 화면에 보이지 않습니다. 설정을 다시 읽어 없어진 테이블은 켜져 있어도 건너뜁니다.
- macOS가 아니면 기본 묶음의 `super`는 `ctrl+shift`로 바뀝니다.
- `global:` 접두사를 붙이면 시스템 전역 단축키가 됩니다(Ghostty와 같은 문법). 다른 앱이 앞에 있어도 듣습니다. 기본 전역 묶음은 없습니다. 다른 앱이 이미 잡은 조합이면 경고를 남기고 건너뜁니다.

| 동작 | 하는 일 | macOS 기본 |
|---|---|---|
| `copy_to_clipboard` / `paste_from_clipboard` | 복사 / 붙여넣기 | `super+c` / `super+v` |
| `new_window` | 새 창(창이 하나도 없을 때도 열림. 마지막 창을 닫았을 때 앱이 끝나는지는 `quit-after-last-window-closed`) | `super+n` |
| `new_tab` / `close_surface` / `close_tab` | 새 탭 / 칸 닫기(칸이 하나면 탭) / 탭을 칸째 닫기 | `super+t` / `super+w` / `super+alt+w` |
| `goto_tab:N` / `last_tab` | N번째 / 마지막 탭 | `super+1`~`8` / `super+9` |
| `next_tab` / `previous_tab` | 다음 / 앞 탭 | `super+shift+]` / `super+shift+[` |
| `new_split:right` `down` `left` `up` `auto` | 고른 칸을 나눠 그쪽에 새 칸(`auto`는 긴 쪽) | `super+d`(right) / `super+shift+d`(down) |
| `goto_split:previous` `next` `left` `right` `up` `down` | 앞·다음 칸, 그쪽에 붙은 칸으로(`top`·`bottom`도 받음) | `super+[` / `super+]` / `super+alt+방향키` |
| `resize_split:방향,포인트` | 고른 칸을 감싼 경계를 그쪽으로 민다 | `super+ctrl+방향키`(10) |
| `equalize_splits` | 같은 줄에 선 칸들을 같은 크기로 | `super+ctrl+equal` |
| `toggle_split_zoom` | 고른 칸만 탭 전체로 크게 보기 / 되돌리기 | `super+shift+enter` |
| `undo` / `redo` | 닫은 칸·탭을 원래 자리에 되살리기(`undo-timeout` 안에서) / 되살린 것을 다시 닫기. 되살릴 것이 없으면 키는 프로그램으로 갑니다(Ghostty의 `performable`과 같다) | `super+z` `super+shift+t` / `super+shift+z`(macOS만) |
| `increase_font_size:N` / `decrease_font_size:N` / `reset_font_size` | 글자 크기 | `super+=` / `super+-` / `super+0` |
| `scroll_page_up` / `scroll_page_down` | 스크롤백 한 쪽 | `shift+page_up` `super+page_up` / 아래도 같음 |
| `jump_to_prompt:-1` / `jump_to_prompt:1` | 앞·뒤 프롬프트로(셸 통합) | `super+arrow_up` / `super+arrow_down` |
| `scroll_to_top` / `scroll_to_bottom` | 맨 위 / 맨 아래 | `shift+home` `super+home` / `shift+end` `super+end` |
| `scroll_page_lines:N` / `scroll_page_fractional:F` | N줄 / 화면의 F배만큼 스크롤(양수면 아래로, 음수면 위로) | (없음) |
| `toggle_fullscreen` | 전체 화면 켜고 끄기 | `super+ctrl+f` |
| `toggle_maximize` | 지금 창 최대화 켜고 끄기(Ghostty와 같이 기본 키 없음) | (없음) |
| `toggle_window_float_on_top` | 지금 창을 다른 창 위에 항상 띄우기 켜고 끄기(Ghostty는 macOS만 구현) | (없음) |
| `close_window` / `close_all_windows` | 지금 창 / 모든 창 닫기. 프로그램이 도는 칸이 있으면 `confirm-close-surface`에 따라 확인 막대로 묻습니다(`close_all_windows`는 Ghostty 문서가 "효과 없음, `all:close_window`를 쓰라"고 한 동작인데 b-terminal은 닫습니다) | (없음, 창 닫기 단추) |
| `goto_window:previous` / `goto_window:next` | 창을 만든 차례대로 앞·뒤 창에 포커스(끝에서 돌아감, 퀵 터미널은 건너뜀) | (없음) |
| `move_tab:N` | 지금 탭을 N칸 옮김(양수는 오른쪽, 음수는 왼쪽, 끝을 넘으면 돌아감) | (없음) |
| `move_tab_to_new_window` | 지금 탭을 새 창으로 떼어 냄. 탭이 하나뿐이면 아무것도 하지 않습니다 | (없음) |
| `reset_window_size` | 창을 새 창이 만들어질 크기(`window-width`·`window-height`, 없으면 기본)로 되돌림. 전체 화면에서는 하지 않고, 최대화는 풀고 되돌립니다 | (없음) |
| `toggle_window_decorations` | 지금 창의 제목 막대·테두리 켜고 끄기(Ghostty는 Linux만 구현). 설정을 다시 읽으면 `window-decoration`으로 돌아갑니다 | (없음) |
| `toggle_visibility` | 모든 창 숨기기 / 다시 보이기(macOS만). 보고 있는 칸이 전체 화면이면 아무것도 하지 않습니다 | (없음, 전역 단축키 `global:`와 함께 쓰는 동작) |
| `start_search` / `end_search` | 찾기 막대 열기 / 닫기. 막대에서 글을 치면 바로 찾고, Enter는 위로·Shift-Enter는 아래로, Esc로 닫습니다 | `super+f` / (Esc) |
| `navigate_search:next` / `navigate_search:previous` | 위(오래된 쪽) / 아래 맞은 곳으로 | `super+g` / `super+shift+g` |
| `toggle_command_palette` | 명령 팔레트 열기·닫기 | `super+shift+p` |
| `clear_screen` | 화면 지우기(셸에 Ctrl-L) | `super+k` |
| `select_all` | 보이는 화면 모두 고르기 | `super+a` |
| `reload_config` | 설정 다시 읽기 | `super+shift+comma` |
| `quit` | 앱 끝내기(프로그램이 도는 칸이 있으면 묻습니다) | `super+q` |
| `prompt_surface_title` / `set_surface_title:글` | 지금 칸의 제목을 입력 막대로 묻기 / 정하기. 사람이 붙인 제목은 프로그램이 정한 제목보다 먼저 보이고, 빈 글이면 지워져 프로그램 제목으로 돌아갑니다 | (없음, 메뉴 "셸 › 제목 바꾸기…"와 팔레트) |
| `prompt_tab_title` / `set_tab_title:글` | 지금 탭의 제목을 입력 막대로 묻기 / 정하기. 탭 제목은 그 탭 칸들의 제목보다 먼저 보이고, 빈 글이면 지워져 칸 제목으로 돌아갑니다. 닫았다 `undo`하면 따라 돌아옵니다 | (없음, 메뉴 "셸 › 탭 제목 바꾸기…"와 팔레트) |
| `prompt_window_title` / `set_window_title:글` | 지금 창의 제목을 입력 막대로 묻기 / 정하기. 설정 `title`이 있으면 그것이 먼저이고, 아니면 이 제목, 없으면 지금 탭 제목입니다. 빈 글이면 지웁니다. 탭 줄에는 영향이 없습니다 | (없음, 메뉴 "셸 › 창 제목 바꾸기…"와 팔레트) |
| `copy_title_to_clipboard` | 지금 칸의 터미널 제목(사람이 붙인 것, 없으면 프로그램이 정한 것)을 클립보드로 복사합니다. 제목이 없거나 비면 하지 않습니다 | (없음, 팔레트) |
| `write_scrollback_file:할일` / `write_screen_file:할일` / `write_selection_file:할일` | 스크롤백 전체 / 화면 / 고른 글을 파일로 쓰고, 할일이 `copy`면 경로를 클립보드에, `paste`면 칸에 붙여넣고, `open`이면 기본 앱으로 엽니다. 파일은 이 사용자 전용 임시 폴더에 두고 앱이 끝날 때 지웁니다. 고른 글이 없으면 아무것도 하지 않습니다 | (없음, 팔레트에 열기·복사) |
| `adjust_selection:방향` | 고른 것의 끝점을 옮깁니다. 방향은 `left` `right`(줄 끝에서 앞·뒤 줄로) `up` `down` `page_up` `page_down` `home` `end` `beginning_of_line` `end_of_line`. 고른 것이 없으면 아무것도 하지 않습니다 | (없음) |
| `copy_url_to_clipboard` | 마우스 아래 링크(화면의 URL·OSC 8) 주소를 클립보드에 | (없음) |
| `toggle_quick_terminal` | 퀵 터미널(화면 가장자리에서 미끄러져 나오는 창) 보이기·숨기기. 자리·크기·화면·움직임은 `quick-terminal-*` 설정. 창 상태 복원에서 빠집니다 | (없음, `global:`로 묶으세요) |
| `toggle_background_opacity` | 지금 창의 바탕 투명도를 `background-opacity`와 불투명 사이에서 바꿉니다. `background-opacity`가 1 이상이면 아무것도 하지 않습니다(창이 처음부터 불투명하게 만들어집니다). 창마다 따로이고 설정을 다시 읽어도 남습니다. 창 자체를 불투명으로 바꾸는 것이 아니라 바탕을 칠하는 알파만 바꿉니다 | (없음) |
| `toggle_mouse_reporting` | 지금 칸의 마우스 보고(프로그램에 마우스 보내기)를 끄고 켭니다. 끄면 프로그램이 요청해도 보내지 않아 마우스로 늘 고르기를 할 수 있습니다. 처음 값은 `mouse-reporting`이고, 이 동작으로 정한 값은 그 칸에서 다시 바꿀 때까지 설정을 다시 읽어도 남습니다 | (없음) |
| `toggle_secure_input` | 초점 칸의 칸 단위 보안 입력을 켜고 끕니다(그 칸이 초점일 때만 보안 입력이 켜짐). `macos-auto-secure-input = false`면 아무 일도 없습니다. 앱 전체를 켜고 끄는 메뉴 `보안 키보드 입력`과 다릅니다 | (없음) |
| `toggle_readonly` | 지금 칸을 읽기 전용으로 하거나 풉니다. 읽기 전용 칸에는 키·IME 확정 글자·붙여넣기·마우스 보고·초점 보고·`text:`/`csi:`/`esc:`(`all:` 포함)·`btctl send`·`btctl run`이 모두 가지 않습니다(ADR-018). 고르기·복사·스크롤·키 묶음 동작은 그대로 되고, 칸을 닫거나 끝낼 때는 프로그램이 돌지 않아도 늘 묻습니다. 탭 제목 앞에 🔒가 붙습니다. 프로그램이 물은 것에 터미널이 답하는 바이트(커서 위치·색 질의 등)는 막지 않습니다 | (없음) |
| `set_window_theme:config` `system` `light` `dark` | 이번 실행 동안의 창 테마. 설정 파일은 바꾸지 않고, 설정을 다시 읽어도 남으며, `config`면 설정의 `window-theme`으로 돌아갑니다. 모든 창에 적용됩니다. `window-theme = auto`처럼 바탕색을 따르는 일은 `config`일 때만 합니다 | (없음) |
| `open_config` | 설정 파일(`~/.config/b-terminal/config`)을 기본 텍스트 편집기로 엽니다(macOS `open -t`, 셸을 거치지 않고 인자 배열로). 파일이 없으면 주석만 든 견본을 만들어 엽니다. `open_config:os_open`이 기본이고 `new_window`(Ghostty에서는 GTK 전용)는 받아서 같은 일을 합니다. 팔레트·메뉴의 "설정 파일 열기"와 같습니다 | (없음) |
| `text:글` / `csi:…` / `esc:…` | 글·이스케이프 보내기 | |
| `cursor_key` | Ghostty처럼 설정에 쓸 수 없습니다(Ghostty 안에서만 쓰는 동작). 쓰면 경고하고 묶지 않습니다 |
| `reset` | 터미널 초기화(VT의 RIS와 같다): 화면·스크롤백·모드·고른 것을 비웁니다. 칸의 프로그램에는 아무것도 보내지 않으므로 셸은 Enter를 눌러야 새 프롬프트를 그립니다 | (없음) |
| `scroll_to_row:N` | 스크롤백 맨 위를 0으로 센 N번째 줄이 화면 맨 위에 오게 스크롤. 범위를 넘으면 맨 아래 | (없음) |
| `scroll_to_selection` | 고른 곳이 보이게 스크롤(시작 줄이 이미 보이면 그대로, 아니면 그 줄이 맨 위로). 고른 것이 없으면 아무것도 하지 않습니다 | (없음) |
| `search:글` / `search_selection` | 그 글로 / 고른 글로 찾기를 시작하고 찾기 막대에 보입니다. 이미 찾는 중이면 글을 바꿉니다. `search:`처럼 글이 비면 찾기만 취소하고 막대는 닫지 않습니다. 글은 이스케이프를 풀지 않고, 첫 줄·256자까지 씁니다. `search_selection`은 고른 것이 없으면 아무것도 하지 않습니다 | (없음) |
| `set_font_size:N` | 글자 크기를 N pt로(`increase_font_size`와 같이 6~72로 제한) | (없음) |
| `unbind` | 그 트리거 지우기 | |

### 받기만 하는 키 묶음 동작

Ghostty 설정에 흔한 아래 동작은 `keybind`에 써도 경고가 나지 않습니다. 동작이 있는 묶음이라 키를 먹고(프로그램에 보내지 않음, Ghostty와 같음) 하는 일은 없습니다. `inspector`와 `crash`는 Ghostty처럼 인자가 꼭 있어야 하고, 틀린 인자는 오류입니다.

| 동작 | 왜 받기만 하는가 |
|---|---|
| `check_for_updates` | b-terminal에는 자동 업데이트가 없습니다(릴리스는 직접 받습니다). 만들지 않습니다 |
| `inspector:toggle` `show` `hide` | 터미널 검사기가 없습니다. 만들지 않습니다 |
| `show_gtk_inspector` | GTK 전용입니다(Ghostty도 macOS에서는 아무 일도 하지 않음) |
| `show_on_screen_keyboard` | 화상 키보드는 GTK·Windows 쪽 기능입니다 |
| `toggle_tab_overview` | 네이티브 탭이 없어(탭은 앱이 직접 그림) 모아 보는 화면이 없습니다 |
| `crash:main` `io` `render` | 일부러 앱을 죽여 충돌 처리를 시험하는 디버그 동작입니다. 데이터를 잃을 수 있어 하지 않습니다 |

설정 다시 읽기는 글꼴·색(이미 열린 탭 포함)·키 묶음(전역 단축키 포함)·여백·Option 키를 바꿉니다. 바깥에서 부르려면 `btctl reload-config`. 퀵 터미널은 `btctl quick`.

## 색 구성표 보고

설정 키는 없고, Ghostty처럼 프로그램(nvim 등)이 밝음·어두움을 알 수 있게 합니다.

- `CSI ? 996 n`으로 지금 구성표를 묻습니다. 답은 `CSI ? 997 ; 1 n`(어두움) 또는 `CSI ? 997 ; 2 n`(밝음)입니다. 읽기 전용 칸에서도 답합니다.
- DEC 모드 2031(`CSI ? 2031 h`로 켜고 `l`로 끔)을 켜 두면 구성표가 바뀔 때 같은 `CSI ? 997 ; 1|2 n`을 알립니다. 켠 직후에는 알리지 않고(물어서 알아야 함), 같은 값이 또 오면 다시 보내지 않으며, 초기화(RIS)는 모드를 끕니다.
- 구성표는 테마 바탕색이 아니라 시스템 모양입니다(`theme = light:…,dark:…`가 없어도, 테마가 하나뿐이어도 시스템이 바뀌면 알립니다). `window-theme = light|dark`로 고정한 창은 그 값이 구성표이고 시스템 모양이 바뀌어도 알리지 않습니다. 새 칸은 지금 구성표로 시작합니다.
- 알림은 열린 모든 칸 가운데 모드를 켠 칸에만 갑니다.

## 마우스 포인터 모양(OSC 22)

설정 키는 없고, Ghostty처럼 프로그램이 `OSC 22 ; 이름 BEL`(또는 `ST`)로 마우스 포인터 모양을 정합니다(예 `printf '\033]22;crosshair\033\\'`).

- 이름은 Ghostty와 같습니다: W3C CSS cursor 이름(`pointer`, `text`, `crosshair`, `col-resize`, `zoom-in` 등 34가지)과 xterm·foot 별칭(`left_ptr`, `hand`, `xterm`, `watch`, `fleur` 등). 대소문자를 구분합니다. 모르는 이름과 64바이트가 넘거나 제어 문자가 든 이름은 무시하고 모양을 그대로 둡니다.
- 이름이 비어 있으면(`OSC 22 ; BEL`) 정한 모양을 지웁니다. 초기화(RIS, `printf '\033c'`)와 "터미널 초기화" 동작도 지웁니다.
- 모양은 칸마다 들고 있고, 마우스가 그 칸의 글 영역 위에 있을 때만 창 포인터에 적용합니다. 칸을 옮기거나 탭 줄·칸 사이 틈으로 나가면 그 칸의 모양을 쓰지 않습니다. 정한 모양이 없으면 지금까지처럼 화살표입니다.
- Cmd(Linux는 Ctrl)를 누른 채 링크 위에 있을 때의 손가락 모양이 프로그램이 정한 모양보다 앞섭니다(Ghostty와 같습니다).
- 마우스 보고 모드(X10·버튼·움직임, `?1000h` 등)를 켜거나 끄면 프로그램이 앞서 정한 모양을 지웁니다(Ghostty는 이때 모양을 `default`·`text`로 바꿉니다). 모드를 바꾼 뒤에 정한 모양은 남습니다. 같은 상태의 모드를 다시 켜는 것은 모드가 바뀌지 않아 지우지 않습니다.
- Shift를 누르면 마우스가 고르기를 하게 될 때(마우스 보고가 꺼져 있거나, 켜져 있어도 Shift가 보고를 끊을 때. `mouse-shift-capture`) 포인터가 I자(`text`)가 되고, 떼면 프로그램이 정한 모양(없으면 화살표)으로 돌아갑니다. Shift를 눌러도 프로그램이 마우스를 계속 받으면(`mouse-shift-capture = true`·`always`) 모양을 바꾸지 않습니다. 링크 위의 손가락이 가장 앞서고, 입력 중 마우스 숨김(`mouse-hide-while-typing`)으로 포인터가 숨은 동안은 Shift가 모양을 바꾸지 않습니다. 수식 키를 누르거나 뗄 때 모양을 다시 정합니다.
- Ghostty와 다른 점: Ghostty는 기본 모양이 I자(`text`)이지만 b-terminal은 정한 모양이 없으면 화살표입니다(바꾸지 않음). 그래서 모드를 끄거나 `OSC 22 ; BEL`로 지울 때도 `text`가 아니라 화살표로 돌아갑니다. Ghostty는 macOS에서 Alt를 누르면 사각 고르기용 `crosshair`로 바꾸지만 b-terminal에는 사각 고르기가 없어(고르기는 글자·낱말·줄뿐) 넣지 않았습니다.

## 셸 통합

탭에서 띄우는 zsh·bash에 셸 통합을 자동으로 싣습니다(Ghostty와 같은 방식, 사용자 설정은 그대로 읽힘. [assets/shell-integration](../assets/shell-integration/README.md)). 셸이 명령의 시작·끝·종료 코드(OSC 133)와 명령 글(OSC 633;E)을 알려 주면 터미널이 이렇게 씁니다.

- Cmd-↑ / Cmd-↓: 앞·뒤 프롬프트로 스크롤
- 기본은 창을 보지 않을 때 10초(`notify-command-seconds`) 넘게 걸린 명령이 끝나면 데스크톱 알림. 실패하면 "명령이 실패했습니다". `notify-on-command-finish`를 적으면 Ghostty처럼 그 키들(`-action`, `-after`)을 따릅니다
- `btctl run -- <명령>`: 그 탭에서 명령을 돌리고 끝날 때까지 기다려 출력을 찍고 그 종료 코드로 끝납니다. 에이전트가 다른 탭을 도구로 쓰는 길입니다. `btctl last`는 마지막으로 끝난 명령

fish·elvish는 아직 스크립트가 없습니다. 끄려면 `shell-integration = none`.

### 셸 통합 기능

`shell-integration-features`로 기능마다 켜고 끕니다(Ghostty와 같은 이름·기본·문법). 켜진 이름은 환경 변수 `BT_SHELL_FEATURES`(쉼표로 이은 글)로 셸 스크립트에 가고, 스크립트는 이름이 정확히 맞는지만 비교합니다(값을 eval하지 않습니다). 셸 통합이 없는 탭(`shell-integration = none`, 다른 셸, `command`를 준 탭)에서는 어느 기능도 일하지 않습니다.

| 기능 | 기본 | 하는 일 |
|---|---|---|
| `cursor` | 켬 | 프롬프트에서 커서를 깜박이는 막대(`CSI 5 SP q`)로, 명령이 도는 동안은 원래 모양(`CSI 0 SP q`, `cursor-style`)으로. 끄면 `cursor-style`이 그대로 보입니다 |
| `title` | 켬 | 프롬프트에서는 창 제목을 지금 폴더(`~` 줄임, 길면 `…/뒤 세 마디`)로, 명령이 도는 동안은 그 명령 글(200자까지)로 정합니다. 제어 문자는 공백으로 바꿉니다. 설정 `title`로 제목을 고정했으면 그 값이 먼저입니다 |
| `path` | 켬 | b-terminal 실행 파일이 있는 폴더(`btctl`이 함께 있을 때만)를 첫 프롬프트에서 PATH **끝**에 한 번 더합니다. 이미 있으면 더하지 않습니다. 앱 묶음 밖에서 `btctl`을 바로 부르게 하려는 것입니다. Ghostty의 `path`와 같은 뜻입니다 |
| `sudo` | 끔 | 받기만 합니다. Ghostty에서는 `sudo`가 terminfo(`TERMINFO`)를 지키게 감싸는 기능인데, b-terminal은 `TERM=xterm-256color`라 지킬 terminfo가 없어 하는 일이 없습니다 |
| `ssh-env` | 끔 | 받기만 합니다. Ghostty에서는 TERM을 `xterm-ghostty`에서 `xterm-256color`로 바꾸고 `COLORTERM`·`TERM_PROGRAM` 등을 원격에 보내는 기능인데, b-terminal은 처음부터 `xterm-256color`라 바꿀 TERM이 없습니다. `ssh`를 감싸지 않습니다 |
| `ssh-terminfo` | 끔 | 받기만 합니다. b-terminal은 자기 terminfo가 없고, 원격 호스트에 무엇을 설치하는 동작은 하지 않습니다 |

`cursor`와 `title`은 zsh·bash 모두에서 `precmd`·`preexec`(bash는 `PROMPT_COMMAND`·DEBUG 덫) 훅으로 보냅니다. zsh의 `zle` 위젯(줄 편집 중 모양 바꾸기)은 쓰지 않아, 줄 편집 도중 프로그램이 모양을 바꾸면 다음 프롬프트까지 그대로입니다.
