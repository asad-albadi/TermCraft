#!/usr/bin/env python3
"""Run named endpoint cells; retain unavailable results and assertion logs, never skip as pass."""
import argparse
import datetime
import hashlib
import json
import os
from pathlib import Path
import platform
import re
import shlex
import shutil
import subprocess


def file_sha256(path):
    digest = hashlib.sha256()
    with Path(path).open('rb') as stream:
        for block in iter(lambda: stream.read(1024 * 1024), b''):
            digest.update(block)
    return digest.hexdigest()


def fixture_unavailability(missing):
    return 'Missing isolated fixture prerequisites: ' + ', '.join(missing) if missing else None


def test_outcome(code, output):
    return 'passed' if code == 0 and re.search(r'test result: ok\. 1 passed; 0 failed; 0 ignored;', output) else 'failed'


def evidence_record(point, case, outcome, command, log, digest, date):
    dimensions = '; '.join(f'{key}={case[key]}' for key in ('id', 'endpoint', 'version', 'auth', 'trust', 'jump', 'operation', 'expected'))
    return dict(point_id=point, target=dict(product='TermCraft', version='0.1.0', edition='Personal 26.6 parity validation'), sources=[str(log)], capture_date=date,
                observation_kind='implementation-test', observed_behavior=dimensions,
                termcraft_behavior=f'{outcome}: named test asserts the declared observation; see complete log', test_command=command,
                platform=platform.platform(), result=outcome, remaining_limitations=[case['unavailable']] if outcome == 'unavailable' else ['Fixture evidence does not establish native Windows/macOS or original Personal runtime parity.'], artifact_sha256=digest)


def main():
    p = argparse.ArgumentParser(__doc__)
    p.add_argument('point', choices=['V01', 'V02'])
    p.add_argument('--output', type=Path)
    p.add_argument('--case', action='append', default=[])
    args = p.parse_args()
    root = Path(__file__).resolve().parent.parent
    output = (args.output or root / 'docs/parity-evidence' / args.point / 'runs' / datetime.datetime.now(datetime.timezone.utc).strftime('%Y%m%dT%H%M%SZ')).resolve()
    output.mkdir(parents=True, exist_ok=True)
    cases = json.loads((root / 'docs/parity-evidence' / args.point / 'matrix.json').read_text())
    if args.case:
        unknown = set(args.case) - {c['id'] for c in cases}
        if unknown: p.error(f'Unknown cases: {sorted(unknown)}')
        cases = [c for c in cases if c['id'] in args.case]
    env = dict(os.environ, CARGO_BUILD_JOBS='2')
    # Resolve installed endpoint and compiler/library versions in the retained run manifest.
    versions = {'platform': platform.platform(), 'russh': '0.65.4', 'russh-sftp': '4.0.0'}
    for name, command in [('openssh', ['/usr/bin/sshd', '-V']), ('client', ['ssh', '-V']), ('rustc', ['rustc', '--version'])]:
        try:
            r = subprocess.run(command, capture_output=True, text=True, env=env, timeout=10)
            versions[name] = (r.stdout + r.stderr).strip()
        except (OSError, subprocess.TimeoutExpired) as e: versions[name] = str(e)
    (output / 'versions.json').write_text(json.dumps(versions, indent=2) + '\n')
    missing = [tool for tool in ['ssh', 'ssh-keygen', 'ssh-agent'] if shutil.which(tool) is None]
    if not os.access('/usr/bin/sshd', os.X_OK): missing.append('/usr/bin/sshd')
    reason = fixture_unavailability(missing)
    if os.name != 'posix': reason = 'These endpoint fixtures require a native Unix runner; native Windows outcomes remain open.'
    if reason:
        for case in cases:
            case.setdefault('unavailable', reason)
    dropbear_dir = Path(env.get('TERMCRAFT_DROPBEAR_DIR', str(root / 'target/interop-tools/dropbear/dropbear-2026.94'))).resolve()
    env['TERMCRAFT_DROPBEAR_DIR'] = str(dropbear_dir)
    for case in cases:
        if case.get('requires_dropbear') and not all(os.access(dropbear_dir / program, os.X_OK) for program in ['dropbear', 'dropbearkey']):
            case.setdefault('unavailable', 'Private independent Dropbear endpoint not staged. Run scripts/prepare-dropbear-interop.sh; no host installation required.')
    targets = sorted({c['target'] for c in cases if 'unavailable' not in c})
    executables = {}
    if targets:
        command = ['cargo', 'test', '--offline', '--no-run', '--message-format=json']
        for target in targets: command += ['--test', target]
        build = subprocess.run(command, cwd=root, env=env, capture_output=True, text=True)
        (output / 'build.log').write_text(shlex.join(command) + '\n' + build.stdout + build.stderr)
        for line in build.stdout.splitlines():
            try: message = json.loads(line)
            except ValueError: continue
            if message.get('reason') == 'compiler-artifact' and message.get('executable') and message.get('target', {}).get('kind') == ['test']:
                executables[message['target']['name']] = message['executable']
        if build.returncode: executables = {}
    records = []
    for original in cases:
        case = dict(original)
        if case['version'] == 'installed': case['version'] = versions['openssh']
        log = output / (case['id'] + '.log')
        if 'unavailable' in case:
            outcome, command, text = 'unavailable', 'not executed: endpoint unavailable', case['unavailable'] + '\n'
        elif case['target'] not in executables:
            outcome, command, text = 'failed', 'cargo test --offline --no-run', 'Test executable unavailable; inspect build.log\n'
        else:
            executable = executables[case['target']]
            test_args = [executable, '--exact', case['test'], '--nocapture', '--test-threads=1']
            if case.get('ignored_fixture'): test_args.append('--ignored')
            command = shlex.join(test_args)
            try:
                case_env = dict(env, TERMCRAFT_INTEROP_LOG_DIR=str(output / (case['id'] + '-servers')))
                result = subprocess.run(shlex.split(command), cwd=root, env=case_env, capture_output=True, text=True, timeout=180)
                text = result.stdout + result.stderr
                outcome = test_outcome(result.returncode, text)
            except subprocess.TimeoutExpired as e:
                text = 'Timed out after 180 seconds\n' + str(e.stdout or '') + str(e.stderr or '')
                outcome = 'failed'
            text = command + '\nExecutable SHA256: ' + file_sha256(executable) + '\n' + text
        log.write_text(text)
        records.append(evidence_record(args.point, case, outcome, command, log.relative_to(output), file_sha256(log), datetime.datetime.now(datetime.timezone.utc).date().isoformat()))
        print(f"{case['id']}: {outcome}", flush=True)
    (output / 'evidence.json').write_text(json.dumps(records, indent=2) + '\n')
    print(f'Evidence: {output}')
    # Unavailable cells remain open while fixture readiness can be independently green.
    return 1 if any(r['result'] == 'failed' for r in records) else 0

if __name__ == '__main__': raise SystemExit(main())
