# Current platform scope

The requested targets for this stage are:

| Platform | Architecture | Required scope |
| --- | --- | --- |
| Linux | x86-64 | Declared qualified core/optional-runtime profiles and their documented ABI requirements |
| Windows | x64 | Native implementation, packages and qualification tracked in issue #1 |
| macOS | Apple Silicon / arm64 | Native implementation, packages and qualification tracked in issue #2 |

**Intel macOS is not a supported target for this stage.** Intel builds, universal binaries and Intel runtime evidence must not block arm64 implementation or release approval. Existing generic tooling and historical evidence may mention Intel; that does not restore it as a current support requirement.

macOS arm64 still needs its own dependency/resource closure, approved artifact hashes, signatures where needed for execution, native software workflows and declared OS-version qualification. Hardware, GPU and human-operated permission gaps remain explicit. Removing Intel does not waive these checks or count as additional implementation/test completion.

The current release-workflow matrix excludes Intel. The macOS working branch and future release automation must carry the same scope through their evidence records, source/artifact contracts, approval gates and workflow dependencies. Preserve unrelated Windows/Linux work and historical measurements.

- [Windows issue #1](https://github.com/asad-albadi/TermCraft/issues/1)
- [Apple Silicon macOS issue #2](https://github.com/asad-albadi/TermCraft/issues/2)
- [Desktop release automation issue #3](https://github.com/asad-albadi/TermCraft/issues/3)

Both platform PRs remain unmerged while their remaining platform-specific software work is completed. The existing 46/60 implementation checkpoint count is unchanged by this scope decision.
