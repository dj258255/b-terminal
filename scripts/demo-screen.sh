#!/bin/sh
# README 그림(docs/images/*.png)에 찍는 화면을 그린다. /bin/sh와 printf만 쓴다.
# 찍는 법은 docs/images/README.md에 있다. 셸 프롬프트(컴퓨터 이름·사용자 이름)가 찍히지 않게
# 끝에 `$ `만 그리고 sleep으로 멈춘다.
#   scripts/demo-screen.sh main|test|build|ssh
e=$(printf '\033')
b=$(printf '\007')
r="$e[0m"
title() { printf '%s]2;%s%s' "$e" "$1" "$b"; }
hold() { printf '$ '; exec sleep 86400; }

main() {

  printf '%s[1mb-terminal%s 화면 확인   한글: 안녕하세요, 터미널!\n' "$e" "$r"

  # 16색
  i=0; while [ $i -le 7 ]; do printf '%s[4%dm   ' "$e" $i; i=$((i+1)); done; printf '%s ' "$r"
  i=0; while [ $i -le 7 ]; do printf '%s[10%dm   ' "$e" $i; i=$((i+1)); done; printf '%s\n' "$r"

  # 글자 모양
  printf '%s[31m빨강%s %s[32m초록%s %s[1m굵게%s %s[34m파랑%s %s[3m기울임%s %s[4m밑줄%s %s[4:3m%s[58:2::255:120:80m물결 밑줄%s %s[21m겹밑줄%s %s[7m 반전 %s %s[9m취소선%s\n' \
    "$e" "$r" "$e" "$r" "$e" "$r" "$e" "$r" "$e" "$r" "$e" "$r" "$e" "$e" "$r" "$e" "$r" "$e" "$r" "$e" "$r"

  # 256색과 24비트 색
  printf '256색 '
  i=16; while [ $i -le 231 ]; do printf '%s[48;5;%dm ' "$e" $i; i=$((i+6)); done; printf '%s\n' "$r"
  printf '24비트 '
  i=0; while [ $i -le 35 ]; do
    red=$((255 - i * 7)); green=$((i * 7)); blue=$((128 + i * 3))
    printf '%s[48;2;%d;%d;%dm ' "$e" $red $green $blue; i=$((i+1))
  done; printf '%s\n\n' "$r"

  # 상자 선과 막대
  printf ' ╭──────────────────────────╮\n'
  printf ' │ 폴더   ~/dev/b-terminal  │\n'
  printf ' │ 디스크 ▕██████▋    ▏ 45%% │\n'
  printf ' ╰──────────────────────────╯\n'
  printf ' ▁▂▃▄▅▆▇█  ░▒▓█  ┼┤├┬┴\n\n'

  # 이모지(두 칸), 링크(OSC 8)
  printf '이모지: 😀 🚀 ✅ 🇰🇷 👩‍💻   링크: %s]8;;https://github.com/dj258255/b-terminal%s저장소 열기%s]8;;%s\n' "$e" "$b" "$e" "$b"
}

test_scene() {
  title "cargo test"
  printf '$ cargo test --workspace\n'
  printf '%s[1;32m   Compiling%s bt-term v0.85.0\n' "$e" "$r"
  printf '%s[1;32m    Finished%s `test` profile in 41.2s\n' "$e" "$r"
  printf '     %s[1;32mRunning%s unittests (bt-app)\n\n' "$e" "$r"
  for t in script_config::env_needs_key_equals_value script_mouse::position_is_clamped \
           keymap::script_keys_keep_their_own_sequence notify::osc9_777_and_99 \
           session::hup_on_close mouse::sgr_1006_press_release; do
    printf 'test %s ... %s[32mok%s\n' "$t" "$e" "$r"
  done
  printf '\ntest result: %s[32mok%s. 2186 passed; 0 failed\n\n' "$e" "$r"
}

build_scene() {
  title "cargo build"
  # 프로그램 상태(OSC 7501): 탭에 "일하는 중" 막대가 뜬다
  printf '%s]7501;state=working:progress=60%s' "$e" "$b"
  printf '$ cargo build --release\n'
  for c in bt-core bt-term bt-render bt-gpu bt-ctl; do
    printf '%s[1;32m   Compiling%s %s v0.85.0\n' "$e" "$r" "$c"
  done
  printf '%s[1;36m    Building%s [=============>          ] 312/520: bt-app\n' "$e" "$r"
}

ssh_scene() {
  title "ssh 서버"
  printf '$ ssh 서버\n마지막 접속: 11월 19일 09:12\n\n'
  printf '%s[34m~/srv%s %s[33m(main)%s $ tail -f app.log\n' "$e" "$r" "$e" "$r"
  printf '%s[2m12:04:01%s %s[32mINFO%s  요청 처리 200 (14ms)\n' "$e" "$r" "$e" "$r"
  printf '%s[2m12:04:02%s %s[33mWARN%s  느린 질의 820ms\n' "$e" "$r" "$e" "$r"
}

case "${1:-main}" in
  main) title "b-terminal 화면 확인"; main ;;
  test) test_scene ;;
  build) build_scene ;;
  ssh) ssh_scene ;;
  *) echo "사용: $0 main|test|build|ssh" >&2; exit 2 ;;
esac
hold
