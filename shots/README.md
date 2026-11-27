# TermCraft screenshots

Seven original **1920 × 1080** screenshots show the unsigned Linux x86-64 package from source `c2dded603fcc5710dfee7e19ac60c728d552c3cf`. The Linux package job passed native archive, Debian, AppImage, X11, Wayland and desktop lifecycle checks. The capture manifest records the archive and executable SHA-256 hashes. [Linux package job](https://github.com/asad-albadi/TermCraft/actions/runs/46023047742/job/140587387520)

An unprivileged user ran the packaged app on Ubuntu 24.04.5 with an isolated 1920 × 1080 Xvfb display, software rendering and 100% UI scale. Each capture preserves the original app pixels. Five synthetic bookmarks use reserved `.example.test` hosts and remained disconnected. The local terminal ran `printf` and `ls` in a disposable workspace. The file-list example contains synthetic filenames and uses a locally installed Noto Sans CJK font. [Packaged filename comparison and detached-window checks](../docs/uiux/issue-4/validation.md)

The gallery covers the light/dark home workspace, local terminal, local files, network tools/runtime providers, settings and stopped server controls. `full-hd/capture-manifest.json` records dimensions, package identity and screenshot hashes.

`termcraft-mockup.png` is a 1920 × 1080 presentation composition of the original dark terminal and light home captures, using the existing logo and emerald palette. It does not propose a new app UI. The editable, self-contained source is `termcraft-mockup.svg`.

To rebuild the mockup from the committed screenshots, install `rsvg-convert` (librsvg) and run from the repository root:

```sh
python3 scripts/render-readme-mockup.py
```

The renderer uses only Python's standard library plus librsvg. The original captures remain unchanged. Existing older images outside `full-hd` are retained for historical references.
