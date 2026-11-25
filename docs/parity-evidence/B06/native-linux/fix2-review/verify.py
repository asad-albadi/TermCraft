from pathlib import Path
import json,hashlib
from PIL import Image,ImageChops
p=Path(__file__).resolve().parent;old=p.parent
h=lambda q:hashlib.sha256(q.read_bytes()).hexdigest()
f=json.loads((p/'frozen-binary.json').read_text())
def crop(n,r): return Image.open(p/n).convert('RGB').crop(r)
def equal(a,b,r): return ImageChops.difference(crop(a,r),crop(b,r)).getbbox() is None
def changed(a,b,r): return not equal(a,b,r)
checks={
'old_254_sealed_artifacts_still_exact':all(h(old/k)==v for k,v in json.loads((old/'SHA256SUMS.json').read_text()).items()),
'new_frozen_binary_exact':h(Path(f['frozen_path']))==f['binary_sha256'],
'solo_failure_preserves_seed1_board':equal('001-solo-options.png','002-solo-seed2-failed.png',(16,417,358,758)),
'solo_next_new_changes_board':changed('002-solo-seed2-failed.png','003-solo-seed3-advanced.png',(16,417,358,758)),
'map_cancel_preserves_dirty_board':equal('006-map-dirty-seed1.png','008-map-cancel-preserved.png',(16,462,774,840)),
'map_failure_preserves_dirty_board':equal('008-map-cancel-preserved.png','009-map-seed2-failure-preserved.png',(16,462,775,840)),
'map_retry_cancel_preserves_dirty_board':equal('009-map-seed2-failure-preserved.png','011-map-retry-cancel.png',(16,462,775,840)),
'map_success_changes_board':changed('011-map-retry-cancel.png','012-map-seed3-unreasonable.png',(16,462,775,840)),
'map_undo_survives_failed_new':changed('016-map-second-consent-position.png','017-map-history-undo-after-failure.png',(54,340,72,373)),
'map_redo_restores_after_failed_new':equal('016-map-second-consent-position.png','018-map-history-redo-after-failure.png',(16,338,775,908)),
'no_action_nonzero_exit_artifacts':not list(p.glob('action-*.exit')),
'owned_app_native_exit_asserted':'owned-app-exited-after-native-close' in (p/'action-011.log').read_text(),
'final_display_empty':Image.open(p/'021-final-close.png').convert('RGB').getbbox() is None,
'cleanup_reaped_owned_processes':json.loads((p/'cleanup-host-check.json').read_text())['matching_processes']==[] and (p/'cleanup-native2.txt').read_text().strip()=='cleaned',
}
out={'checks':checks,'passed':sum(checks.values()),'total':len(checks),'visual_semantic_review':'REPORT.md records the titles, grades, errors and exact consent sequence inspected in native screenshots. Pixel checks corroborate transitions; they are not grade/seed OCR or independent solver proofs.'}
(p/'verification.json').write_text(json.dumps(out,indent=2)+'\n');print(json.dumps(out,indent=2));assert all(checks.values())
