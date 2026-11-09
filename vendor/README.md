# vendor

외부 크레이트를 고쳐 쓰는 곳입니다. 고친 것마다 이유와 되돌릴 조건을 적습니다.

## winit 0.30.13

- 원본: crates.io `winit 0.30.13`(Apache-2.0, `vendor/winit/LICENSE`). 예제·문서·테스트는 뺐습니다.
- 연결: 루트 `Cargo.toml`의 `[patch.crates-io]`.
- 고친 곳: `src/platform_impl/macos/view.rs`의 `insert_text`와 `do_command_by_selector` 두 군데. `b-terminal 패치`로 찾을 수 있습니다.
- 고친 곳(#110): `src/platform_impl/macos/window.rs`의 `WinitWindow`에 `constrainFrameRect:toScreen:`를 더했습니다. 테두리 없는 창(퀵 터미널)은 macOS가 메뉴 막대 아래로 밀어 넣지 않고 준 자리 그대로 둡니다. 제목 막대가 있는 창은 원래대로(부모 클래스의 것)입니다. 이것이 없으면 위쪽 퀵 터미널이 화면 위쪽 바깥에서 시작하지 못해 미끄러지는 움직임이 보이지 않습니다(실제로 `y=39`에 붙박였음).

### 왜

macOS 한글 IME는 "안" 조합 중에 스페이스나 Enter를 누르면 한 번의 keyDown 안에서 확정(`insertText:"안"`)을 먼저 부르고, 이어서 스페이스(`insertText:" "`)나 줄 넘김(`doCommandBySelector:insertNewline:`)을 부릅니다. 원본 winit은 확정 직후의 이 두 번째 호출을 버리고, 그 키의 `KeyboardInput`도 보내지 않습니다. 그래서 터미널에서

- "안녕 하세요"를 치면 스페이스가 사라지고,
- "ls 폴더" 뒤 Enter를 한 번 누르면 확정만 되고 명령이 실행되지 않습니다(Enter를 두 번 눌러야 함).

winit 저장소의 main(0.31 베타)에도 같은 코드가 남아 있습니다(2026-11-05 확인, `winit-appkit/src/view.rs`).

### 되돌릴 조건

winit이 같은 문제를 고친 판을 내면 패치를 지우고 그 판으로 올립니다. 그 전에 한글 2벌식으로 "안녕 하세요⏎"를 쳐서 스페이스와 Enter가 한 번에 들어가는지 확인합니다.

## softbuffer 0.4.8

- 원본: crates.io `softbuffer 0.4.8`(MIT 또는 Apache-2.0, `vendor/softbuffer/LICENSE-*`). 벤치마크는 뺐습니다.
- 연결: 루트 `Cargo.toml`의 `[patch.crates-io]`.
- 고친 곳: `src/lib.rs` 끝의 `set_native_color_space`·`set_transparent`(스위치 둘)와 `src/backends/cg.rs`의 색 공간 만들기·알파 형식 두 군데. `b-terminal(ADR-009)`·`b-terminal(#78)`로 찾을 수 있습니다. 투명 창이면 그림을 곱한 알파(ARGB)로 넘깁니다. `b-terminal(#79)`: Core Animation이 다 쓰고 돌려준 그림 버퍼를 버리지 않고 다음 프레임에 다시 씁니다(같은 크기, 최대 3개, 실험 E4). `b-terminal(#97)`: 색 공간을 주 화면이 아니라 창이 있는 화면(`NSScreenNumber`)에서 가져오고, 올릴 때 창의 NSScreen이 바뀌었으면 그 화면의 색 공간으로 바꿉니다. 기능 플래그 `CGDirectDisplay`·`CGDisplayConfiguration`을 더했습니다(`Cargo.toml`).

### 왜

macOS 백엔드는 그림을 기기 RGB 색 공간으로 Core Animation에 넘깁니다. 그러면 Core Animation이 프레임마다 화면 색 공간으로 색 맞춤 변환을 하고, 이것이 큰 창(2000×1360px)에서 present 한 번에 약 14ms였습니다(실험 E3). 화면 자신의 색 공간(`CGDisplayCopyColorSpace`)을 주면 변환이 없어 약 0.1ms입니다. 설정 `window-colorspace = srgb`면 원래대로 둡니다.

### 되돌릴 조건

softbuffer가 색 공간을 고를 수 있게 되거나, GPU 렌더러로 바꿔 softbuffer를 쓰지 않게 되면 지웁니다.

## alacritty_terminal 0.26.0

- 원본: crates.io `alacritty_terminal 0.26.0`(Apache-2.0, `vendor/alacritty_terminal/LICENSE-APACHE`). 테스트(`tests/`, 참조 그림 46MB)는 뺐습니다.
- 연결: 루트 `Cargo.toml`의 `[patch.crates-io]`. 새 크레이트는 없습니다. `Cargo.toml`에 `unicode-segmentation = "1"`을 직접 의존성으로 더했고(잠금 파일에 이미 1.13.3이 있었음) `[[test]]`를 지웠습니다.
- 고친 곳(#180, ADR-014): `src/term/mod.rs` 한 파일, 더한 줄 118·지운 줄 2. `b-terminal 패치(#180)`로 찾을 수 있습니다.
  - `TermMode::GRAPHEME_CLUSTER`(DECSET 2027)와 `Config::grapheme_cluster`(처음 값, 기본 켬). 모드 켜기·끄기·DECRQM은 vte가 이름 없는 모드로 넘기는 2027을 `set_private_mode`·`unset_private_mode`·`report_private_mode`가 받게 했습니다.
  - `Handler::input`이 글자를 찍기 전에 `join_grapheme`을 부릅니다. 방금 찍은 칸(`grapheme_anchor`, 커서가 그 자리 그대로일 때만)의 글과 새 글자를 합쳐 `unicode-segmentation`이 한 묶음으로 보면 새 칸 없이 그 칸의 결합 문자로 잇습니다. 좁은 글자에 U+FE0F가 붙어 두 칸(`unicode-width`의 문자열 폭)이 되면 뒤 칸을 채웁니다. 지역 표시 글자(국기)는 한 글자로도 두 칸으로 셉니다.
  - 칸 내용을 바꾸는 `scroll_*_relative`·`insert_blank`·`erase_chars`·`delete_chars`·`clear_line`·`clear_screen`·`swap_alt`·`reset_state`는 `grapheme_anchor`를 지웁니다.

### 왜

alacritty_terminal은 글자마다 `unicode-width`로 폭을 세고, 폭 0인 글자만 앞 칸에 붙입니다. 그래서 ZWJ 가족은 6칸, 피부색은 4칸을 차지해 Ghostty(`grapheme-width-method = unicode`)와 어긋나고 프로그램이 mode 2027로 묶음 폭을 요청해도 받지 못합니다. 엔진 바깥(바이트를 바꾸는 방식)에서는 줄바꿈·커서·지우기와 섞여 안전하게 할 수 없었습니다.

### 되돌릴 조건

alacritty_terminal이 글자 묶음 폭(mode 2027)을 지원하는 판을 내면 패치를 지우고 그 판으로 올립니다. 올릴 때는 `tests/grapheme.rs`(bt-term)가 그대로 통과해야 합니다. 새 판으로 올릴 때마다 이 패치를 새 `mod.rs`에 다시 얹어야 하므로, 판이 바뀌면 `diff`로 먼저 확인합니다.

