#!/usr/bin/env python3
"""Bind captured notices to an exact browser solely inside a private source copy.

This does not enroll a production runtime or produce a distributable package.
"""
import argparse
import hashlib
import importlib.util
import json
import os
from pathlib import Path
import shutil
import stat
import subprocess
import tempfile

ROOT = Path(__file__).resolve().parents[1]
MANIFEST = Path('packaging/browser/runtime-manifest-windows.json')
BUNDLE = Path('target/debug/deps/browser/windows-x86_64')
SPEC = importlib.util.spec_from_file_location('browser_intake', Path(__file__).with_name('browser-intake.py'))
INTAKE = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(INTAKE)
require = INTAKE.require


def ordinary(path):
    metadata = path.lstat()
    require(not stat.S_ISLNK(metadata.st_mode) and not getattr(metadata, 'st_file_attributes', 0)
            & getattr(stat, 'FILE_ATTRIBUTE_REPARSE_POINT', 0x400), 'Refusing symlink or reparse path')


def bounded(path, maximum):
    ordinary(path)
    require(path.is_file() and path.stat().st_size <= maximum, 'Evidence exceeds regular-file bound')
    with path.open('rb') as stream:
        value = stream.read(maximum + 1)
    require(len(value) <= maximum, 'Evidence exceeds read bound')
    return value


def git(source, *arguments):
    return subprocess.check_output(['git', '-C', str(source), *arguments])


