# b-terminal 셸 통합(bash): OSC 133 명령 경계, OSC 633;E 명령 글, OSC 7 폴더
[ -n "$_BT_INTEGRATED" ] && return
_BT_INTEGRATED=1
_bt_running=
_bt_at_prompt=

_bt_status() { _bt_ret=$?; }
_bt_precmd() {
  if [ -n "$_bt_running" ]; then
    printf '\e]133;D;%d\a' "$_bt_ret"
    _bt_running=
  fi
  printf '\e]133;A\a'
  printf '\e]7;file://%s%s\a' "$HOSTNAME" "${PWD// /%20}"
  _bt_at_prompt=1
}
_bt_preexec() {
  # 프롬프트에서 사람이 친 첫 명령에서만(PROMPT_COMMAND 안의 명령·자동 완성은 건너뛴다)
  [ -n "$COMP_LINE" ] && return
  [ -z "$_bt_at_prompt" ] && return
  _bt_at_prompt=
  _bt_running=1
  local cmd="${BASH_COMMAND//[[:cntrl:]]/ }"
  printf '\e]633;E;%s\a' "$cmd"
  printf '\e]133;C\a'
}
trap '_bt_preexec' DEBUG
# 종료 코드는 맨 앞에서 읽고, 표지는 맨 뒤에서 보낸다(DEBUG 덫이 PROMPT_COMMAND 안의 명령을 명령으로 세지 않게)
PROMPT_COMMAND="_bt_status${PROMPT_COMMAND:+;$PROMPT_COMMAND};_bt_precmd"
