# b-terminal 셸 통합(bash): OSC 133 명령 경계, OSC 633;E 명령 글, OSC 7 폴더.
# 기능별 켜고 끄기(설정 shell-integration-features)는 환경 변수 BT_SHELL_FEATURES(켜진 이름을 쉼표로 이은 글)로 온다.
# 그 값은 이름과 맞는지 비교만 한다(eval하지 않는다). 변수가 없으면 모두 꺼진 것으로 본다
[ -n "$_BT_INTEGRATED" ] && return
_BT_INTEGRATED=1
_bt_running=
_bt_at_prompt=
_bt_path_done=
_bt_f_cursor=
_bt_f_title=
_bt_f_path=
[[ ",${BT_SHELL_FEATURES}," == *",cursor,"* ]] && _bt_f_cursor=1
[[ ",${BT_SHELL_FEATURES}," == *",title,"* ]] && _bt_f_title=1
[[ ",${BT_SHELL_FEATURES}," == *",path,"* ]] && _bt_f_path=1

_bt_status() { _bt_ret=$?; }
_bt_precmd() {
  if [ -n "$_bt_running" ]; then
    printf '\e]133;D;%d\a' "$_bt_ret"
    _bt_running=
  fi
  printf '\e]133;A\a'
  printf '\e]7;file://%s%s\a' "$HOSTNAME" "${PWD// /%20}"
  # path: 사용자의 rc가 PATH를 다시 쓴 뒤인 첫 프롬프트에서 한 번, 없을 때만 끝에 더한다
  if [ -n "$_bt_f_path" ] && [ -z "$_bt_path_done" ]; then
    _bt_path_done=1
    if [[ "$BT_BIN_DIR" == /* && "$BT_BIN_DIR" != *[:[:cntrl:]]* && ":$PATH:" != *":$BT_BIN_DIR:"* ]]; then
      PATH="$PATH:$BT_BIN_DIR"
    fi
  fi
  # cursor: 프롬프트에서는 깜박이는 막대
  [ -n "$_bt_f_cursor" ] && printf '\e[5 q'
  # title: 프롬프트에서는 폴더(길면 뒤 세 마디). 제어 문자는 공백으로
  if [ -n "$_bt_f_title" ]; then
    local d="$PWD" p1 p2 p3
    case "$d" in "$HOME" | "$HOME"/*) d="~${d#"$HOME"}" ;; esac
    p1="${d%/*}"; p2="${p1%/*}"; p3="${p2%/*}"
    if [ -n "$p3" ] && [ "$p3" != "$p2" ]; then d="…/${d#"$p3"/}"; fi
    printf '\e]2;%s\a' "${d//[[:cntrl:]]/ }"
  fi
  _bt_at_prompt=1
}
_bt_preexec() {
  # 프롬프트에서 사람이 친 첫 명령에서만(PROMPT_COMMAND 안의 명령·자동 완성은 건너뛴다)
  [ -n "$COMP_LINE" ] && return
  [ -z "$_bt_at_prompt" ] && return
  _bt_at_prompt=
  _bt_running=1
  local cmd="${BASH_COMMAND//[[:cntrl:]]/ }"
  # cursor: 명령이 도는 동안은 원래 모양(DECSCUSR 0)
  [ -n "$_bt_f_cursor" ] && printf '\e[0 q'
  # title: 명령이 도는 동안은 명령 글(200자까지)
  [ -n "$_bt_f_title" ] && printf '\e]2;%s\a' "${cmd:0:200}"
  printf '\e]633;E;%s\a' "$cmd"
  printf '\e]133;C\a'
}
trap '_bt_preexec' DEBUG
# 종료 코드는 맨 앞에서 읽고, 표지는 맨 뒤에서 보낸다(DEBUG 덫이 PROMPT_COMMAND 안의 명령을 명령으로 세지 않게)
PROMPT_COMMAND="_bt_status${PROMPT_COMMAND:+;$PROMPT_COMMAND};_bt_precmd"
