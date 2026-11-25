function __termcraft_osc7 --on-event fish_prompt
    set -l __tc_status $status
    set -l __tc_url (string escape --style=url -- "$PWD")
    printf '\033]7;file://%s%s\007' @TERMCRAFT_HOST@ "$__tc_url"
    return $__tc_status
end
function __termcraft_osc7_remove
    functions --erase __termcraft_osc7
    functions --erase __termcraft_osc7_remove
end
