"""Read-only release review with temporary strict extraction."""
import hashlib, json, runpy, subprocess, tempfile
from pathlib import Path
from collections import Counter

root = Path.cwd()
e = Path(__file__).resolve().parent
q = e/'package-qualification'
read = lambda p: json.loads(p.read_text())
digest = lambda p: hashlib.file_digest(p.open('rb'), 'sha256').hexdigest()
package = runpy.run_path(str(root/'scripts/package-release.py'))
startup = runpy.run_path(str(root/'scripts/package-check-startup.py'))
integrity = read(q/'integrity.json')
archive = Path(integrity['archive'])
expected_archive = 'eb8145e78a000c02daf6de8327dc9f96b19222a74b53dfd0e024b926f3992eed'
expected_source = 'b5893bf70362241fbff489979d315307a34549e107f24b6f0de99388fbde6f2e'
assert digest(archive) == integrity['archive_sha256'] == expected_archive
captured = read(e/'source-sha256.json')
snapshot = root/'target/package-sources'/expected_source
assert len(captured) == 739
released = package['source_inputs'](root)
assert released == package['source_inputs'](snapshot) and len(released) == 593
assert released.items() <= captured.items()
assert all(digest(root/name) == pin for name, pin in captured.items())
assert package['source_digest'](released) == expected_source
with tempfile.TemporaryDirectory(prefix='termcraft-review-', dir=root/'target') as temp:
    destination = Path(temp)/'extract'
    startup['extract'](archive, destination)
    exact, = destination.iterdir()
    paths = list(exact.rglob('PACKAGE-MANIFEST.json'))
    manifest_path, = paths
    _, manifest = startup['verify'](exact, manifest_path)
    assert manifest == integrity['manifest']
    assert len(manifest['files']) == integrity['files_verified'] == 3372
    assert manifest['build']['source_inputs'] == released
    assert manifest['build']['snapshot_sha256'] == expected_source
    assert manifest['build']['provenance'] == 'local locked release build'
    assert (manifest['platform'], manifest['architecture'], manifest['profile'], manifest['signing']) == ('linux', 'x86_64', 'release', 'unsigned')
    preserved = Path(integrity['package_root'])
    assert startup['verify'](preserved, preserved/manifest_path.relative_to(exact))[1] == manifest
    for folder, name in [('package-app', 'harbor-workspace'), ('package-helper', 'termcraft-rdp-engine')]:
        assert digest(root/'target'/folder/'release'/name) == manifest['files'][name]
    rt = exact/'runtime/linux-x86_64/mosh-deps'
    dependency_manifest = snapshot/'packaging/runtime/mosh-dependencies.json'
    assert digest(rt/'MOSH-DEPENDENCIES.json') == digest(dependency_manifest)
    deps = read(dependency_manifest)
    sidecar = read(rt/'MOSH-SOURCES.json')
    cache = root/'target/agent-work/mosh-input-recovery/newcache'
    seen = {}
    escaped = 0
    runtime_bytes = 0
    for item in deps['packages']:
        assert digest(cache/item['filename']) == item['archive_sha256']
        roles = sidecar['packages'][item['name']]
        for role, name, pin in [('source', item['source_filename'], item['source_sha256']), ('recipe', item['recipe_filename'], item['recipe_sha256']), ('signature', item['filename']+'.sig', item['signature_sha256'])]:
            entry = roles[role]
            filename = ''.join(chr(b) if chr(b) in 'ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789._+-' else '~%02X' % b for b in name.encode())
            assert entry == {'original_filename': name, 'path': 'sources/'+filename, 'sha256': pin}
            escaped += (filename != name)
            assert digest(rt/entry['path']) == digest(cache/name) == pin
            seen[filename] = pin
        path = [d for d in deps['files'] if d['package'] == item['name']]
        for dest in path:
            out = rt/dest['path']
            assert out.stat().st_size == dest['size'] and digest(out) == dest['sha256']
            runtime_bytes += dest['size']
    assert set(sidecar['packages']) == {e['name'] for e in deps['packages']}
    assert set(seen) == {p.name for p in (rt/'sources').iterdir()}
    assert len(deps['packages'])*3 == 60 and len(seen) == 56 and escaped > 0
    # Validate the pinned runtime declaration and the feature-engine claim.
    rtm, = exact.rglob('RUNTIME-MANIFEST.json')
    assert digest(rtm) == digest(snapshot/'packaging/runtime-manifest.json')
    xdm = root/'target/agent-work/packaged-xdmcp-validation'
    observations = (xdm/'evidence.Nr0NdjmM/observations.log').read_text()
    engine = manifest['files']['x11/linux-x86_64/Xephyr-session-ready']
    assert engine in observations and engine in (snapshot/'packaging/x11/xdmcp/manifest.json').read_text()
assert read(q/'command.json')['script_sha256'] == digest(q/'startup.py')
assert read(q/'result.json')['archive_sha256'] == expected_archive
assert read(q/'startup-result.json')['files_verified'] == len(manifest['files'])
print(json.dumps(dict(archive_sha256=expected_archive, source_sha256=expected_source, test_capture_files=len(captured), release_source_files=len(released), extracted_payload_hashes=len(manifest['files']), retained_payload_hashes=len(manifest['files']), mosh_refs=len(deps['packages'])*3, mosh_unique=len(seen), escaped_refs=escaped, mosh_pinned_runtime_files=len(deps['files']), mosh_runtime_bytes=runtime_bytes, cached_genuine_release_binaries_matched=2), indent=2))
