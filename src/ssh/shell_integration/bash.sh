__termcraft_osc7_install() {
    local __tc_decl __tc_flags __tc_hook __tc_found=0
    __tc_decl=$(declare -p PROMPT_COMMAND 2>/dev/null) || __tc_decl=''
    __tc_flags=${__tc_decl#declare }; __tc_flags=${__tc_flags%% *}
    if [[ $__tc_flags == *r* || $__tc_flags == *A* || $__tc_flags == *x* ]]; then
        printf '%s\n' 'TermCraft: this prompt hook is not writable/supported; use manual navigation.' >&2
        return 1
    fi
    if [[ $__tc_flags == *a* ]] && (( BASH_VERSINFO[0] < 5 || (BASH_VERSINFO[0] == 5 && BASH_VERSINFO[1] < 1) )); then
        printf '%s\n' 'TermCraft: array prompt hooks need Bash 5.1 or newer; use manual navigation.' >&2
        return 1
    fi
    __termcraft_osc7() {
        local __tc_status=$? __tc_url='' __tc_c __tc_hex __tc_i
        local LC_ALL=C
        for ((__tc_i=0; __tc_i<${#PWD}; __tc_i++)); do
            __tc_c=${PWD:__tc_i:1}
            case $__tc_c in
                [a-zA-Z0-9/._~-]) __tc_url+=$__tc_c ;;
                *) printf -v __tc_hex '%%%02X' "'$__tc_c"; __tc_url+=$__tc_hex ;;
            esac
        done
        printf '\033]7;file://%s%s\007' @TERMCRAFT_HOST@ "$__tc_url"
        return "$__tc_status"
    }
    __termcraft_osc7_remove() {
        local __tc_decl __tc_flags __tc_i
        __tc_decl=$(declare -p PROMPT_COMMAND 2>/dev/null) || __tc_decl=''
        __tc_flags=${__tc_decl#declare }; __tc_flags=${__tc_flags%% *}
        if [[ $__tc_flags == *r* || $__tc_flags == *A* ]]; then
            printf '%s\n' 'TermCraft: the prompt hook changed; close this shell to remove its integration.' >&2
            return 1
        fi
        if [[ $__tc_flags == *a* ]]; then
            for __tc_i in "${!PROMPT_COMMAND[@]}"; do
                if [[ ${PROMPT_COMMAND[__tc_i]} == __termcraft_osc7 ]]; then unset 'PROMPT_COMMAND[__tc_i]'; fi
            done
            if [[ ${__termcraft_osc7_was_array-} == 0 ]]; then
                if [[ ${__termcraft_osc7_was_unset-} == 1 && ${#PROMPT_COMMAND[@]} == 0 ]]; then
                    unset PROMPT_COMMAND
                elif [[ ${#PROMPT_COMMAND[@]} == 1 && ${PROMPT_COMMAND[0]-} == "$__termcraft_osc7_original" ]]; then
                    unset PROMPT_COMMAND
                    PROMPT_COMMAND=$__termcraft_osc7_original
                fi
            fi
        else
            if [[ $PROMPT_COMMAND != *'; __termcraft_osc7' ]]; then
                printf '%s\n' 'TermCraft: the prompt hook changed; leaving its function available. Close this shell to remove the integration.' >&2
                return 1
            fi
            PROMPT_COMMAND=${PROMPT_COMMAND%'; __termcraft_osc7'}
            if [[ ${__termcraft_osc7_was_unset-} == 1 && -z $PROMPT_COMMAND ]]; then unset PROMPT_COMMAND; fi
        fi
        unset __termcraft_osc7_installed __termcraft_osc7_was_unset __termcraft_osc7_was_array __termcraft_osc7_original
        unset -f __termcraft_osc7 __termcraft_osc7_remove
    }
    if [[ ${__termcraft_osc7_installed-} != 1 ]]; then
        if [[ ${PROMPT_COMMAND+x} ]]; then __termcraft_osc7_was_unset=0; else __termcraft_osc7_was_unset=1; fi
        __termcraft_osc7_was_array=0
        __termcraft_osc7_original=${PROMPT_COMMAND-}
        if [[ $__tc_flags == *a* ]]; then
            __termcraft_osc7_was_array=1
            for __tc_hook in "${PROMPT_COMMAND[@]}"; do [[ $__tc_hook == __termcraft_osc7 ]] && __tc_found=1; done
            if (( !__tc_found )); then PROMPT_COMMAND+=(__termcraft_osc7); fi
        else
            if (( BASH_VERSINFO[0] > 5 || (BASH_VERSINFO[0] == 5 && BASH_VERSINFO[1] >= 1) )); then
                if [[ $__termcraft_osc7_was_unset == 1 ]]; then PROMPT_COMMAND=(__termcraft_osc7)
                else PROMPT_COMMAND=("$__termcraft_osc7_original" __termcraft_osc7); fi
            else
                PROMPT_COMMAND="${PROMPT_COMMAND-}; __termcraft_osc7"
            fi
        fi
        __termcraft_osc7_installed=1
    fi
}
__termcraft_osc7_install
unset -f __termcraft_osc7_install
