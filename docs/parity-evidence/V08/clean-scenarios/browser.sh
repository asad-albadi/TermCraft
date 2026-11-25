python -m http.server 25380 --bind 127.0.0.1 --directory /tmp > /evidence/clean-browser-http.log 2>&1 & hp=$!
trap 'kill "$hp" 2>/dev/null || true; wait "$hp" 2>/dev/null || true; cleanup' EXIT
launch browser
/opt/termcraft/harbor-workspace --data-dir /evidence/browser --session 0123456789abcdef0123456789abcdef
sleep 8
shot clean-bundled-browser
set +e
LD_LIBRARY_PATH=/opt/termcraft/browser/linux-x86_64/libraries /opt/termcraft/browser/linux-x86_64/chromium --version > /evidence/clean-browser-version.log 2>&1
version_rc=$?
printf 'browser --version exit=%s\n' "$version_rc"
