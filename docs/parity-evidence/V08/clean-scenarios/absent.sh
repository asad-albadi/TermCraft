launch absent
if command -v mosh; then echo 'Unexpected installed Mosh' >&2; exit 1; fi
/opt/termcraft/harbor-workspace --data-dir /evidence/absent --session 0123456789abcdef0123456789abcdef
sleep 3
shot absent-mosh-error
