#!/usr/bin/env python3
"""Validate the machine-readable evidence record for a macOS release target."""

import argparse
import json
import re
import sys
from pathlib import PurePosixPath


SHA256 = re.compile(r"^[0-9a-f]{64}$")
REVISION = re.compile(r"^[0-9a-f]{40}$")
STATUSES = {"qualified", "partial", "unavailable"}


def validate(value):
    errors = []

    def required(mapping, key, path):
        if not isinstance(mapping, dict) or key not in mapping:
            errors.append(f"{path}.{key} is required")
            return None
        return mapping[key]

    if not isinstance(value, dict):
        return ["record must be a JSON object"]
    if value.get("schema_version") != 1:
        errors.append("schema_version must be 1")
    if value.get("status") not in STATUSES:
        errors.append("status must be qualified, partial, or unavailable")
    revision = required(value, "source_revision", "record")
    if revision is not None and (not isinstance(revision, str) or not REVISION.fullmatch(revision)):
        errors.append("source_revision must be a 40-character lowercase commit hash")

    host = required(value, "host", "record")
    if isinstance(host, dict):
        for key in ("os", "architecture"):
            if not isinstance(required(host, key, "host"), str):
                errors.append(f"host.{key} must be a string")
    target = required(value, "target", "record")
    if isinstance(target, dict):
        for key in ("architecture", "deployment_target", "sdk_version", "toolchain"):
            if not isinstance(required(target, key, "target"), str):
                errors.append(f"target.{key} must be a string")

    lockfiles = required(value, "lockfiles", "record")
    if not isinstance(lockfiles, list) or not lockfiles or not all(isinstance(item, str) for item in lockfiles):
        errors.append("lockfiles must be a non-empty list of strings")

    artifacts = required(value, "artifacts", "record")
    if not isinstance(artifacts, list):
        errors.append("artifacts must be a list")
    else:
        for index, artifact in enumerate(artifacts):
            path = f"artifacts[{index}]"
            if not isinstance(artifact, dict):
                errors.append(f"{path} must be an object")
                continue
            artifact_path = required(artifact, "path", path)
            if not isinstance(artifact_path, str) or not artifact_path:
                errors.append(f"{path}.path must be a non-empty relative path")
            else:
                normalized = PurePosixPath(artifact_path)
                if normalized.is_absolute() or ".." in normalized.parts:
                    errors.append(f"{path}.path must not escape the package")
            digest = required(artifact, "sha256", path)
            if not isinstance(digest, str) or not SHA256.fullmatch(digest):
                errors.append(f"{path}.sha256 must be a lowercase SHA-256 digest")

    workflows = required(value, "workflows", "record")
    if not isinstance(workflows, list) or not workflows:
        errors.append("workflows must be a non-empty list")
    elif value.get("status") == "qualified":
        if not artifacts:
            errors.append("qualified record requires artifacts")
        for index, workflow in enumerate(workflows):
            if not isinstance(workflow, dict) or workflow.get("result") in {"not-run", "blocked"}:
                errors.append(f"workflows[{index}] must have a completed result")
    unavailable = required(value, "unavailable", "record")
    if not isinstance(unavailable, list):
        errors.append("unavailable must be a list")
    elif value.get("status") != "qualified" and not unavailable:
        errors.append("unavailable must explain every open requirement")
    elif any(
        not isinstance(item, dict)
        or not isinstance(item.get("requirement"), str)
        or not isinstance(item.get("reason"), str)
        for item in unavailable
    ):
        errors.append("unavailable entries require string requirement and reason")
    return errors


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("record", type=str)
    args = parser.parse_args()
    try:
        with open(args.record, encoding="utf-8") as stream:
            value = json.load(stream)
    except (OSError, json.JSONDecodeError) as exc:
        print(f"cannot read evidence record: {exc}", file=sys.stderr)
        return 2
    errors = validate(value)
    if errors:
        for error in errors:
            print(error, file=sys.stderr)
        return 1
    print(f"valid macOS evidence record: {args.record}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
