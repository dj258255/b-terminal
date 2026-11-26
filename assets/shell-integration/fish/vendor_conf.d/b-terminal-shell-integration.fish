# b-terminal 셸 통합(fish): 명령 경계(OSC 133 A·C·D), 명령 글(OSC 633;E), 폴더(OSC 7)를 알린다.
# b-terminal이 XDG_DATA_DIRS 맨 앞에 이 스크립트가 든 폴더를 더해 fish를 띄우면 fish가 vendor_conf.d의 이 파일을 읽는다(Ghostty와 같은 방식).
# 사용자의 config.fish는 건드리지 않는다. 기능별 켜고 끄기는 환경 변수 BT_SHELL_FEATURES(켜진 이름을 쉼표로 이은 글)로 온다.
# 그 값은 이름과 맞는지 비교만 한다(eval하지 않는다)

function __bt_restore_xdg_data_dirs -d "restore the original XDG_DATA_DIRS value"
    # 우리 폴더를 싣지 않았으면 할 일이 없다
    if not set -q BT_SHELL_INTEGRATION_XDG_DIR
        return
    end
    if not set -q XDG_DATA_DIRS
        return
    end

    # XDG_DATA_DIRS를 목록으로 만든다
    set --function --path xdg_data_dirs "$XDG_DATA_DIRS"

    # 우리 폴더가 있으면 뺀다
    if set --function index (contains --index "$BT_SHELL_INTEGRATION_XDG_DIR" $xdg_data_dirs)
        set --erase --function xdg_data_dirs[$index]
    end

    if set -q xdg_data_dirs[1]
        set --global --export --unpath XDG_DATA_DIRS "$xdg_data_dirs"
    else
        set --erase --global XDG_DATA_DIRS
    end

    set --erase BT_SHELL_INTEGRATION_XDG_DIR
end

function __bt_exit -d "exit the shell integration setup"
    functions -e __bt_restore_xdg_data_dirs
    functions -e __bt_exit
    exit 0
end

# 하위 프로세스에 우리 폴더가 새지 않게 언제나 되돌린다
__bt_restore_xdg_data_dirs

# 대화형이 아니면 하지 않는다
status --is-interactive || __bt_exit

# 설정은 첫 프롬프트에서 한다. 다른 셸 통합이 프롬프트를 만든 뒤에 _마지막으로_ 돌기 위해서다
function __bt_setup --on-event fish_prompt -d "Setup b-terminal integration"
    functions -e __bt_setup

    set --local features (string split , -- "$BT_SHELL_FEATURES")

    # fish 4.1부터 click_events를 받으므로 프롬프트 시작 표지에 밝힌다(Ghostty와 같음)
    set -l fish_major 0
    set -l fish_minor 0
    if set -q version[1]
        set -l fish_ver (string match -r '(\d+)\.(\d+)' -- $version[1])
        if set -q fish_ver[2]; and test -n "$fish_ver[2]"
            set fish_major "$fish_ver[2]"
        end
        if set -q fish_ver[3]; and test -n "$fish_ver[3]"
            set fish_minor "$fish_ver[3]"
        end
    end
    set -g __bt_prompt_start_mark "\e]133;A\a"
    if test "$fish_major" -gt 4; or test "$fish_major" -eq 4 -a "$fish_minor" -ge 1
        set -g __bt_prompt_start_mark "\e]133;A;click_events=1\a"
    end

    # cursor: 프롬프트에서는 깜박이는 막대, 명령이 도는 동안은 원래 모양. vi 모드의 커서 처리가 있으면 그쪽에 맡긴다
    if contains cursor $features
        function __bt_set_cursor_beam --on-event fish_prompt -d "Set cursor shape"
            if not functions -q fish_vi_cursor_handle
                echo -en "\e[5 q"
            end
        end
        function __bt_reset_cursor --on-event fish_preexec -d "Reset cursor shape"
            if not functions -q fish_vi_cursor_handle
                echo -en "\e[0 q"
            end
        end
    end

    # path: 실행 파일 폴더(BT_BIN_DIR)를 PATH 끝에. 절대 경로이고 `:`·제어 문자가 없을 때만, 이미 있으면 더하지 않는다
    if contains path $features; and string match -q -r '^/[^:[:cntrl:]]*$' -- "$BT_BIN_DIR"
        fish_add_path --global --path --append "$BT_BIN_DIR"
    end

    # 프롬프트 표지. 명령이 끝나는 이벤트를 못 받았으면(Ctrl-C 등) 여기서 끝을 보낸다
    function __bt_mark_prompt_start --on-event fish_prompt --on-event fish_posterror
        if test "$__bt_prompt_state" != prompt-start
            echo -en "\e]133;D\a"
        end

        set --global __bt_prompt_state prompt-start
        echo -en $__bt_prompt_start_mark
    end

    function __bt_mark_output_start --on-event fish_preexec
        set --global __bt_prompt_state pre-exec
        # 명령 글: 제어 문자는 공백으로(OSC를 끊고 다른 OSC를 끼우지 못하게)
        printf '\e]633;E;%s\a' (string replace -r -a '[[:cntrl:]]' ' ' -- "$argv[1]")
        echo -en "\e]133;C\a"
    end

    function __bt_mark_output_end --on-event fish_postexec
        set --global __bt_prompt_state post-exec
        echo -en "\e]133;D;$status\a"
    end

    # 폴더 알림. fish는 이것을 내장하지만 허용 목록의 터미널에만 보낸다. 경로는 퍼센트 인코딩해(제어 문자도 인코딩된다) 이름이 OSC를 끊지 못한다
    function __bt_update_cwd_osc --on-variable PWD -d 'Notify capable terminals when $PWD changes'
        if status --is-command-substitution || set -q INSIDE_EMACS
            return
        end
        printf '\e]7;file://%s%s\a' (string escape --style=url -- $hostname) (string escape --style=url -- $PWD)
    end

    # 창 크기를 바꾸면 터미널이 프롬프트를 지우므로 fish가 다시 그리게 한다
    set --global fish_handle_reflow 1

    # 첫 프롬프트
    if contains cursor $features
        __bt_set_cursor_beam
    end
    __bt_mark_prompt_start
    __bt_update_cwd_osc
end

__bt_exit
