# 그림

README에 쓰는 그림과 다시 찍는 법입니다. 화면이 크게 바뀐 판에서 다시 찍습니다.

| 파일 | 내용 | 마지막으로 찍은 판 |
|---|---|---|
| `window-demo.png` | 기본 설정. 탭 셋(가운데 탭에 프로그램 상태 막대), 오른쪽 분할, 글자 모양·색·상자 선·색 이모지·링크 | 0.85 |
| `config-dracula-d2coding.png` | README "설정" 절의 예시 그대로(Dracula, D2Coding 15pt, 막대 커서, 여백 12) | 0.85 |
| `icon.png` | 앱 아이콘(`scripts/make-icon.swift`) | 0.5 |

## 다시 찍기

화면은 [`scripts/demo-screen.sh`](../../scripts/demo-screen.sh)가 그립니다(`/bin/sh`와 `printf`만 씀). 장면은 `main`(글자 모양·색·이모지), `test`(시험 출력), `build`(빌드, 프로그램 상태 `working` 막대), `ssh`(원격 로그)입니다. 대본은 셸 프롬프트 대신 `$ `만 그리고 `sleep`으로 멈춥니다. 그래서 컴퓨터 이름·사용자 이름이 그림에 찍히지 않고, 찍는 동안 키를 눌러도 셸에 들어가지 않습니다.

1. 앱 묶음을 만들고(`scripts/bundle-macos.sh`) 사용자 설정과 섞이지 않게 빈 `HOME`과 짧은 `TMPDIR`로 띄웁니다. 빈 `HOME`에는 사용자 글꼴이 없으므로 D2Coding 그림은 `HOME/Library/Fonts`에 그 글꼴 파일을 이어 둡니다.

   ```sh
   # HOME/.config/b-terminal/config (window-demo)
   initial-command = /bin/sh <저장소>/scripts/demo-screen.sh main
   window-width = 136
   window-height = 19
   ```

   ```sh
   # HOME/.config/b-terminal/config (config-dracula-d2coding)
   theme = Dracula
   font-family = D2Coding
   font-size = 15
   cursor-style = bar
   window-padding-x = 12
   initial-command = /bin/sh <저장소>/scripts/demo-screen.sh main
   window-width = 90
   window-height = 18
   ```

   ```sh
   open -n -g --env HOME=$H --env SHELL=/bin/sh --env TMPDIR=/tmp/짧은폴더/ dist/b-terminal.app
   ```

2. `window-demo`는 같은 `HOME`·`TMPDIR`의 `btctl`로 장면을 더합니다.

   ```sh
   btctl new --split right --tab 1 -- /bin/sh scripts/demo-screen.sh test
   btctl new -- /bin/sh scripts/demo-screen.sh build
   btctl new -- /bin/sh scripts/demo-screen.sh ssh
   btctl focus 1
   ```

3. 가려진 창은 그리기를 건너뛰므로 앱을 잠깐 앞으로 가져와(`tell application "<묶음 경로>" to activate`) 창만 찍고(`screencapture -o -l<창 번호>`), 원래 앞 앱으로 되돌립니다. 창 번호는 `CGWindowListCopyWindowInfo`에서 그 pid의 층 0 창입니다. 프로그램 상태는 `working`만 씁니다. `blocked`·`done`은 보고 있지 않은 탭이면 데스크톱 알림을 띄웁니다.
4. 가로 1520픽셀로 줄여 넣습니다(`sips --resampleWidth 1520`). README는 760픽셀 폭으로 보입니다.
