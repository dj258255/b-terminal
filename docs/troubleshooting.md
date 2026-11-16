# 문제 해결

겪은 증상과 원인, 해결을 번호로 적습니다. 번호는 병합 순서로 붙입니다.

## 1. 셸에 `35;15;35M35;8;32M…`가 쏟아지고 `command not found`가 줄줄이 뜬다

- 언제: 2026-11-06 00:36, 범수 님 herdr의 셸 칸(Ghostty 안). 그 칸에서 돌던 프로그램(claude로 보임)이 끝난 뒤 마우스를 칸 위에서 움직일 때마다
- 원인: 프로그램이 마우스 추적 모드(`ESC[?1003h` 움직임 보고 + `ESC[?1006h` SGR 꼴)를 켠 채 끝났다. 터미널은 계속 마우스 움직임을 `ESC[<35;x;yM`으로 보내고, 셸은 그것을 글자로 받는다. 터미널 탓도 셸 탓도 아니고 프로그램이 정리하지 못한 것이다(강제 종료되면 정리할 기회가 없다)
- 바로 푸는 법: 마우스를 그 칸 밖에 두고 `reset` + Enter
- b-terminal: 탭의 전경 프로세스가 셸 자신으로 돌아왔을 때 마우스 보고 모드가 켜져 있으면 끈다(#24). 1초마다 보므로 돌아온 뒤 1초 안의 움직임은 그대로 들어갈 수 있다. 끄려면 설정에 `mouse-reset-on-shell = false`

## 2. 다른 탭의 제목에 이모지가 들어오면 터미널이 통째로 꺼진다

- 언제: 2026-11-09 09:49, #107(벨) 확인 중. 보이지 않는 탭의 제목 앞에 🔔를 붙이자 바로 꺼졌다. 0.6.0까지는 프로그램이 `printf '\e]2;😀\a'`처럼 탭 제목에 색 이모지를 넣어도 같은 일이 난다(탭이 둘 이상이라 탭 막대가 보일 때)
- 원인: 탭 막대·찾기 막대·팔레트·토스트의 글을 그리는 `draw_text`가 색 이모지 글리프도 흑백 그리기(`draw_glyph`)로 넘겼다. 색 글리프는 흑백 비트맵이 비어 있어 첫 픽셀에서 범위를 벗어나 패닉했다(`canvas.rs:116 index out of bounds: the len is 0`)
- b-terminal: 색 글리프는 색으로 그리고, 흑백 그리기는 비트맵이 모자라면 그리지 않는다(#112)

## 3. OSC 11로 바탕색을 바꾼 뒤 같은 색을 물으면 옛 색이 온다

- 언제: 2026-11-10, #266(OSC 21) 확인 중 격리한 창에서 한 번 잼. `OSC 11;#ff0000 ST` 뒤 `OSC 11;? ST`가 `rgb:2828/2c2c/3434`(테마)로 왔고, OSC 4;1과 OSC 10도 같았다. 화면은 바뀐 색으로 그려졌다
- 원인: 엔진(alacritty)은 색 질의 사건을 읽기 스레드가 `Term`을 잠근 채 보낸다. 사건을 받는 `Listener`는 `Term::colors()`(OSC 4·10·11·12·21이 바꾼 색)를 읽을 수 없어 띄울 때의 테마에서 색을 찾았다. 그리는 쪽은 `Term::colors()`를 읽어 화면만 맞았고, OSC 21 질의(읽기 스레드가 직접 답함)와도 어긋났다. vim처럼 바탕색으로 밝은·어두운 테마를 고르는 프로그램이 색을 바꾼 뒤 틀린 값을 받는다
- b-terminal: vendor `alacritty_terminal`의 질의 사건(`Event::ColorRequest`)에 프로그램이 바꾼 색(없으면 `None`)을 실어 보낸다. `Listener`는 실린 색이 있으면 그것으로, 없으면 테마로 답한다. 답 순서는 건드리지 않았다(#272)

## 4. OSC 10으로 글자색을 바꾼 뒤 커서 색을 물으면 옛 글자색이 온다

- 언제: 2026-11-10, #272(OSC 질의) 뒤 격리한 창에서 한 번 잼. `OSC 10;#123456 ST` 뒤 `OSC 12;? ST`가 `rgb:ffff/ffff/ffff`(테마 글자색)로 왔다. Ghostty는 `rgb:1212/3434/5656`으로 답한다. 같은 자리에서 `cursor-style = bar` 설정으로 `CSI 4 SP q`를 보낸 뒤 DECSTR(`CSI ! p`)를 보내면 DECRQSS ` q`가 `4 q`(바꾼 모양 그대로)로 왔다. DECSCUSR 0과 RIS는 설정 모양(`6 q`)으로 맞았다
- 원인: 색은 #272가 엔진의 바꾼 색을 사건에 실어 보내게 했지만 커서 칸(인덱스 258)에는 바뀐 색이 없어 테마 커서 색(기본은 글자색)으로 떨어졌다. Ghostty의 커서 색은 `cursor.get() orelse foreground.get()`로 둘 다 지금 값이다. 모양은 DECSTR만 팔레트만 되돌리고 `set_cursor_style(None)`(DECSCUSR 0이 쓰는 길)을 부르지 않았다
- b-terminal: 사건에 바꾼 글자색도 싣고(vendor), 커서 색을 정한 적 없고 테마도 커서를 글자색과 다르게 정하지 않았으면 그 글자색으로 답한다. DECSTR은 팔레트와 함께 커서 모양·깜박임을 설정 값으로 되돌린다(#286). 한계: `cursor-color`를 글자색과 같은 값으로 명시해도 구별하지 못해 바뀐 글자색을 따른다

## 5. bash에서 빈 줄에 Enter만 쳐도 명령 하나가 돈 것으로 보인다

- 언제: 2026-11-10, #338(프롬프트 클릭) 검토 중 macOS `/bin/bash` 3.2.57을 PTY에 띄워 셸 통합 바이트를 한 번 읽음. 빈 Enter 뒤에 `OSC 633;E;_bt_status`, `OSC 133;C`, `OSC 133;D;0`이 나왔다. 셸 통합을 처음 넣은 #50의 스크립트도 같은 덫 구조다(코드로 읽었고 그 판을 돌려 보지는 않았다)
- 원인: bash는 빈 Enter에서 명령 없이 `PROMPT_COMMAND`(`_bt_status;…;_bt_precmd`)를 다시 돌린다. DEBUG 덫 `_bt_preexec`는 그 첫 명령 `_bt_status` 앞에서도 불리는데, 프롬프트에 있다는 표시(`_bt_at_prompt`)가 켜져 있어 사람이 친 명령으로 셌다. 명령 글이 `_bt_status`인 짧은 명령이 하나 생겨 제목 기능과 명령 경계에 들어갔다
- b-terminal: 덫이 `_bt_status` 앞에서 불리면 PS1만 표지 전 값으로 되돌리고 명령 표지를 보내지 않는다(#342). Ghostty가 bash 4.4 미만에서 쓰는 `bash-preexec.sh`도 `PROMPT_COMMAND` 안의 명령에서는 preexec를 부르지 않는다(`__bp_in_prompt_command`)

## 6. PTY 시험이 가끔 `Session::spawn`에서 실패한다 (원인 미확인)

- 언제: 0.28.0부터 릴리스 시험마다 한 번꼴. `cargo test --workspace` 안에서 bt-term PTY 시험 하나가 `expect("spawn")`에서 패닉했고 같은 파일만 다시 돌리면 통과했다. 0.31까지는 오류 본문을 남기지 않았다(#365). 2026-11-11 #364 작업 중 한 번 `kitty_clip_session::full_reset_turns_mode_5522_off`가 `spawn: failed to openpty`로 실패했다(openpty 단계. errno는 보고에 없었다). 그 무렵 #365 재현 작업이 pty를 한도까지 여는 실험을 했는지 시각을 맞춰 보지 않아, 이 한 번이 그 실험 때문인지는 모른다
- 0.33(2026-11-11)에서 두 번 났고, 이번에는 #366 덕분에 단계가 남았다: 둘 다 `PTY 열기(openpty): failed to openpty: Os { code: -6, kind: Uncategorized, message: "Unknown error: -6" }`. 하나는 #372 작업 중 `kitty_clip_session::the_fixed_one_mib_cap_applies_even_when_the_limit_is_unlimited`, 하나는 0.33.0 릴리스 검사의 `session::dcs_answers_still_come_from_a_readonly_pane`이다. 둘 다 `cargo test --workspace` 전체를 돌릴 때였고 같은 파일만 다시 돌리면 통과했다. portable-pty 0.9.0은 `libc::openpty`가 0이 아닌 값을 돌려주면 그때의 errno(`io::Error::last_os_error`)를 찍는다(`src/unix.rs` 45~46줄). macOS errno는 양수인데 -6이 찍혔으니, errno가 `openpty` 안에서 정상적으로 정해지지 않았을 수 있다. 위의 ENXIO(한도 초과)와는 다른 실패다
- 0.34(2026-11-11) 릴리스 검사에서 한 번 더 났다. `kitty_clip_session::write_over_the_limit_answers_efbig_and_never_reaches_the_app`가 같은 `Os { code: -6 }`이었다. 이번 판에는 PTY 코드를 바꾸지 않았고, 같은 파일만 다시 돌리니 13개 모두 통과했다. 이번에도 `cargo test --workspace` 전체를 돌릴 때였고, 기기를 다시 켠 지 약 8시간 뒤였다
- 재현 시도(2026-11-11, macOS 26, M2 Pro): `cargo test --workspace` 3번, bt-term 시험 바이너리 19개를 동시에 30번(570개 프로세스 실행), 한 프로세스에서 16스레드가 `Session::spawn`을 2400번. 모두 실패 0번. 재현하지 못했다
- 확인한 것: 시험 전체가 쓰는 pty는 최대 약 9개(`/dev/ttys*` 24개에서 33개)이고 `kern.tty.ptmx_max`는 511이다. 일부러 세션을 쥔 채 511개를 넘기면 `failed to openpty: Device not configured`(ENXIO)가 난다. portable-pty는 없는 작업 폴더를 홈으로 바꾸므로 작업 폴더 문제는 띄우기 실패가 아니다
- 짐작(확인 안 함): 같은 기계에서 다른 작업이 pty나 프로세스 한도를 거의 채웠을 때 일시적으로 `openpty`/`fork`가 실패. 또는 portable-pty `pre_exec`의 `setsid`/`TIOCSCTTY` 경쟁
- b-terminal: 원인을 모르므로 조용히 다시 시도하지 않는다. 대신 다음에 나면 원인이 남게 했다. 띄우기 오류에 단계와 프로그램·작업 폴더를 붙이고(`Session::spawn`) 시험은 `{e:#}`로 사슬 전체를 찍는다

## 7. 120Hz 화면인데 빠른 출력 중에는 16.6ms마다 한 장씩만 그린다

- 언제: 2026-11-11, #429. Apple M2 Pro 내장 120Hz 화면에서 `seq 1 300000`을 돌릴 때. 앱은 화면에서 120Hz(8.33ms)를 들었지만, 미룸을 끈 실험 빌드에서 그림판이 16.6ms마다 돌아왔다. 그때 저전력 모드가 켜져 있었다(`pmset -g`의 `lowpowermode 1`). 끈 상태는 재지 않았다
- 원인: 앱 밖(화면·합성기)이다. 그림판 수(2·3·4장)를 바꿔도 같았고, 앱의 프레임 미룸은 이 간격을 배워 따라갈 뿐이다. 저전력 모드에서 ProMotion 화면이 60Hz까지로 묶인다는 사용자 보고가 있지만 Apple 공식 문서는 찾지 못했고, 끈 상태로 견주지 않아 확정하지 않았다(실험 E19)
- 확인하는 법: `RUST_LOG=b_terminal=debug,bt_gpu=info`로 띄우면 "화면 한 장 간격: …ms"(앱이 들은 값)와 "다시 그리기" 줄의 `얻기`, 누적 "미룬 프레임"이 찍힌다. 출력 중 미룬 프레임이 거의 매 장 늘면 화면이 들은 값보다 느리게 그림판을 돌려주는 것이다
- b-terminal: 고치지 않았다. 높은 주사율을 바라는 길(`CADisplayLink`)은 `unsafe`가 필요하고, `unsafe_code = "deny"`를 풀지는 범수 님이 정한다

## 8. GPU 길에서 출력이 끝난 뒤 마지막 화면이 보이지 않는다

- 언제: 2026-11-12, #447. Apple M2 Pro 내장 120Hz 화면, 창을 띄운 직후. 셸에 `clear; echo pane`을 보내 셸이 출력을 마쳤는데, 3초 뒤에도 창에는 `clear`를 친 줄이 남아 있었다. 셰이더와 상관없이 0.39.0부터 났다(0.35 이전은 재지 않음). 회마다 새로 띄운 main 빌드에서 25회 중 11회였다
- 원인: 앱은 마지막 프레임을 올렸다(로그의 `Presented`). 그림판(drawable)을 둘까지만 쓰게 한 설정(0.36부터)에서, 화면이 한 장을 오래 쥔 때 올린 프레임이 화면에 붙지 않았다. 그때마다 footprint가 그림판 한 장(2000×1360px에서 10.88MB)만큼 컸다. 합성기 안에서 왜 붙지 않는지는 확인하지 못했다(실험 E22)
- 확인하는 법: `RUST_LOG=b_terminal=debug,bt_gpu=info`로 띄워 "다시 그리기" 마지막 줄이 `Presented`인데 화면이 그 앞 내용이면 이 일이다. `BT_FRAME_LATENCY=2`로 띄워 사라지면 같은 원인이다
- b-terminal: 앱의 프레임 미룸 줄 깊이(1)는 그대로 두고 surface에는 그림판을 셋까지 허락했다. 평소에는 둘만 쓰고, 화면이 한 장을 오래 쥔 때만 셋째가 생긴다(셰이더 없는 창 10회 중 4회). 셰이더를 건 창은 늘 셋째를 써 약 10MB 크다. `window-vsync = false`(Immediate)에서는 이렇게 해도 네 회 중 세 회 남았다. 이것은 9번에서 고쳤다

## 9. `window-vsync = false`에서 출력 뒤 마지막 화면이 멈춘다

- 언제: 2026-11-12, #451. `window-vsync = false`로 띄운 GPU 길 창(Apple M2 Pro 내장 120Hz, macOS 26, release, 2000×1360px). 회마다 새로 띄워 `clear; echo pane`을 보내고 3초 뒤 찍었을 때 0.46.0은 16회 중 14회 옛 화면이었다. 8번의 그림판 셋째 장으로도 남던 것이다
- 원인: 올리기 방식 `Immediate`에서 프레임 미룸을 끄면, 화면 주기보다 빨리 그림판을 몰아 올려 마지막 올림이 화면에 붙지 않는다. 합성기 안에서 왜 그런지는 확인하지 못했다. 앱이 넘긴 화면 내용은 맞았다(진단 로그로 확인). 실험 빌드에서 가른 것: Immediate+미룸 끔 19회 중 17회, Immediate+미룸 켬 11회 중 0회, Fifo+미룸 끔 6회 중 3회, Fifo+미룸 켬 6회 중 0회. 마지막 프레임 16ms 뒤에 같은 내용을 한 번 더 올려도 사라졌지만(12회 중 0회), 올린 뒤 GPU를 기다리는 것으로는 남았다(6회 중 5회). 옛 화면 회는 모두 footprint가 그림판 한두 장만큼 컸다(2000×1360px에서 107~121MB, 새 화면 회는 86~101MB)
- 확인하는 법: `RUST_LOG=b_terminal=debug,bt_gpu=info`로 띄워 "다시 그리기" 마지막 줄이 `Presented`인데 화면이 그 앞 내용이고 `window-vsync = false`이면 이 일이다
- b-terminal: `window-vsync = false`에서도 프레임 미룸을 켠다. 올리기 방식은 `Immediate` 그대로다(찢어질 수 있음). 고친 빌드에서 15회 중 0회였다. 대가로 빠른 출력 이득이 없어졌다: `seq 1 300000`이 0.46.0의 `false`에서 0.39~0.40초였는데 고친 빌드에서 0.56~0.58초로 `true`(0.56~0.57초)와 같다. 출력 중 그림판 얻기로 메인 스레드가 막히던 시간(합 336~342ms)은 10~15ms로 줄었다(실험 E23)

## 10. 창 폭을 바꾸면 프롬프트 위에 빈 줄이 쌓인다

- 언제: 2026-11-13, #482. 셸 통합(OSC 133)이 켜진 zsh가 프롬프트에 있을 때 창 폭을 바꿨다. 폭을 꽉 채운 줄과 `%` 줄로 된 두 줄 프롬프트에서, 124열에서 70열로 한 번이면 빈 줄 하나, 다섯 번 바꾸면 빈 줄 다섯 개가 새 프롬프트 위에 남고 앞 내용은 그만큼 스크롤백으로 밀렸다(0.52 코드, 실제 창에서 잰 값). 앞 출력은 지워지지 않았다
- 원인: 크기를 바꾼 뒤에야 프롬프트 줄을 비웠다(#329). zsh는 SIGWINCH 뒤 옛 폭에서 프롬프트가 차지한 줄 수만큼만 커서를 올려 지우고 다시 쓴다. 폭을 좁히면 줄 나누기가 프롬프트를 더 많은 줄로 나눠, 올라간 자리가 프롬프트 첫 줄보다 아래가 되고 그 위가 빈 줄로 남았다. Ghostty에도 같은 문제가 있었다(ghostty-org/ghostty #15199)
- b-terminal: 폭을 바꾸기 전에, 다시 그리기 조건이면(주 화면, `redraw`가 `0`이 아님, 커서가 출력 중이 아님) 프롬프트 첫 줄부터 다시 그릴 범위의 줄을 칸이 빈 줄로 만들고 줄 이음을 뗀다. 0번 칸의 프롬프트 표시는 남긴다. 줄 나누기가 이 줄들을 나누거나 잇지 않아 프롬프트가 옛 폭에서의 줄 수를 그대로 갖는다. 커서 칸은 새 폭 안으로 옮긴다. 폭을 바꾼 뒤의 지우기는 그대로 남겼다. 이 판에서 시험으로 확인한 것은 `Term`에 바이트를 먹이고 zsh의 다시 그리기(`ESC[nA`, `\r`, `ESC[J`)를 흉내 낸 장면이다(좁히기, 넓히기, 여러 번 오가기, 스크롤백으로 넘친 경우, 줄 수만 바꾸기, `redraw=last`·`0`, 대체 화면). 실제 창(주 세션, release, 시스템 zsh)에서 같은 프롬프트로 124 → 70열, 124 → 70 → 110 → 60 → 100 → 50열을 다시 재 빈 줄이 남지 않았다. bash(`redraw=last`, 두 줄 PS1)는 0.52와 화면 글이 같았다. 프롬프트 첫 줄 바로 위 줄이 줄 이음(WRAPLINE)으로 프롬프트에 이어진 경우는 다루지 않았다

## 11. kitty 그림을 같은 번호로 다시 보내면 옛 자리에 새 그림이 남는다

- 언제: 2026-11-14, #487. 0.53 release 실제 창에서 `a=T`로 빨강을 `i=5`로 놓고 네 줄 아래에서 같은 `i=5`로 파랑을 `a=T`로 보내니 파란 칸이 둘이었다(옛 자리도 파랑). 빨강 뒤 같은 `i=5`로 파랑을 `a=t`(보내기만)로 보내도 옛 자리에 파랑이 보였다. 단위 탐침에서도 `a=T` 두 번 뒤 `placements.len()`이 2, 이어서 `a=t` 뒤에도 2였다
- 원인: `Graphics::handle`의 `t`/`T` 갈래가 `store`만 불렀고, `store`는 그림 표에 넣기만 하고 `placements`에서 그 번호의 자리를 지우지 않았다. kitty 명세는 "When re-transmitting image data for a specific id, the existing image and all its placements must be deleted. The new data replaces the old image data but is not actually displayed until a placement for it is created."라고 한다
- b-terminal: 번호가 0이 아닌 전송이 성공하면 `remove_image`로 그 번호의 옛 그림과 모든 자리를 지운 뒤 새 그림을 저장한다(`a=T`면 새 자리 하나, `a=t`면 자리 없음). 실패해도(디코딩 오류, `ENOSPC`, 거절하는 전송 방식) 그 번호의 옛 그림과 자리를 지우고 같은 오류 코드를 답한다. 명세는 성공이든 실패든 지우라고 하지만 언제인지는 말하지 않아, Ghostty #15184처럼 나눠 보낸 전송이 끝난 뒤(성공·실패 모두)에 지운다(kitty는 첫 조각에서 지운다). 번호가 없는 전송(`i=0`)은 아무것도 지우지 않는다. 다른 번호의 그림과 자리, `a=q`·`a=p`·`a=d`는 그대로다. 시험으로 확인한 것: `a=T` 두 번 뒤 자리 하나(둘째 줄)와 새 그림 데이터, `a=T` 뒤 `a=t`면 자리 0개, 다른 번호는 남음, 잘못된 base64·크기 부족·`t=f`·저장 상한 초과 뒤 그 번호가 지워지고 오류 코드는 그대로, 번호 없는 실패 전송은 아무것도 건드리지 않음. 성공 길만 되돌리면 3개, 실패 길만 되돌리면 2개, 둘 다 되돌리면 5개 시험이 실패한다. 실제 창(주 세션, release, 같은 바이트 파일을 `cat`)에서 `a=T` 두 번은 아래쪽 파란 칸 하나만, `a=T` 뒤 `a=t`는 그림이 보이지 않았다

## 12. 한글 조합 중 창·칸이 포커스를 잃으면 조합 글자가 사라지거나 다른 칸으로 간다

- 언제: 2026-11-14, #502. 코드 읽기로 얻은 추정이다. 한글 입력을 흉내 낼 수 없어 실제 창에서 쳐 보지 않았다. 입력기 호출 순서는 Ghostty PR #15137이 `NSTextInputClient` 호출을 기록해 확인한 것이고 우리가 잰 값이 아니다
- 현상: 한국어 입력기로 `가나`를 치는 중(`나`가 조합 중)에 창이 포커스를 잃었다가 돌아와 Enter를 누르면 `가`만 들어가고 `나`가 사라질 것으로 본다. 마우스로 다른 분할 칸이나 탭을 고르면 조합 중인 `나`가 새 칸으로 따라간다
- 원인: 칸이 포커스를 잃을 때 한국어 입력기는 `insertText`도 `unmarkText`도 부르지 않고 조합을 버린다. 돌아와 Enter를 누르면 `insertText ""` 다음에 `unmarkText`가 오고, 벤더 winit의 `unmark_text`가 보내는 `Ime::Preedit("")`를 앱이 받아 `나`를 지운다. b-terminal은 분할 칸과 탭이 NSView 하나를 함께 써서 칸을 옮겨도 입력기는 모르므로 조합 글자가 새 칸으로 따라간다
- b-terminal: 창이 포커스를 잃을 때(칸에 `ESC[O`를 쓰기 전), 고른 칸이 바뀌기 전(`focus_pane_keeping`), 탭이 바뀌기 전(`activate`), 새 탭·새 분할로 고른 칸이 바뀌기 전에 `settle_preedit`을 부른다. 조합 글자가 있고 입력 소스가 `com.apple.inputmethod.Korean`으로 시작하면 조합 글자를 옛 칸에 확정 글과 같은 길(`commit_text`)로 보내고, 벤더 winit의 `discard_marked_text`로 입력기가 조합을 버리게 한다. 일본어 등은 포커스가 돌아오면 조합을 이어 가므로 건드리지 않는다(확정하면 `にほn`처럼 변환 전 글이 들어간다). 시험으로 확인한 것은 입력 소스 판별 표(한국어 2벌식·3벌식은 참, 일본어·창힐·간체 병음·ABC·없음은 거짓)뿐이다. 호출 자리와 벤더 함수는 시험으로 잡지 못하고 실제 한글 입력으로 확인하지 않았다

## 13. 한글·이모지를 지우면 반쪽 글자가 남는다

- 언제: 2026-11-15, #524. Ghostty `Terminal.zig`의 단위 시험 154개를 같은 escape 바이트로 엔진에 넣어 견줘 찾았다. 격리한 실제 창(`zsh -f`)에서는 `printf`로 고친 뒤를 확인했고(아래), 셸 줄 편집이나 vim에서 지워 보지는 않았다
- 현상: `😀a😀b😀`(8열)에서 두 번째 칸부터 ECH 3(`ESC[1;2H ESC[3X`)를 하면 Ghostty는 `     b😀`인데 우리는 `😀    b😀`처럼 첫 이모지의 앞 칸이 남았다. DCH 3은 `😀b😀`(Ghostty `  b😀`), `AB橋DE`에서 4열부터 EL 0은 `AB橋`(Ghostty `AB`), ED 0도 같다. 줄을 넘어 이어진 넓은 글자도 줄 끝의 앞 칸 자리 표시가 남았다. 셸 줄 편집(`ESC[P`·`ESC[K`)이나 vim에서 한글을 지울 때 반쪽 글자가 보일 수 있다
- 원인: `erase_chars`·`delete_chars`·`erase_line`·`clear_screen`(ED 0)이 지우는 구간의 경계가 넓은 글자(앞 칸 `WIDE_CHAR`, 뒤 칸 `WIDE_CHAR_SPACER`)를 가르는지 보지 않았다. 앞 칸만 지우면 뒤 칸이, 뒤 칸만 지우면 앞 칸이 짝 없이 남는다
- 확인하는 법: 엔진 단위 시험 `crates/bt-term/tests/ghostty_terminal.rs`의 `erase_chars_wide_char_boundary_conditions` 같은 시험. 격자에서 뒤 칸 없는 앞 칸이나 앞 칸 없는 뒤 칸을 직접 찾는다(`plain`은 짝 잃은 뒤 칸을 빈칸으로 그려 가린다)
- b-terminal: Ghostty `Screen.splitCellBoundary`와 같게 구간 양 끝의 경계를 먼저 가른다. 경계의 왼쪽 칸이 넓은 글자의 앞 칸이면 두 칸을 함께 지운다. ECH는 구간 끝이 앞 칸이면 뒤 칸까지 늘리고, EL 0·ED 0은 커서가 뒤 칸이면 앞 칸부터, EL 1·ED 1은 커서가 앞 칸이면 뒤 칸까지 지운다. 줄 끝 앞 칸 자리 표시와 이어진 줄 표시도 푼다. 격리한 실제 창에서 `printf 'AB가CD\033[4G\033[K\n'`이 고치기 전 `AB가`, 고친 뒤 `AB`로 나왔다

## 14. 칸이 1열까지 줄어든 채 한글·이모지가 찍히면 칸이 멈춘다

- 언제: 2026-11-15, #523. 코드로 만든 1열 × 80줄 격자에 `😀`(U+1F600)를 넣어 엔진 단위 시험에서 났다. 실제 창을 1열까지 줄여 보지 않았고, 앱에서 칸이 1열까지 줄어드는지도 확인하지 않았다
- 원인: 넓은 글자는 앞 칸과 뒤 칸 두 칸을 쓰는데, 1열 격자는 뒤 칸이 없어 `index out of bounds: the len is 1 but the index is 1`(`vendor/alacritty_terminal/src/grid/mod.rs`의 `cursor_cell`)로 엔진 스레드가 죽었다. 좌우 여백이 한 칸 폭일 때도 같은 자리다
- 확인하는 법: 엔진 단위 시험 `print_wide_char_in_single_width_terminal`(`crates/bt-term/tests/ghostty_terminal.rs`)
- b-terminal: 넓은 글자가 들어갈 자리가 없으면(오른쪽 한계 − 왼쪽 여백이 1 이하) 글자를 찍지 않고 커서 칸만 빈칸으로 두며 줄바꿈 대기가 된다(Ghostty `Terminal.zig` `print`와 같다). 단위 시험으로만 확인했다

## 15. kitty 그림을 쓰는 프로그램이 돌면 다른 탭·창까지 멈춘다

- 언제: 2026-11-16, #557. 0.67 병합 뒤 보안 검토에서 짚었다. 실제 창에서 겪은 사례가 아니라, 릴리스 빌드에서 그림 처리 시간을 재고(2048×2048 반투명 합성 한 번 24ms, 한 번 잰 값) 세션 시험으로 재현했다
- 원인: PTY 읽기 스레드가 한 번 읽은 덩어리(최대 64KB)를 다 처리할 때까지 세션 잠금을 쥐었다. 앱은 모든 탭을 한 스레드에서 그리고 그때마다 같은 잠금으로 화면을 읽으므로, 무거운 그림 명령 몇 개가 앱 전체를 세웠다
- 확인하는 법: 세션 시험 `heavy_graphics_commands_do_not_block_snapshots`(`crates/bt-term/tests/graphics_lock.rs`)
- b-terminal: 그림 명령에 쓴 시간이 4ms 쌓일 때마다 기다리는 화면 그리기에 잠금을 넘긴다. 명령 하나가 긴 경우는 그 명령만큼 기다린다

## 16. 화면이 한 줄일 때 여백을 정하고 커서를 옮기면 엔진이 죽는다

- 언제: 2026-11-16, #587. 0.71 보안 검토의 퍼징(시드 고정 3만 회)과 한 줄 재현에서 났다. 실제 창은 띄우지 않았고, 칸이 한 줄까지 줄어드는 일이 앱에서 얼마나 되는지, 패닉이 탭만 죽이는지 앱을 끄는지는 재지 않았다
- 원인 둘:
  - 위아래 여백(`CSI Pt ; Pb r`)이 아래 경계를 줄 수로 자르기 전에 `top >= bottom`을 봤다. 한 줄 화면의 `2;4r`이 화면 밖 영역(시작 1, 끝 1)으로 받아들여졌고, 원점 모드(`?6h`)에서 커서가 그 영역 기준으로 옮겨져 `index out of bounds: the len is 1 but the index is 1`(`vendor/alacritty_terminal/src/term/mod.rs`의 `damage_line`)로 죽었다. Ghostty `setTopAndBottomMargin`(`Terminal.zig` 3335~3343줄)은 자른 뒤에 `top >= bottom`을 본다
  - 줄 수 2 이상에서 나던 같은 자리 패닉은 여백이 아니라 `split_cell_boundary`였다. 화면 맨 윗줄 첫 칸이 넓은 글자이고 앞줄 끝이 그 글자의 자리 표지(스크롤백)인 채 DCH·ECH를 하면, 스크롤백 줄(음수)을 `as usize`로 바꿔 손상 기록에 적어 `index out of bounds: ... 18446744073709551615`로 죽었다
- 확인하는 법: `crates/bt-term/tests/engine_panics72.rs`의 `single_line_*`·`margin_*`·`dch_with_wide_char_at_top_of_screen_and_history_above`
- b-terminal: 여백은 Ghostty와 같은 순서로 정하고, 스크롤백 줄은 화면 손상에 적지 않는다(화면에 보이는 줄이 아니다)

## 17. 올려 본 뒤 영역 스크롤을 받으면 올려 본 자리가 스크롤백보다 위를 가리킨다

- 언제: 2026-11-16, #589. 0.71 보안 검토의 퍼징(시드 고정 3만 회)에서 스냅샷의 `display_offset > history`가 887회 나왔다(0.70.0은 1823회). 실제 창에서 겪은 사례가 아니다. `grid/mod.rs`의 `display_iter` 패닉(`snapshot_of`에서 부름)이 이 상태 뒤에 나온다고 보았고, 고친 뒤 같은 퍼징에서 둘 다 0이 된 것만 확인했다(둘의 인과를 따로 증명하지는 않았다)
- 만드는 바이트 순서(최소 재현): 스크롤백을 만들고(`ESC[3S`) 올려 본 뒤, 줄 수를 늘리고, `ESC[2;4r ESC[3B ESC D`. 위 여백이 맨 위가 아닌 영역에서 줄을 올리는 어떤 출력(IND·LF·SU)이든 같다
- 원인: `Grid::scroll_up`이 스크롤백이 생기지 않는 영역 스크롤(위 여백 ≠ 0)에서도 올려 본 자리를 줄 수만큼 늘렸다. 올려 본 자리의 상한도 스크롤백 크기가 아니라 최대 스크롤백 한도였다. Ghostty는 올려 본 자리를 페이지 목록의 핀으로 가져(`PageList.zig` viewport) 영역 스크롤에 움직이지 않고 스크롤백 밖을 가리킬 수 없다
- 확인하는 법: `crates/bt-term/tests/engine_panics72.rs`의 `region_scroll_*`와 시드 고정 퍼징 `fuzz_fixed_seed_has_no_panic_and_keeps_offset_in_history`
- b-terminal: 올려 본 자리는 스크롤백이 실제로 생기는 길에서만 늘고 스크롤백 크기로 자른다. 그리기 쪽에서 자르는 방어선은 두지 않았다

## 18. CAN이나 ESC로 끊긴 문자열 뒤의 질의에 답이 가지 않는다

- 언제: 2026-11-16, #595. 엔진 밖 파서 퍼징 조사(#593)에서 찾았다. 실제 창에서 겪은 사례가 아니고, 프로그램이 이런 바이트를 실제로 내는 빈도는 재지 않았다
- 만드는 바이트 순서(최소 재현): `ESC ] 9 ; hi CAN`, `ESC ] 9 ; hi`, `ESC _ G a=q,i=1 CAN`, `ESC P 1 $ r` 가운데 하나 뒤에 DA1(`ESC [ c`). `/bin/sh`의 `printf`로 PTY에 보내고 읽으면 기준(앞에 아무것도 없음)은 `ESC[?62;22c`가 오는데 넷 모두 빈 답이었다
- 원인: 스캐너(`notify.rs`)가 엔진(vte)·Ghostty와 문자열이 끝나는 자리를 달리 봤다. OSC·APC 안의 CAN·SUB를 글자로 모았고, 문자열 안의 ESC 다음이 `\`가 아니면 그 바이트를 새 이스케이프로 읽지 않고 버렸다(`[`가 사라져 CSI가 글자가 됨). CSI 안의 CAN은 CSI를 끊지 않아 `ESC[2 CAN J`의 J를 ED로 읽었다. Ghostty `Parser.zig`는 OSC·DCS·APC 상태를 나갈 때(BEL·CAN·SUB·ESC 모두) 늘 dispatch·unhook·apc_end를 부른다(276·280·281줄)
- 확인하는 법: `crates/bt-term/tests/parser_fuzz73.rs`의 `can_*`·`esc_*`·`session_answers_da1_after_interrupted_strings`
- b-terminal: 문자열 안의 CAN·SUB·ESC는 그 문자열을 끝낸다. OSC·APC는 끝난 것으로 내보내고, ESC 뒤 바이트는 새 이스케이프로 다시 읽는다. 일부러 다르게 둔 것: DCS 질의는 끊기면 내보내지 않고(답을 PTY에 쓰는 길이라, 기존 시험이 정한 규칙), 겹친 ESC 뒤(tmux 패스스루)는 다시 읽지 않는다

## 19. 신호 스캐너와 엔진이 C0·8비트 ST·SOS/PM·DCS 머리를 다르게 읽어 질의 답이 빠진다

- 언제: 2026-11-16, #598. 0.73 보안 검토(코드 읽기)에서 찾았다. 실제 창에서 겪은 사례가 아니고, 프로그램이 이런 바이트를 내는 빈도는 재지 않았다. 모두 스캐너가 덜 보는 쪽이라 답이나 알림이 빠질 뿐 주입은 되지 않는다(4·5번은 답이 더 나가는 쪽이지만 Ground에서 같은 질의·ENQ를 직접 보내도 같아 새 능력은 아니다). 5번(DCS 머리)은 0.74 PR 전체 보안 검토에서 찾았다
- 만드는 바이트 순서(최소 재현)와 엔진의 동작(기준은 vendor/vte `src/lib.rs`):
  - OSC 안의 C0: `ESC ] 133 ; 0x01 A BEL`. 엔진은 0x01을 버려 `133;A`로 읽는데(407~413줄) 스캐너는 본문에 쌓아 프롬프트 표시를 놓쳤다
  - ESC 바로 뒤의 C0: `ESC LF [ c`·`ESC LF c`. 엔진은 LF를 실행하고 Escape에 머물러 DA1·RIS를 읽는데(340~347줄) 스캐너는 Ground로 가서 신호가 빠졌다
  - DCS 안의 8비트 ST: `ESC P $ q m 0x9C`. 엔진은 0x9C에서 unhook하고 Ground로 가는데(332줄) 스캐너는 다음 ESC까지 본문으로 모아 DECRQSS 답이 안 갔고 뒤의 바이트를 읽지 못했다
  - SOS·PM: `ESC X … ENQ … ESC \`·`ESC ^ …`. 엔진은 문자열 상태로 들어가 안의 바이트를 무시하는데(359·377줄) 스캐너는 Ground로 읽어 안의 ENQ에 답했다
  - 엔진이 무시하는 DCS 머리: `ESC P $ 0 q m 0x9C ENQ`·`ESC P 1 ? $ q m 0x9C ENQ`. 엔진은 중간 바이트 뒤의 0x30~0x3F(291줄)와 첫 자리가 아닌 0x3C~0x3F(309줄)에서 무시 상태로 가고, 무시 상태는 ESC·CAN·SUB로만 끝난다(175줄). 스캐너는 매개변수를 건너뛰고 DECRQSS로 받아 0x9C에서 답했고 뒤의 ENQ에도 답했다. 맨 앞의 `?`(`ESC P ? $ q`)는 엔진·Ghostty가 중간 바이트로 모아 중간 바이트가 둘이 되는데, 스캐너는 `$`만 보고 답했다
- 확인하는 법: `crates/bt-term/tests/scanner_gaps74.rs`(vte 파서에 같은 바이트를 먹여 엔진의 동작을 확인하고, `Emulator::feed`에 같이 넣어 신호와 화면을 견준다)
- b-terminal: 네 틈 모두 엔진과 같게 맞췄다.
  - OSC 안의 C0(BEL·CAN·SUB·ESC 말고)는 버린다
  - ESC 뒤의 C0는 Escape에 머문다(ENQ는 Enquiry를 내고, CAN·SUB는 Ground)
  - DCS 본문의 0x9C는 `ESC \`와 같이 DCS를 끝내고 질의에 답한다(Ghostty `Parser.zig` 280줄도 dcs_passthrough를 나갈 때 늘 unhook한다). 머리의 0x9C는 vte가 무시하므로 그대로 둔다
  - 엔진이 무시하는 DCS 머리는 스캐너도 따로 둔 무시 상태로 ESC·CAN·SUB까지 버린다(0x9C로 끝나지 않음). 머리 상한(16바이트)을 넘겨도 중간 바이트가 나왔는지는 센다. 맨 앞의 `< = > ?`는 중간 바이트로 세어 `$q`·`+q`에 맞지 않게 한다. 숫자 매개변수(`ESC P 1 $ q`)는 전처럼 보지 않는다
  - SOS·PM은 ST(`ESC \`)·CAN·SUB·ESC까지 무시한다. SOS·PM 안의 0x9C는 vte가 끝으로 보지 않아(`SosPmApcString`은 `anywhere`만 읽는다) 같게 무시한다. Ghostty는 0x9C로 끝내므로(`Parser.zig` 시험 437~438줄) 이 한 가지는 vte와 Ghostty가 다르고, 엔진(vte)을 따랐다
- 일부러 엔진과 다르게 둔 것:
  - 상한(8KiB)을 넘겨 버리는 중인 OSC(Skip)는 BEL·CAN·SUB·ESC로 끝나도 내보내지 않는다. 엔진은 잘린 OSC를 내보낼 수 있다. 문자열 앞부분만 믿고 알림·폴더·클립보드로 쓰지 않으려는 보안 결정이다
  - tmux 패스스루 안의 겹친 ESC(`ESC P tmux; ESC ESC [ c`) 뒤는 읽지 않는다. 엔진은 둘째 ESC를 새 이스케이프로 읽어 DA1에 답하지만 안쪽 글에 답하지 않는 쪽이 안전하다
- 남은 틈(고치지 않았다): ESC 뒤 0x7F·0x80 이상, ESC 중간 바이트(`ESC SP` 등) 뒤의 C0도 엔진은 다르게 읽는다(vte `advance_esc`·`advance_esc_intermediate`). Ghostty 상태표(`parse_table.zig`)는 이 저장소에 없어 대조하지 않았다
