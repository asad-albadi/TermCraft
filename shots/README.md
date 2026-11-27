# TermCraft screenshots

Seven original **1920 × 1080** screenshots show the unsigned Linux x86-64 package from source `9c6f10c23df3875a95b88d55748f470540080f58`. The workflow retained the core archive in its Linux diagnostics artifact after the AppImage build hit an Alpine dependency pin. The core passed hosted X11 and Wayland startup, Debian and desktop lifecycle checks. The capture manifest records the archive and executable SHA-256 hashes. [Linux package job](https://github.com/asad-albadi/TermCraft/actions/runs/46037795885/job/140637606384)

An unprivileged user ran the packaged app on Ubuntu 24.04.5 with an isolated 1920 × 1080 Xvfb display, software rendering and 100% UI scale. Each capture preserves the original app pixels. Five synthetic bookmarks use reserved `.example.test` hosts and remained disconnected. The local terminal ran `printf` and `ls` in a disposable workspace. The file-list example contains synthetic filenames and uses a locally installed Noto Sans CJK font. [Packaged SSH, RDP and games captures](../docs/uiux/issue-4/captures/human-correction-package/manifest.json)

The gallery covers the light/dark home workspace, local terminal, local files, network tools/runtime providers, settings and stopped server controls. `full-hd/capture-manifest.json` records dimensions, package identity and screenshot hashes.

`termcraft-mockup.png` is a 1920 × 1080 presentation composition of the original dark terminal and light home captures, using the existing logo and emerald palette. It does not propose a new app UI. The editable, self-contained source is `termcraft-mockup.svg`.

To rebuild the mockup from the committed screenshots, install `rsvg-convert` (librsvg) and run from the repository root:

```sh
python3 scripts/render-readme-mockup.py
```

The renderer uses only Python's standard library plus librsvg. The original captures remain unchanged. Existing older images outside `full-hd` are retained for historical references.
