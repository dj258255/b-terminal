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
- b-terminal: 문자열 안의 CAN·SUB·ESC는 그 문자열을 끝낸다. OSC·APC는 끝난 것으로 내보내고(CAN·SUB로 끝난 OSC는 0.96부터 버린다, 48번), ESC 뒤 바이트는 새 이스케이프로 다시 읽는다. 일부러 다르게 둔 것: DCS 질의는 끊기면 내보내지 않고(답을 PTY에 쓰는 길이라, 기존 시험이 정한 규칙), 겹친 ESC 뒤(tmux 패스스루)는 다시 읽지 않는다

## 19. 신호 스캐너와 엔진이 C0·8비트 ST·SOS/PM·DCS 머리를 다르게 읽어 질의 답이 빠진다

- 언제: 2026-11-16, #598. 0.73 보안 검토(코드 읽기)에서 찾았다. 실제 창에서 겪은 사례가 아니고, 프로그램이 이런 바이트를 내는 빈도는 재지 않았다. 1~3번은 스캐너가 덜 보는 쪽이라 답이나 알림이 빠질 뿐이고, 4·5번은 답이 더 나가는 쪽이다. 더 나가는 답은 Ground에서 같은 질의·ENQ를 직접 보내도 같아 새 능력이 아니고, 프로그램이 보낸 글이 답에 들어가는 길은 없다. 5번(DCS 머리)은 0.74 PR 전체 보안 검토에서 찾았다
- 만드는 바이트 순서(최소 재현)와 엔진의 동작(기준은 vendor/vte `src/lib.rs`):
  - OSC 안의 C0: `ESC ] 133 ; 0x01 A BEL`. 엔진은 0x01을 버려 `133;A`로 읽는데(407~413줄) 스캐너는 본문에 쌓아 프롬프트 표시를 놓쳤다
  - ESC 바로 뒤의 C0: `ESC LF [ c`·`ESC LF c`. 엔진은 LF를 실행하고 Escape에 머물러 DA1·RIS를 읽는데(340~347줄) 스캐너는 Ground로 가서 신호가 빠졌다
  - DCS 안의 8비트 ST: `ESC P $ q m 0x9C`. 엔진은 0x9C에서 unhook하고 Ground로 가는데(332줄) 스캐너는 다음 ESC까지 본문으로 모아 DECRQSS 답이 안 갔고 뒤의 바이트를 읽지 못했다
  - SOS·PM: `ESC X … ENQ … ESC \`·`ESC ^ …`. 엔진은 문자열 상태로 들어가 안의 바이트를 무시하는데(359·377줄) 스캐너는 Ground로 읽어 안의 ENQ에 답했다
  - 엔진이 무시하는 DCS 머리: `ESC P $ 0 q m 0x9C ENQ`·`ESC P 1 ? $ q m 0x9C ENQ`. 엔진은 중간 바이트 뒤의 0x30~0x3F(291줄)와 첫 자리가 아닌 0x3C~0x3F(309줄)에서 무시 상태로 가고, 무시 상태는 ESC·CAN·SUB로만 끝난다(175줄). 스캐너는 매개변수를 건너뛰고 DECRQSS로 받아 0x9C에서 답했고 뒤의 ENQ에도 답했다. 맨 앞의 `?`(`ESC P ? $ q`)는 엔진·Ghostty가 중간 바이트로 모아 중간 바이트가 둘이 되는데, 스캐너는 `$`만 보고 답했다
- 확인하는 법: `crates/bt-term/tests/scanner_gaps74.rs`(vte 파서에 같은 바이트를 먹여 엔진의 동작을 확인하고, `Emulator::feed`에 같이 넣어 신호와 화면을 견준다)
- b-terminal: 다섯 틈 모두 엔진과 같게 맞췄다.
  - OSC 안의 C0(BEL·CAN·SUB·ESC 말고)는 버린다
  - ESC 뒤의 C0는 Escape에 머문다(ENQ는 Enquiry를 내고, CAN·SUB는 Ground)
  - DCS 본문의 0x9C는 `ESC \`와 같이 DCS를 끝내고 질의에 답한다(Ghostty `Parser.zig` 280줄도 dcs_passthrough를 나갈 때 늘 unhook한다). 머리의 0x9C는 vte가 무시하므로 그대로 둔다
  - 엔진이 무시하는 DCS 머리는 스캐너도 따로 둔 무시 상태로 ESC·CAN·SUB까지 버린다(0x9C로 끝나지 않음). 머리 상한(16바이트)을 넘겨도 중간 바이트가 나왔는지는 센다. 맨 앞의 `< = > ?`는 중간 바이트로 세어 `$q`·`+q`에 맞지 않게 한다. 숫자 매개변수(`ESC P 1 $ q`)는 전처럼 보지 않는다
  - SOS·PM은 ST(`ESC \`)·CAN·SUB·ESC까지 무시한다. SOS·PM 안의 0x9C는 vte가 끝으로 보지 않아(`SosPmApcString`은 `anywhere`만 읽는다) 같게 무시한다. Ghostty는 0x9C로 끝내므로(`Parser.zig` 시험 437~438줄) 이 한 가지는 vte와 Ghostty가 다르고, 엔진(vte)을 따랐다
- 일부러 엔진과 다르게 둔 것:
  - 상한(8KiB)을 넘겨 버리는 중인 OSC(Skip)는 BEL·CAN·SUB·ESC로 끝나도 내보내지 않는다. 엔진은 잘린 OSC를 내보낼 수 있다. 문자열 앞부분만 믿고 알림·폴더·클립보드로 쓰지 않으려는 보안 결정이다
  - tmux 패스스루 안의 겹친 ESC(`ESC P tmux; ESC ESC [ c`) 뒤의 `[ c` 같은 CSI는 읽지 않는다(겹친 ESC 바로 뒤의 첫 시퀀스만, 20번). 엔진은 둘째 ESC를 새 이스케이프로 읽어 DA1에 답하지만 안쪽 글에 답하지 않는 쪽이 안전하다. 겹친 ESC 뒤가 문자열을 여는 `X ^ _ ] P`일 때와 ENQ는 엔진과 같게 읽도록 20번에서 고쳤다
- 남은 틈: ESC 뒤 0x7F·0x80 이상, 겹친 ESC 뒤의 ENQ·문자열은 20번(#603)에서 고쳤다. ESC 중간 바이트(`ESC SP` 등) 뒤의 C0는 대조해 보니 스캐너가 이미 엔진과 같은 수로 답한다(20번). Ghostty 상태표(`parse_table.zig`)는 이 저장소에 없어 대조하지 않았다

## 20. 신호 스캐너가 엔진이 버린 ENQ에 더 답한다(겹친 ESC 뒤, ESC 뒤의 0x7F·0x80 이상)

- 언제: 2026-11-16, #603. 0.74 보안 재검토의 퍼저(스캐너와 vte에 같은 바이트를 먹임)가 찾았다. 실제 창에서 겪은 사례가 아니고, 프로그램이 이런 바이트를 내는 빈도는 재지 않았다. 답은 사용자가 정한 answerback 고정 글이라 새 능력은 아니고, 프로그램이 보낸 글이 답에 들어가는 길도 없다. 다만 스캐너가 엔진보다 신호를 더 내면 안 된다는 기준에 어긋났다
- 만드는 바이트 순서(최소 재현)와 엔진의 동작(기준은 vendor/vte `src/lib.rs`):
  - 겹친 ESC 뒤: `ESC P $ 0 q ESC ESC X ENQ`·`ESC P ESC ESC X ENQ`(`X` 자리에 `^ _ ] P`도 같다). 엔진은 둘째 ESC를 새 이스케이프로 읽고 `X`로 문자열에 들어가 ENQ를 버리는데, 스캐너는 겹친 ESC 뒤 바이트를 Ground로 삼켜 ENQ에 답했다. `ESC P ESC ESC ENQ X ENQ`도 엔진은 Escape에서 첫 ENQ만 실행하는데 스캐너는 둘 다 답했다
  - ESC 뒤의 0x7F·0x80 이상: `ESC 0x9C ENQ X ENQ`·`ESC DEL ENQ X ENQ`. 엔진은 `advance_esc`의 `_ => ()`로 Escape에 머물러 둘째 ENQ를 `X` 문자열 안에서 버리는데, 스캐너는 Ground로 가서 둘 다 답했다. 머문 뒤의 `[ c`(DA1)도 스캐너가 읽지 못했다
  - ESC 중간 바이트(`ESC SP`·`ESC (` 등) 뒤의 C0: vte `advance_esc_intermediate`는 C0를 실행하고 머물며 0x7F는 무시한다. 스캐너는 중간 바이트에서 Ground로 가는데 Ground도 ESC·CAN·SUB·ENQ를 같게 읽고 이어지는 바이트가 엔진과 같은 자리에서 Ground로 가서, ENQ 수가 같다는 것을 시험(`c0_after_esc_intermediate_matches_the_engine`)과 퍼저로 확인했다. 스캐너는 고치지 않았다
- 확인하는 법: `crates/bt-term/tests/scanner_enq75.rs`(시험마다 vte 파서에 같은 바이트를 먹여 엔진 쪽 사실을 먼저 확인하고, `Emulator::feed`로도 본다). 차분 퍼징 시험 `scanner_never_answers_more_than_the_engine`은 시드를 고정한 무작위 바이트열 30만 개에서 스캐너의 Enquiry·Da1·DcsQuery 수가 엔진(vte의 `execute(0x05)`·`csi_dispatch c`·`hook q`) 수를 넘지 않는지 본다. 큰 판(`#[ignore]`, 1000만 개)도 한 번 돌려 통과했다(release, 한 번 잼)
- b-terminal:
  - ESC 뒤의 0x7F·0x80 이상은 Escape에 머문다
  - 겹친 ESC 뒤(tmux 패스스루 안쪽)는 안쪽 Escape 상태(`EscQuiet`)로 따라간다. C0는 엔진처럼 실행하고(ENQ는 Enquiry), `X ^ _ ] P`는 엔진처럼 그 문자열이 끝날 때까지 신호 없이 버린다. 끝나는 조건은 vte와 같다(SOS·PM·APC는 CAN·SUB·ESC, OSC는 BEL·CAN·SUB·ESC, DCS는 머리·통과·무시 상태). 안쪽 DCS는 DECRQSS·XTGETTCAP 질의도 내지 않고, 안쪽 OSC는 알림도 내지 않는다. 이것은 겹친 ESC 바로 뒤의 첫 시퀀스 하나에만 해당한다
- 일부러 엔진과 다르게 둔 것(그대로): tmux 패스스루 안의 `ESC ESC [ c` 같은 CSI와 `ESC ESC c`는 읽지 않는다(안쪽 글에는 답하지 않는다). 안쪽 문자열이 ESC로 끝난 뒤의 `[ c`도 안쪽 글로 보아 읽지 않는다. 모두 스캐너가 엔진보다 덜 내는 쪽이다. 한계: 조용한 것은 겹친 ESC 바로 뒤의 첫 시퀀스 하나라, `ESC P tmux; ESC ESC ] 7;… BEL ESC ESC [ c ESC \`처럼 패스스루 하나에 안쪽 시퀀스가 둘 이상이면 둘째부터는 보통처럼 읽어 답과 알림을 낸다(0.73부터 같았다, 0.75 PR 보안 검토가 찾음). 엔진도 같은 시퀀스를 처리하므로 엔진보다 더 내지는 않는다. 안쪽 본문 전체를 조용하게 할지는 2026-11-18에 지금대로 두기로 정했다(#619, 주 세션 추천안). 둘째부터의 시퀀스는 엔진이 실제로 처리하므로 그 신호까지 버리면 엔진과 더 멀어지고, tmux는 패스스루를 풀어 안쪽 시퀀스만 보내므로 감싼 꼴이 날것으로 오는 것은 tmux가 풀지 않은 경우(겹친 tmux, 파일 `cat`)뿐이라 얻는 것이 적다. `only_the_first_inner_sequence_after_double_esc_is_quiet`가 지금 동작을 고정한다
- 남은 틈: 퍼저는 ESC·DEL·0x80·0x9C·C0·`X ^ _ ] P \ [ c $ q + ( ; 0 1 ? >`와 ENQ·CAN·SUB·BEL로 만든 24바이트 이하의 열만 봤다. 8KiB를 넘는 OSC·긴 DCS·UTF-8 여러 바이트 글자가 섞인 열, 입력을 조각내는 모든 경계는 퍼저로 보지 않았다(두 조각 나누기만 봤다). Ghostty 상태표는 대조하지 않았다

## 21. 신호 스캐너가 엔진이 dispatch하지 않는 CSI에서 화면 지우기(ED)와 질의 신호를 낸다

- 언제: 2026-11-18, #605. 0.75 보안 검토의 퍼저(스캐너와 vte에 같은 바이트를 먹임)가 찾았다. 실제 창에서 겪은 사례가 아니고, 프로그램이 이런 바이트를 내는 빈도는 재지 않았다. `Signal::Erase`는 앱 안 상태(kitty 그림 배치 지우기 등)에 쓰이고 PTY 답이 아니라 영향은 낮다. 다만 스캐너가 엔진보다 신호를 더 내면 안 된다는 기준(20번)에 어긋났다
- 만드는 바이트 순서(최소 재현)와 엔진의 동작(기준은 vendor/vte `src/lib.rs` 188~260줄의 `advance_csi_*`, `action_csi_dispatch`, `MAX_INTERMEDIATES`(2), `params::MAX_PARAMS`(32)와 `src/ansi.rs`의 `csi_dispatch`):
  - ED가 `csi_taint`·`csi_inter` 검사 바깥에 있었다. 스캐너는 모은 매개변수가 `2`·`3`이면 끝 바이트 `J`에서 Erase를 냈다. 중간 바이트 뒤의 매개변수(`ESC [ ( 2 J`, `ESC [ 0x80 $ 3 J`, `ESC [ 0x9D ( 2 J`)는 엔진이 CsiIgnore로 보내 dispatch하지 않는다
  - 맨 앞의 `< = >`(`ESC [ > 2 J`)나 매개변수 뒤의 중간 바이트(`ESC [ 2 $ J`)는 엔진이 `J`를 ED가 아닌 다른 꼴로 dispatch한다(`ansi.rs`는 `('J', [])`와 `('J', [b'?'])`만 처리). 맨 앞의 `:`(`ESC [ : 2 J`)는 첫 매개변수 묶음이 [0, 2]라 엔진은 ED 0(아래로 지우기)이다. 모두 스캐너가 Erase를 냈다
  - 매개변수 상한: 엔진은 세미콜론이 32개 이상이면 `ignore`로 dispatch하고 `ansi.rs`가 버린다. 스캐너는 모은 바이트가 40개 미만이면 믿고 `CSI 2 ; × 32 h`(KAM), `CSI > ; × 32 c`(DA2) 같은 것에 신호를 냈다. 이 길은 ED만이 아니라 CSI 신호 전부에 있었다
  - 재현으로 받은 `ESC [ DEL 3 J`는 틈이 아니다. 엔진은 DEL을 CSI 어느 상태에서나 무시하고 ED 3으로 dispatch하며(`advance_csi_entry`의 `_ => anywhere`), 스캐너도 Erase를 하나 낸다. 시험(`plain_ed_and_the_bytes_the_engine_ignores_still_erase`)이 이것을 고정한다
- 확인하는 법: `crates/bt-term/tests/scanner_csi76.rs`(시험마다 vte 파서에 같은 바이트를 먹여 엔진의 `csi_dispatch`(끝 바이트·중간 바이트·`ignore`)를 먼저 확인하고, `Emulator::feed`로도 본다). 차분 퍼징 `scanner_csi_signal_shapes_never_exceed_the_engine`은 dispatch 꼴(끝 바이트·중간 바이트·`ignore`)만 견주고 매개변수 값은 ED에서만 본다(아래 남은 틈의 DA1·DA2 값 차이는 이 퍼저가 잡지 않는다). 이 퍼저는 시드를 고정한 20만 개(debug 약 1초)에서 스캐너의 CSI 신호(Erase·Da1·Da2·XtVersion·KAM·모드 신호 등)가 엔진이 `ignore` 없이 같은 꼴로 dispatch한 수를 넘지 않는지 보고, 두 조각으로 나눠 먹여도 같은지 본다. 큰 판(`#[ignore]`, 1000만 개)도 한 번 돌려 통과했다(release, 한 번 잼, 약 9초). 고친 부분을 하나씩 되돌려 새 시험이 다시 실패하는 것도 확인했다(ED 위치를 되돌리면 6개, 매개변수 상한을 되돌리면 2개)
- b-terminal: ED도 다른 CSI 신호처럼 `csi_taint`가 없는 CSI에서만 읽는다. 세 갈래(일반·`> c`·중간 바이트) 모두 세미콜론이 31개 이하일 때만 읽는다(`MAX_PARAMS`)
- 일부러 하지 않은 것(스캐너가 엔진보다 덜 내는 쪽이라 이번 범위 밖): `CSI ? 2 J`·`CSI ? 3 J`(DECSED, 엔진은 `selective_clear_screen`), `CSI 2 ; 5 J`·`CSI 02 J`(엔진은 첫 매개변수로 ED 2), `CSI 2 : J`(맨 뒤의 `:`는 엔진이 ED 2로 읽지만 스캐너는 `:`가 있으면 읽지 않는다. 이번 고침으로 예전에 내던 Erase가 빠졌다), `CSI $ c`·`CSI = c`·`CSI ? c`(엔진은 첫 중간 바이트를 `identify_terminal`에 넘긴다)
- 남은 틈: Da1(`CSI Ps c`)과 Da2(`CSI > Ps c`), XtVersion(`CSI > Ps q`)은 매개변수 값을 보지 않는다(Ghostty와 같게 둔 것, `csi_query::private_gt`). 엔진(`ansi.rs`)은 `c`를 첫 매개변수가 0일 때만 처리하므로 `CSI 1 c`는 엔진이 처리하지 않는데 스캐너는 Da1을 낸다. dispatch 꼴(끝 바이트·중간 바이트·`ignore`)로만 견줬고 이 값 차이는 고치지 않았다. 퍼저는 `0 1 2 3 8 15 16 19 21 63 996 2031 2033 2048 5522 ; : < = > ? SP $ ! * # (`와 DEL·0x80·0x9C·0x9D·C0·CAN·SUB·ESC, 끝 바이트 `J c q t y p n h l s r K m @ DEL`로 만든 열만 봤다. 40바이트를 넘는 CSI와 Ghostty 상태표는 대조하지 않았다

## 22. 여러 스레드가 동시에 PTY를 열면 가끔 `openpty` 가 `Os { code: -6 }`으로 실패한다

- 언제: 2026-11-16~18, #607. `cargo test --workspace --no-fail-fast`에서 PTY를 띄우는 시험이 가끔, 실패하는 시험이 매번 달리 실패했다(`session`·`kitty_clip_session`의 여러 시험). 메시지는 매번 `spawn: PTY 열기(openpty): failed to openpty: Os { code: -6, kind: Uncategorized, message: "Unknown error: -6" }`. 실패 무렵 열린 PTY는 25~26개, `kern.tty.ptmx_max`는 511이라 한도에 닿은 것이 아니다. GitHub CI(ubuntu)에서는 나지 않았고 macOS Darwin 25.3에서만 봤다
- 재현(macOS Darwin 25.3, 이 기기에서 한 번씩 잰 값): 스레드마다 `openpty`→닫기를 되풀이하는 작은 프로그램(저장소 밖)에서 스레드 1개는 20000번 모두 성공했고, 스레드 4개는 2/20000, 16개는 10/20000, 64개는 58/19968번 실패했다(실패는 모두 errno -6). `posix_openpt`·`grantpt`·`unlockpt`·`ptsname`·슬레이브 `open`으로 쪼개 스레드 32개로 3번(각 48000번) 돌리면 실패 64·69·72번이 모두 첫 단계 `posix_openpt`(`/dev/ptmx` 열기)였다. 같은 프로그램에서 `/bin/sh`를 띄우고 기다리는 길은 스레드 8·32개 576~600번에서 실패가 없었다(횟수가 적어 못 잡았을 수 있다). 시험 바이너리 안에서는 시험이 스레드로 병렬로 돌므로 `session`만 따로 돌려도 한 번 실패한 것과 맞는다
- errno가 -6인 이유: 모른다. 음수는 libc의 오류 번호(`EPERM`=1 …, 양수 6은 `ENXIO`)가 아니고 `std::io::Error::last_os_error()`가 `errno` 변수를 그대로 읽은 값이다. 커널이 `/dev/ptmx`의 복제 열기(여러 스레드가 동시에 새 마스터를 받는 길)에서 일시적으로 낸 실패로 보이지만, 이는 추정이다(XNU의 `tty_ptmx.c`와 Libc의 `openpty`는 읽지 않았다). 다시 시도하면 대개 성공하고, 한도(PTY 수·파일 수)와는 관련이 없다는 것만 위 측정으로 확인했다
- 실제 앱: 같은 `openpty`를 부르지만, 앱은 탭(칸)을 메인 이벤트 루프의 `App::spawn_pane`에서 하나씩 열므로 앱 안에서는 두 열기가 겹치지 않는다. 겹치는 상대는 같은 순간 PTY를 여는 다른 프로세스(다른 터미널, tmux, 시험 실행 등)다. 2026-11-18에 저장소 밖 Python 프로그램(`os.openpty`→닫기)으로 재니(한 번씩 잰 값) 경쟁은 프로세스 사이에서도 났다: 프로세스 1개 20000번은 실패 0, 16개(합 20000번)는 9와 다시 재서 11, 64개(합 19968번)는 61과 60이었다(모두 errno -6). 앱과 같은 모양으로 한 스레드가 20000번 여는 동안 다른 프로세스가 계속 열게 하면, 그 한 스레드의 실패는 다른 프로세스 0개일 때 0, 4개일 때 1, 16개일 때 9였다. 그래서 다른 프로그램이 PTY를 많이 여는 순간에 탭을 열면 낮은 확률로 실패할 수 있고, 아래 재시도가 이 경우도 덮는다. 앱에서 실패가 난 것을 본 적은 없고, 재시도는 성공하면 로그를 남기지 않아 앱에서 몇 번 다시 시도했는지는 재지 않았다. Python 스레드로 같은 측정을 하면 실패가 0이었는데(16·64 스레드), `os.openpty`가 GIL을 쥔 채 불려 열기가 줄을 서는 것으로 보인다(확인하지 않음). 스레드 측정은 위 재현의 Rust 값을 쓴다
- Ghostty와 견줌: `src/pty.zig`(main, 2026-11-18에 읽음)는 `openpty`가 실패하면 `error.OpenptyFailed`를 돌려주고 다시 시도하지 않는다. 같은 일시 실패가 Ghostty에도 날 수 있는지는 재지 않았다
- 만드는 법(고치기 전): `crates/bt-term/tests/spawn_concurrent607.rs`. 스레드 32개가 `Session::spawn`으로 `/bin/sh -c "exit 0"`을 200번씩(6400번) 띄운다. 고치기 전 코드에서 4번 돌려 4번 모두 실패했다(1~4건, 각 약 10초): `6400번 띄우는 동안 4번 실패했다. 첫 오류: PTY 열기(openpty): failed to openpty: Os { code: -6, kind: Uncategorized, message: "Unknown error: -6" }`
- b-terminal: `Session::spawn`이 `openpty`를 최대 5번 시도한다(시도 사이에 1·2·5·10ms 쉼, 합 18ms). 끝내 실패하면 `PTY 열기(openpty), N번 시도해도 실패`와 음수 errno 설명을 오류에 붙인다. portable-pty가 errno를 문자열로만 주므로 실패 종류로 가르지 않고 모든 실패를 다시 시도한다. 진짜 한도에 닿은 경우에는 18ms 더 늦게 같은 오류가 난다. 다시 시도하는 부분은 `open_pty_with_retry`로 열기 함수와 쉬기 함수를 주입해 `session.rs`의 단위 시험이 결정적으로 본다
- 확인한 것: 고친 뒤 위 시험을 5번 돌려 5번 통과했다. 재시도를 빼면(빈 backoff) 3번 중 2번 실패했다(실패 확률이 낮아 1번은 우연히 통과). 단위 시험 3개(재시도 후 성공, 첫 성공이면 쉬지 않음, 상한과 오류 문구)와 음수 errno 문구 시험 1개를 더했다
- 일부러 하지 않은 것: 시험 쪽에만 재시도를 넣지 않았다(앱에도 같은 길이 있다). portable-pty를 고치거나 `libc`·`nix`로 직접 열지 않았다(의존성·`unsafe` 제한). errno를 풀기 위해 문자열의 `code: -N`을 읽는 것은 문구용이라 취약하며, 재시도 판단에는 쓰지 않는다

## 23. 데스크톱 알림을 누르면 b-terminal이 아니라 스크립트 편집기가 열린다

- 언제: 2026-11-18, #623. 0.78에서 프로그램 상태(OSC 7501) 알림을 넣으며 알림 길을 다시 읽다가 적었다. 사용자 제보는 없다
- 까닭: macOS의 b-terminal 데스크톱 알림(OSC 9·777·99, OSC 7501, 명령 끝 알림, `btctl notify`)은 모두 `crates/bt-app/src/desktop.rs`가 `osascript -e 'display notification …'`로 띄운다. macOS는 `osascript`의 알림을 스크립트 편집기의 알림으로 다루므로, 알림을 누르면 스크립트 편집기가 열리고 알림 설정도 스크립트 편집기 항목을 따른다. 이것은 널리 알려진 macOS 동작이고, 이 기기(macOS 26.3.1)에서 알림을 띄워 눌러 보지는 않았다(데스크톱 알림을 보내지 않는다는 작업 규칙, 마우스 흉내를 쓰지 않음)
- 알림이 아예 안 보이면: 시스템 설정 → 알림에서 b-terminal(앱 묶음일 때) 또는 스크립트 편집기(그 밖) 항목이 허용인지 본다. b-terminal 쪽은 `desktop-notifications = false`(프로그램 알림만 끔)인지 본다
- b-terminal(#625, ADR-058): `scripts/bundle-macos.sh`로 만든 앱 묶음으로 돌면 알림을 `UNUserNotificationCenter`로 내고, 누르면 그 칸의 창·탭·칸으로 간다(Ghostty와 같다). 처음 알림을 낼 때 b-terminal 알림 권한을 묻는다. 그 칸을 보면 알림 센터의 그 칸 알림이 지워진다. 앱 묶음이 아닌 실행(`cargo run`, 내려받은 실행 파일만 돌림)은 번들 식별자가 없어 이 프레임워크를 쓸 수 없으므로(쓰면 앱이 죽는다) 여전히 `osascript`이고 스크립트 편집기가 열린다. 프레임워크 호출은 vendor winit 안에만 두어 작업 공간의 `unsafe_code = "deny"`는 그대로다. 실제로 알림을 띄워 눌러 보지는 않았다(데스크톱 알림을 보내지 않는다는 작업 규칙)

## 24. AppleScript가 "터미널이 답하지 않습니다"를 낸다

- 언제: 2026-11-18, #634. AppleScript 사전을 넣으며 시간 초과 길을 따져 적었다. 사용자 제보는 없다
- 까닭: 스크립트 명령은 주 스레드로 오고, 앱의 이벤트 루프가 처리해야 답이 나간다. 오른쪽 클릭 메뉴가 열려 있는 동안(메뉴 추적이 이벤트 처리기 안에서 도는 모달이다)이나 앱이 바쁠 때는 그 사건이 처리되지 못해 5초 뒤 이 오류가 난다
- 이때 명령은 실행되지 않고 버려진다. 늦게 처리되어 `input text`의 글이 뒤늦게 칸에 들어가거나 창이 뜨는 일은 없다(요청마다 표를 달아 시간 초과 쪽이 취소하고 앱은 실행 직전에 표를 가져간다. 앱이 이미 실행을 시작했으면 답을 5초 더 기다리고, 그래도 답이 없으면 "명령이 실행 중이었을 수 있습니다"를 낸다. 이때는 실행됐을 수 있다). 목록·속성을 읽는 쪽은 앱이 답하지 않으면 2초 동안 다시 묻지 않고 마지막 스냅숏을 쓴다
- 풀기: 메뉴를 닫거나 앱이 한가해진 뒤 스크립트를 다시 보낸다. 이 환경에서 메뉴를 띄워 재현해 보지는 않았다(마우스 흉내를 쓰지 않는다는 작업 규칙)

## 25. 0.80.0 앱 묶음이 AppleScript `quit`(로그아웃·종료 때 오는 Apple Event)로 끝나지 않는다

- 언제: 2026-11-18, #644. 0.80에서 범수 님 확인 몫으로 남긴 AppleScript를 주 세션이 격리한 실제 창에서 돌리다 찾았다. 사용자 제보는 없다
- 증상: `osascript -e 'tell application "b-terminal" to quit'`이 오류 없이 끝나지만 앱이 남는다. 앱 로그에는 아무것도 없다. macOS는 로그아웃·재시동·종료 때 앱마다 같은 `aevtquit` Apple Event를 보내므로 로그아웃이 b-terminal에서 막힐 수 있다(로그아웃 자체는 재지 않았다)
- 까닭: `Info.plist`에 AppleScript 사전(`OSAScriptingDefinition`)이 있으면 Cocoa Scripting이 `aevtquit`를 사전의 `quit` 명령(`NSQuitCommand`)으로 처리한다. 받는 객체의 클래스(application)에 `<responds-to command="quit">`가 있어야 메서드가 불리는데, #634에서 Ghostty sdef를 줄여 옮기며 이 줄을 빠뜨렸다. 사전이 없던 0.79는 AppKit 기본 처리기가 바로 `terminate:`를 불렀다
- 가리는 법: 같은 묶음 사본에서 `OSAScriptingDefinition`·`NSAppleScriptEnabled`를 빼면 같은 명령으로 끝난다
- 고침(0.81): application 클래스에 `quit` 응답을 넣어 AppKit의 `handleQuitScriptCommand:`(→ `terminate:`)로 보낸다. `macos-applescript = false`여도 막지 않는다(Ghostty는 이때 `quit`도 막지만, 로그아웃이 막히면 안 된다)
- 0.80.0에서 풀기: 메뉴의 끝내기(⌘Q)는 이 길과 상관없이 된다. 로그아웃 전에 b-terminal을 ⌘Q로 끈다

## 26. AppleScript `focus`·`activate window`가 b-terminal을 앞으로 가져오지 못한다

- 언제: 2026-11-18, #645를 찾던 실제 창 확인. 사용자 제보는 없다
- 증상: 칸·창은 골라지지만(`btctl list`의 `*`가 옮겨 감) 다른 앱이 앞에 있으면 b-terminal이 앞으로 오지 않을 때가 있다. 같은 실행에서 처음 한 번은 앞으로 왔다
- 까닭: macOS 14부터 앱은 스스로 앞으로 나올 수 없고(협력 활성화), 앞에 있는 앱이 양보해야 한다. b-terminal은 `activateIgnoringOtherApps`를 부르지만 이 규칙을 따른다. 스크립트를 보낸 `osascript`는 앞 앱이 아니어서 양보할 수 없다
- 풀기: 스크립트에서 먼저 `activate`를 보낸다(`tell application "b-terminal" to activate`는 보내는 쪽이 활성화를 요청하므로 앞으로 온다). 실제 창에서 이 방법으로 앞으로 오는 것을 확인했다

## 27. AppleScript `send key`가 "알 수 없는 키 이름"을 낸다

- 언제: 2026-11-18, #650. `send key`를 넣으며 Ghostty의 이름 규칙을 시험으로 확인했다. 사용자 제보는 없다
- 증상: `send key "Enter" to t`나 `send key "return" to t`가 `알 수 없는 키 이름: Enter` 오류(-10000)를 낸다
- 까닭: 키 이름은 Ghostty `Ghostty.Input.Key`의 rawValue(`enter`, `arrowUp`, `digit1`, `bracketLeft`)라 대소문자를 가리고 다른 표기(`Return`, `arrow_up`, `ctrl`)는 모른다. 수식 키는 `modifiers "shift,control,option,command"` 인자로 따로 준다(`send key "c" modifiers "control" to t`). `send key "ctrl-c"`는 되지 않는다
- 풀기: 이름은 소문자 시작 camelCase로 쓴다(`enter`, `tab`, `escape`, `space`, `backspace`, `arrowUp`, `pageDown`, `f5`). 뒤에 키 묶음(keybind)에 걸린 키를 보냈다면(예: `modifiers "command"`와 `t`) 그 동작이 일어난다
- 2026-11-18 주 세션이 격리한 실제 창에서 `send key "Enter"`와 `modifiers "hyper"`가 이 오류(-10000)를 내는 것을 확인했다. `modifiers " shift , "`처럼 공백만 남는 조각이 있어도 오류다(Ghostty `split(separator:)`와 같다). `"shift,"`처럼 빈 조각은 건너뛴다

## 28. 칸·탭·창을 닫아도 셸과 그 프로그램이 끝나지 않는다

- 언제: 2026-11-18, #651. 주 세션이 격리한 실제 창에서 찾았다(칸 4개가 남았는데 앱 아래 `/bin/sh -l` 8개, ptmx fd 20개, 닫은 칸의 `sleep 100`도 살아 있었다). 0.81.0까지의 동작이다
- 증상: 닫은 칸의 셸·ssh·편집기·빌드가 보이지 않는 채 계속 돈다. 칸을 열고 닫을수록 프로세스와 fd가 쌓인다
- 까닭: 읽기 스레드가 복제한 읽기 fd와 쓰기 끝을 쥔 채 `read`에서 막혀 있어, `Session`을 떨어뜨려도 PTY 주 쪽이 닫히지 않는다. 주 쪽이 닫혀야 셸이 끊김(SIGHUP)을 받는데 그 일이 없었다
- 고침: `Session`의 `Drop`이 자식에게 SIGHUP을 보낸다(Ghostty `termio/Exec.zig`의 `killPid`를 따랐다). 셸이 끝나면 slave 쪽이 닫혀 읽기 스레드의 `read`가 끝나고 fd가 풀린다. 되살리기(undo-timeout) 동안 들고 있는 칸은 떨어지기 전까지 셸이 산다
  - 이미 끝난 칸에는 보내지 않는다. 읽기 스레드가 자식을 거두고 끝남을 표시하는 일을 `Drop`이 신호를 보내는 일과 같은 잠금 안에서 하므로, 거둔 뒤 재사용된 pid에 신호가 가지 않는다
  - SIGHUP을 막은 셸(`trap '' HUP`)에는 100ms 간격으로 최대 약 3초 되풀이한 뒤 그만둔다. SIGKILL로 넘어가지 않는다(Ghostty도 SIGHUP만 되풀이한다). 이런 셸과 그 프로그램은 앱이 끝날 때까지 남는다. 이 경우는 시험하지 않았다
  - Ghostty는 자식의 프로세스 그룹에 `killpg`를 보내지만, 여기서는 `unsafe`를 쓰지 않으려고 portable-pty의 `kill`(자식 pid에 SIGHUP)을 쓴다. 로그인 셸이 끊김을 받으면 자기 작업에 다시 전한다. 셸을 거치지 않는 전경 프로그램이 따로 남는지는 재지 않았다
- 0.81.0에서 풀기: 앱을 끝내면 남은 셸도 끝난다(앱 프로세스가 사라지면 주 쪽 fd가 닫힌다)

## 29. 분할 경계선을 두 번 눌러도 칸 크기가 고르게 되지 않는다

- 언제: 2026-11-19, #694. 주 세션이 격리한 실제 창에서 마우스(CGEvent)로 끌고 두 번 눌러 찾았다. 0.86.0~0.87.0의 동작이다
- 증상: 경계선을 끌어 30/68칸으로 만든 뒤 두 번 눌러도 30/68칸 그대로다. 끌기는 된다
- 까닭: winit 0.30 macOS는 단추를 놓을 때 같은 자리여도 `CursorMoved`를 먼저 보낸다. 두 번째 누름이 칸을 고르게 하면서 끌기도 시작했고, 놓을 때 온 이동을 끌기로 받아 비율을 마우스 자리로 되돌렸다. 단위 시험은 두 번 판정만 보고 이벤트 순서를 흉내 내지 않았다
- 고침: Ghostty `DragGesture()` 기본처럼 누른 자리에서 10pt 넘게 움직인 뒤에만 비율을 따라간다. 한 번 넘으면 돌아와도 계속 따라간다
- 0.87.0에서 풀기: 키 묶음 `equalize_splits`(기본 ⌃⌘=)나 명령 팔레트의 "칸 크기 고르게"를 쓴다

## 30. 한글 입력 소스에서 ⌘T 같은 글자 키 단축키가 듣지 않는다

- 언제: 2026-11-20, #709. 주 세션이 쌓인 실제 창 확인 목록(0.7의 `ctrl+a>n`)을 격리한 창에서 돌리다 찾았다. 0.90.0까지의 동작이다
- 증상: 입력 소스가 한글 2벌식이면 ⌘T·⌘W·⌘N·⌘F 같은 글자 키 단축키와 `keybind = ctrl+a>…` 같은 사용자 묶음이 듣지 않고 셸로 간다. ⌘1처럼 숫자 키는 듣는다. 영문 입력 소스에서는 다 듣는다
- 까닭: 키 묶음 트리거를 winit `key_without_modifiers()`로 만들었다. winit은 수식 키를 모두 빼고 자판을 읽어 T 자리에 `ㅅ`을 주고, `super+t`와 맞지 않았다. macOS 앱은 보통 메뉴 단축키가 ⌘ 층 글자(`t`)로 맞춰 주는데, b-terminal은 메뉴 항목에 단축키를 걸지 않는다(설정 keybind로 바꿀 수 있게). 단위 시험은 라틴 글자 키만 썼다
- 고침: `ctrl`이나 `super`가 붙은 키의 글자가 ASCII가 아니면 지금 자판의 ⌘ 층 글자로 트리거를 만든다(벤더 winit에 `command_layer_char`를 더함)
- 0.90.0에서 풀기: 단축키를 누를 때 영문 입력 소스로 바꾼다

## 31. `adjust_selection`(⇧→ 등)으로 줄 끝에서 다음 줄로 넘어가지 않는다

- 언제: 2026-11-20, #711. 주 세션이 쌓인 실제 창 확인 목록(0.9의 `adjust_selection`)을 격리한 창에서 돌리다 찾았다. 0.90.0까지의 동작이다
- 증상: 글을 고른 뒤 ⇧→(또는 `adjust_selection:right`)를 누르면 줄 끝에서 고른 글이 늘지 않는다. 끝점이 줄 뒤 빈 칸을 한 칸씩 지나는 중이라 80칸 창이면 70번 넘게 눌러야 다음 줄로 넘어간다. ⇧↓는 빈 줄로도 내려간다
- 까닭: 끝점 옮기기(`adjusted_point`)가 칸 좌표만 보고 한 칸씩 옮겼다. Ghostty `Selection.adjust`는 글이 있는 칸까지 건너뛴다. 시험은 Ghostty 시험을 옮기지 않고 좌표만 견줬다
- 고침: Ghostty `Selection.adjust`의 규칙(←·→는 글 있는 칸까지, ↓는 글 있는 줄까지, 맨 윗줄 ↑는 줄 처음, 넘치는 PageUp·PageDown은 Home·End, End는 글 있는 마지막 줄 끝)과 시험 열 개를 옮겼다
- 0.90.0에서 풀기: 줄을 넘을 때는 ⇧↓로 내려간 뒤 ⇧←·→로 맞춘다

## 32. `window-height`로 띄운 창의 줄 수가 모자란다

- 언제: 2026-11-20, #713. 주 세션이 쌓인 실제 창 확인 목록(0.12의 `vt-window-resize-allowed`)을 격리한 창에서 돌리다 찾았다. 0.90.0까지의 동작이다
- 증상: `window-width = 80`·`window-height = 20`으로 띄운 창에서 `stty size`가 `18 80`이다. `reset_window_size`도 같다
- 까닭: 창 내용이 제목 막대 밑까지 깔려 격자는 제목 막대 아래 비움만큼 아래에서 시작하는데, 처음 크기를 칸 수 × 칸 크기 + 여백으로만 구했다. `CSI 8 t`로 크기를 바꾸는 길은 이 높이와 탭 줄을 더해 줄 수가 맞았다
- 고침: 처음 크기와 `reset_window_size`에도 제목 막대 아래 비움과 탭 줄 높이를 더한다
- 0.90.0에서 풀기: `window-height`를 2 크게 적는다(기본 글꼴 크기에서)

## 33. 한국어 입력 소스에서 끝난 칸이 스페이스로 닫히지 않는다

- 언제: 2026-11-20, #715. 주 세션이 쌓인 실제 창 확인 목록(0.12의 "빨리 끝난 칸에서 아무 키나 눌러 닫기")을 격리한 창에서 돌리다 찾았다. 0.90.0까지의 동작이다
- 증상: `wait-after-command = true`나 빨리 끝난 칸에서 "아무 키나 누르면 이 칸을 닫습니다"가 나온 뒤, 입력 소스가 한국어면 스페이스와 글자 키로 닫히지 않는다. 화살표·Enter·Esc는 닫는다
- 까닭: 닫기 판단을 키 이벤트에서만 했는데, 한국어 입력기는 스페이스와 조합을 마친 글자를 입력기 확정으로 보낸다. 확정 글은 끝난 칸에 쓰이기만 했다
- 고침: 입력기가 확정한 글도 남은 칸을 닫는다. 조합 중인 글자로는 닫지 않는다(Ghostty와 같음)
- 0.90.0에서 풀기: 화살표·Enter·Esc로 닫는다

## 34. `open -g`로 띄워도 b-terminal이 앞으로 온다

- 언제: 2026-11-20, #717. 주 세션이 실제 창 확인을 돌리다, 뒤에서 띄운 시험 앱이 맨 앞 앱이 된 것을 보고 찾았다. 0.90.0까지의 동작이다
- 증상: `open -g`나 스크립트로 뒤에서 띄워도 뜨자마자 맨 앞 앱이 되어, 그 사이 치던 글이 b-terminal 창으로 간다
- 까닭: winit이 앱이 뜰 때 기본으로 `activateIgnoringOtherApps(true)`를 부른다. Ghostty는 LaunchServices로 뜨면(부모가 launchd) 활성화를 macOS에 맡긴다
- 고침: 부모가 launchd(1)면 스스로 활성화하지 않는다. 명령줄에서 바이너리를 바로 띄우면 지금처럼 스스로 앞으로 온다
- 0.90.0에서 풀기: 없음. 뒤에서 띄운 직후에는 치지 않는다

## 35. `toggle_visibility`로 다시 보인 창이 앞으로 오지 않는다

- 언제: 2026-11-20, #718. 주 세션이 쌓인 실제 창 확인 목록(0.14의 `toggle_visibility`)을 격리한 창에서 돌리다 찾았다. 0.90.0까지의 동작이다
- 증상: 숨긴 뒤 다시 토글하면 창은 보이지만 다른 앱이 맨 앞에 남는다. 창이 다른 앱 뒤에 보일 때 토글하면 앞으로 가져오지 않고 숨긴다
- 까닭: "숨겨져 있나"로 가르고 보일 때 `unhide`만 불렀다. `unhide`는 창을 되살릴 뿐 앱을 활성화하지 않는다. Ghostty는 "앱이 활성인가"로 가르고 보일 때 활성화한다
- 고침: 앱이 활성이면 숨기고, 아니면 되살린 뒤 활성화한다
- 같이 고친 것: AppleScript `perform action "toggle_visibility"`는 숨기기가 다음 키·마우스 이벤트가 올 때까지 미뤄졌다. 스크립트 명령의 답을 기다리는 동안 AppKit 이벤트 큐를 꺼내지 않아서다. 답을 받은 뒤 빈 이벤트를 넣어 큐를 깨운다
- 0.90.0에서 풀기: 다시 보인 뒤 Dock 아이콘이나 창을 누른다

## 36. 한국어 입력 소스에서 확인 막대의 y·n이 듣지 않는다

- 언제: 2026-11-20, #721. 주 세션이 쌓인 실제 창 확인 목록(0.21의 Dock 끌어놓기)을 격리한 창에서 돌리다 찾았다. 0.90.0까지의 동작이다
- 증상: 입력 소스가 한국어(2벌식)이면 "끌어놓은 파일을 실행할까요?"·클립보드·kitty 파일 확인 막대에서 Y 키로 허용되지 않고 N 키로 거절되지 않는다. Esc는 듣는다. 프로그램 막대는 Enter로 허용하지 않아 허용할 길이 없었다
- 까닭: 한국어 입력기가 Y 키를 `ㅛ` 조합으로 가져가 winit이 키 이벤트를 주지 않았다. 입력기를 꺼도 자판 글자가 `ㅛ`라 `y`와 맞지 않는다
- 고침: 막대가 떠 있는 동안 입력기를 끄고, 자판 글자가 ASCII가 아니면 키 묶음처럼 ⌘ 층 글자(`y`·`n`)로 읽는다
- 0.90.0에서 풀기: 막대가 뜨면 입력 소스를 영문으로 바꾼 뒤 `y`를 누른다

## 37. 처음부터 가려진 채 뜬 창을 보인다고 본다

- 언제: 2026-11-22, #723. 주 세션이 쌓인 실제 창 확인 목록(0.24의 2033 보고)을 격리한 창에서 돌리다 찾았다. 0.90.0까지의 동작이다
- 증상: `open -g`로 띄워 다른 앱 창 밑에 놓인 창에서 모드 2033을 켜면 "보임"(`CSI ? 999 ; 1 n`)이 온다. 앱을 숨겼다 풀어도 바뀌지 않는다. 사용자 셰이더 애니메이션과 kitty 애니메이션도 가려진 창에서 멈추지 않는다
- 까닭: winit은 가림 상태가 바뀔 때만 알린다. 앱은 창을 "보임"으로 시작하고 실제 상태를 읽지 않았다. 처음부터 가려진 창은 바뀐 적이 없어 알림이 오지 않는다
- 고침: 창을 만들 때 `NSWindow.occlusionState`를 읽어 그 값으로 시작한다(Ghostty `syncSurfaceTreeOcclusionState`와 같다). 보이게 되면 바뀜 알림이 와 곧 그린다
- 0.90.0에서 풀기: 창을 한 번 앞으로 가져왔다가 다른 창으로 가리면 그 뒤로는 맞다

## 38. Ctrl+왼쪽 클릭 메뉴와 십자·I자 포인터가 Ghostty와 다르다

- 언제: 2026-11-23, #725. 실제 창 확인 목록(0.21 오른쪽 클릭 메뉴, 0.43 사각 고르기 포인터)을 Ghostty 1.4.0 원본과 견주다 찾았다. 0.90.0까지의 동작이다
- 증상: vim(`set mouse=a`) 안에서 Ctrl+Shift+왼쪽 클릭에 메뉴가 뜬다. `right-click-action`이 `context-menu`가 아니면 셸에서 Ctrl+왼쪽 클릭이 메뉴도 오른쪽 클릭 동작도 하지 않는다. Shift 잡기를 켠 프로그램 안에서 Shift(+Option)를 눌러도 I자(십자)가 되지 않는다
- 까닭: 둘 다 "이 클릭을 지금 프로그램에 보고하는가"로 갈랐다. Ghostty는 프로그램이 마우스 모드를 켰는가만 본다
- 고침: Ghostty `menu(for:)`·`keyToMouseShape`처럼 프로그램의 마우스 모드만 본다. Ctrl+왼쪽은 오른쪽 클릭 동작 뒤 메뉴를 띄운다
- 0.90.0에서 풀기: 메뉴는 오른쪽 클릭으로 띄운다

## 39. 퀵 터미널이 Space를 옮길 때도 숨는다

- 언제: 2026-11-23, #727. 실제 창 확인 목록(0.17 퀵 터미널 blur)을 Ghostty 1.4.0 원본과 견주다 찾았다. 0.90.0까지의 동작이다
- 증상: 기본 설정(`quick-terminal-autohide = true`, `quick-terminal-space-behavior = move`)에서 퀵 터미널을 띄운 채 다른 Space로 옮기면 퀵 터미널이 숨는다
- 까닭: 포커스를 잃으면 Space를 보지 않고 늘 숨겼다
- 고침: Ghostty `windowDidResignKey`처럼 띄울 때의 Space(`CGSGetActiveSpace`)를 적어 두고, `move`에서 Space가 바뀌었으면 숨기지 않고 새 Space에 다시 띄운다
- 0.90.0에서 풀기: 옮긴 Space에서 퀵 터미널 단축키를 다시 누른다

## 40. 수식 키를 누른 채 프롬프트를 누르면 커서가 옮겨 가지 않는다

- 언제: 2026-11-23, #729. 실제 창 확인 목록(0.28 클릭으로 커서 옮기기)을 Ghostty 1.4.0 원본과 견주다 찾았다. 0.90.0까지의 동작이다
- 증상: 셸 통합 프롬프트에서 Option이나 Cmd를 누른 채 입력 글을 한 번 누르면 커서가 그대로다
- 까닭: 앱이 "수식 키가 없을 때"만 프롬프트 클릭을 보냈다. Ghostty는 수식 키를 보지 않고 고른 글이 남았는지를 본다
- 고침: 수식 키 조건을 빼고, 놓은 뒤 고른 글이 남았으면 보내지 않는다(Ghostty `maybePromptClick`과 같다)
- 0.90.0에서 풀기: 수식 키를 떼고 누른다

## 41. 입력 중 마우스를 숨겨도 칸 끌기 손잡이 점이 남는다

- 언제: 2026-11-23, #731. 실제 창 확인 목록(0.39 칸 끌기)을 Ghostty 1.4.0 `SurfaceGrabHandle.swift`와 견주다 찾았다. 0.90.0까지의 동작이다
- 증상: `drag-handle = auto`에서 칸 위쪽 띠에 마우스를 둔 채 글을 쳐 포인터가 숨어도 점 셋이 보인다
- 까닭: 손잡이 판단이 포인터가 숨었는지 보지 않았다. Ghostty `auto`는 `cursorVisible`이 거짓이면 점을 보이지 않는다
- 고침: `auto`이면 포인터가 숨은 동안 손잡이를 보이지 않는다. `always`는 그대로다
- 0.90.0에서 풀기: 마우스를 조금 움직이면 포인터와 함께 점이 다시 정상으로 보인다(보이는 것만의 차이)

## 42. kitty 키보드를 켠 프로그램에서 Shift+Tab·Shift+Enter가 Shift 없이 간다

- 언제: 2026-11-23, #735. 실제 창 확인 목록(0.66 kitty 키보드)을 격리한 실제 창에서 재다 찾았다. 0.91.0까지의 동작이다
- 증상: 프로그램이 kitty 키보드 프로토콜을 플래그 1(모호한 키 구분)이나 3으로 켜면 Shift+Tab·Shift+Enter·Shift+Backspace가 `\t`·`\r`·`0x7f`로 간다. Shift+Enter로 줄을 바꾸는 TUI에서 그냥 Enter가 된다. 플래그 8(모든 키를 이스케이프로)을 켠 프로그램은 맞다
- 까닭: winit은 Tab·Enter·Backspace·Esc 키의 글에 제어 문자(`\t`·`\r` 등)를 넣는다. 이것을 그대로 인코더에 넘겨, 인코더가 "글이 있으니 Shift는 글을 만드는 데 쓰였다"고 보고 Shift를 뺐다. Ghostty 1.4.0 macOS는 제어 문자로 시작하는 글을 키 글로 넘기지 않는다(`keyEventText`)
- 고침: 제어 문자로 시작하는 글은 kitty 인코더에 넘기지 않는다. Shift+Tab은 `ESC[9;2u`, Shift+Enter는 `ESC[13;2u`, Shift+Backspace는 `ESC[127;2u`이고 수식 키 없는 Tab·Enter·Backspace는 그대로 옛 바이트다. kitty를 켜지 않은 프로그램의 바이트는 바뀌지 않는다
- 0.91.0에서 풀기: 그 프로그램의 키 설정에서 Shift+Enter 대신 다른 키(예: Ctrl+j, Option+Enter)를 쓴다

## 43. 평소 셸에 붙여넣었는데 막대가 "암호 입력 중이라 내용은 숨깁니다"라고 한다

- 언제: 2026-11-23, #737. 실제 창 확인 목록(0.59 암호 입력 중 붙여넣기)을 격리한 실제 창에서 재다 찾았다. 0.91.0까지의 동작이다
- 증상: 창에 초점이 없는 동안 `read -s`·`sudo` 같은 암호 입력이 끝난 칸에, 줄바꿈이 든 글을 붙여넣으면 막대가 첫 줄 대신 줄 수와 "암호 입력 중이라 내용은 숨깁니다"만 보인다. 커서는 보통 모양이다
- 까닭: 칸의 암호 표시는 창에 초점이 있을 때만 200ms마다 살펴, 초점이 없는 동안 끝난 암호 입력은 옛 값으로 남는다. 붙여넣기 판단이 "옛 값 또는 요청 직전에 다시 읽은 값"이라 다시 읽은 값이 "아님"이어도 옛 값이 이겼다
- 고침: 요청 직전에 다시 읽은 termios 값을 따르고, 읽지 못할 때만 옛 값을 본다. 살핌 사이에 켜진 암호 입력은 다시 읽은 값으로 지금처럼 숨긴다. 같은 판단을 쓰는 선택 클립보드 나누기도 같다
- 0.91.0에서 풀기: 창을 한 번 눌러 초점을 준 뒤(0.2초 안에 표시가 새 값이 된다) 다시 붙여넣는다. 내용을 덜 보이는 쪽으로만 틀린다

## 44. `open --args`로 띄웠더니 창이 뜨지 않고 앱이 끝난다

- 언제: 2026-11-23, #743. 시험 앱 하나만 다크 모양으로 띄우려고 `open -n b-terminal.app --args -AppleInterfaceStyle Dark`를 쓰다 찾았다. 0.92.0까지의 동작이다
- 증상: 창이 뜨지 않는다. 표준 출력(`open --stdout`)에 `모르는 인자: -AppleInterfaceStyle`과 사용법이 있다. `--font-size=20`처럼 Ghostty식 설정 인자를 줘도 같다
- 까닭: 명령줄 해석이 모르는 인자를 모두 오류로 돌려 앱이 사용법만 찍고 끝났다. Ghostty는 이런 인자를 설정 진단으로 적고 그대로 뜨며 `--키=값`은 설정 덮어쓰기다
- 고침: `--키=값`은 설정 덮어쓰기(기본 설정 파일 다음, `--config-file` 앞)이고, 모르는 키와 `--`로 시작하지 않는 인자는 경고로 남기고 뜬다. 경고는 `b-terminal --show-config …`나 로그로 본다
- 0.92.0에서 풀기: 모르는 인자를 빼고 띄우거나, 설정은 `--config-file <파일>`로 넘긴다

## 45. yazi에서 정사각형 그림 미리보기가 납작하게(아래가 잘려) 보인다

- 언제: 2026-11-23, #747. 받은 yazi 26.9.1로 격리한 실제 창에서 96×96 PNG를 미리 보다 찾았다. 0.93.0까지의 동작이다
- 증상: 그림의 위 두 줄만 그려지고 맨 아래 줄이 비어 96×65px처럼 보인다. 화면 값에는 자리표시 글자(U+10EEEE) 6칸×3줄이 모두 있다
- 까닭: yazi는 kitty 유니코드 자리표시(`U=1`)로 그림을 놓는다. 그림을 칸 격자 가운데에 맞추면 여백이 4.5px처럼 .5로 떨어질 수 있고, 줄마다 푼 조각을 반올림하면 끝 줄 조각이 그림 끝을 1px 넘는다(Ghostty도 같은 값). Ghostty는 이 값을 GPU 텍스처 좌표로 써서 가장자리 픽셀로 그리는데, b-terminal은 그림을 넘는 조각을 통째로 그리지 않았다
- 고침: 그림을 넘는 부분은 가장자리 픽셀로 그린다. CPU 길과 GPU 길이 같은 함수로 그림 층을 만들어 둘 다 고쳐졌다
- 0.93.0에서 풀기: 없다. 칸 크기(글꼴 크기)를 바꾸면 여백이 .5로 떨어지지 않는 크기에서는 다 보인다

## 46. 설정을 다시 읽었는데 이미 나온 글자는 옛 글꼴 모양 그대로다

- 언제: 2026-11-23, #754. Ghostty #15280(격자를 다시 만든 뒤 대체 글꼴 글자가 깨짐)을 대조하다 코드에서 찾았다. 0.94.0까지 GPU 길(macOS 기본)의 동작이다
- 증상: `font-thicken`, `font-feature`, `font-variation`, `font-synthetic-style`, `font-codepoint-map`, 대체 글꼴처럼 칸 크기를 바꾸지 않는 글꼴 설정을 고치고 다시 읽으면, 화면에 이미 나왔던 글자는 옛 모양이고 처음 나오는 글자만 새 모양이다. 합자로 다듬은 글자는 다른 글자로 보일 수 있다
- 까닭: GPU 아틀라스는 글리프를 글꼴 픽셀 크기와 칸 치수로 가렸다. 설정을 다시 읽으면 글꼴 묶음은 새로 만들지만, 둘이 같으면 아틀라스가 옛 묶음의 글리프를 그대로 썼다
- 고침: 글꼴 묶음마다 만든 차례 번호를 주고 아틀라스가 그 번호로도 가른다(Ghostty가 격자를 늘어나는 번호로 가르는 것과 같다)
- 0.94.0에서 풀기: 창을 새로 열거나 글자 크기를 한 번 바꿨다 되돌린다. `BT_GPU=0`(CPU 길)에서는 생기지 않는다

## 47. 잘못 만든 SGR을 받은 글자에 Ghostty에는 없는 밑줄이나 색이 생긴다

- 언제: 2026-11-23, #761. Ghostty `sgr.zig` 시험을 옮겨 견주다 찾았다. 0.95.0까지의 동작이다
- 증상: `ESC[4:2:3m`(밑줄 모양 뒤에 값이 하나 더)을 받으면 홑밑줄이 켜지고, `ESC[38:2:0:1:2:3:4m`(콜론 색 뒤에 값이 하나 더)을 받으면 전경색이 바뀐다. Ghostty는 둘 다 무시한다
- 까닭: vendor/vte가 `4:` 뒤 하위 매개변수 수를 보지 않고 홑밑줄로 보았고, 콜론 직접 색은 넘치는 값을 버리고 받아들였다
- 고침: Ghostty처럼 둘 다 알 수 없는 속성으로 버린다(바른 `4:n`·`38:2:r:g:b`·`38:2:색공간:r:g:b`는 그대로)
- 0.95.0에서 풀기: 없다. 프로그램이 바른 SGR을 보내면 생기지 않는다

## 48. CAN이나 SUB로 끊긴 OSC가 제목·알림·클립보드·작업 폴더를 바꾼다

- 언제: 2026-11-23, #763. Ghostty OSC 파서 시험을 우리 엔진에 같은 바이트로 넣어 견주다(#760) 찾았다. 0.95.0까지의 동작이다. 실제로 이런 바이트를 내는 프로그램은 찾지 않았다
- 증상: `printf '\033]2;끊긴 제목\030'`을 받으면 창 제목이 "끊긴 제목"으로 바뀐다. `ESC]52;c;Zm9v SUB`는 클립보드 쓰기(허락했으면), `ESC]9;hello CAN`·`ESC]99;;hello SUB`는 알림, `ESC]7;file:///tmp/x CAN`은 작업 폴더 바꾸기가 된다. Ghostty와 xterm은 모두 버린다
- 까닭: 엔진(vendor/vte `advance_osc_string`)과 신호 스캐너(`notify.rs`)가 CAN·SUB에서 OSC를 끝낸 뒤 모인 바이트를 명령으로 썼다. #595(18번)에서 Ghostty `Parser.zig` 276줄만 보고 "끝낸 바이트와 상관없이 dispatch"로 맞췄는데, 그 줄이 부르는 `osc.zig` `Parser.end`가 CAN·SUB면 null을 돌려주는 것을 놓쳤다
- 고침: CAN·SUB로 끝난 OSC는 엔진과 스캐너 모두 버린다. CAN·SUB 자체는 실행하고, 뒤따르는 질의·새 OSC는 그대로 읽는다. BEL·`ESC \`·다른 ESC로 끝난 OSC, DCS·APC는 바뀌지 않는다
- 확인하는 법: `crates/bt-term/tests/osc_can_sub763.rs`, vendor/vte `osc_cancelled_by_can_or_sub_is_not_dispatched`
- 0.95.0에서 풀기: OSC를 BEL이나 `ESC \`로 끝내면 생기지 않는다. 받는 쪽에서 막는 설정은 없다

## 49. OSC 9 알림이 사라지거나 엉뚱하게 뜬다(`9;12`가 "12" 알림, 진행 막대가 0%)

- 언제: 2026-11-23, #764. Ghostty `osc9.zig` 시험을 옮겨 견주다 찾았다. 0.95.0까지의 동작이다
- 증상: ConEmu 식 프롬프트 표시 `ESC]9;12`가 프롬프트 표시 없이 "12" 알림을 띄우고, `9;5`·`9;10`은 "5"·"10" 알림이 된다. 반대로 `9;10;4`·`9;4;5`·`9;42;hi` 같은 알림은 사라진다. `9;4;1;900`은 100%가 아니라 0%로, 읽을 수 없는 값(`9;4;2`·`9;4;1;4_2`)은 값 없음이 아니라 0%로 보인다. 번호만 있는 `ESC]7 ST`가 알려진 작업 폴더를 지우고, `ESC]99;foobar ST`가 알림을 낸다
- 까닭: OSC 9를 "뒤가 `숫자;`이면 다른 ConEmu 명령"으로 뭉뚱그려 읽었다. Ghostty는 하위 명령마다 형식(`;` 위치, 값 범위)을 따로 보고, 맞지 않으면 알림으로 돌린다. 진행 값은 `?u8`(없을 수 있음)인데 우리는 `u8`이었다
- 고침: `osc9.zig`의 분기를 그대로 옮겼다. `9;12`는 133;A와 같은 신호(엔진의 의미 있는 프롬프트와 알림 쪽 `Prompt`)를 낸다. 진행 값은 0~100으로 자르고 없으면 없음으로 두며, 앱은 Ghostty macOS처럼 값 없는 보통·오류를 끝 모름 막대(오류는 빨강), 값 없는 멈춤을 가득 찬 막대로 그린다. `ESC]7 ST`와 `ESC]99;foobar ST`는 Ghostty처럼 명령이 아니다
- 0.95.0에서 풀기: 없다. 프로그램이 `9;12`·`9;4;…` 같은 값을 쓰지 않으면 생기지 않는다

## 50. 색 이름으로 바탕색을 바꾸는 OSC가 먹지 않거나, 깨진 글자가 든 제목이 제목을 지운다

- 언제: 2026-11-23, #765. Ghostty `osc/parsers/color.zig`·`hyperlink.zig` 시험을 같은 입력으로 옮겨 견주다 찾았다. 0.95.0까지의 동작이다
- 증상: `printf '\033]11;red\a'`로 바탕색이 바뀌지 않는다(`rgbi:`, `#` 없는 `ff0000`, 앞뒤 공백도 같다). `4;0;rgb:ff/00/00;`처럼 끝에 `;`가 붙은 OSC 4는 통째로 버려지고, 잘못된 번호 뒤에서 멈추지 않는다. `110;junk`가 전경색을 되돌린다. 깨진 UTF-8이 섞인 OSC 0·2는 제목을 빈 글자로 지우고, 앞뒤 공백이 지워지고, `;`가 16개를 넘으면 제목이 잘린다. `8;id=;주소`는 빈 id 링크가 되고 `8;id=foo;`는 열린 링크를 끝낸다
- 까닭: vendor/vte의 색 해석이 `#…`·`rgb:…`만 알았고, OSC 4·10~12는 조각 수·짝수 검사와 `continue`로 Ghostty와 다른 곳에서 멈추거나 이어 갔다. 제목은 깨진 조각만 빼고 이었고 `trim()`을 했으며, OSC 문자열을 16조각까지만 들고 있어 나머지를 버렸다
- 고침: Ghostty `RGB.parse`대로 읽고(X11 이름 표 포함), 색 OSC는 Ghostty처럼 빈 조각을 건너뛰며 첫 틀린 곳에서 멈춘다. 제목은 깨진 UTF-8이면 요청 전체를 무시하고 앞뒤 공백을 두며, 파서가 16번째 조각에 남은 `;` 포함 전부를 담는다. OSC 0·2·8은 `N;` 뒤가 2048바이트 이상이면 무시한다. 링크는 빈 `id=`를 id 없음으로 보고 `8;id=foo;`를 무효로 둔다
- 0.95.0에서 풀기: 색은 `rgb:ff/00/00`처럼 쓰고, 제목은 UTF-8이 맞는 글자만 보낸다. 다만 `#fff`는 Ghostty가 `ffffff`, 우리가 xterm처럼 `f0f0f0`으로 읽는 차이가 남는다

## 52. 키 묶음에 오타 인자를 썼는데 경고 없이 기본값으로 동작하고, `write_screen_file:copy,html`은 막힌다

- 언제: 2026-11-23, #768. Ghostty 1.4.0 `Binding.zig` 시험(13·45·46·69·70)을 옮겨 견주다 찾았다. 0.95.0까지의 동작이다
- 증상: `ignore:A`·`new_tab:x`·`quit:1`이 받아들여지고, `csi`·`esc`·`text`가 빈 글로 읽힌다. `jump_to_prompt:abc`는 -1, `increase_font_size:abc`는 1.0(`:nan`은 NaN)으로 바뀐다. 반대로 `write_screen_file:copy,html`은 "copy·paste·open" 경고로 버려진다. 같은 동작에 키가 둘이면 명령 팔레트가 가장 짧은 키를 보이고 `performable:` 묶음도 보인다
- 까닭: 동작 해석이 인자를 느슨하게 읽었고(Ghostty는 값 없는 동작의 콜론·글 동작의 콜론 빠짐·정수/실수 틀림을 모두 오류로 한다), `write_*_file`에는 형식 자리가 없었으며, 역방향 조회가 정한 차례를 모르는 해시 순서였다
- 고침: Ghostty `Action.parse`처럼 읽는다(틀린 값만 경고한다. 전부터 뜻이 분명했던 꼴은 Ghostty보다 넓게 계속 받는다: 콜론 없는 `set_tab_title`(제목 지우기), 값 없는 `increase_font_size`·`decrease_font_size`(1)·`jump_to_prompt`(-1). Ghostty는 이 꼴들을 오류로 본다). `write_*_file`은 `,plain`·`,vt`·`,html`을 받아 그 형식으로 쓰고(`html`은 `.html`), 팔레트는 가장 나중에 정한 키를 보이며 `performable:`은 뺀다
- 0.95.0에서 풀기: 인자를 틀리지 않게 쓴다. 형식(`vt`·`html`)으로 쓰는 길은 없었다

## 51. 키 묶음의 키 이름을 틀리게 써도 경고가 없거나, Ghostty 이름이 다른 키로 읽힌다

- 언제: 2026-11-23, #767. Ghostty 1.4.0 `Binding.zig`의 트리거 시험을 같은 입력으로 옮겨 견주다 찾았다. 0.95.0까지의 동작이다
- 증상: `foo=ignore`·`Keya=ignore`가 경고 없이 받아져 아무 키에도 듣지 않는다. 반대로 `KeyA`·`Digit1`·`ArrowUp`·`zero`·`kp_0`·`left_shift`·`physical:zero`와 `a+shift`는 "모르는 수식 키"로 버려진다. `ctrl++`·`plus`는 `=` 키에 묶이고, `==text:=`·`ctrl+==text:=hello`는 경고가 나며, `ctrl+Ö`는 눌린 `ö`에 맞지 않는다. `f13`~`f25`·`caps_lock`·`print_screen`·`pause`·`context_menu`·미디어 키·숫자 패드 키는 쓸 수 있는데 눌러도 묶음이 듣지 않는다. `shift+shift+a`·`all:all:a`는 경고 없이 받아진다
- 까닭: 트리거 해석이 키 이름을 소문자로 바꿔 아무 글이나 받았다(이름 표가 없다). 수식 키와 키를 `+`로 나눈 뒤 마지막 조각만 키로 보았고, 설정 줄은 마지막 `=`로 나눠 `=`가 든 트리거를 못 읽었다. 눌린 키를 이름으로 바꾸는 쪽은 이름 있는 키 27개만 만들었다
- 고침: Ghostty와 같은 순서로 해석한다. 키 이름 표, 글자 한 개(유니코드는 소문자로 접음), W3C 이름, `catch_all`, 1.1 호환 이름 순이고 모르는 이름은 경고다. 수식 키·접두사를 겹치면 경고. 전에 받던 꼴 가운데 뜻이 분명한 것은 Ghostty보다 넓게 계속 받는다: 수식 키 이름의 대소문자(`SHIFT+a`)와 `+` 둘레 공백(`shift + a`), 옛 별칭(`return`·`esc`·`pageup`·`pagedown`), 대소문자 다른 키 이름(`Enter`). `=`는 바로 뒤에 `+`·`=`가 오지 않는 첫 것이 나눔 자리다. `+`는 글자 `+`다. winit이 주는 이름 있는 키와 숫자 패드 키(물리 이름 `numpad_1`을 먼저, 없으면 글자)를 Ghostty 이름으로 맞춘다. 글자 키를 물리 자리가 아니라 글자로 맞추는 것(`key_a`는 `a`)은 그대로다
- 0.95.0에서 풀기: 설정의 키 이름을 Ghostty 이름으로 고친다(`=` 키를 `ctrl++`나 `plus`로 묶었던 곳은 `ctrl+equal`로). 경고는 `b-terminal --show-config`나 로그에서 본다

## 56. iTerm2 도구(`it2copy`)나 `OSC 1337 ; Copy=`가 클립보드에 쓰지 못한다

- 언제: 2026-11-23, #778. Ghostty 1.4.0 `stream_terminal.zig` 시험(`ESC]1337;Copy=:aVRlcm0y ESC\` → "iTerm2" 쓰기 한 번)을 같은 입력으로 넣어 견주다 찾았다. 0.96까지의 동작이다
- 증상: `printf '\033]1337;Copy=:aVRlcm0y\033\\'`을 찍어도 클립보드가 바뀌지 않고 확인 막대도 뜨지 않는다
- 까닭: 엔진(vendor vte)이 OSC 1337을 몰라 `unhandled`로 버렸다
- 고침: 엔진이 `1337;Copy=:base64`를 OSC 52 쓰기(`c`)와 같은 호출로 넘긴다. 그 뒤는 OSC 52와 같은 길이라 `clipboard-write`(`deny`면 엔진이 사건을 내지 않고, `ask`면 확인 막대)와 보고 있는 칸 규칙·크기 상한·base64 검사를 그대로 거친다. Ghostty처럼 키는 대소문자를 가리지 않고, `=` 바로 뒤가 `:`가 아니거나(종류 글자 포함) 값이 비었거나 `?`면 버린다. 클립보드를 읽는 길은 만들지 않았다
- 0.97.0에서 풀기: `clipboard-write = deny`로 두었다면 그대로 막혀 있다. 확인은 `clipboard-write = ask`로 두고 위 `printf`를 찍어 막대가 뜨는지 본다

## 53. UTF-8이 아닌 8비트 C1 바이트(0x80~0x9F)가 화면에서 아무 표시 없이 사라진다

- 언제: 2026-11-23, #775. Ghostty 1.4.0 `stream.zig` 시험 `stream: ground state UTF-8-decoded C1 controls are ignored`(3764줄)를 같은 바이트로 옮겨 견주다 찾았다. 0.95.0까지의 동작이다
- 증상: `printf 'A\x9bB'`가 `AB`로 보이고, `A\x84B\x85C\x8dD\x88E`는 `ABCDE`(커서 5열)다. Latin-1·CP949 같은 옛 인코딩 글이나 깨진 바이트가 섞인 출력에서 글자가 말없이 빠진다
- 까닭: vendor/vte가 바닥 상태에서 UTF-8로 읽을 수 없는 한 바이트가 0x9F 이하이면 C1 제어로 `execute`에 보냈고, 엔진이 모르는 제어라 버렸다. Ghostty는 바닥 상태의 8비트 C1을 제어로 읽지 않고 UTF-8 디코더가 거절한 바이트마다 U+FFFD 하나를 찍는다
- 고침: UTF-8로 읽을 수 없는 바이트는 C1이어도 U+FFFD 하나로 찍는다. `C2 80`~`C2 9F`처럼 UTF-8로 풀린 C1은 전처럼 버리고(Ghostty와 xterm이 그렇다), NBSP(`C2 A0`)와 `Ü`(`C3 9C`)는 찍는다. 시퀀스 중간(`E0 9B`, `E0 80 9B`)은 Ghostty 디코더처럼 거절된 앞 바이트와 그 C1이 각각 U+FFFD이고, `E2 82 9B`처럼 이어지는 바이트로 맞으면 한 글자(U+209B)다. 문자열(OSC·DCS·CSI) 안의 C1은 이 고침의 범위가 아니다
- 0.95.0에서 풀기: 없다. 프로그램이 0x80~0x9F를 UTF-8이 아닌 채로 내보내지 않으면 생기지 않는다

## 54. APC가 원시 0x9C(8비트 ST)에서 끝나지 않아 뒤의 글이 통째로 사라진다

- 언제: 2026-11-23, #776. Ghostty 1.4.0 `stream.zig` 시험 `stream: apc bulk slice C1 ST`(5337줄)와 `stream_terminal.zig` 6781줄 시험을 옮겨 견주다 찾았다. 0.95.0까지의 동작이다
- 증상: `printf '\033_Gpayload\234AB'`처럼 APC를 8비트 ST(0x9C)로 끝내면 `AB`가 찍히지 않는다. 엔진이 CAN·SUB·ESC가 올 때까지 문자열 안에 있고, 신호 스캐너도 같아서 kitty 그래픽 명령이 실행되지 않는다. 0x9B·0x9D·0x90이 와도 문자열이 이어진다
- 까닭: 엔진(`SosPmApcString`)과 스캐너(`St::Apc`·`St::SosPm`)가 CAN·SUB·ESC만 문자열을 끝내는 바이트로 봤다. Ghostty `parse_table.zig`의 "anywhere" 전이는 C1도 상태를 바꾼다
- 고침: Ghostty 표를 그대로 옮겼다. 0x80~0x8F·0x91~0x97·0x99·0x9A·0x9C는 APC를 끝내고 Ground로 간다(앞의 것들은 `ESC (c-0x40)`처럼 실행: 0x85는 NEL, 0x84는 IND). 0x9B는 CSI, 0x90은 DCS, 0x9D는 OSC를 연다. 0x98·0x9E·0x9F와 0xA0~0xFF는 문자열에 머물고 자료도 아니다. BEL은 전처럼 자료다. 끝난 kitty 그래픽 명령은 Ghostty 앱(`stream_handler.zig` `apcEnd`)처럼 끝난 까닭과 상관없이 실행한다(CAN·SUB로 끝난 것과 같다). SOS(`ESC X`)·PM(`ESC ^`)은 같은 상태라 함께 맞는다
- 0.95.0에서 풀기: APC는 `ESC \`로 끝낸다(kitty 그래픽 도구는 이미 그렇게 쓴다). APC 자료에 UTF-8 글(`Ü`의 0x9C 같은 바이트)을 넣으면 이제 거기서 끝나니, 자료는 base64 같은 ASCII로 보낸다

## 55. 모드 질의(DECRQM)가 잘못된 꼴에도 답하거나, 바꿀 수 없는 모드를 "모름"으로 답한다

- 언제: 2026-11-23, #777. Ghostty 1.4.0 `stream.zig`·`modes.zig` 시험과 견주다 찾았다. 0.95.0까지의 동작이다
- 증상: `printf '\033[$p'`가 `ESC[0;0$y`, `\033[4;20$p`가 `ESC[4;2$y`, `\033[?4;7$p`가 `ESC[?4;2$y`로 답한다(`4:20$p`도 같다). 응용이 질의 하나에 답이 둘 오거나 엉뚱한 모드의 답을 읽는다. 또 `\033[?117$p`(DECECM)와 ANSI 1·5·7·10·11·13~19는 "바꿀 수 없음"(4)이어야 하는데 0(모름)을 답해, 응용이 지원 여부를 잘못 가른다
- 까닭: `$p` 처리가 첫 매개변수만 읽고 개수를 보지 않았다. 파서가 마지막 매개변수를 늘 밀어 넣어 `CSI $ p`가 `0`으로 보였다. Ghostty는 정확히 하나일 때만 답하고(숫자 없는 끝 매개변수는 세지 않고, 콜론이 든 CSI는 `m`이 아니면 버린다) 위 모드에 영구 리셋을 답한다
- 고침: 숫자 없는 끝 매개변수를 파서가 표시하고(`Params::trailing_empty`), DECRQM은 Ghostty가 세는 대로 하나일 때만 답한다. 위 모드는 4를 답한다(우리가 구현한 모드와 겹치지 않음을 확인했다). `?65653`처럼 큰 번호가 다른 모드로 읽히지 않는 것은 그대로다
- 0.95.0에서 풀기: 없다. 응용이 잘못된 꼴을 보내거나 위 모드를 질의하지 않으면 생기지 않는다

## 59. `clipboard-codepoint-map`이 `copy_to_clipboard:vt`·`html`(과 `mixed`의 HTML)에는 걸리지 않는다

- 언제: 2026-11-23, #787. Ghostty 1.4.0 `formatter.zig`의 시험 `Page codepoint_map with styled formats`(7637줄)를 같은 입력으로 옮겨 견주다 찾았다. 0.96.0까지의 동작이다
- 증상: 10열에 `\e[31mred text\e[0m`를 찍고 규칙 `U+0065=X`(e→X)로 `copy_to_clipboard:vt`를 하면 `"\x1b[0m\x1b[38;2;204;102;102mred text\x1b[0m"`가 나온다. 맵이 걸리지 않아 `rXd tXxt`가 아니다. `html`과 `mixed`의 HTML도 같고 text/plain만 바뀐다
- 까닭: 맵을 글을 뽑은 뒤의 문자열(`map_clipboard_text`)에만 걸었고, VT·HTML을 만드는 `copy_format`에는 맵 단계가 없었다. Ghostty는 `Surface.zig` 2338줄이 `codepoint_map`을 담은 한 옵션을 만들어 `vt`·`html` 갈래가 그것을 복사해 `emit`만 바꾸고, `writeCodepointWithReplacement`(2099줄)가 형식과 상관없이 글자를 바꾼다(그 밑의 "VT에는 적용하지 않는다"는 주석은 코드와 반대다)
- 고침: `copy_format::Options`에 `codepoint_map`을 두고, 글자 칸의 글자(결합 글자 포함)마다 마지막 규칙을 찾아 바꾼 글을 글자마다 그 형식의 `codepoint`로 낸다. 그래서 스타일은 칸의 것을 그대로 쓰고, `html`에서는 바꾼 글도 이스케이프·숫자 참조가 된다. 하이퍼링크 주소와 빈 칸에는 걸지 않는다. `write_selection_file`·`write_screen_file`·`write_scrollback_file`은 Ghostty `writeScreenFile`이 맵을 주지 않으므로 그대로 걸지 않는다
- 0.96.0에서 풀기: 없다. `vt`·`html`로 복사한 글에 맵이 필요하면 붙여넣은 뒤 바꾼다

## 58. 줄 끝에서 넘어간 넓은 글자 자리에서 끝나는 고르기가 엉뚱한 글자를 복사하거나 앱이 죽는다

- 언제: 2026-11-23, #786. Ghostty 1.4.0 `formatter.zig` 시험과 견주다(#760) 찾았다. 0.96.0까지의 동작이다
- 증상: 3열 창에서 `printf '1A⚡Z'` 뒤 첫 줄 세 칸을 끌어 복사하면 `1A`가 나온다. 윗줄이 있으면 그 줄의 첫 글자가 섞인다(`Q`와 `1A⚡Z`면 `1AQ`). 개발(debug) 빌드에서는 격자 밖을 읽어 `storage.rs`에서 panic이 난다
- 까닭: 넓은 글자가 줄 끝에 들어가지 않아 다음 줄로 넘어가면 줄 끝 칸에 자리표시(spacer head)가 남고 글자는 다음 줄 첫 칸에 있다. 고르기가 그 칸에서 끝나면 그 글자를 덧붙이는데, 엔진이 다음 줄이 아니라 윗줄(`line - 1`)을 읽었다. 윗줄이 없으면 격자 밖이다. 또 사각 고르기도 같은 덧붙이기를 했다(Ghostty는 사각이면 끝을 넓히지 않는다)
- 고침: 다음 줄 첫 칸(결합 문자 포함)을 읽고, 다음 줄이 없으면 덧붙이지 않는다. 사각 고르기는 넓히지 않는다. 기대는 `1A⚡`(글자 고르기), `⚡`(끝 한 칸만), `1A`(사각)이다
- 0.96.0에서 풀기: 줄 끝에 넓은 글자가 걸리는 줄을 줄 끝 칸까지 끌어 고르지 않으면 생기지 않는다(그 칸 앞에서 멈추면 `1A`)

## 57. 글 복사가 끝에 빈 줄을 잔뜩 붙이고, 화면 파일이 긴 줄을 끊고, 친 끝 공백이 늘 사라진다

- 언제: 2026-11-23, #785. Ghostty 1.4.0 `formatter.zig` 시험 124개와 견주다(#760) 찾았다. 0.96.0까지의 동작이다
- 증상: `clear; printf 'hello, world\n'` 뒤 Cmd+A, Cmd+C로 복사하면 `hello, world\n$\n` 뒤에 줄바꿈이 21개 더 붙는다(한 번 잼). 괄호 붙여넣기를 끈 프로그램에 붙이면 끝 줄바꿈이 명령을 실행시킬 수 있다. `write_screen_file:plain`은 창 폭에서 접힌 줄을 잇지 않아(`hello worl\nd test`) 긴 줄이 끊긴다. `hello   `처럼 직접 친 줄 끝 공백은 `clipboard-trim-trailing-spaces = false`에서도, `write_*_file`에서도 사라진다
- 까닭: 고른 글을 엔진(`selection_to_string`)이 줄마다 `\n`을 붙여 만들고 마지막 하나만 뗐다. 줄 끝 공백은 엔진이 "공백 칸 = 빈 칸"으로 세어 먼저 지웠다. 화면 파일은 접힘을 보지 않는 한 줄 읽기에 `trim_end()`를 걸었다. Ghostty는 글 없는 줄을 모았다가 뒤에 글 있는 줄이 올 때만 줄바꿈으로 내고(`blank_rows`), 쓴 칸과 한 번도 쓰지 않은 칸을 가른다
- 고침: VT·HTML 복사가 이미 쓰던 `formatter.zig` 옮김(`copy_format`)에 글만 내는 꼴(`Emit::Plain`)을 더해 고른 글·화면 파일·스크롤백 파일이 같은 길로 나온다. 엔진 칸의 `WRITTEN` 표시로 쓴 공백과 빈 칸을 가르므로 칸 표현은 바꾸지 않았다. 복사의 줄 끝 공백은 전처럼 앱이 `clipboard-trim-trailing-spaces`로 지운다. 고른 영역 가운데의 빈 줄과 탭 글자는 그대로다. 줄 고르기(세 번 누르기)가 끝에 붙이던 줄바꿈은 없어졌다(Ghostty 줄 고르기도 줄바꿈이 없다)
- 0.96.0에서 풀기: 끝의 빈 줄은 붙여넣기 전에 지운다. 화면 파일의 긴 줄은 `plain` 대신 `vt`나 `html`로 쓴다(이쪽은 접힌 줄을 잇는다)

## 62. 글리프 프로토콜(`ESC _ 25a1;…`)에 터미널이 답하지 않아 응용이 지원 여부를 알지 못한다

- 언제: 2026-11-23, #779. Ghostty 1.4.0의 글리프 프로토콜(`src/terminal/apc/glyph/`) 시험을 같은 입력으로 옮겨 견주다 찾았다. 0.97.0까지의 동작이다
- 증상: `printf '\033_25a1;s\033\\'`로 지원을 물으면 답이 없다. 등록(`r;cp=e0a0;…`)·질의·지우기도 마찬가지라, 이 프로토콜로 아이콘을 등록하는 TUI는 답을 기다리다 시간 초과로 "지원 안 함"으로 본다. 시험 26개 가운데 24개가 이 때문에 실패했다(답이 빈 글)
- 까닭: 신호 스캐너가 APC를 kitty 그래픽(`G`)만 넘기고 나머지는 버렸다. 이 프로토콜을 받는 길이 없었다
- 고침: 스캐너가 `25a1;`로 시작하는 APC를 글리프 신호로 넘기고(요청 한 줄은 식별자 뒤 1MiB까지), 세션이 Ghostty 앱처럼 검사·저장·답한다(`crates/bt-term/src/glyph.rs`). 지원 형식은 `glyf`만, 등록은 칸마다 1024개, PUA만, 풀면 64KiB까지이고 합성·힌팅은 거절한다. 전체 초기화(RIS)에서 지운다. 등록한 글리프를 **그리지는 않는다**: Ghostty 1.4.0도 렌더러와 글꼴이 글로서리를 부르지 않는다(다만 쓰는 쪽이 `q`로 덮임을 물어 아이콘 글꼴 대신 쓸지 정하므로, 답은 Ghostty와 같다). 답에는 요청의 글을 넣지 않고 cp·상태를 숫자로 다시 쓴다
- 0.97.0에서 풀기: 없다. 이 프로토콜을 쓰지 않는 응용은 영향이 없다

## 60. DCS 안의 한글이나 `Ü`에서 DCS가 끝나고 뒤 글이 화면에 찍힌다

- 언제: 2026-11-23, #792. #775·#776을 고칠 때 Ghostty 1.4.0 `parse_table.zig` 시험 `dcs_passthrough: high bytes are payload data`(412줄)와 범위 밖으로 남긴 차이다. 0.97.0까지의 동작이다
- 증상: 날 한글이 든 DCS(`printf '\033P1000p%output %1 한글\n\033\\after'` 같은 tmux 제어 모드 모양, `\033Ptmux;한글\033\\after`)에서 `한`은 사라지고 `글\nafter`처럼 남은 글이 화면에 찍힌다. 질의 DCS(DECRQSS·XTGETTCAP) 본문에 높은 바이트가 끼어도 거기서 답한다
- 까닭: 엔진과 신호 스캐너가 DCS 통과 상태에서 원시 0x9C(8비트 ST)로 DCS를 끝냈고, 0x80~0xFF의 나머지는 말없이 버렸다. 0x9C는 UTF-8 연속 바이트이기도 해서 한글 대부분(`한` = ED 95 9C)과 `Ü`(C3 9C)에 들어 있다
- 고침: DCS 통과 상태에서 0x80~0xFF를 모두 자료로 둔다(Ghostty와 같음). DCS는 `ESC \`와 CAN·SUB·ESC로만 끝난다. 무시 상태(`dcs_ignore`)는 전부터 0x80 이상을 버렸다. 이슈에 적은 `ESC P tmux; ESC ESC ]9;한글 알림 BEL ESC \`는 전에도 깨지지 않았다(안쪽 글은 OSC 문자열이라 0x9C가 자료였고, 안쪽 알림은 신호를 내지 않는 것이 설계다). 시험으로 고정했다
- 0.97.0에서 풀기: DCS 안에 UTF-8 글을 넣지 않거나 `ESC \`로 끝낸다

## 61. 이스케이프·CSI·DCS 머리 안의 8비트 C1이 무시되어 시퀀스가 Ghostty와 다르게 읽힌다

- 언제: 2026-11-23, #793. #775(바닥 상태)와 #776(APC·SOS·PM)을 고칠 때 범위 밖으로 남긴 차이다. 0.97.0까지의 동작이다
- 증상: `printf '\033[1\2332m'`처럼 CSI 도중에 0x9B가 오면 Ghostty는 CSI를 새로 시작해 `2m`만 적용하는데, 우리는 0x9B를 버리고 `12m`으로 읽었다. `ESC [ 0x9C …`·`ESC 0x85`·`ESC [ 0x9D…`도 C1을 무시하고 시퀀스를 이어 갔다. 이진 파일이나 깨진 출력에서만 보인다
- 까닭: vendor/vte가 Escape·EscapeIntermediate·CSI·DCS 머리 상태에서 0x80 이상을 `anywhere`(CAN·SUB·ESC만)로 넘겨 버렸다. 스캐너도 같았고 `ESC (` 같은 중간 바이트 뒤는 상태 없이 Ground로 보았다
- 고침: Ghostty `parse_table.zig` 51~66줄 "어디서나" 전이를 그 표가 덮지 않는 상태 전부에 그대로 둔다. 0x80~0x8F·0x91~0x97·0x99·0x9A는 Ground로 가며 `ESC (c-0x40)`처럼 실행(0x85는 NEL, 0x84는 IND), 0x9C는 Ground, 0x9B·0x90·0x9D는 CSI·DCS·OSC를 새로 열고, 0x98·0x9E·0x9F는 SOS·PM·APC 문자열로 가며, 0xA0~0xFF는 표에 없어 머문다. 표가 0x80 이상을 덮는 세 상태는 그대로 자료다: OSC 문자열(0x20~0xFF 자료), DCS 통과(#792, 자료), DCS 무시(버림). 스캐너는 같은 표를 따르고 `ESC (` 뒤를 `EscInter` 상태로 따로 두었다. tmux 패스스루 안쪽은 신호를 내지 않는 쪽으로 같이 움직인다
- 0.97.0에서 풀기: 시퀀스 한가운데에 0x80~0x9F를 넣지 않는다

## 63. kitty 그래픽 명령이 틀린 값을 0이나 기본값으로 읽어 실행하고, 조각 전송이 Ghostty와 다르게 풀린다

- 언제: 2026-11-23, #798. Ghostty 1.4.0 kitty 그래픽 시험 견주기(#760)에서 찾았다. 0.98.0까지의 동작이다
- 증상: `i=10000000000`·`z=-9999999999`·`i=abc`·`a=z`·`t=q`·`o=q`를 보내면 Ghostty는 명령을 버리고 답하지 않는데 우리는 0이나 기본값으로 읽어 실행했다(번호 없는 그림으로 저장하거나 `EINVAL`을 답함). 조각마다 `=` 채움을 붙이는 클라이언트(`a=t,f=32,s=1,v=1,i=5,m=1;AAA=` 다음 `m=0;AAA=`)는 Ghostty에서 `OK`, 우리는 `ENODATA`였다. 마지막 조각의 `q=1`을 무시해 `OK` 답이 셸 입력으로 샜고, 같은 번호의 옛 그림은 첫 조각이 아니라 마지막 조각에서 지웠으며, `t=f,m=1`은 조각을 기다렸고, `a=f`에 `i`·`I`가 함께 오면 `EINVAL`이 아니라 `ENOENT`였다
- 까닭: 제어 글을 `,`로 잘라 값을 `unwrap_or(0)`으로 읽었고, base64를 조각을 이은 뒤 한 번에 풀었다. `q`와 지우기 시점은 첫 조각의 제어와 마지막 조각 때를 썼고, `m`은 전송 방식을 보지 않았다
- 고침: Ghostty `Parser`의 상태 기계를 옮겨(`graphics.rs`의 `scan_command`) 읽을 수 없는 명령을 버린다. 조각마다 base64를 풀고, 마지막 조각이 올린 `q`를 쓰며, 옛 그림은 `t`·`T`의 첫 조각에서 지운다. `m`은 직접 전송하는 `t`·`T`·`f`에서만 읽고, `i`·`I` 동시 사용은 조각을 열기 전에 `EINVAL`로 거른다. 기능 질의(`a=q`에 `e=`)는 전처럼 느슨하게 읽는다(Ghostty에 없는 규약). 바뀐 시험은 `crates/bt-term/tests/kitty_ghostty798.rs`
- 0.98.0에서 풀기: 클라이언트가 틀린 값을 보내지 않게 한다. 조각 크기를 3바이트의 배수로 맞추면 채움 문제는 피한다

## 64. kitty 그림 자료: 남는 RGB 자료를 받고, PNG 크기는 다 푼 뒤에 본다

- 언제: 2026-11-23, #802. Ghostty 1.4.0 kitty 그래픽 시험 견주기(#760)에서 찾았다. 0.98.0까지의 동작이다
- 증상: `a=t,f=24,s=1,v=1,i=31;AAAA`(1x1 RGB에 4바이트)가 `OK`로 저장됐다(Ghostty는 `EINVAL`). 10000x10000 RGBA PNG(1.9MB)를 보내면 PNG를 다 푼 뒤에 크기 상한을 보아, 상주 메모리가 잠깐 약 415MB까지 오르고(개발 빌드, 이슈가 한 번 잼) `EFBIG`로 답했다. 믿지 않는 출력이 상한(64MiB)보다 훨씬 큰 메모리를 잠깐 잡는 길이다
- 까닭: RGB·RGBA 자료는 모자란 쪽만 보고 남는 쪽은 잘랐다(프레임 `a=f`용 규칙을 그림에도 썼다). PNG는 폭·높이를 읽은 뒤 한 변(10000px)만 보고 풀 버퍼(`폭*높이*4`)를 잡은 다음에 `64MiB`를 견줬다
- 고침: 자료 길이가 `s*v*바이트`와 다르면 거절한다(짧으면 전처럼 `ENODATA`, 길면 `EINVAL`). 프레임만 자른다. PNG는 앞 24바이트(IHDR)로 풀 크기를 RGBA 8비트(`폭*높이*4`)로 먼저 세어 한 변 10000px·64MiB를 넘으면 풀기 전에 거절한다(전과 같은 코드: 한 변이 넘으면 `EBADPNG`, 바이트가 넘으면 `EFBIG`). 풀 버퍼를 잡은 뒤 한 번 더 본다. 시험은 머리만 큰 작은 PNG가 `EFBIG`로 답하는지 본다(풀기를 시작했다면 자료가 쓰레기라 `EBADPNG`가 된다). 메모리 꼭대기는 재지 않았다
- 0.98.0에서 풀기: 없다. 이 출력을 터미널에 찍지 않는다

## 68. 세 번 눌러 줄을 고르면 앞 공백과 프롬프트까지 들어온다

- 언제: 2026-11-23, #803. Ghostty 1.4.0 단위 시험 견주기(#760)에서 찾았다. 0.98.0까지의 동작이다
- 증상: `printf '  들여쓴 줄\n'`을 찍고 그 줄을 세 번 눌러 복사하면 앞 공백 두 칸이 함께 복사된다. 셸 통합(OSC 133)이 있어도 프롬프트 줄을 세 번 누르면 `$ `까지 들어오고(Ghostty는 입력만), 글을 쓰지 않은 칸이나 빈 줄을 눌러도 줄 전체가 골라졌다
- 까닭: 줄 고르기가 엔진의 `Lines` 고르기라 줄 첫 칸부터 끝 칸까지를 그대로 잡았다. 앞뒤 공백도, OSC 133 칸 종류가 바뀌는 곳도 보지 않았다
- 고침: `Screen.selectLine`(3197줄)과 `SelectionGesture.dragSelectionLine`을 엔진의 고르기 범위 계산에 그대로 옮겼다. 접힌 줄을 이어 한 줄로 보고, 누른 칸의 칸 종류(프롬프트·입력·출력, 표시 없는 칸은 출력)가 바뀌는 곳에서 줄을 끊고, 앞뒤의 널·빈칸·탭을 뺀다. 글이 하나도 없는 줄은 고르지 않는다. 하이라이트와 복사, 끌기가 모두 같은 범위 계산을 지난다. 시험: `crates/bt-term/tests/select_ghostty803.rs`(Ghostty `selectLine` 시험 9546·9628·9681·9792·9837·9888·9907·9957·10003·10072·10175·10264)
- 0.98.0에서 풀기: 줄을 한 번 더 끌어서 앞 공백이 빠지게 고르거나 복사한 글을 손으로 다듬는다

## 69. 두 번 눌러 낱말을 고를 때 경계 글자에서 이웃 낱말까지 번진다

- 언제: 2026-11-23, #804. Ghostty 1.4.0 단위 시험 견주기(#760)에서 찾았다. 0.98.0까지의 동작이다
- 증상: `foo  bar`의 두 공백 가운데를 두 번 누르면 `foo `가 골라진다(Ghostty는 공백 두 칸). ` 'abc' `의 따옴표를 누르면 `'abc`가 골라지고, 공백만 있는 줄을 누르면 눌린 칸 하나만 골라진다. `abcd語ef`를 5열에서 `abcd`를 고르면 줄 끝 자리표시 칸까지 잡혀 `abcd語`가 복사된다
- 까닭: 엔진의 낱말 고르기가 누른 칸에서 왼쪽·오른쪽으로 "경계 글자 하나를 만날 때까지"만 찾아, 경계 글자를 눌렀을 때 그 글자의 종류를 따라가지 않았다. 넓은 글자의 뒤 칸과 줄 끝 자리표시 칸도 따로 다루지 않았다
- 고침: `Screen.selectWord`(3457줄)와 `SelectionGesture.dragSelectionWord`를 그대로 옮겼다. 누른 칸이 경계 글자인지로 "이어진 경계 구간"과 "이어진 낱말 구간"을 가르고, 글을 찍지 않은 칸은 늘 경계이며, 넓은 글자의 뒤 칸은 앞 칸의 글자를, 줄 끝 자리표시 칸은 다음 줄 첫 칸의 넓은 글자를 따른다. 줄 끝을 넘는 것은 그 줄이 다음 줄로 이어질 때만 한다. 글을 찍지 않은 칸을 누르면 고르지 않는다. 짝 괄호를 누르면 괄호 안까지 고르던 엔진의 동작은 Ghostty에 없어 뺐다. URL 두 번 누르기(`linkAtPin`)는 그대로다. 시험: `crates/bt-term/tests/select_ghostty803.rs`(Ghostty `selectWord` 시험 10299·10424·10464·10496·10558·10634·10704)
- 0.98.0에서 풀기: 경계 글자 위가 아니라 낱말 위를 두 번 누른다

## 70. 전체 고르기(⌘A)가 보이는 화면만 고르고 빈 칸까지 하이라이트한다

- 언제: 2026-11-23, #805. Ghostty 1.4.0 단위 시험 견주기(#760)에서 찾았다. 0.98.0까지의 동작이다
- 증상: `seq 1 200` 뒤 ⌘A·⌘C를 하면 보이는 줄만 복사된다(Ghostty는 1부터 200까지). `ABC  DEF\n 123\n456`(10x10)에서 하이라이트가 (0,0)-(9,9)로 빈 줄과 빈 칸까지 덮는다(Ghostty는 (0,0)-(2,2))
- 까닭: 앱의 `select_all`이 화면 왼쪽 위에서 오른쪽 아래까지를 줄 고르기로 잡았다. 스크롤백도, 앞뒤 공백도 보지 않았다
- 고침: `Screen.selectAll`(3364줄)처럼 스크롤백을 포함한 전체에서 글이 있는 첫 칸(널·빈칸·탭이 아닌 글자)과 마지막 칸을 찾아 그 둘을 고른다. 글이 하나도 없으면 아무것도 바꾸지 않는다(Ghostty `Surface`의 `select_all`도 null이면 앞서 고른 것을 둔다). 시험: `crates/bt-term/tests/select_ghostty803.rs`(Ghostty `selectAll` 시험 9509)
- 0.98.0에서 풀기: `write_scrollback_file` 계열 동작으로 스크롤백을 파일로 뽑는다

## 65. 화면을 지워도 자리 없는 그림 자료가 남고, 저장 상한에 닿으면 화면에 보이는 그림이 먼저 사라질 수 있다

- 언제: 2026-11-23, #799. Ghostty 1.4.0 kitty 그래픽 시험 `erase display preserves scrollback and reclaims unplaced images`(graphics_storage.zig 3243)·`evicts images in priority order`(3768)를 같은 입력으로 옮겨 견주다 찾았다. 0.98.0까지의 동작이다
- 증상: `printf '\033_Ga=t,f=24,s=1,v=1,i=3;AAAA\033\\\033[2J\033_Ga=p,i=3\033\\'`에서 Ghostty는 `ENOENT`, 우리는 `OK`였다(화면을 지워도 자리가 없던 그림이 저장소에 남아 쌓임). 저장 상한(`image-storage-limit`)에 닿으면 가장 오래된 그림부터 쫓아내서, 오래 쓰는 화면 속 그림이 방금 보내 둔 안 쓰는 그림보다 먼저 사라질 수 있었다. 전송 명령의 `N=1`(임시 표시)은 읽지 않았다
- 까닭: ED 2가 자리만 지우고 자료는 지우지 않았다(`d=A`처럼 "자리가 없어진 그림"만 지우는 길이 없었다). 쫓아내기는 저장한 차례만 보았다
- 고침: ED 2와 ED 22는 그 화면(주·대체)에서 자리가 없는 그림의 자료를 모두 거둔다(스크롤백에 다 들어간 자리는 남는다). 쫓아내는 차례를 Ghostty와 같게 했다: 임시이고 안 쓰는 그림 → 안 쓰는 그림 → 임시이고 쓰는 그림 → 쓰는 그림, 같은 급이면 오래된 것부터. `N`의 맨 아래 비트가 임시다. ED 3이 주 화면 자리를 모두 지우는 것(ADR-010 결정 4)은 그대로다(ADR-080)
- 0.98.0에서 풀기: 안 쓰는 그림은 `a=d,d=I,i=번호`로 직접 지운다. 상한 근처에서는 오래 쓸 그림을 마지막에 보낸다

## 66. 스크롤 영역(DECSTBM)이 있으면 kitty 그림이 글과 따로 움직인다

- 언제: 2026-11-23, #800. Ghostty 1.4.0 `graphics_storage.zig`의 `scroll margins ...` 시험 11개(4206~4628줄)를 같은 입력으로 옮겨 견주다 찾았다. 0.98.0까지의 동작이다
- 증상: 영역 안에서 IND·RI·SU·SD가 일어나도 영역 안 그림은 움직이지 않고(영역 안 IND로 줄 2의 그림이 Ghostty는 1, 우리는 2), 영역 아래(상태줄)의 그림은 영역 스크롤에 딸려 올라갔으며(영역 `1;4r` 아래 줄 5의 그림에 IND 두 번: Ghostty 5, 우리 3), 영역 위에 걸친 그림도 올라갔다(Ghostty 0, 우리 -1)
- 까닭: 자리를 스크롤백 맨 위를 0으로 센 줄에 붙이므로(ADR-010 결정 4) 영역이 맨 위에서 시작하면 스크롤백이 쌓이며 모든 줄이 한 칸씩 밀리는 것으로 보이고, 맨 위가 아니면 영역 스크롤이 줄 번호를 바꾸지 않아 자리가 움직이지 않았다
- 고침: 엔진이 영역 스크롤(IND·RI·SU·SD, IL·DL 제외)을 기록하고 세션이 Ghostty `scrollMarginsBegin`처럼 자리에 반영한다. 영역에 온전히 든 자리는 글과 함께 움직이고, 여백에서 잘리며(원본의 위·아래 줄만큼 잘라 냄), 다 나가면 자리가 지워진다(그림 자료는 남음). 영역에 걸치거나 밖이거나 스크롤백에 있는 자리는 화면의 같은 줄에 머문다. 좌우 여백이 있으면 그 칸 안에 든 자리만 움직인다. `vendor/alacritty_terminal`에 스크롤 기록을 더했다(ADR-080)
- 0.98.0에서 풀기: 스크롤 영역을 쓰는 화면에서는 그림을 영역 밖에 놓거나, 영역을 바꾼 뒤 그림을 다시 놓는다

## 67. 그림을 놓은 뒤 커서 이동이 느슨해 스크롤백을 크게 밀어내고, 오른쪽 끝에서 감기지 않는다

- 언제: 2026-11-23, #801. Ghostty 1.4.0 `graphics_exec.zig`의 `placement bounds cursor movement for untrusted dimensions`(1938)·`placement moves cursor past a tall image`(1960)와 `display`의 `.after`(403)를 같은 입력으로 옮겨 견주다 찾았다. 0.98.0까지의 동작이다
- 증상: 5x5 화면에서 `printf '\033_Ga=T,t=d,f=24,i=1,s=1,v=1,c=4294967295,r=4294967295;AAAA\033\\'` 한 번에 스크롤백이 995줄 생겼다(Ghostty는 5줄). 그림이 오른쪽 끝에 닿으면(5열 화면의 `c=5,r=1`) Ghostty는 한 줄 더 내려 열 0(커서 (0,1))으로 감기는데, 우리는 마지막 줄 끝 칸(4,0)에 머물러 뒤따르는 글이 그림 위에 찍혔다
- 까닭: 내려가는 줄 수를 1000에서만 자르고 오른쪽 끝 감김을 보지 않았다. 줄 이동은 `\n`을 먹이고(줄바꿈 모드(LNM)면 열 0으로도 간다) 칸 이동은 `ESC[nC`(좌우 여백에 막힌다)였다
- 고침: Ghostty 규칙대로 상한을 "스크롤 영역 맨 아래까지 남은 줄 + 화면 높이"로 하고, 오른쪽 끝에 닿으면 한 줄 더 내려 열 0으로 감기며, 줄 이동은 스크롤 영역을 따르는 index(IND)로 한다. 그다음 줄바꿈 대기를 풀고 칸으로 간다(ADR-080)
- 0.98.0에서 풀기: 그림의 `c`·`r`을 화면 안으로 제한해 보내고, 오른쪽 끝에 닿는 그림 뒤에는 `C=1`을 쓴다

## 73. 엔진을 1열로 줄일 때 넓은 글자가 있으면 끝나지 않고, 줄 수를 줄일 때 커서가 맨 위면 아래 줄을 버린다

- 언제: 2026-11-23, #808. Ghostty 1.4.0 `Screen.zig` 시험 견주기(#760)에서 찾았다. 0.98.0까지의 동작이다
- 증상 1: 엔진(vendor alacritty `Term::resize`)을 1열로 줄이면서 넓은 글자(😀·한글)가 있으면 `resize`가 돌아오지 않는다(탐침에서 CPU 100%, 메모리 1GB 넘게 늘었다). 앱은 `GridSize::clamped`가 열을 2 이상으로 올려 닿지 않았고, 막는 곳이 그 한 곳뿐이었다
- 까닭 1: 1열에는 넓은 글자가 들어가지 않는데 다시 흘리기 반복(`shrink_columns`)이 넓은 글자를 다음 줄 맨 앞으로 밀고 또 밀었다
- 고침 1: 열이 1이 되면 다시 흘리기 전에 모든 줄(스크롤백 포함)의 넓은 글자 머리와 자리표시 칸을 빈 칸으로 바꾼다(Ghostty 시험 `resize less cols to eliminate wide char`·`... with row space`, 8974·9050줄과 같다). 줄 이음 표시는 남긴다. `vendor/README.md`에 적었다
- 증상 2: 5x3(스크롤백 0)의 `1ABCD\n2EFGH\n3IJKL`에서 커서가 (0,0)인 채 5x1로 줄이면 Ghostty(시험 `resize less rows no scrollback`, 8436줄)는 `3IJKL`이 보이고 우리는 `1ABCD`가 보인다. 스크롤백이 있어도 2·3행은 스크롤백에 남지 않는다
- 판단 2: **그대로 둔다.** xterm 소스(`screen.c`의 `ScreenResize`, 2217줄 이하. ThomasDickey/xterm-snapshots master를 받아 읽었고 실제 xterm을 돌려 보지는 않았다)가 우리와 같다. 기본 resizeGravity는 SouthWest(`charproc.c`의 resource 기본값)이고, 줄일 때 `amount = (MaxRows - move_up - 1) - cur_row`를 구해 0 이상(커서 아래에 버릴 줄이 충분함)이면 위 줄을 스크롤백으로 옮기지 않고(`move_down_by = 0`) 버퍼를 줄여 아래 줄을 버린다. 5x3에서 커서 (0,0)이면 amount = (3 - 2 - 1) - 0 = 0이라 `1ABCD`가 남는다. xterm과 같고 Ghostty만 다르므로 ADR-056 3항대로 xterm을 남긴다. 커서가 아래에 있으면 두 터미널 모두 위 줄이 스크롤백으로 가고 아래 줄이 남는다(시험으로 고정했다)
- 0.98.0에서 풀기: 증상 1은 앱에서 닿지 않았으므로 풀 것이 없다. 증상 2는 동작이다

## 71. 끌어 고를 때 포인터가 칸 위·아래 밖에 있어도 스크롤되지 않아 한 화면보다 긴 글을 고를 수 없다

- 언제: 2026-11-23, #806. Ghostty 1.4.0 `SelectionGesture.zig` 시험 견주기(#760)에서 찾았다. 0.98.0까지의 동작이다
- 증상: `seq 1 200` 뒤 마지막 줄에서 눌러 창 위 바깥까지 끌고 기다려도 스크롤되지 않는다. 고르기 끝이 첫 줄에 붙는다
- 까닭: `cell_in`이 칸 밖 좌표를 가장자리 칸으로 붙이기만 했고, 끄는 중 시간마다 스크롤하는 길이 없었다(`autoscroll`이 bt-app에 없었다)
- 고침: Ghostty와 같게 한다. 끌 때마다 포인터의 세로 자리가 칸(여백 포함) 위 끝에서 1픽셀 안이거나 그 위면 위, 아래 끝에서 1픽셀 안이거나 그 아래면 아래로 정한다(`SelectionGesture.drag`의 `autoscroll_buffer` 1). 방향이 있으면 15ms마다(Ghostty `selection_scroll_ms`) 정확히 한 줄 스크롤하고 마지막 포인터 자리의 칸까지 고르기를 늘린다(`autoscrollTick`). 포인터가 멈춰 있어도 계속되고, 칸 안으로 돌아오거나 단추를 놓으면 멈춘다. 끌지 않을 때는 이벤트 루프가 이것 때문에 깨지 않는다(쉬는 CPU를 지킨다). 마우스 보고 중인 프로그램은 누름이 고르기가 아니라 프로그램에 가므로 해당이 없고, 대체 화면은 스크롤백이 없어 스크롤은 하지 않고 끝 칸만 가장자리로 늘어난다. 누른 뒤 화면이 바뀌면(대체 화면 진입·퇴장) 틱이 끌기를 끝낸다(Ghostty의 `validatedLeftClickPin`). 스크립트(AppleScript) 마우스의 끌기도 같은 틱을 탄다
- 0.98.0에서 풀기: 휠이나 스크롤바로 스크롤해 가며 나누어 고른다. 한 번에 다 고르려면 `select_all` 같은 전체 고르기를 쓴다

## 72. 같은 자리를 네 번 누르면 줄 고르기가 풀리고, 한 칸 안에서 끌어 고른 뒤 링크가 열리고, 트랙패드 강하게 누르기가 낱말을 고르지 않는다

- 언제: 2026-11-23, #807. Ghostty 1.4.0 `SelectionGesture.zig`·`Surface.zig` 시험 견주기(#760)에서 찾았다. 0.98.0까지의 동작이다
- 증상 1: 낱말 위를 빠르게 네 번 누르면 누름 횟수가 `[1,2,3,1]`이라 네 번째에 줄 고르기가 풀리고 한 번 누르기로 돌아간다(Ghostty는 `[1,2,3,3]`이라 줄 그대로). 이어지는 누름인지도 우리는 같은 칸인지로 봤고 Ghostty는 처음 누른 자리에서 픽셀 거리가 칸 너비 이하인지로 본다(`Surface.zig` 4129줄 `max_distance`, 시험 2004줄). 그래서 칸 경계 양쪽 1~2픽셀 떨어진 두 누름이 끊기고, 같은 칸 안이라도 세로로 칸 너비보다 멀면 이어졌다
- 증상 2: ⌘(macOS가 아니면 Ctrl)를 누르고 링크 위 한 칸 안에서 칸 너비의 60%를 넘겨 끌어 그 칸이 골라진 뒤 같은 칸에서 떼면 링크가 열렸다. Ghostty는 끌었다고 보고(`left_click_dragged`) 열지 않는다(시험 1646줄)
- 증상 3: macOS 트랙패드로 강하게 누르면(force click) Ghostty는 처음 누른 낱말을 고르는데(`mousePressureCallback`, 시험 1780줄) 우리는 `TouchpadPressure`를 받지 않았다
- 고침: 누름 횟수는 `min(앞 횟수 + 1, 3)`이고, 이어지는지는 간격 `<=` 설정과 처음 누른 자리에서의 픽셀 거리 `<=` 칸 너비로 가른다(이어지는 누름은 처음 누른 자리를 쥔다). 링크는 떼는 칸에 고른 것이 있으면 열지 않는다. 트랙패드 단계가 강함(2)으로 바뀔 때 고르는 중이면 처음 누른 칸의 낱말을 고르고(두 번 누르기와 같은 낱말 고르기를 그대로 부름) 끌기를 끝낸다(누름 기록을 지우고 자동 스크롤을 멈추며 이어 움직여도 늘리지 않는다). vendor winit(macOS)은 `pressureChangeWithEvent:`를 `WindowEvent::TouchpadPressure { pressure, stage }`로 이미 내므로 vendor는 고치지 않았다
- 한계: 트랙패드 강하게 누르기는 실제 트랙패드로 확인하지 못했다(합성 이벤트로는 만들 수 없다). 누름 사이에 화면이 바뀌면 횟수를 끊는 Ghostty의 규칙(시험 2054줄)은 넣지 않았다
- 0.98.0에서 풀기: 네 번째 이후를 누르지 않는다. 링크는 끌지 않고 누른다

## 74. vim처럼 스크롤 영역과 DL로 화면을 올리면 접힌 긴 줄(URL)이 두 줄로 갈린다

- 언제: 2026-11-25, #815. Ghostty 새 병합분(11-23 10:30~11-25 UTC)을 엔진 탐침으로 견줘 찾았다. 0.99.0까지의 동작이다
- 증상: 5x6에서 `ESC[3;1H` 뒤 `ABCDEFG`를 찍고 `ESC[1;1H` `ESC[M`(DL)을 하면 `ABCDE`/`FG`가 화면에는 그대로 있는데 `ABCDE` 줄의 접힘 표시가 없다. 접힌 URL이 두 링크로 갈려 ⌘-클릭이 반쪽만 열고, 고르기·복사·`write_screen_file`에서 접힌 줄이 두 줄로 나온다. SU(`ESC[S`)는 표시를 남겼다
- 까닭: #582에서 Ghostty 1.4.0 `deleteLines`의 `wrap = false`를 따라 DL이 커서 줄부터 영역 끝까지 모든 줄의 표시(`WRAPLINE`)를 풀었다. Ghostty main이 이것을 11-23에 고쳤다(ghostty-org/ghostty#15309): 함께 움직이는 줄은 표시를 지키고 이웃이 바뀐 줄만 푼다
- 고침: IL·DL(과 SU·SD·RI 가운데 Ghostty가 `deleteLines`·`insertLines`로 처리하는 것)이 전체 폭 영역을 옮긴 뒤 이웃이 바뀐 줄만 푼다. 커서 바로 윗줄, DL이면 옮겨진 마지막 줄, IL이면 새 맨 아래 줄이다. 풀 때 줄 끝이 앞 칸 자리표시(spacer head)였으면 그 칸도 빈 칸으로 만든다. 스크롤백을 만드는 SU와 LF·IND는 건드리지 않는다(Ghostty도 다른 길). 좌우 여백이 있는 길(`scroll_rect`)은 원래 표시를 지켰고 그대로다
- 0.99.0에서 풀기: 없다(화면에서 접힘이 풀린 줄은 이어 붙지 않는다)
- 한계: 우리는 "윗줄에서 이어받는 줄" 표시가 따로 없고 윗줄의 `WRAPLINE`이 그것이라 Ghostty의 `wrap_continuation`을 따로 지우지 않는다. 같은 입력의 줄 상태는 Ghostty 시험 표와 같다. Ghostty의 페이지 경계 시험은 우리에게 페이지가 없어 같은 입력의 화면과 표시만 견줬다

## 75. 좌우 여백 안 IL·DL에서 OSC 133 프롬프트 줄 표시가 칸과 함께 휩쓸린다

- 언제: 2026-11-25, #817. #815(IL·DL 줄 접힘 표시)를 고치다 찾았다. 0.99.0까지의 동작이다
- 증상: DECLRMM(`ESC[?69h`)에서 IL·DL을 하면 Ghostty main(#15327)과 줄 표시가 다르다. 5줄 10열, 커서 `ESC[3;3H`에서 줄 3(0부터)에 프롬프트(P)가 있을 때 IL 여백 3;10이면 우리는 `...P.`, Ghostty는 `...PP`이고, 줄 2에 P가 있을 때 DL 여백 1;8이면 우리는 `.....`, Ghostty는 `..P..`이다. 0번 열이 여백 안이면 표시가 칸과 함께 옮겨지고 비워지는 줄은 표시를 잃으며, 여백 밖이면 표시가 제자리에 남고 옮겨 온 칸의 표시는 합쳐지지 않았다. 프롬프트 건너뛰기(`jump_to_prompt`)가 한 프롬프트를 놓치거나 표시 없는 줄에 멈춘다
- 까닭: 줄 표시를 그 줄 0번 칸의 `ROW_PROMPT`·`ROW_PROMPT_CONTINUATION` 칸 표시로 두는데 `scroll_rect`가 칸을 `copy_row`로 덮고 `clear_row`로 지웠다
- 고침: `scroll_rect`가 옮기기 전에 영역 안 줄들의 표시를 읽어 두고, 옮긴 뒤 줄 단위로 다시 쓴다. 줄 일부만 옮기면 받는 줄 표시는 받는 줄과 보낸 줄의 합(`combine`: 프롬프트 > 이어짐 > 없음), 줄 전체를 옮기면 보낸 줄 표시, 칸만 지운 줄은 그대로다. 0번 칸의 다른 표시(넓은 글자 등)는 건드리지 않는다. 시험: `crates/bt-term/tests/margin_prompt817.rs`(Ghostty `insertLines and deleteLines in margins keep semantic prompt` 네 경우, `Page cloneRowFrom partial combines semantic prompt`의 아홉 조합, 이슈 표 여섯 입력)
- 한계: kitty 유니코드 자리표시의 줄 단위 표시(`kitty_virtual_placeholder`)는 우리에게 없어 옮기지 않았다
- 0.99.0에서 풀기: 좌우 여백과 OSC 133을 함께 쓰지 않는다

## 76. ⌘K가 실행 중인 프로그램에 Ctrl-L을 보내고 스크롤백을 지우지 않는다

- 언제: 2026-11-25, #821. Ghostty 1.4.0 `Surface.zig`·`Termio.zig`와 같은 입력으로 견주다 찾았다. 0.100.0까지의 동작이다
- 증상: `sleep 30`을 실행하고 ⌘K를 누르면 `sleep`의 입력으로 FF(0x0C)가 들어가고, 스크롤백은 그대로 남는다. vim 같은 대체 화면에서는 키를 먹지 않아야 하는데 (이 부분은 `performable`로 이미 맞았다) 그 밖의 칸에서는 `clear_screen`이 늘 `write(FF)` 하나였다
- 까닭: `Action::ClearScreen`이 `write_active(b"\x0c")`뿐이었다. Ghostty는 엔진에서 스크롤백·고르기·화면을 지우고(`Termio.clearScreen(history = true)`), 커서가 프롬프트일 때만 FF를 보낸다
- 고침: `bt-term`에 `Session::clear_screen`(`ClearScreen::{Ignored, Cleared, Redraw}`)을 두고 앱은 `Redraw`일 때만 FF를 쓴다. 대체 화면은 `Ignored`. 아니면 고르기를 풀고 스크롤백을 지우며(ED 3), 프롬프트가 아니면 커서 윗줄을 떼어 내(남은 줄이 올라와 커서 줄이 맨 위) kitty 그림을 지우고(`d=A`) `Cleared`로 끝낸다. 프롬프트이면 ED 2와 같은 지우기를 하고 `Redraw`. 프롬프트 판단은 Ghostty `cursorIsAtPrompt`처럼 커서 줄 표시나 커서 칸 종류(입력·프롬프트)다
- 눈에 띄는 차이: 셸 통합이 없으면 커서 칸 종류의 기본값이 출력(Ghostty `Screen.Cursor.semantic_content = .output`)이라 늘 "프롬프트가 아님"이다. 그래서 셸 통합 없는 셸에서 ⌘K는 FF를 보내지 않고, 커서 윗줄을 지우고 프롬프트 줄을 맨 위로 올린다(Ghostty 1.4.0과 같다). 이전의 "화면을 지우고 셸이 다시 그림"과 다르다
- 한계: 프롬프트에서 ED 2는 Ghostty처럼 화면 내용을 스크롤백으로 올리는 규칙(마지막 글 줄이 프롬프트일 때)을 따르므로, 스크롤백을 지운 직후 지워지는 화면이 스크롤백에 남을 수 있다(Ghostty 소스에 TODO로 남은 같은 동작). 읽기 전용 칸에서는 지우되 FF는 쓰지 않는다
- 0.100.0에서 풀기: ⌘K 대신 `clear`나 Ctrl-L을 쓴다

## 77. OSC 133;D의 공백 낀 종료 코드, 긴 OSC 133, OSC 99 본문의 DEL·C1이 Ghostty와 다르다

- 언제: 2026-11-25, #822. Ghostty 1.4.0 `osc.zig`·`semantic_prompt.zig`·`kitty_desktop_notification.zig`와 같은 입력으로 견주다 찾았다. 0.100.0까지의 동작이다
- 증상: (1) `ESC]133;D; 5 BEL`이 종료 코드 5로 읽혔다(Ghostty는 `parseInt`라 없음). (2) OSC 133은 OSC 전체 상한 8192바이트까지 받았다(Ghostty는 `133;` 뒤 2048바이트를 넘으면 명령 전체를 버린다). (3) `ESC]99;;a DEL b ST`나 U+0085가 든 본문이 `a b` 알림이 됐다(Ghostty `isSafeUtf8`는 0x00~0x1F, 0x7F, U+0080~U+009F가 페이로드에 있으면 명령 전체를 버린다)
- 까닭: (1) `notify.rs`가 `osc133::parse`와 따로 `trim().parse()`로 종료 코드를 읽었다. (2)(3) 해당 검사가 없었고 OSC 99 본문은 제어 문자를 공백으로 바꿔 받았다
- 고침: (1) 종료 코드는 `osc133::parse` 결과 하나만 쓴다(옛 `133;D` 표지와 `633;D` 모두). (2) `133;` 뒤가 2048바이트 이하면 받고 2049부터 버린다(Ghostty 고정 버퍼 `MAX_BUF`가 2048바이트까지 담고 다음 바이트에서 실패한다). 바이트로 센다. (3) OSC 99 페이로드(`;` 뒤)에 위 범위가 있으면 조각 하나를 통째로 버려 쌓인 알림도 건드리지 않고 질의에도 답하지 않는다. 메타데이터에는 걸지 않는다(Ghostty도 페이로드에만 건다). `e=1`이면 검사는 base64 글자에 걸리고, 푼 글의 제어 문자는 전처럼 공백으로 바꾼다
- 한계: OSC 안의 C0(BEL·CAN·SUB·ESC 말고)는 VT 상태표가 먼저 버리므로(엔진·Ghostty 같음) 검사에 닿는 것은 DEL과 C1이다. 메타데이터에 깨진 UTF-8이 있으면 우리는 조각 전체를 버리지만 Ghostty는 페이로드만 본다(알림을 못 받는 쪽이라 그대로 뒀다)
- 0.100.0에서 풀기: 없다

## 80. 칸을 지우거나 밀면(EL·ED·ECH·DCH·ICH) OSC 133 줄 프롬프트 표시도 사라진다

- 언제: 2026-11-25, #825. Ghostty 1.4.0 단위 시험과 같은 입력으로 견준 조사(0.101)에서 찾았다. 0.100.0까지의 동작이다
- 증상: 줄 표시를 0번 칸의 `ROW_PROMPT`·`ROW_PROMPT_CONTINUATION`에 두어, 프롬프트 줄에서 0번 칸을 지우는 `\r ESC[K`, `ESC[2K`, `\r ESC[J`, `ESC[4G ESC[1K`, `\r ESC[2P`, `\r ESC[2X`, `\r ESC[2@`가 줄 표시를 지웠다(0번 칸을 건드리지 않으면 남았다). 셸이 줄을 지우고 다시 그리는 사이에 프롬프트 건너뛰기·출력 고르기가 그 프롬프트를 놓친다
- 까닭: Ghostty `Screen.clearCells`는 칸만 비우고 줄의 `semantic_prompt`를 건드리지 않는다. 줄 속성을 지우는 것은 줄째 지우는 `clearRows`(ED 0의 아랫줄, ED 1의 윗줄, ED 2, DECALN)뿐이다. 우리는 줄 표시가 칸에 있어 칸을 `bg.into()`로 덮는 곳마다 표시가 함께 사라졌다
- 고침: 칸을 비우는 곳이 0번 칸의 표시를 남기는 `blank_cell`을 쓰고(`erase_line`·`clear_unprotected`·`erase_chars`·ED 0·1의 커서 줄·`blank_cell_keep_wrap`·`repair_wide_cells`·`scroll_rect`의 경계 정리), 칸을 밀어 옮기는 ICH·DCH는 옮기기 전에 표시를 떼어 두었다가 0번 칸에 다시 붙인다(`take_marks`). 줄째 지우는 연산(`reset_region`)은 표시도 지운다. 글자 쓰기(print)는 이미 표시를 남겼다(`write_at_cursor`·`blank_wide_partner`). 마지막 보호 방식이 ISO이면 ED는 Ghostty처럼 줄 표시를 남기고, DEC(DECSCA)이면 지운다. 시험: `crates/bt-term/tests/erase_prompt825.rs`
- 0.100.0에서 풀기: 프롬프트 줄을 `ESC[2K` 대신 줄을 건드리지 않는 방법으로 다시 그린다(없음)

## 81. OSC 133 상태(커서 칸 종류·클릭 방식·표지를 받은 적)를 주 화면과 대체 화면이 함께 쓴다

- 언제: 2026-11-25, #826. Ghostty 1.4.0 단위 시험과 같은 입력으로 견준 조사(0.101)에서 찾았다. 0.100.0까지의 동작이다
- 증상: `Term`이 `semantic` 하나를 쥐어 두 화면이 나눠 썼다. 입력 중(`A`, `$ `, `I`)에 대체 화면에 갔다 오면 주 화면의 칸 종류가 대체 화면의 줄바꿈 때문에 출력이 되어 있고(Ghostty는 입력), 대체 화면에서 `A`를 받고 나오면 주 화면의 칸 종류가 프롬프트이고(Ghostty는 출력), 대체 화면에서 `A;cl=line`을 받고 나오면 주 화면의 클릭 방식이 `line`이다(Ghostty는 없음). 입력 중 대체 화면을 쓰는 프로그램(fzf 등)과 `I` 표지를 쓰는 셸에서만 드러났다
- 까닭: Ghostty는 `Screen.cursor.semantic_content`(180줄)와 `Screen.semantic_prompt.{click,seen}`(84줄)을 화면마다 둔다
- 고침: 쉬고 있는 화면의 상태를 `inactive_semantic`에 두고 화면을 바꿀 때 맞바꾼다(`swap_alt_grids`). `redraw=`는 Ghostty `Terminal.flags.shell_redraws_prompt`라 터미널 전체의 것이어서 맞바꿔도 함께 간다. 글의 종류와 줄 끝 지우기 표지는 Ghostty `cursorCopy`(`self.cursor = other`)가 복사하는 때에 맞춰 복사한다: 47·1047은 들어갈 때와 나올 때, 1049는 들어갈 때만(나올 때는 `restoreCursor`라 건드리지 않는다). 클릭 방식과 받은 적은 복사하지 않고, 대체 화면의 것은 RIS까지 남는다(Ghostty는 대체 화면 객체를 다음에 다시 쓴다). 크기를 바꿀 때 프롬프트를 비우는 길은 대체 화면을 보고 있어도 주 화면의 상태를 본다. 접근자(`semantic_content`·`semantic_seen`·`semantic_click`·`semantic_redraw`)의 이름과 시그니처는 그대로이고 보고 있는 화면의 값을 돌려준다. 시험: `crates/bt-term/tests/screen_semantic826.rs`
- 이슈 본문과 다른 점: 이슈는 `A`, `$ `, `B` 뒤 `ESC[?1049h`에서 `x\r\ny`를 쓰면 Ghostty의 대체 화면 줄 표시가 `nnnnn`이라 했으나, 1.4.0 원본 `cursorCopy`는 입력 상태를 대체 화면으로 복사하므로 Ghostty도 다음 줄을 이어진 프롬프트로 표시한다(`nCnn`). 우리 값도 그대로라 이 입력은 바뀌지 않았다
- 0.100.0에서 풀기: 입력 중 대체 화면 프로그램을 쓴 뒤 새 프롬프트(`A`)를 기다린다
