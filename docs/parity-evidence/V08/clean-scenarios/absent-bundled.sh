launch absent-bundled
if command -v mosh; then echo 'Unexpected installed Mosh' >&2; exit 1; fi
/opt/termcraft/harbor-workspace --data-dir /evidence/absent-bundled --session 0123456789abcdef0123456789abcdef
sleep 3
shot absent-bundled-mosh-error
