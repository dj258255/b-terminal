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

## 0.8.0 (2026-11-09)

Ghostty 키 채우기. 추적 이슈 [#121](https://github.com/dj258255/b-terminal/issues/121). 사람 없이 코딩 에이전트가 진행했다. 이슈마다 실제 시간은 재지 않았고 병합 시각(KST)만 적는다: 추적 이슈를 연 11:22부터 마지막 PR 병합 13:07까지(중간에 대화가 끊긴 시간 포함).

| 이슈 | 내용 | 예상 | 실제 | PR | 상태 |
|---|---|---|---|---|---|
| [#126](https://github.com/dj258255/b-terminal/issues/126) | 마우스·창의 작은 키(`focus-follows-mouse` 등) | 2~3시간 | 재지 않음(병합 11:27) | [#127](https://github.com/dj258255/b-terminal/pull/127) | 끝(실제 마우스로는 해 보지 않음) |
| [#123](https://github.com/dj258255/b-terminal/issues/123) | 최소 명암비(`minimum-contrast`) | 2~3시간 | 재지 않음(병합 12:52) | [#128](https://github.com/dj258255/b-terminal/pull/128) | 끝(렌더 테스트로 확인) |
| [#122](https://github.com/dj258255/b-terminal/issues/122) | 크기 표시(`resize-overlay`) | 2~3시간 | 재지 않음(병합 12:57) | [#129](https://github.com/dj258255/b-terminal/pull/129) | 끝 |
| [#124](https://github.com/dj258255/b-terminal/issues/124) | 칸 제목 바꾸기(`prompt_surface_title`) | 2~3시간 | 재지 않음(병합 13:02) | [#130](https://github.com/dj258255/b-terminal/pull/130) | 끝(실제 키로는 해 보지 않음) |
| [#125](https://github.com/dj258255/b-terminal/issues/125) | 화면을 파일로(`write_*_file`) | 2~3시간 | 재지 않음(병합 13:07) | [#131](https://github.com/dj258255/b-terminal/pull/131) | 끝(실제 키로는 해 보지 않음) |

이번에 배운 것:

- 검사 명령을 `;`로 이으면 fmt가 실패해도 커밋·올리기가 이어진다(#128에서 fmt가 깨진 채 올림). 배포 스크립트가 올리기 전에 fmt·clippy·작업 트리를 직접 보게 했다.
- 스크립트로 글을 바꿔 넣을 때, fmt가 줄 모양을 바꾸면 바꿔 넣기가 조용히 빠진다(#124). 바꿔 넣을 때마다 맞은 수를 확인한다.
- 설정 해석기가 값을 다듬으면(`trim`) 공백 자체가 뜻인 값(`selection-word-chars`)이 망가진다(#126).
- 커밋 보안 검토 기록 파일은 비워질 수 있다. 결과는 본 즉시 PR에 남긴다(#131).

## 0.9.0 (2026-11-09)

그림·커서·여백. 추적 이슈 [#133](https://github.com/dj258255/b-terminal/issues/133). 사람 없이 코딩 에이전트가 진행했다. 이슈마다 실제 시간은 재지 않았고 병합 시각(KST)만 적는다: 추적 이슈를 연 13:31부터 마지막 PR 병합 20:40까지(중간에 대화가 멈춘 시간 포함).

| 이슈 | 내용 | 예상 | 실제 | PR | 상태 |
|---|---|---|---|---|---|
| [#135](https://github.com/dj258255/b-terminal/issues/135) | 커서 키(`cursor-opacity`, `cell-foreground`) | 2시간 | 재지 않음(병합 14:18) | [#139](https://github.com/dj258255/b-terminal/pull/139) | 끝 |
| [#136](https://github.com/dj258255/b-terminal/issues/136) | 여백 키(`window-padding-balance`·`-color`) | 2~3시간 | 재지 않음(병합 14:25) | [#140](https://github.com/dj258255/b-terminal/pull/140) | 끝 |
| [#138](https://github.com/dj258255/b-terminal/issues/138) | 키보드로 고르기 늘리기·링크 복사 | 2~3시간 | 재지 않음(병합 14:30) | [#141](https://github.com/dj258255/b-terminal/pull/141) | 끝(실제 키로는 해 보지 않음) |
| [#137](https://github.com/dj258255/b-terminal/issues/137) | 프롬프트를 눌러 커서 옮기기 | 2~3시간 | 재지 않음(병합 20:31) | [#142](https://github.com/dj258255/b-terminal/pull/142) | 끝(실제 마우스로는 해 보지 않음) |
| [#134](https://github.com/dj258255/b-terminal/issues/134) | 배경 그림(`background-image`) | 3~4시간 | 재지 않음(병합 20:40) | [#143](https://github.com/dj258255/b-terminal/pull/143) | 끝 |

이번에 배운 것:

- 캔버스에 "기본 바탕색을 칠할 때만 다른 픽셀"이라는 갈래 하나를 두면, 칸 바탕·여백·화면 지우기·바뀐 줄 다시 그리기가 한꺼번에 따라온다(투명 창 #86에 이어 배경 그림 #134).
- `sips -c`는 가운데를 기준으로 자른다. 위쪽을 보려면 전체 캡처를 본다(#136).
- 스크립트로 문서 주석 바로 아래에 새 함수를 끼우면 주석이 새 함수에 붙는다(#135·#134). 끼울 자리는 주석 위로 잡는다.

## 0.10.0 (2026-11-09)

Ghostty 키 채우기 3. 추적 이슈 [#145](https://github.com/dj258255/b-terminal/issues/145). 범수 님 요청("멀티에이전트 여러개해서 구현 다해줘")으로 하위 이슈 여섯을 에이전트 여섯이 동시에 각자의 worktree에서 구현하고(커밋까지), 주 세션이 최신 main 위로 옮기고 검토·보완한 뒤 하나씩 병합했다. 에이전트는 각각 약 5~7분 걸렸다(작업 알림의 시간). 병합 시각(KST): 추적 이슈를 연 20:43부터 마지막 병합 21:10까지.

| 이슈 | 내용 | 예상 | 실제 | PR | 주 세션이 고친 것 |
|---|---|---|---|---|---|
| [#147](https://github.com/dj258255/b-terminal/issues/147) | 맨 아래로 스크롤(`scroll-to-bottom`) | 2~3시간 | 에이전트 약 5분(병합 20:53) | [#152](https://github.com/dj258255/b-terminal/pull/152) | 없음 |
| [#146](https://github.com/dj258255/b-terminal/issues/146) | 클립보드·오른쪽 클릭 키 | 2~3시간 | 에이전트 약 5분(병합 20:57) | [#153](https://github.com/dj258255/b-terminal/pull/153) | 기본값을 Ghostty 소스로 확인, 1.4 값 이름 받기 |
| [#150](https://github.com/dj258255/b-terminal/issues/150) | 팔레트 사용자 항목 | 2~3시간 | 에이전트 약 6분(병합 20:59) | [#154](https://github.com/dj258255/b-terminal/pull/154) | 없음 |
| [#148](https://github.com/dj258255/b-terminal/issues/148) | 시작 창 키 | 2~3시간 | 에이전트 약 6분(병합 21:03) | [#155](https://github.com/dj258255/b-terminal/pull/155) | macOS 창 자리를 보이는 영역 기준으로, 실제 창 확인 |
| [#149](https://github.com/dj258255/b-terminal/issues/149) | 키 묶음 접두사·catch_all | 2~3시간 | 에이전트 약 7분(병합 21:06) | [#156](https://github.com/dj258255/b-terminal/pull/156) | Ghostty 소스 규칙 넷과 테스트 |
| [#151](https://github.com/dj258255/b-terminal/issues/151) | 제목·색 보고(ADR-012) | 2~3시간 | 에이전트 약 6분(병합 21:10) | [#157](https://github.com/dj258255/b-terminal/pull/157) | 충돌 여섯 파일 풀기, 보안 검토 |

이번에 배운 것:

- 에이전트가 읽은 Ghostty 문서는 요약이거나 일부뿐이라 기본값·세부 규칙이 빠지곤 했다. Ghostty 소스의 `src/config/Config.zig`(설정과 키 묶음 설명, 기본값)를 받아 대조하면 확실하다. 여섯 중 셋을 이것으로 고쳤다.
- 같은 파일 같은 자리에 여럿이 덧붙이면 병합 순서대로 충돌이 커진다(마지막 브랜치는 여섯 파일). 대부분 둘 다 남기면 되지만, 괄호를 사이에 두는 자리는 손으로 맞춰야 한다.
- zsh에서는 `$B:path`의 `:p` 같은 꼴이 변수 꾸밈으로 읽혀 빈 값이 된다. `${B}:path`로 쓴다.
- worktree에서 `gh pr merge --delete-branch`는 로컬 main으로 바꾸려다 실패하지만 원격 병합과 브랜치 지우기는 된다.

## 0.11.0 (2026-11-09)

Ghostty 키 채우기 4. 추적 이슈 [#159](https://github.com/dj258255/b-terminal/issues/159). 0.10과 같은 방식(에이전트 여섯이 각자 worktree에서 구현·커밋, 주 세션이 옮기고 검토·병합). 이번에는 처음부터 Ghostty 소스 `Config.zig`를 에이전트에게 읽혔다. 병합 시각(KST): 추적 이슈를 연 21:36부터 마지막 병합 21:59까지.

| 이슈 | 내용 | 예상 | 실제 | PR | 주 세션이 한 것 |
|---|---|---|---|---|---|
| [#160](https://github.com/dj258255/b-terminal/issues/160) | 창·탭 이어받기, 탭 막대 | 2~3시간 | 에이전트 약 4분(병합 21:45) | [#166](https://github.com/dj258255/b-terminal/pull/166) | 빠진 되돌림 확인, 실제 창 |
| [#165](https://github.com/dj258255/b-terminal/issues/165) | 소리 벨 | 2~3시간 | 에이전트 약 5분(병합 21:48) | [#167](https://github.com/dj258255/b-terminal/pull/167) | 커밋 섞임 확인 |
| [#161](https://github.com/dj258255/b-terminal/issues/161) | 명령 끝 알림 | 2~3시간 | 에이전트 약 6분(병합 21:50) | [#168](https://github.com/dj258255/b-terminal/pull/168) | 충돌(소리 벨과 같은 자리) |
| [#162](https://github.com/dj258255/b-terminal/issues/162) | 선택·색 키 | 2~3시간 | 에이전트 약 7분(병합 21:53) | [#169](https://github.com/dj258255/b-terminal/pull/169) | 소스 대조, 실제 창 |
| [#163](https://github.com/dj258255/b-terminal/issues/163) | 글꼴 미세 조정 | 2~3시간 | 에이전트 약 8분(병합 21:56) | [#170](https://github.com/dj258255/b-terminal/pull/170) | 테스트 파일 합치기, 실제 창 |
| [#164](https://github.com/dj258255/b-terminal/issues/164) | 닫은 것 되살리기 | 2~3시간 | 에이전트 약 8분(병합 21:59) | [#171](https://github.com/dj258255/b-terminal/pull/171) | 기본 키를 소스와 대조 |

이번에 배운 것:

- 에이전트에게 처음부터 Ghostty 소스를 읽히니, 주 세션이 고쳐야 했던 규칙 차이가 0.10의 셋에서 0으로 줄었다.
- 공유 임시 폴더(scratchpad)에서 에이전트끼리 같은 파일 이름(`app.rs.good`)을 써서 서로 덮어쓴 일이 있었다. 커밋에는 섞이지 않았지만, 에이전트마다 임시 파일 이름에 이슈 번호를 붙이게 해야 한다.
- 기본값을 Ghostty와 맞추면 사용자 동작이 바뀌는 경우(알림 `never`)는 "적지 않음"을 따로 두어 지금 동작을 지키는 쪽이 낫다(#161).

## 0.12.0 (2026-11-09)

Ghostty 키 채우기 5. 추적 이슈 [#173](https://github.com/dj258255/b-terminal/issues/173). 0.11과 같은 방식으로 진행했습니다. 에이전트 일곱이 각자 worktree에서 구현·커밋하고, 주 세션이 옮기고 검토·병합했습니다. 병합 시각(KST)은 추적 이슈를 연 22:32부터 마지막 병합 23:12까지입니다.

| 이슈 | 내용 | 예상 | 실제 | PR | 주 세션이 한 것 |
|---|---|---|---|---|---|
| [#174](https://github.com/dj258255/b-terminal/issues/174) | 글꼴 고르기 | 2~4시간 | 에이전트 시간 기록 없음(병합 23:01) | [#185](https://github.com/dj258255/b-terminal/pull/185) | 테스트 파일 합치기, 소스 대조, 실제 창 |
| [#175](https://github.com/dj258255/b-terminal/issues/175) | 마우스 키 | 2~4시간 | 에이전트 시간 기록 없음(병합 22:41) | [#181](https://github.com/dj258255/b-terminal/pull/181) | 소스 대조 |
| [#176](https://github.com/dj258255/b-terminal/issues/176) | VT 제한 키(보안 경계) | 2~4시간 | 에이전트 약 12분(병합 23:08) | [#186](https://github.com/dj258255/b-terminal/pull/186) | 보안 관점 diff 읽기, 비네이티브 전체 화면도 무시 조건에, 커밋 보안 리뷰 |
| [#177](https://github.com/dj258255/b-terminal/issues/177) | 스크롤백·팔레트 | 2~4시간 | 에이전트 시간 기록 없음(병합 22:51) | [#183](https://github.com/dj258255/b-terminal/pull/183) | 스크롤백 기본을 릴리스된 값(10MB)으로 되돌림 |
| [#178](https://github.com/dj258255/b-terminal/issues/178) | 명령 실행·창 수명 | 2~4시간 | 에이전트 시간 기록 없음(병합 22:58) | [#184](https://github.com/dj258255/b-terminal/pull/184) | 실제 창(마지막 창 닫기, 빨리 끝난 칸) |
| [#179](https://github.com/dj258255/b-terminal/issues/179) | macOS 창 | 2~4시간 | 에이전트 약 5분(병합 22:49) | [#182](https://github.com/dj258255/b-terminal/pull/182) | 리눅스 clippy dead_code 고침, 실제 창 |
| [#180](https://github.com/dj258255/b-terminal/issues/180) | 글자 묶음 폭(mode 2027) | 2~4시간 | 에이전트 약 11분(병합 23:12) | [#187](https://github.com/dj258255/b-terminal/pull/187) | 이어 붙이기 O(n²) 상한, vendor 원본 diff 대조, 실제 창 |

이번에 배운 것:

- Ghostty 소스의 미출시(1.4) 기본값을 그대로 옮기면 안 되는 경우가 있었습니다. 스크롤백 50MB는 같은 판에 들어간 스크롤백 압축을 전제로 한 값이었습니다(#177). 기본값은 릴리스된 판을 따르고, 이름만 1.4 것을 받는 원칙을 에이전트 지시에도 적어야 합니다.
- 엔진을 고치는 패치(#180)는 커밋 보안 리뷰가 O(n²) 처리 시간을 잡았습니다. 결합 문자 2만 개에 3.1초가 걸렸습니다. 출력이 곧 공격 입력인 터미널에서는, 글자마다 도는 경로에 "앞 상태 전체를 다시 보는" 일이 있으면 상한을 함께 두어야 합니다.
- 실창 확인을 격리(HOME, 소켓 폴더)해서 하려면 소켓 경로가 짧아야 합니다. 임시 폴더가 길면 SUN_LEN을 넘어 제어 소켓이 열리지 않습니다.
- macOS 전용 코드에서만 쓰는 함수는 리눅스 CI에서 dead_code가 됩니다(#119에 이어 #182에서 또). 에이전트 지시에 `cfg_attr`을 미리 적어 둘 만합니다.

## 0.13.0 (2026-11-10)

Ghostty 키 채우기 6. 추적 이슈 [#189](https://github.com/dj258255/b-terminal/issues/189). 0.12와 같은 방식으로 진행했습니다. 에이전트 일곱이 각자 worktree에서 구현·커밋하고, 주 세션이 옮기고 검토·병합했습니다. 병합 시각(KST)은 추적 이슈를 연 11-09 23:27부터 마지막 병합 11-10 00:46까지입니다. 대화가 길어져 세션이 이어지는 사이에 마지막 두 PR(#202, #203)은 이어진 세션에서 확인·병합했습니다.

| 이슈 | 내용 | 예상 | 실제 | PR | 주 세션이 한 것 |
|---|---|---|---|---|---|
| [#190](https://github.com/dj258255/b-terminal/issues/190) | 환경·시작 입력(`term`, `env`, `input`) | 2~4시간 | 에이전트 약 7분(병합 23:45) | [#199](https://github.com/dj258255/b-terminal/pull/199) | ADR 번호를 병합 순서로 옮김(016), decisions.md 합치기, 커밋 보안 리뷰, 실제 창 |
| [#191](https://github.com/dj258255/b-terminal/issues/191) | 링크 키 | 2~4시간 | 에이전트 약 4분(병합 23:35) | [#197](https://github.com/dj258255/b-terminal/pull/197) | 소스 대조(실제 창은 Cmd를 누른 채 올려야 해서 못 봄) |
| [#192](https://github.com/dj258255/b-terminal/issues/192) | 창 모양 | 2~4시간 | 에이전트 약 9분(병합 00:40) | [#202](https://github.com/dj258255/b-terminal/pull/202) | 실제 창에서 `hidden`의 제목 겹침을 찾아 테스트 먼저 쓰고 고침 |
| [#193](https://github.com/dj258255/b-terminal/issues/193) | 글꼴 합성·두껍게 | 2~4시간 | 에이전트 약 9분(병합 23:55) | [#201](https://github.com/dj258255/b-terminal/pull/201) | 소스 대조, 실제 창(Monaco 합성 켬·끔) |
| [#194](https://github.com/dj258255/b-terminal/issues/194) | 셸 통합 기능 | 2~4시간 | 에이전트 약 7분(병합 23:50) | [#200](https://github.com/dj258255/b-terminal/pull/200) | 스크립트 diff 읽기, ADR 번호 옮김(017), 커밋 보안 리뷰, 실제 창 |
| [#195](https://github.com/dj258255/b-terminal/issues/195) | 스크롤바·진행 표시·분할 확대 | 2~4시간 | 에이전트 약 6분(병합 23:39) | [#198](https://github.com/dj258255/b-terminal/pull/198) | 소스 대조, 실제 창(진행 표시 켬·끔) |
| [#196](https://github.com/dj258255/b-terminal/issues/196) | 수정 키 바꾸기, Space, Dock 아이콘, 플랫폼 전용 키 | 2~4시간 | 에이전트 약 10분(병합 00:46) | [#203](https://github.com/dj258255/b-terminal/pull/203) | 충돌 네 곳 정리, 받기만 목록에서 이미 구현된 키 빼기, 실제 창(경고 없음) |

이번에 배운 것:

- 에이전트는 창을 띄우지 않으므로, 화면에서만 보이는 문제는 주 세션의 실제 창 확인이 잡습니다. `hidden` 제목 막대에서 신호등 단추만 숨기고 창 제목 글은 남아 격자 첫 줄 위에 겹쳤습니다(#192). 단위 테스트는 "단추를 숨긴다"만 봤습니다.
- 여러 브랜치가 같은 목록(`KNOWN_IGNORED`, 문서의 "경고 없이 넘기는 키")을 고치면, 나중에 병합하는 쪽이 이미 구현된 키를 다시 "받기만"으로 되돌릴 수 있습니다(#196 충돌). 목록과 해석 코드를 스크립트로 대조해 겹치는 키가 없는 것을 확인했습니다.
- 배포 스크립트(`ship.sh`)와 충돌 해결 스크립트를 세션 임시 폴더에만 두었더니, 세션이 이어질 때 폴더가 비어 대화 기록에서 되살려야 했습니다.
- worktree를 일곱 개 한꺼번에 만들면 경합으로 하나가 실패할 수 있습니다(#194, 남은 브랜치를 지우고 다시 띄움).

## 다음

- 범수 님이 직접 해야 할 것: 키보드·마우스 확인(한글 2벌식 "안녕 하세요⏎", Claude Code의 Shift+Enter, Cmd-D·Cmd-F·Cmd-클릭·Cmd-N·Cmd-Shift-P, 메뉴, 창 닫기 단추, vim·htop, kitty icat·yazi, 투명 창), Developer ID 계정으로 `scripts/bundle-macos.sh --sign … --notarize … --dmg`
- 범수 님이 정할 것: `window-colorspace` 기본값(ADR-009)
- 범수 님이 확인할 것(0.6): 퀵 터미널을 실제 전역 단축키로(`keybind = global:ctrl+backquote=toggle_quick_terminal`), 외부 모니터로 창을 옮겼을 때 present 시간(`RUST_LOG=b_terminal=debug`)
- 범수 님이 확인할 것(0.7): `sleep 100` 중에 ⌘W → 확인 막대 → esc는 그대로·↩는 닫힘, 프롬프트에서 ⌘W는 바로 닫힘, ⌘Q도 같게. 셸에서 `cat` 중에 두 줄 글 ⌘V → 붙여넣기 확인 막대. `keybind = ctrl+a>n=new_window` 뒤 Ctrl-A·N → 새 창, Ctrl-A·X → `^A`와 `x`가 셸로. 다른 탭에서 `sleep 2; printf '\a'` → 탭 제목에 🔔
- 범수 님이 확인할 것(0.8): 메뉴 "셸 › 제목 바꾸기…"로 탭 제목 붙이고 비우기, 팔레트 "스크롤백을 파일로 열기"와 앱을 끈 뒤 `$TMPDIR/b-terminal-$(id -u)/files`가 비었는지, `focus-follows-mouse = true`로 분할 칸 위를 마우스로 오가기, 단어 두 번 눌러 고르기(`;`·`$`에서 끊김), ⌃⌘F 전체 화면
- 범수 님이 확인할 것(0.9): zsh 프롬프트에서 친 명령의 한 글자를 눌러 커서가 거기로 가는지, `adjust_selection`을 키에 묶어 고르기를 늘려 보기, 배경 그림을 투명 창(`background-opacity = 0.8`)과 함께 써 보기
- 범수 님이 확인할 것(0.10): 고른 글 ⌘C 뒤 줄 끝 공백이 없는지, `right-click-action = copy-or-paste`, 팔레트 "창 항상 위", `keybind = ctrl+a>catch_all=ignore`와 `ctrl+a>n=new_window`, `scroll-to-bottom = output`으로 긴 출력 중 위로 스크롤
- 범수 님이 확인할 것(0.11): 탭을 닫고 5초 안에 ⌘Z, `notify-on-command-finish = unfocused`로 긴 명령 뒤 다른 앱으로, `bell-features = audio`와 `bell-audio-path`로 벨 소리, ⌘F 찾기 강조 색, 고른 뒤 키를 치면 고르기가 지워지는지
- 범수 님이 확인할 것(0.12): `vt-window-resize-allowed = true`에서 `printf '\033[8;20;60t'`로 창 크기가 바뀌는지, `macos-non-native-fullscreen = true`에서 ⌃⌘F, `window-step-resize = true`에서 가장자리 끌기, 빨리 끝난 칸에서 아무 키나 눌러 닫기, `palette-generate = true`로 256색 표, tmux 안의 가족 이모지 뒤 커서 자리
- 범수 님이 확인할 것(0.13): OSC 8 링크를 Cmd를 누른 채 올려 `link-previews = osc8`·`false`에서 주소 띠가 어떻게 나오는지, `seq 1 500` 뒤 휠로 올려 스크롤 막대가 나왔다 사라지는지, `key-remap = left_ctrl=super`와 `keybind = super+t=new_tab`으로 왼쪽 Ctrl-T(오른쪽 Ctrl-C는 그대로 SIGINT), `macos-icon = custom`과 `macos-custom-icon`으로 Dock 아이콘, `quick-terminal-space-behavior = remain`에서 Space 옮기기, `macos-titlebar-style = hidden`에서 창 위쪽 끌기, `font-thicken = true`의 두께
- 큰 일로 남긴 것: GPU(또는 IOSurface) 렌더러
