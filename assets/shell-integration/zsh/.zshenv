# b-terminal 셸 통합(zsh). b-terminal이 ZDOTDIR을 이 폴더로 바꿔 zsh를 띄운다.
# 여기서 ZDOTDIR을 원래대로 돌리고 사용자의 .zshenv를 읽은 뒤 통합을 싣는다.
# 그 뒤 zsh는 원래 ZDOTDIR에서 .zprofile·.zshrc·.zlogin을 그대로 읽는다(Ghostty와 같은 방식)
if [[ -n "${BT_ORIG_ZDOTDIR+x}" ]]; then
  ZDOTDIR="$BT_ORIG_ZDOTDIR"
  unset BT_ORIG_ZDOTDIR
else
  unset ZDOTDIR
fi
{
  builtin typeset _bt_f="${ZDOTDIR-$HOME}/.zshenv"
  [[ -r "$_bt_f" ]] && builtin source "$_bt_f"
  builtin unset _bt_f
}
if [[ -o interactive && -n "$BT_SHELL_INTEGRATION_DIR" ]]; then
  builtin source "$BT_SHELL_INTEGRATION_DIR/zsh/b-terminal.zsh"
fi
