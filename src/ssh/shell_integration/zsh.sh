__termcraft_osc7_install() {
    emulate -L zsh
    if [[ ${(t)precmd_functions} == *readonly* || ( -n ${(t)precmd_functions} && ${(t)precmd_functions} != array* ) ]]; then
        printf '%s\n' 'TermCraft: this prompt hook is not writable/supported; use manual navigation.' >&2
        return 1
    fi
    __termcraft_osc7() {
        local __tc_status=$? __tc_url='' __tc_c __tc_hex __tc_i
        emulate -L zsh
        setopt nomultibyte
        local LC_ALL=C
        for ((__tc_i=1; __tc_i<=${#PWD}; __tc_i++)); do
            __tc_c=${PWD[__tc_i]}
            case $__tc_c in
                [a-zA-Z0-9/._~-]) __tc_url+=$__tc_c ;;
                *) printf -v __tc_hex '%%%02X' "'$__tc_c"; __tc_url+=$__tc_hex ;;
            esac
        done
        printf '\033]7;file://%s%s\007' @TERMCRAFT_HOST@ "$__tc_url"
        return "$__tc_status"
    }
    __termcraft_osc7_remove() {
        emulate -L zsh
        if [[ ${(t)precmd_functions} == *readonly* ]]; then
            printf '%s\n' 'TermCraft: the prompt hook changed; close this shell to remove its integration.' >&2
            return 1
        fi
        precmd_functions=("${(@)precmd_functions:#__termcraft_osc7}")
        if [[ ${__termcraft_osc7_was_unset-} == 1 && ${#precmd_functions} == 0 ]]; then unset precmd_functions; fi
        unset __termcraft_osc7_installed __termcraft_osc7_was_unset
        unfunction __termcraft_osc7 __termcraft_osc7_remove
    }
    if [[ ${__termcraft_osc7_installed-} != 1 ]]; then
        if (( ${+precmd_functions} )); then __termcraft_osc7_was_unset=0; else __termcraft_osc7_was_unset=1; fi
        typeset -ga precmd_functions
        local __tc_hook __tc_found=0
        for __tc_hook in "${precmd_functions[@]}"; do [[ $__tc_hook == __termcraft_osc7 ]] && __tc_found=1; done
        if (( !__tc_found )); then precmd_functions+=(__termcraft_osc7); fi
        __termcraft_osc7_installed=1
    fi
}
__termcraft_osc7_install
unfunction __termcraft_osc7_install
