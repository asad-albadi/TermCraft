from pathlib import Path
import json,hashlib
from PIL import Image,ImageChops
p=Path(__file__).resolve().parent;b=p.parents[3]
h=lambda q:hashlib.sha256(q.read_bytes()).hexdigest()
r=json.loads((p/'frozen-binary.json').read_text());checks={}
checks['frozen_executable_identity']=h(Path(r['frozen_path']))==r['binary_sha256']
checks['captured_source_manifest_identity']=h(p/'source-sha256.json')==r['source_capture_sha256']
s=json.loads((p/'source-sha256.json').read_text()); game_paths=[n for n in s if n.startswith(('src/games/','src/ui/games')) or n=='src/ui.rs']
checks['captured_games_and_ui_source_unchanged']=all(h(b/n)==s[n] for n in game_paths)
rows=json.loads((p/'matrix.json').read_text());checks['thirteen_distinct_catalog_boards']=len(rows)==len({x['game'] for x in rows})==13 and all((p/x['board']).exists() and (p/x['action']).exists() for x in rows)
def difference(a,c,rect):return ImageChops.difference(Image.open(p/a).convert('RGB').crop(rect),Image.open(p/c).convert('RGB').crop(rect)).getbbox() is not None
checks['native_reserve_transfer_pixels_changed']=difference('002-soliterm.png','003-soliterm-move.png',(16,215,124,245))
checks['native_greed_board_changed']=difference('018-greed.png','019-greed-move.png',(16,178,776,556))
checks['jigsaw_solve_undo_redo_observed_pixel_transitions']=difference('033-jigsaw-solved.png','034-jigsaw-undo.png',(16,400,358,780)) and difference('034-jigsaw-undo.png','035-jigsaw-redo.png',(16,400,358,780))
checks['idle_overlay_replaced_terminal']=difference('083-fresh-idle30-config.png','084-fresh-idle-after34.png',(320,180,1350,820))
checks['wake_key_absent_and_focused_followup_exact']=(p/'pty-input.bin').read_bytes()==b'y!'
checks['native_close_guard_cancel_transition']=difference('088-native-close-unsaved-guard.png','089-native-close-keep-playing.png',(466,388,895,490))
checks['owned_app_exit_assertion']='owned-app-exited-after-consent' in (p/'action-043.log').read_text()
checks['namespace_cleanup_ran']=(p/'cleanup-native2.txt').read_text().strip()=='cleaned'
checks['final_xvfb_root_cleared']=Image.open(p/'091-native-final-close.png').convert('RGB').getbbox() is None
out={'checks':checks,'passed':sum(checks.values()),'total':len(checks),'verified_game_ui_source_paths':len(game_paths),'limits':['Visual observations are in REPORT.md; pixel checks corroborate transitions, not semantic OCR.','Repeated failed-seed New is a confirmed finding, so this is not an all-green product verdict.','Earlier post-game idle nonactivation remains unisolated; fresh idle/wake proof passed.','Native Save/Load chooser/roundtrip not available in portal-free namespace.']}
(p/'verification.json').write_text(json.dumps(out,indent=2)+'\n');print(json.dumps(out,indent=2));assert all(checks.values())
