# vendor

외부 크레이트를 고쳐 쓰는 곳입니다. 고친 것마다 이유와 되돌릴 조건을 적습니다.

## winit 0.30.13

- 원본: crates.io `winit 0.30.13`(Apache-2.0, `vendor/winit/LICENSE`). 예제·문서·테스트는 뺐습니다.
- 연결: 루트 `Cargo.toml`의 `[patch.crates-io]`.
- 고친 곳: `src/platform_impl/macos/view.rs`의 `insert_text`와 `do_command_by_selector` 두 군데. `b-terminal 패치`로 찾을 수 있습니다.
- 고친 곳(#110): `src/platform_impl/macos/window.rs`의 `WinitWindow`에 `constrainFrameRect:toScreen:`를 더했습니다. 테두리 없는 창(퀵 터미널)은 macOS가 메뉴 막대 아래로 밀어 넣지 않고 준 자리 그대로 둡니다. 제목 막대가 있는 창은 원래대로(부모 클래스의 것)입니다. 이것이 없으면 위쪽 퀵 터미널이 화면 위쪽 바깥에서 시작하지 못해 미끄러지는 움직임이 보이지 않습니다(실제로 `y=39`에 붙박였음).
- 고친 곳(#238): `src/platform/macos.rs`의 `WindowExtMacOS::set_blur_radius(i64)`와 `src/platform_impl/macos/window_delegate.rs`의 `WindowExtMacOS` 구현 안의 같은 이름 함수. 기존 `Window::set_blur(bool)`은 반지름을 80으로 고정해 그대로 두고, 반지름을 받는 것을 더했습니다(`CGSSetWindowBackgroundBlurRadius`에 그대로 넘김). `b-terminal(#238)`로 찾을 수 있습니다. 이유: Ghostty `background-blur`는 0 이상 정수(세기)를 그대로 반지름으로 씁니다. 되돌릴 조건: winit이 반지름을 받는 공개 함수를 내면 지우고 그것을 씁니다.

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
- 고친 곳(#180, ADR-014): `src/term/mod.rs` 한 파일, 더한 줄 125·지운 줄 2. `b-terminal 패치(#180)`로 찾을 수 있습니다.
  - `TermMode::GRAPHEME_CLUSTER`(DECSET 2027)와 `Config::grapheme_cluster`(처음 값, 기본 켬). 모드 켜기·끄기·DECRQM은 vte가 이름 없는 모드로 넘기는 2027을 `set_private_mode`·`unset_private_mode`·`report_private_mode`가 받게 했습니다.
  - `Handler::input`이 글자를 찍기 전에 `join_grapheme`을 부릅니다. 방금 찍은 칸(`grapheme_anchor`, 커서가 그 자리 그대로일 때만)의 글과 새 글자를 합쳐 `unicode-segmentation`이 한 묶음으로 보면 새 칸 없이 그 칸의 결합 문자로 잇습니다. 좁은 글자에 U+FE0F가 붙어 두 칸(`unicode-width`의 문자열 폭)이 되면 뒤 칸을 채웁니다. 지역 표시 글자(국기)는 한 글자로도 두 칸으로 셉니다.
  - 칸 내용을 바꾸는 `scroll_*_relative`·`insert_blank`·`erase_chars`·`delete_chars`·`clear_line`·`clear_screen`·`swap_alt`·`reset_state`는 `grapheme_anchor`를 지웁니다.

- 더한 곳(#259, ADR-021): 같은 파일에 `Term::scroll_region()`(스크롤 영역 줄 범위를 읽는 getter) 여섯 줄. `b-terminal 패치(#259)`로 찾을 수 있습니다. DECRQCRA(체크섬 보고)가 원점 모드의 위 여백을 읽으려고 더했고, 필드는 그대로 비공개입니다. 되돌릴 조건: alacritty_terminal이 같은 getter를 내놓으면 지웁니다.

- 고친 곳(#264, ADR-022): 같은 파일의 `identify_terminal`(DA1·DA2에 답하던 것)을 답하지 않게 했고, `report_private_mode`가 모드 2048에는 답하지 않게 한 줄을 더했습니다. `b-terminal(#264)`로 찾을 수 있습니다. 이유: DA1·DA2는 Ghostty 값(`?62;22;52c`·`>1;10;0c`)으로, 모드 2048은 켜짐·꺼짐(1·2)으로 bt-term 스캐너가 답합니다. 엔진이 옛 값(`?6c`, `>0;버전;1c`, 모르는 모드 0)으로 또 답하면 PTY에 답이 둘 나갑니다. 되돌릴 조건: alacritty_terminal이 장치 속성 답과 모르는 모드 보고를 이벤트로 바꿔 밖에서 정할 수 있게 하면 지웁니다.

- 더한 곳(#272, ADR-024): `src/event.rs`의 `Event::ColorRequest`에 두 번째 필드 `Option<Rgb>`(프로그램이 OSC 4·10·11·12·21로 바꾼 색, 없으면 `None`)를 더하고, `src/term/mod.rs`의 `dynamic_color_sequence`가 `self.colors[index]`를 실어 보냅니다. `b-terminal(#272)`로 찾을 수 있습니다. 이유: 이 사건은 읽기 스레드가 `Term`을 잠근 채 오므로 받는 쪽(`Listener`)이 `Term::colors()`를 읽을 수 없어 질의가 바꾼 색이 아니라 띄울 때의 테마 색으로 답했습니다. 되돌릴 조건: alacritty_terminal이 색 질의 사건에 지금 색을 실어 주거나 받는 쪽이 엔진을 읽을 수 있게 하면 지웁니다.

### 왜

alacritty_terminal은 글자마다 `unicode-width`로 폭을 세고, 폭 0인 글자만 앞 칸에 붙입니다. 그래서 ZWJ 가족은 6칸, 피부색은 4칸을 차지해 Ghostty(`grapheme-width-method = unicode`)와 어긋나고 프로그램이 mode 2027로 묶음 폭을 요청해도 받지 못합니다. 엔진 바깥(바이트를 바꾸는 방식)에서는 줄바꿈·커서·지우기와 섞여 안전하게 할 수 없었습니다.

### 되돌릴 조건

alacritty_terminal이 글자 묶음 폭(mode 2027)을 지원하는 판을 내면 패치를 지우고 그 판으로 올립니다. 올릴 때는 `tests/grapheme.rs`(bt-term)가 그대로 통과해야 합니다. 새 판으로 올릴 때마다 이 패치를 새 `mod.rs`에 다시 얹어야 하므로, 판이 바뀌면 `diff`로 먼저 확인합니다.


## winit: 보안 키보드 입력(#258)

- 고친 곳: `src/platform/macos.rs` 끝의 `pub fn set_secure_event_input(bool) -> bool`, `src/platform_impl/macos/mod.rs`의 같은 이름 함수(Carbon `EnableSecureEventInput`·`DisableSecureEventInput`을 부르고 `noErr`면 참), `src/platform_impl/macos/ffi.rs`의 Carbon 선언 두 줄. `b-terminal(#258)`로 찾을 수 있습니다. 창에 묶이지 않는 프로세스 전역 호출이라 창 메서드가 아니라 자유 함수입니다. 이유: Ghostty `macos-auto-secure-input`·메뉴 `Secure Keyboard Entry`·`toggle_secure_input`. 작업 공간이 `unsafe_code = "deny"`라 앱 코드는 FFI를 못 부릅니다. 되돌릴 조건: winit이 보안 이벤트 입력 API를 내면 지우고 그것을 씁니다.