def prepare(candidate_path, archive, evidence, source):
    source = Path(source).absolute()
    require(source.resolve() != ROOT.resolve(), 'Refusing to modify the original checkout')
    for directory in (*reversed(source.parents), source):
        ordinary(directory)
    source = source.resolve()
    require(Path(git(source, 'rev-parse', '--show-toplevel').decode().strip()).resolve() == source,
            'Expected the root of a private git source copy')
    require(not git(source, 'status', '--porcelain', '--untracked-files=no'),
            'Private source has tracked changes')
    pin = source / MANIFEST
    for directory in pin.parents:
        if directory == source:
            break
        ordinary(directory)
    original = bounded(pin, 1024 * 1024)
    production = bounded(ROOT / MANIFEST, 1024 * 1024)
    require(not os.path.samefile(pin, ROOT / MANIFEST), 'Private pin aliases the original checkout')
    require(original == production, 'Private source must start with the original unapproved manifest')
    require(json.loads(original)['approved'] is False, 'Original runtime must remain unapproved')
    candidate = json.loads(bounded(Path(candidate_path), 1024 * 1024))
    INTAKE.validate_candidate(candidate)
    evidence = Path(evidence)
    ordinary(evidence)
    record_bytes = bounded(evidence / 'intake.json', 2 * 1024 * 1024)
    record = json.loads(record_bytes)
    inventory_bytes = bounded(evidence / 'archive-inventory.json', 16 * 1024 * 1024)
    inventory = json.loads(inventory_bytes)
    require(record['schema_version'] == 1 and record['scope'] == 'qualification-only'
            and record['approved'] is False and record['native_capture'] is True
            and record['redistribution_approved'] is False and record['temporary_runtime_removed'] is True
            and record['candidate'] == candidate, 'Native candidate capture identity differs')
    process = record['process']
    require(type(process['exit_code']) is int and process['exit_code'] == 0
            and process['timed_out'] is False and process['assigned_before_resume'] is True
            and type(process['cleanup']['active_processes']) is int
            and process['cleanup']['active_processes'] == 0
            and process['cleanup']['job_terminated'] is True and process['cleanup']['kill_on_close'] is True,
            'Native candidate did not finish and drain its owned job')
    require(hashlib.sha256(inventory_bytes).hexdigest() == record['inventory_sha256'],
            'Captured inventory checksum mismatch')
    notices = bounded(evidence / 'THIRD-PARTY-NOTICES.html', 64 * 1024 * 1024)
    credits = INTAKE.credits_summary(notices)
    require(credits == record['credits'], 'Captured notices checksum/content mismatch')
    inspected = INTAKE.inspect_archive(Path(archive), candidate)
    require(inspected == inventory, 'Native evidence inventory differs from exact archive')
    require('THIRD-PARTY-NOTICES.html'.casefold() not in
            {entry['path'].casefold() for entry in inspected['files']}, 'Archive collides with captured notices')
    bundle = source / BUNDLE
    require(not os.path.lexists(bundle), 'Refusing an existing bundle')
    report_path = evidence / 'qualification-preparation.json'
    require(not os.path.lexists(report_path), 'Qualification preparation evidence already exists')
    for directory in reversed(bundle.parents):
        if directory.exists() or os.path.lexists(directory):
            ordinary(directory)
    source_sha = git(source, 'rev-parse', 'HEAD').decode().strip()
    original_sha = git(ROOT, 'rev-parse', 'HEAD').decode().strip()
    files = [{**entry, 'executable': entry.get('machine') == 'x86_64'} for entry in inspected['files']]
    files.append({'path': 'THIRD-PARTY-NOTICES.html', 'size': len(notices),
                  'sha256': credits['sha256'], 'executable': False})
    manifest = {'schema_version': 1, 'platform': 'windows', 'architecture': 'x86_64',
                'root': 'browser/windows-x86_64', 'executable': candidate['executable'],
                'version': candidate['version'], 'approved': True, 'scope': 'qualification-only',
                'production_enrollment': False, 'archive_sha256': candidate['archive_sha256'],
                'candidate_source_recipe_revision': candidate['source_revision'],
                'native_credits_sha256': credits['sha256'], 'files': files,
                'qualification_limit': 'Private native test source only; not a redistribution or production approval.'}
    declaration = (json.dumps(manifest, indent=2) + '\n').encode()
    bundle.parent.mkdir(parents=True, exist_ok=True)
    with tempfile.TemporaryDirectory(prefix='.browser-qualification-', dir=bundle.parent) as temporary:
        stage = Path(temporary) / 'payload'
        verified = INTAKE.inspect_archive(Path(archive), candidate, destination=stage)
        require(verified == inspected, 'Candidate archive changed during private staging')
        (stage / 'THIRD-PARTY-NOTICES.html').write_bytes(notices)
        (stage / 'BROWSER-MANIFEST.json').write_bytes(declaration)
        # Prepare the rollback bytes before modifying the private pin. Restoring
        # by rename also works when a later report write runs out of disk space.
        with tempfile.TemporaryDirectory(prefix='.qualification-pin-', dir=pin.parent) as pin_work:
            backup = Path(pin_work) / 'original.json'
            replacement = Path(pin_work) / 'qualification.json'
            backup.write_bytes(original)
            replacement.write_bytes(declaration)
            installed_bundle = False
            os.replace(replacement, pin)
            try:
                require(git(source, 'diff', '--name-only').decode().splitlines() == [MANIFEST.as_posix()],
                        'Private qualification changed unexpected tracked source')
                diff = git(source, 'diff', '--', MANIFEST.as_posix()).decode()
                require((ROOT / MANIFEST).read_bytes() == production, 'Original checkout manifest changed')
                report = {'schema_version': 1, 'scope': 'qualification-only', 'production_enrollment': False,
                          'source_sha': source_sha, 'original_checkout_sha': original_sha,
                          'original_manifest_sha256': hashlib.sha256(original).hexdigest(),
                          'qualification_manifest_sha256': hashlib.sha256(declaration).hexdigest(),
                          'archive_sha256': candidate['archive_sha256'], 'native_credits': credits,
                          'native_intake_sha256': hashlib.sha256(record_bytes).hexdigest(),
                          'native_inventory_sha256': record['inventory_sha256'],
                          'bundle': BUNDLE.as_posix(), 'source_diff': diff,
                          'limits': 'Browser protocol/source qualification only; no GUI lifecycle, signing or redistribution approval.'}
                # Build a complete report first, then publish it atomically with
                # an exclusive hard link. Another writer's evidence is never replaced.
                with tempfile.TemporaryDirectory(prefix='.qualification-report-', dir=evidence) as report_work:
                    temporary_report = Path(report_work) / 'report.json'
                    with temporary_report.open('x', encoding='utf-8', newline='\n') as output:
                        json.dump(report, output, indent=2)
                        output.write('\n')
                        output.flush()
                        os.fsync(output.fileno())
                    require(not os.path.lexists(bundle), 'Refusing a concurrently created existing bundle')
                    # Exclusive mkdir establishes ownership before copying into
                    # the destination; rollback removes only this newly owned tree.
                    bundle.mkdir()
                    installed_bundle = True
                    for entry in stage.iterdir():
                        os.rename(entry, bundle / entry.name)
                    os.link(temporary_report, report_path)
            except Exception:
                os.replace(backup, pin)
                if installed_bundle:
                    shutil.rmtree(bundle)
                raise
    return report


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    for name in ('candidate', 'archive', 'evidence', 'source'):
        parser.add_argument('--' + name, required=True, type=Path)
    args = parser.parse_args()
    prepare(args.candidate, args.archive, args.evidence, args.source)


if __name__ == '__main__':
    main()
