"""Extract unchanged production completion helpers for the pure component."""
from pathlib import Path
import hashlib, json
root = Path(__file__).resolve().parents[2]
source = root / 'engines/rdp/src/devices.rs'
text = source.read_text()
def function(name):
    start = text.index(f'fn {name}(')
    pos = text.index('{', start)
    depth = 1
    cursor = pos + 1
    while depth:
        if text[cursor] == '{': depth += 1
        if text[cursor] == '}': depth -= 1
        cursor += 1
    return text[start:cursor] + '\n'
out = Path(__file__).parent / 'src/completion_helpers.rs'
out.write_text('const INVALID: u32 = 0xc000000d;\n' + '\n'.join(function(name) for name in ['u32_at','word','header','length_data','complete','completion_header','failure']))
inputs = [source, root/'engines/rdp/src/serial_completion.rs',root/'engines/rdp/src/serial_stats.rs',root/'engines/rdp/src/serial_windows_control.rs', out]
(Path(__file__).parent/'production-input-sha256.json').write_text(json.dumps({str(p.relative_to(root)):hashlib.sha256(p.read_bytes()).hexdigest() for p in inputs},indent=2)+'\n')
