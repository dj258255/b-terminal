# b-terminal 셸 통합(zsh): 명령 경계를 OSC 133으로, 명령 글을 OSC 633;E로, 폴더를 OSC 7로 알린다.
# 터미널은 이것으로 프롬프트 사이 이동, 오래 걸린 명령이 끝나면 알림, btctl run을 한다
[[ -n "$_BT_INTEGRATED" ]] && return
typeset -g _BT_INTEGRATED=1
typeset -g _bt_running=0
autoload -Uz add-zsh-hook

_bt_precmd() {
  local ret=$?
  if (( _bt_running )); then
    builtin printf '\e]133;D;%d\a' "$ret"
    _bt_running=0
  fi
  builtin printf '\e]133;A\a'
  builtin printf '\e]7;file://%s%s\a' "${HOST}" "${PWD// /%20}"
}

_bt_preexec() {
  _bt_running=1
  # 제어 문자는 공백으로(OSC를 끊지 않게)
  builtin printf '\e]633;E;%s\a' "${1//[[:cntrl:]]/ }"
  builtin printf '\e]133;C\a'
}

# 맨 앞에 둔다: 다른 precmd가 $?를 바꾸기 전에 종료 코드를 읽는다
precmd_functions=(_bt_precmd ${precmd_functions[@]})
add-zsh-hook preexec _bt_preexec
