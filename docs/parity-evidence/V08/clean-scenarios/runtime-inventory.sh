cat /opt/termcraft-validation-runtime-packages.txt > /evidence/clean-runtime-image-packages.txt
python - <<'PY'
from pathlib import Path
import json,ctypes,os
assert not Path('/usr/bin/chromium').exists()
assert not Path('/usr/bin/mosh').exists()
libc=ctypes.CDLL(None,use_errno=True); namespace_result=libc.unshare(0x10000000); namespace_errno=ctypes.get_errno()
Path('/evidence/clean-runtime-namespace.json').write_text(json.dumps({'unshare_CLONE_NEWUSER_return':namespace_result,'errno':namespace_errno,'error':os.strerror(namespace_errno)},indent=2)+'\n')
Path('/evidence/clean-runtime-system-clients-absent.json').write_text(json.dumps({'system_chromium_absent':True,'system_mosh_absent':True,'perl':Path('/usr/bin/perl').exists(),'openssh':Path('/usr/bin/ssh').exists()},indent=2)+'\n')
PY
