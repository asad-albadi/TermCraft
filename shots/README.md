# TermCraft screenshots

Seven unmodified **1920 × 1080** app screenshots were captured on 2026-11-27 using the verified Linux x86-64 package from [release validation45974136835](https://github.com/asad-albadi/TermCraft/actions/runs/45974136835), source `6571b95a0489f350680c969b75617a0377254cd4`. The subsequent merged code has the same application source. This is Linux screenshot evidence, not screenshots of Windows or macOS.

The app ran as an unprivileged user in a disposable Ubuntu container, with an isolated Xvfb display at 1920 × 1080 and software rendering. The actual application window was resized to fill that display; the captures were not enlarged afterward. A separate data directory supplied five example bookmarks under `.example.test` hosts and explicit Light/Dark preferences. These bookmarks were not connected. The local terminal is real; its demonstration output uses `printf` and `ls`. Personal sessions, credentials, services and desktop state were not used.

The gallery covers the light/dark home workspace, local terminal, local files, network tools/runtime providers, settings and stopped server controls. `full-hd/capture-manifest.json` records dimensions, package identity and screenshot hashes.

`termcraft-mockup.png` is a 1920 × 1080 presentation composition of the original dark terminal and light home captures, using the existing logo and emerald palette. It does not propose a new app UI. The editable, self-contained source is `termcraft-mockup.svg`.

To rebuild the mockup from the committed screenshots, install `rsvg-convert` (librsvg) and run from the repository root:

```sh
python3 scripts/render-readme-mockup.py
```

The renderer uses only Python's standard library plus librsvg. The original captures remain unchanged. Existing older images outside `full-hd` are retained for historical references.
