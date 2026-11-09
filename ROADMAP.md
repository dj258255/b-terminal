# 로드맵

이슈 단위 계획과 실제 시간입니다. 예상은 이슈를 열 때 적은 값이고, 실제는 브랜치를 딴 때부터 PR 병합까지의 대략값입니다(2026-11-05 23시~11-06 01시, 사람 없이 코딩 에이전트가 진행). 틀린 예상은 지우지 않습니다.

## 0.1.0 (2026-11-06)

범위는 [ADR-005](docs/decisions.md#adr-005-b-terminal은-순수-터미널이고-ai-작업-화면은-b-studio가-맡는다): Ghostty 같은 순수 터미널. 추적 이슈 [#1](https://github.com/dj258255/b-terminal/issues/1).

| 이슈 | 내용 | 예상 | 실제 | PR | 상태 |
|---|---|---|---|---|---|
| [#2](https://github.com/dj258255/b-terminal/issues/2) | 조사 기록, ADR-001~003 | 1~2시간 | 약 1시간 | [#14](https://github.com/dj258255/b-terminal/pull/14) | 끝 |
| [#3](https://github.com/dj258255/b-terminal/issues/3) | Cargo 작업 공간과 CI | 1시간 | 약 20분 | [#15](https://github.com/dj258255/b-terminal/pull/15) | 끝 |
| [#4](https://github.com/dj258255/b-terminal/issues/4) | 터미널 코어(PTY·VT) | 3~5시간 | 약 1.5시간 | [#16](https://github.com/dj258255/b-terminal/pull/16) | 끝 |
| [#5](https://github.com/dj258255/b-terminal/issues/5) | CPU 렌더러, 실험 E1 | 3~5시간 | 약 1.5시간 | [#17](https://github.com/dj258255/b-terminal/pull/17) | 끝 |
| [#6](https://github.com/dj258255/b-terminal/issues/6) | 창 앱, 한글 IME winit 패치(ADR-004) | 4~6시간 | 약 2시간 | [#18](https://github.com/dj258255/b-terminal/pull/18) | 끝 |
| [#7](https://github.com/dj258255/b-terminal/issues/7) | 탭과 세로 사이드바 | 2~4시간 | 약 1시간 | [#19](https://github.com/dj258255/b-terminal/pull/19) | 끝 → 사이드바는 #25에서 가로 탭 줄로 |
| [#8](https://github.com/dj258255/b-terminal/issues/8) | OSC 9·99·777 알림 | 2~3시간 | 약 1시간 | [#20](https://github.com/dj258255/b-terminal/pull/20) | 끝(보안 지적 3건 병합 전 고침) |
| [#9](https://github.com/dj258255/b-terminal/issues/9) | 제어 소켓과 btctl | 3~4시간 | 약 1.5시간 | [#21](https://github.com/dj258255/b-terminal/pull/21) | 끝(보안 지적 2건 병합 전 고침) |
| [#10](https://github.com/dj258255/b-terminal/issues/10) | 에이전트 훅 | 2~4시간 | 약 1시간 | [#22](https://github.com/dj258255/b-terminal/pull/22) | 끝 → #25에서 걷어냄 |
| [#11](https://github.com/dj258255/b-terminal/issues/11) | 구독 사용량 막대 | 2~3시간 | 약 1시간 | [#23](https://github.com/dj258255/b-terminal/pull/23) | 끝 → #25에서 걷어냄 |
| [#12](https://github.com/dj258255/b-terminal/issues/12) | b-studio 연결 | 4~6시간 | 약 1.5시간(병합 안 함) | — | 하지 않음(ADR-005) |
| [#25](https://github.com/dj258255/b-terminal/issues/25) | AI 기능 걷어내기, 가로 탭 줄, ADR-005 | 2~4시간 | 약 1시간 | [#26](https://github.com/dj258255/b-terminal/pull/26) | 끝 |
| [#13](https://github.com/dj258255/b-terminal/issues/13) | Ghostty 설정 호환, 넣어 둔 글꼴 | 1~2시간 → 2~4시간 | 약 2시간 | [#28](https://github.com/dj258255/b-terminal/pull/28) | 끝 |
| [#27](https://github.com/dj258255/b-terminal/issues/27) | keybind, 설정 다시 읽기 | 2~4시간 | 약 1시간 | [#29](https://github.com/dj258255/b-terminal/pull/29) | 끝 |
| [#24](https://github.com/dj258255/b-terminal/issues/24) | 셸로 돌아오면 마우스 모드 끄기 | 1~3시간 | 약 30분 | [#30](https://github.com/dj258255/b-terminal/pull/30) | 끝 |

예상보다 대체로 짧았던 까닭: 예상은 사람이 하는 시간으로 적었고, 실제는 코딩 에이전트가 쉬지 않고 진행한 시간이다. 대신 키보드·마우스로 직접 쳐 보는 확인이 빠져 있다(각 PR의 "돌리지 않은 검증").

## 0.2.0 (2026-11-06)

범위는 [ADR-006](docs/decisions.md#adr-006-ghostty는-본보기일-뿐-기대지-않는다-ai-cli가-가장-잘-도는-우리-터미널): Ghostty 없이 혼자 서는 우리 터미널. 추적 이슈 [#32](https://github.com/dj258255/b-terminal/issues/32). 실제 시간은 2026-11-06 낮, 사람 없이 코딩 에이전트가 진행한 대략값이다.

| 이슈 | 내용 | 예상 | 실제 | PR | 상태 |
|---|---|---|---|---|---|
| [#33](https://github.com/dj258255/b-terminal/issues/33) | Ghostty 없이 혼자 서기: 테마 905개 넣기, Ghostty 설정 읽기 끔(ADR-006) | 1~2시간 | 약 1시간 | [#44](https://github.com/dj258255/b-terminal/pull/44) | 끝 |
| [#41](https://github.com/dj258255/b-terminal/issues/41) | kitty 키보드 프로토콜(Shift+Enter) | 3~4시간 | 약 40분 | [#45](https://github.com/dj258255/b-terminal/pull/45) | 끝 |
| [#42](https://github.com/dj258255/b-terminal/issues/42) | 진행 표시(OSC 9;4)와 클립보드 쓰기(OSC 52) | 2~3시간 | 약 40분 | [#46](https://github.com/dj258255/b-terminal/pull/46) | 끝 |
| [#47](https://github.com/dj258255/b-terminal/issues/47) | [버그] OSC 52 쓰기가 붙여넣기 하이재킹을 연다 | — | 약 35분 | [#48](https://github.com/dj258255/b-terminal/pull/48)·[#49](https://github.com/dj258255/b-terminal/pull/49) | 끝(보고 있는 칸만, 알림 띠) |
| [#43](https://github.com/dj258255/b-terminal/issues/43) | 셸 통합(OSC 133): 프롬프트 이동, 끝난 명령 알림, btctl run | 6~8시간 | 약 2시간 | [#50](https://github.com/dj258255/b-terminal/pull/50) | 끝 |
| [#34](https://github.com/dj258255/b-terminal/issues/34) | macOS 앱 묶음과 메뉴 막대 | 3~4시간 | 약 50분 | [#51](https://github.com/dj258255/b-terminal/pull/51) | 끝 |
| [#35](https://github.com/dj258255/b-terminal/issues/35) | 화면 분할 | 3~5시간 | 약 1시간 30분 | [#52](https://github.com/dj258255/b-terminal/pull/52) | 끝 |
| [#38](https://github.com/dj258255/b-terminal/issues/38) | 링크 열기(URL·OSC 8, Cmd-클릭, ADR-007) | 2~4시간 | 약 1시간 | [#53](https://github.com/dj258255/b-terminal/pull/53) | 끝(보안 검토 전에 병합한 실수 → #54) |
| [#54](https://github.com/dj258255/b-terminal/issues/54) | [버그] 링크 열기: 실행 파일 판정 우회, 주소 속이기, 제곱 시간 | — | 약 40분 | [#55](https://github.com/dj258255/b-terminal/pull/55) | 끝 |
| [#39](https://github.com/dj258255/b-terminal/issues/39) | 찾기(Cmd-F) | 2~3시간 | 약 1시간 10분 | [#56](https://github.com/dj258255/b-terminal/pull/56) | 끝 |
| [#40](https://github.com/dj258255/b-terminal/issues/40) | 커서 깜빡임과 바뀐 줄만 다시 그리기, 실험 E2 | 3~4시간 | 약 1시간 20분 | [#57](https://github.com/dj258255/b-terminal/pull/57) | 끝 |
| [#37](https://github.com/dj258255/b-terminal/issues/37) | 색 이모지(swash) | 2~3시간 | 약 50분 | [#60](https://github.com/dj258255/b-terminal/pull/60) | 끝 |
| [#36](https://github.com/dj258255/b-terminal/issues/36) | 창 여러 개(Cmd-N) | 3~4시간 | 약 1시간 10분 | [#61](https://github.com/dj258255/b-terminal/pull/61) | 끝(보안 지적 1건 병합 전 고침) |
| [#58](https://github.com/dj258255/b-terminal/issues/58) | [버그] 처음 설정을 다시 읽을 때 메모리가 뛴다 | — | 약 40분 | [#62](https://github.com/dj258255/b-terminal/pull/62) | 끝 |
| [#59](https://github.com/dj258255/b-terminal/issues/59) | 색 이모지 글꼴 메모리 매핑(unsafe 예외) 정하기 | 1~3시간 | — | — | 0.3으로(범수 님 결정) |

이번에 배운 것: 보안 경계를 건드린 PR은 커밋 보안 검토 결과를 확인한 뒤 병합한다(#53에서 어김). 병합 스크립트가 그 결과를 기록에서 읽되, 다른 검토와 겹치면 사람이 확인하게 멈춘다.

## 0.3.0 (2026-11-06)

가볍고 정확하게. 추적 이슈 [#64](https://github.com/dj258255/b-terminal/issues/64). 실제 시간은 사람 없이 코딩 에이전트가 진행한 대략값이다.

| 이슈 | 내용 | 예상 | 실제 | PR | 상태 |
|---|---|---|---|---|---|
| [#59](https://github.com/dj258255/b-terminal/issues/59) | 색 이모지 글꼴은 필요한 크기 한 벌만(sfnt 골라 읽기, unsafe 없이) | 1~3시간 | 약 50분 | [#69](https://github.com/dj258255/b-terminal/pull/69) | 끝(첫 커밋 검토가 끝나지 않아 보안 검토를 따로 돌림) |
| [#65](https://github.com/dj258255/b-terminal/issues/65) | 흑백 글자도 swash로, fontdue 빼기(ADR-008) | 2~4시간 | 약 50분 | [#70](https://github.com/dj258255/b-terminal/pull/70) | 끝(처음 메모리 378→158MiB) |
| [#66](https://github.com/dj258255/b-terminal/issues/66) | btctl read --lines, btctl wait | 2~3시간 | 약 40분 | [#71](https://github.com/dj258255/b-terminal/pull/71) | 끝 |
| [#67](https://github.com/dj258255/b-terminal/issues/67) | U+FE0F 이모지를 색으로 | 1~2시간 | 약 30분 | [#72](https://github.com/dj258255/b-terminal/pull/72) | 끝 |
| [#68](https://github.com/dj258255/b-terminal/issues/68) | 실험 E3: 창에 올리기, 화면 색 공간(ADR-009) | 1~2시간 | 약 1시간 10분 | [#73](https://github.com/dj258255/b-terminal/pull/73) | 끝(올리기 15→2.7ms) |

이번에 배운 것: 자동 커밋 검토는 끝나지 않을 수 있다(모델에 닿지 못함, 결과 없이 멈춤). 기다리는 데에 기한을 두고, 보안 경계를 건드린 변경이면 검토를 따로 돌린다.

## 0.4.0 (2026-11-06)

그림과 손에 잡히는 것. 추적 이슈 [#75](https://github.com/dj258255/b-terminal/issues/75). 실제 시간은 사람 없이 코딩 에이전트가 진행한 대략값이다.

| 이슈 | 내용 | 예상 | 실제 | PR | 상태 |
|---|---|---|---|---|---|
| [#76](https://github.com/dj258255/b-terminal/issues/76) | kitty 그래픽 프로토콜(ADR-010) | 4~6시간 | 약 1시간 40분 | [#80](https://github.com/dj258255/b-terminal/pull/80) | 끝(보안 지적 2건 병합 전 고침, 셀 픽셀 크기 버그도 고침) |
| [#77](https://github.com/dj258255/b-terminal/issues/77) | 명령 팔레트(Cmd-Shift-P) | 2~3시간 | 약 40분 | [#81](https://github.com/dj258255/b-terminal/pull/81) | 끝 |
| [#78](https://github.com/dj258255/b-terminal/issues/78) | 투명 창(background-opacity) | 2~3시간 | 약 40분 | [#82](https://github.com/dj258255/b-terminal/pull/82) | 끝 |
| [#79](https://github.com/dj258255/b-terminal/issues/79) | 실험 E4: 창 버퍼 다시 쓰기 | 1~2시간 | 약 40분 | [#83](https://github.com/dj258255/b-terminal/pull/83) | 끝(이득 작음, 중앙값 약 0.2ms) |

이번에 배운 것: 터미널에 찍힌 글이 보내는 자료(그림)는 바이트 상한만으로 모자라다. 개수와 그리는 양에도 상한이 있어야 한다(보안 검토 지적).

## 0.5.0 (2026-11-06)

다듬기. 추적 이슈 [#85](https://github.com/dj258255/b-terminal/issues/85). 실제 시간은 사람 없이 코딩 에이전트가 진행한 대략값이다.

| 이슈 | 내용 | 예상 | 실제 | PR | 상태 |
|---|---|---|---|---|---|
| [#86](https://github.com/dj258255/b-terminal/issues/86) | 투명 창 글자 가장자리(캔버스 곱한 알파) | 1~2시간 | 약 30분 | [#89](https://github.com/dj258255/b-terminal/pull/89) | 끝 |
| [#87](https://github.com/dj258255/b-terminal/issues/87) | 이모지 묶음(ZWJ·피부색·국기) | 2~3시간 | 약 50분 | [#90](https://github.com/dj258255/b-terminal/pull/90) | 끝 |
| [#88](https://github.com/dj258255/b-terminal/issues/88) | 서명·공증 준비 | 1시간 | 약 30분 | [#91](https://github.com/dj258255/b-terminal/pull/91) | 끝(실제 공증은 계정이 있어야 함) |

## 0.6.0 (2026-11-06)

매일 쓰는 터미널. 추적 이슈 [#93](https://github.com/dj258255/b-terminal/issues/93). 사람 없이 코딩 에이전트가 진행했다. 이번 판은 이슈마다 실제 시간을 재지 않았다. 병합 시각(KST)만 적는다: 0.5.0 릴리스 병합 14:42부터 마지막 PR 병합 15:28까지.

| 이슈 | 내용 | 예상 | 실제 | PR | 상태 |
|---|---|---|---|---|---|
| [#94](https://github.com/dj258255/b-terminal/issues/94) | 창 상태 복원(`window-save-state`) | 2~3시간 | 재지 않음(병합 14:55) | [#98](https://github.com/dj258255/b-terminal/pull/98) | 끝 |
| [#99](https://github.com/dj258255/b-terminal/issues/99) | `btctl read --lines`가 마지막 글 줄부터(#94 확인 중 찾음) | — | 재지 않음(병합 14:58) | [#100](https://github.com/dj258255/b-terminal/pull/100) | 끝 |
| [#95](https://github.com/dj258255/b-terminal/issues/95) | 퀵 터미널(전역 단축키, `btctl quick`) | 2~3시간 | 재지 않음(병합 15:07) | [#101](https://github.com/dj258255/b-terminal/pull/101) | 끝(실제 키로는 눌러 보지 않음) |
| [#96](https://github.com/dj258255/b-terminal/issues/96) | 글꼴 합자(`font-feature`) | 2~3시간 | 재지 않음(병합 15:14) | [#102](https://github.com/dj258255/b-terminal/pull/102) | 끝 |
| [#97](https://github.com/dj258255/b-terminal/issues/97) | 창이 있는 화면의 색 공간 | 1~2시간 | 재지 않음(병합 15:28) | [#103](https://github.com/dj258255/b-terminal/pull/103) | 끝(다른 화면으로 옮겨 보지 못함) |

이번에 배운 것:

- 다른 앱이 앞에 있을 때 창을 띄우면, 앱이 앞으로 오는 사이에 포커스 잃음이 먼저 온다. 포커스를 잃으면 숨는 창은 보인 직후를 무시해야 한다(#95).
- JetBrains Mono의 합자는 글자 수만큼 글리프를 내고 마지막 글리프가 앞 칸으로 넘쳐 그린다. 칸마다 그리는 렌더러에 그대로 맞는다(#96).
- 한 번씩 띄운 올리기 시간은 같은 빌드에서도 0.1ms 넘게 흔들린다. 견줄 때는 번갈아 여러 번 띄운다(#97).


## 0.7.0 (2026-11-09)

확인과 알림. 추적 이슈 [#105](https://github.com/dj258255/b-terminal/issues/105). 사람 없이 코딩 에이전트가 진행했다. 이번 판도 이슈마다 실제 시간을 재지 않았다. 병합 시각(KST)만 적는다: 추적 이슈를 연 09:37부터 마지막 PR 병합 11:11까지.

| 이슈 | 내용 | 예상 | 실제 | PR | 상태 |
|---|---|---|---|---|---|
| [#106](https://github.com/dj258255/b-terminal/issues/106) | 키 순서(`keybind = ctrl+a>n=…`) | 2~3시간 | 재지 않음(병합 09:43) | [#111](https://github.com/dj258255/b-terminal/pull/111) | 끝(실제 키로는 눌러 보지 않음) |
| [#112](https://github.com/dj258255/b-terminal/issues/112) | 탭 막대 글의 색 이모지로 앱이 꺼짐(#107 확인 중 찾음) | — | 재지 않음(병합 09:54) | [#113](https://github.com/dj258255/b-terminal/pull/113) | 끝 |
| [#107](https://github.com/dj258255/b-terminal/issues/107) | 벨(`bell-features`) | 2~3시간 | 재지 않음(병합 09:57) | [#114](https://github.com/dj258255/b-terminal/pull/114) | 끝(Dock·경고음은 확인하지 못함) |
| [#115](https://github.com/dj258255/b-terminal/issues/115) | `btctl new`가 설정의 command를 무시함(#107 확인 중 찾음) | — | 재지 않음(병합 10:00) | [#116](https://github.com/dj258255/b-terminal/pull/116) | 끝 |
| [#108](https://github.com/dj258255/b-terminal/issues/108) | 닫기 확인(`confirm-close-surface`) | 3시간 | 재지 않음(병합 10:08) | [#117](https://github.com/dj258255/b-terminal/pull/117) | 끝(실제 키로는 눌러 보지 않음) |
| [#109](https://github.com/dj258255/b-terminal/issues/109) | 붙여넣기 보호(`clipboard-paste-protection`) | 2~3시간 | 재지 않음(병합 10:14) | [#118](https://github.com/dj258255/b-terminal/pull/118) | 끝(실제로 붙여넣어 보지 않음) |
| [#110](https://github.com/dj258255/b-terminal/issues/110) | 퀵 터미널 자리·크기·화면·움직임 | 3시간 | 재지 않음(병합 11:11) | [#119](https://github.com/dj258255/b-terminal/pull/119) | 끝(화면이 둘일 때는 해 보지 못함) |

이번에 배운 것:

- 탭 막대처럼 셀 격자 밖에서 글을 그리는 길은 셀 격자와 다른 코드라, 셀 격자에서 고친 것(색 이모지)이 거기에는 없었다. 프로그램 출력이 닿는 그리기 길은 모두 같은 글리프 종류를 다뤄야 한다(#112).
- 시험 앱 pid에만 보내는 키 이벤트(`CGEventPostToPid`)는 그 앱이 맨 앞이 아니면 닿지 않는다. 사람의 포커스를 빼앗지 않고는 키 입력을 확인할 길이 없어, 키로 하는 확인은 판단을 순수 함수로 떼어 테스트했다(#108).
- macOS는 테두리 없는 창도 보이거나 옮길 때 메뉴 막대 아래로 밀어 넣는다(`constrainFrameRect:toScreen:`). 위쪽에서 미끄러지는 창은 이것을 건너뛰어야 한다(#110).
- 리눅스 CI에서만 나는 dead_code 경고가 있다. macOS에서만 쓰는 함수는 `cfg_attr`로 다룬다(#110).

## 다음

- 범수 님이 직접 해야 할 것: 키보드·마우스 확인(한글 2벌식 "안녕 하세요⏎", Claude Code의 Shift+Enter, Cmd-D·Cmd-F·Cmd-클릭·Cmd-N·Cmd-Shift-P, 메뉴, 창 닫기 단추, vim·htop, kitty icat·yazi, 투명 창), Developer ID 계정으로 `scripts/bundle-macos.sh --sign … --notarize … --dmg`
- 범수 님이 정할 것: `window-colorspace` 기본값(ADR-009)
- 범수 님이 확인할 것(0.6): 퀵 터미널을 실제 전역 단축키로(`keybind = global:ctrl+backquote=toggle_quick_terminal`), 외부 모니터로 창을 옮겼을 때 present 시간(`RUST_LOG=b_terminal=debug`)
- 범수 님이 확인할 것(0.7): `sleep 100` 중에 ⌘W → 확인 막대 → esc는 그대로·↩는 닫힘, 프롬프트에서 ⌘W는 바로 닫힘, ⌘Q도 같게. 셸에서 `cat` 중에 두 줄 글 ⌘V → 붙여넣기 확인 막대. `keybind = ctrl+a>n=new_window` 뒤 Ctrl-A·N → 새 창, Ctrl-A·X → `^A`와 `x`가 셸로. 다른 탭에서 `sleep 2; printf '\a'` → 탭 제목에 🔔
- 큰 일로 남긴 것: GPU(또는 IOSurface) 렌더러, 글자 묶음 폭(mode 2027)
