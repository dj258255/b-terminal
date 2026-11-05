# b-terminal

> Ghostty 같은 터미널 에뮬레이터를 Rust로 만듭니다. 한글 입력이 깨지지 않고, Ghostty 설정 문법으로 꾸밀 수 있는 그냥 터미널입니다.

아직 만드는 중입니다. 계획과 진행은 [ROADMAP.md](ROADMAP.md)에 있습니다.

<img src="docs/images/window-demo.png" width="760" alt="b-terminal 창. 위쪽 가로 탭 줄에 탭 셋, 그 아래 굵은 글자, 한글 문장, 16색·256색·24비트 색, 기울임·밑줄·반전·취소선 글자, 둥근 상자 선, 반칸 블록과 8단계 막대">

- 터미널이 할 일만 합니다: PTY, VT 해석, 렌더링, 탭, 한글 IME, 복사·붙여넣기, 설정.
- AI 에이전트를 다루는 화면(에이전트 목록·상태·사용량)은 [b-studio](https://github.com/dj258255/b-studio)의 일입니다. b-studio는 b-terminal 안에서 도는 프로그램 중 하나입니다(herdr가 Ghostty 안에서 돌듯이). 경계는 [ADR-005](docs/decisions.md#adr-005-b-terminal은-순수-터미널이고-ai-작업-화면은-b-studio가-맡는다)에 적었습니다.

## 띄우기

```bash
cargo run --release -p bt-app                 # 로그인 셸
cargo run --release -p bt-app -- -e htop      # 처음 탭에서 다른 프로그램
cargo run --release -p bt-app -- --cwd ~/dev  # 시작 폴더
```

| 키 | 하는 일 |
|---|---|
| Cmd-C / Cmd-V | 고른 글 복사 / 붙여넣기(괄호 붙여넣기 모드면 감싸서) |
| Cmd-= / Cmd-- / Cmd-0 | 글자 크기 키우기 / 줄이기 / 처음대로 |
| Cmd-T / Cmd-W | 새 탭 / 탭 닫기 |
| Cmd-1~9, Cmd-[ / Cmd-] | 탭 옮기기(탭 줄을 눌러도 됩니다) |
| Shift-PageUp / PageDown, Cmd-↑ / ↓ | 스크롤백 |
| 마우스 끌기, 두 번·세 번 누르기 | 글자·단어·줄 고르기(앱이 마우스를 쓰면 Shift를 누른 채로) |

## 설정(Ghostty 문법)

Ghostty 설정 파일이 있으면 그대로 읽고, `~/.config/b-terminal/config`가 그 위를 덮습니다. 설정이 없으면 Ghostty 기본과 같게 보입니다(JetBrains Mono 13, Nerd Font 기호를 앱에 넣어 둠).

```
theme = Dracula
font-family = D2Coding
font-size = 15
cursor-style = bar
window-padding-x = 12
```

<img src="docs/images/config-dracula-d2coding.png" width="760" alt="Dracula 테마와 D2Coding 글꼴, 막대 커서, 넓은 여백으로 띄운 b-terminal. 한글이 고정폭으로 고르게 놓였다">

지원하는 키와 읽는 순서는 [docs/configuration.md](docs/configuration.md)에 있습니다. `b-terminal --show-config`가 지금 적용된 값을 보입니다.

## 원격 조종(btctl)

kitty·WezTerm의 원격 조종처럼, 스크립트가 터미널의 탭을 열고 글을 넣고 화면을 읽을 수 있습니다. 탭 안의 셸에는 `BT_SOCKET`(제어 소켓)과 `BT_TAB_ID`(그 탭 번호)가 들어 있습니다.

```bash
cargo install --path crates/bt-ctl      # btctl 설치
btctl list                               # 탭 목록
btctl new --cwd ~/dev -- htop            # 새 탭에서 htop
btctl send --tab 2 --enter "make test"   # 2번 탭에 글과 Enter
btctl read --tab 2                       # 2번 탭 화면을 글로
btctl notify "빌드" "끝났습니다"           # 이 탭에 알림
```

제어 소켓은 같은 사용자만 닿는 폴더(0700)의 소켓(0600)입니다. 자세한 경계는 [SECURITY.md](SECURITY.md)에 있습니다.

## 기여

[CONTRIBUTING.md](CONTRIBUTING.md)와 코딩 에이전트(이 저장소를 고치는 도구)용 [AGENTS.md](AGENTS.md)를 봅니다.

## 라이선스

MIT
