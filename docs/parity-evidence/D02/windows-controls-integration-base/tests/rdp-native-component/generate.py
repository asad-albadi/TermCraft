from pathlib import Path
import re, hashlib, json
root=Path(__file__).resolve().parents[2]
source=root/'engines/rdp/src/devices.rs'
text=source.read_text()
prefix=text[:text.index('type DeviceCompletion')]
helpers=text[text.index('fn access_categories'):text.index('#[cfg(test)]',text.index('fn access_categories'))]
# Extract unchanged production Drives and helpers, excluding only SVC imports.
for line in ['use crate::smartcard;\n','use ironrdp_core::AsAny;\n','use ironrdp_pdu::{PduResult, gcc::ChannelName};\n','use ironrdp_svc::{SvcClientProcessor, SvcMessage, SvcProcessor};\n']:
 prefix=prefix.replace(line,'')
prefix=re.sub(r'#\[path = "([^"]+)"\]',lambda m: '#[path = '+json.dumps(str(source.parent/m[1]))+']',prefix)
out=Path(__file__).parent/'src/drives.rs';out.write_text(prefix+helpers)
inputs = [source, root/'src/desktop/options.rs'] + [source.parent/name for name in ['serial.rs','serial_windows.rs','serial_completion.rs','native_file_windows.rs','native_file_macos.rs']]
proof = {str(path.relative_to(root)):hashlib.sha256(path.read_bytes()).hexdigest() for path in inputs}
proof['generated-drives.rs'] = hashlib.sha256(out.read_bytes()).hexdigest()
(Path(__file__).parent/'production-input-sha256.json').write_text(json.dumps(proof,indent=2)+'\n')
