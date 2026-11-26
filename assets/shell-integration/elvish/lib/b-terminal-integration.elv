# b-terminal 셸 통합(elvish). b-terminal이 XDG_DATA_DIRS 맨 앞에 이 스크립트가 든 폴더를 더해 elvish를 띄우면
# elvish가 elvish/lib/*.elv를 모듈로 쓸 수 있게 한다. 사용자의 rc.elv는 b-terminal이 고치지 않으므로(Ghostty와 같음)
# 켜려면 사용자가 rc.elv에 아래 줄을 적는다:
#
#   if (has-env BT_SHELL_INTEGRATION_DIR) { try { use b-terminal-integration } catch { } }
#
# 기능별 켜고 끄기는 환경 변수 BT_SHELL_FEATURES(켜진 이름을 쉼표로 이은 글)로 온다. 이름과 맞는지만 비교한다.
# 한계: OSC 133;B(입력 시작)는 elvish의 프롬프트 출력이 이스케이프되고 프롬프트를 다른 스레드가 그려 스크립트로 정확히 낼 수 없다(Ghostty도 같다)
{
  use platform
  use re
  use str

  # 하위 프로세스에 우리 폴더가 새지 않게 XDG_DATA_DIRS에서 뺀다
  if (and (has-env BT_SHELL_INTEGRATION_XDG_DIR) (has-env XDG_DATA_DIRS)) {
    var ours = $E:BT_SHELL_INTEGRATION_XDG_DIR
    var kept = [(str:split : $E:XDG_DATA_DIRS | each {|d| if (not-eq $d $ours) { put $d } })]
    set-env XDG_DATA_DIRS (str:join : $kept)
    unset-env BT_SHELL_INTEGRATION_XDG_DIR
  }

  var features = [(str:split , $E:BT_SHELL_FEATURES)]

  # 제어 문자는 공백으로(OSC를 끊고 다른 OSC를 끼우지 못하게)
  fn clean {|s| re:replace '[[:cntrl:]]' ' ' $s }

  fn set-prompt-state {|new| set-env __bt_prompt_state $new }

  fn mark-prompt-start {
    if (not-eq $E:__bt_prompt_state 'prompt-start') {
      printf "\e]133;D;aid="$pid"\a"
    }
    set-prompt-state 'prompt-start'
    printf "\e]133;A;aid="$pid"\a"
  }

  fn mark-output-start {|line|
    set-prompt-state 'pre-exec'
    printf "\e]633;E;%s\a" (clean $line)
    printf "\e]133;C\a"
  }

  fn mark-output-end {|cmd-info|
    set-prompt-state 'post-exec'

    var exit-status = 0

    # 오류면 종료 코드를 꺼내고, 없으면(내장 함수의 실패) 1
    if (not-eq $nil $cmd-info[error]) {
      set exit-status = 1

      if (has-key $cmd-info[error] reason) {
        if (has-key $cmd-info[error][reason] exit-status) {
          set exit-status = $cmd-info[error][reason][exit-status]
        }
      }
    }

    printf "\e]133;D;"$exit-status";aid="$pid"\a"
  }

  defer {
    mark-prompt-start
  }

  set edit:before-readline = (conj $edit:before-readline $mark-prompt-start~)
  set edit:after-readline  = (conj $edit:after-readline $mark-output-start~)
  set edit:after-command   = (conj $edit:after-command $mark-output-end~)

  # cursor: 프롬프트에서는 깜박이는 막대, 명령이 도는 동안은 원래 모양
  if (has-value $features cursor) {
    fn beam  { printf "\e[5 q" }
    fn reset { printf "\e[0 q" }
    set edit:before-readline = (conj $edit:before-readline $beam~)
    set edit:after-readline  = (conj $edit:after-readline {|_| reset })
  }

  # path: 실행 파일 폴더(BT_BIN_DIR)를 PATH 끝에. 절대 경로이고 `:`·제어 문자가 없을 때만, 이미 있으면 더하지 않는다
  if (and (has-value $features path) (has-env BT_BIN_DIR)) {
    if (and (re:match '^/[^:[:cntrl:]]*$' $E:BT_BIN_DIR) (not (has-value $paths $E:BT_BIN_DIR))) {
      set paths = [$@paths $E:BT_BIN_DIR]
    }
  }

  # 폴더 알림. 이름의 제어 문자는 지운다(이름이 OSC를 끊고 다른 OSC를 끼우지 못하게). 경로는 인코딩하지 않는 kitty-shell-cwd 모양
  fn report-pwd { printf "\e]7;kitty-shell-cwd://%s%s\a" (re:replace '[[:cntrl:]]' '' (platform:hostname)) (re:replace '[[:cntrl:]]' '' $pwd) }
  set after-chdir = (conj $after-chdir {|_| report-pwd })
  report-pwd
}
