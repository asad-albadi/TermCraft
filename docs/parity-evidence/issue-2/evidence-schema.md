# macOS issue #2 evidence records

`evidence-template.json` is the shape required for architecture-specific macOS
qualification records. Validate a record with:

```sh
python3 scripts/validate-macos-evidence.py path/to/record.json
```

Every record identifies the source revision, host and target architecture,
deployment target, SDK/toolchain, lockfiles, package hashes, workflow results,
and explicit reasons for any requirements that remain unavailable. A record is
not allowed to claim `qualified` while omitting the evidence needed to explain
the package and workflow results. The template is intentionally an
`unavailable` record until the relevant Apple Silicon evidence is recorded.
