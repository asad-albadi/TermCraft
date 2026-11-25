# Windows Native Implementation and Qualification Design

## Intent

Implement and qualify every locally actionable part of GitHub issue #1 on the
`issue-1-windows` branch. The work turns the existing reviewed Windows source
and packaging foundations into a native Windows build and delivery candidate,
while preserving the repository's central distinction between implemented,
qualified, unavailable, and unsupported behavior.

The target user is a Windows user who needs a coherent TermCraft desktop
package: the GUI and its independently locked RDP helper, adjacent runtime
assets, safe per-user installation, and honest platform documentation. The
work also gives maintainers reproducible evidence tied to exact source,
toolchain, lockfile, artifact, and runtime identities.

## Goals

- Establish a reproducible Windows x86-64 MSVC baseline using the repository's
  pinned Rust 1.96.1 toolchain and the installed Visual Studio 2022 Build Tools.
- Build and test the GUI and independently locked RDP helper natively.
- Fix locally reproducible Windows defects test-first, without weakening tests
  or converting missing fixtures into passes.
- Complete the portable archive and per-user installer behavior that can be
  safely exercised on this host, including install, upgrade, relocation,
  shortcut/protocol integration, uninstall, and user-data preservation.
- Build and qualify the private VcXsrv candidate when its exact pinned inputs
  and toolchain can be obtained and redistributed under the recorded contract.
- Exercise locally available Windows terminal, workspace, theme/scaling,
  browser, service, capture/control, SSH key/agent/Pageant, vault, import,
  gateway, proxy, reconnect, X11, and RDP workflows.
- Record exact native evidence under `docs/parity-evidence/` and keep
  `PARITY_EXECUTION_QUEUE.json`, `PARITY_CHECKLIST.md`, `FEATURE_PARITY.md`,
  platform documentation, and GitHub issue #1 synchronized with demonstrated
  facts.
- Leave unavailable physical-device, remote-endpoint, original-product, GPU,
  signing, or other fixture-dependent evidence explicitly open.

## Non-goals

- Claiming all 60 parity checkpoints from a successful Windows build.
- Expanding MobaXterm Personal 26.6 scope with Professional-only Customizer
  behavior.
- Treating a cross-target build, source inspection, ignored test, or software
  mock as native runtime or physical-device evidence.
- Implementing macOS delivery or the broader issue #3 release-publishing
  program, except where a narrowly shared defect blocks Windows work.
- Adding a signing certificate requirement to unsigned development
  qualification.
- Replacing identity-safe opened-handle operations with pathname fallbacks.

## Anti-goals

- Taking over an existing X display, listener, profile, service, agent,
  registry entry, or user-owned process.
- Uploading credentials, private keys, unredacted connection data, vault
  contents, or other personal evidence.
- Editing checklist labels to look complete when required fixtures are absent.
- Introducing broad framework abstractions, dependency upgrades, or unrelated
  refactors to make the issue appear more comprehensive.
- Building an installer that removes workspace, vault, recovery, corrupt
  originals, or unrelated files and registrations.

## Constraints

- Branch: `issue-1-windows`, based on `origin/main` at `98b213d`.
- Host: Windows x86-64, build 26300, with Visual Studio Build Tools 2022
  17.14.31 installed.
- Verified project Rust version: 1.96.1. The host initially has 1.94.1, so the
  pinned toolchain must be installed and invoked explicitly.
- Cargo commands use locked dependencies and `CARGO_BUILD_JOBS=2`.
- The GUI and `engines/rdp` retain independent lockfiles and build/test gates.
- Evidence records exact commands, exit status, source revision, lockfile
  hashes, toolchain versions, artifact hashes, and fixture limitations.
- The private VcXsrv artifact must follow
  `packaging/x11/native/source-contract.json` and its pinned patch. Stock or
  unqualified engines remain disabled.
- Runtime tests operate only on private temporary workspaces, profiles,
  listeners, registry keys, display numbers, and owned processes.
- Destructive install/uninstall tests use generated test packages and isolated
  user-scoped destinations. User data is snapshotted and checked for
  preservation.
- Existing compatibility paths, especially `%LOCALAPPDATA%\Harbor`, remain
  valid.
- Hardware-only and original-product checks are never synthesized.

## Approaches considered

### One large implementation pass

Touch all subsystems before running a qualification sweep. This minimizes early
coordination but makes regressions, evidence attribution, and review boundaries
poor. It also encourages a false all-or-nothing reading of issue #1.

### Multiple feature branches and pull requests

Split native build, X11, RDP, packaging, and qualification into separate
branches. This gives the cleanest independent reviews but conflicts with the
requested single branch and makes shared evidence/checklist updates harder to
order.

### Staged single branch — selected

Use one isolated `issue-1-windows` branch with small test-first increments and
checkpoint commits. Each stage produces an independently testable result and an
evidence update. This matches the repository's durable-queue methodology while
keeping issue #1 reviewable as one coherent Windows effort.

## Design

### 1. Native baseline and evidence harness

Install or select the pinned Rust toolchain, capture the Windows/MSVC/SDK and
Python identities, hash both lockfiles, and run the existing native baseline.
The baseline includes formatting, root tests, RDP-helper tests, package tests,
and existing Windows component checks. Failures become test-first repair work;
they are not waived. A small Windows qualification recorder may be added if the
existing scripts cannot capture deterministic command/artifact metadata without
manual transcription.

### 2. Native application and helper closure

Build the GUI and RDP helper from their independent locked manifests. Verify PE
architecture, adjacent-helper discovery, runtime dependency closure, clean
startup, bounded shutdown, and owned-process cleanup. Prefer existing package
and startup probes; extend them only for observable Windows behavior they do
not currently cover.

### 3. Portable package and per-user installer

Exercise the existing `packaging/windows/` foundation with generated packages.
Tests cover manifest validation, reparse-point rejection, immutable versioned
directories, shortcut targets, integration ownership, upgrade/relocation,
guarded uninstall, cleanup, and preservation of workspace/vault/recovery data.
Changes stay in the current PowerShell/Rust/Python patterns; no new installer
framework is introduced unless the existing script cannot meet an acceptance
requirement.

### 4. Private Windows X server

Use the exact VcXsrv source archive and patch recorded in the native source
contract. Validate inputs before building. The resulting private staging tree
must inventory executable/DLL/font/XKB/source/license content and record exact
hashes. Runtime qualification covers inherited private listener behavior,
authority cookie enforcement, owned job/PID cleanup, readiness, display
collision, clipboard, keyboard layout, Direct/Indirect XDMCP, SSH forwarding,
and supported graphics/monitor workflows. Unsupported extensions and missing
GPU fixtures remain explicit. The source contract's artifact hash and
redistribution state change only after the exact output qualifies.

If the exact build dependency set cannot be safely provisioned or a required
redistribution notice is unavailable, retain the provider as unqualified and
record the precise dependency rather than substituting a stock VcXsrv build.

### 5. Native interactive application workflows

Exercise ConPTY shells and representative full-screen programs, Unicode/input,
splits, detached tabs, shortcuts, System/Light/Dark appearance, and
100%/150%/200% scaling. Automate state-level checks where stable APIs exist and
retain bounded screenshots/logs for visual or native-window behavior. Manual
observations identify the exact executable hash and desktop/scaling state.

### 6. Native RDP and device workflows

Qualify clipboard, drives, opened-handle file operations, admin/fullscreen,
printers, smartcards, and serial redirection against locally available
independent endpoints. Software-fixture results and physical-device results are
separate evidence classes. Code defects receive a failing regression test
before repair. Missing RDP servers or devices remain named dependencies.

### 7. Browser, services, capture, credentials, and sessions

Qualify the bundled Chromium/client runtime, private profiles, input/resize,
sandbox behavior, permissions, cancellation, and owned cleanup. Exercise
locally available managed service/capture/control providers with independent
peers. Validate Windows key/agent/Pageant behavior, vault ACL/UI/recovery,
session imports, shared definitions, gateways, proxies, and reconnects while
checking legacy workspace and encrypted credential preservation.

### 8. Evidence, checklists, and GitHub issue updates

Each checkpoint commit adds or updates a bounded evidence report containing:

- exact source and artifact identity;
- toolchain and dependency identity;
- commands and outcomes;
- fixtures used and fixtures absent;
- implemented, qualified, unsupported, and remaining statements;
- links to retained raw evidence where appropriate.

Update the durable queue and parity documents in the same checkpoint that
changes their factual basis. GitHub issue #1 receives concise milestone
comments with commit/evidence links. A top-level issue checkbox is checked only
when its entire statement is demonstrated; partial progress is reported in a
comment and retained as partial in repository checklists.

## Stage boundaries

1. Toolchain and clean native baseline.
2. Native GUI/RDP build and lifecycle qualification.
3. Portable package and per-user installer qualification.
4. Private VcXsrv build/inventory and X11 qualification, or an exact recorded
   dependency boundary.
5. Terminal/workspace/theme/scaling qualification.
6. RDP/device qualification with software/physical evidence separated.
7. Browser/services/credentials/session qualification.
8. Final checklist reconciliation, full verification, and issue summary.

Stages may uncover implementation gaps. A gap stays inside its owning stage and
is handled with the smallest test-first change that satisfies the documented
behavior. A stage does not depend on unavailable evidence from a later stage.

## Verification strategy

- Every production fix follows red-green-refactor and adds a behavior-level
  regression test using real code where practical.
- Existing full suites run after each implementation increment; targeted tests
  alone do not establish a green checkpoint.
- Native runtime claims require the exact Windows executable under test.
- Package claims require testing the produced archive/installation, not only
  the staging directory.
- Process/listener cleanup is checked after both success and cancellation or
  failure paths.
- Evidence and checklist claims receive a final line-by-line audit against
  issue #1 acceptance criteria.
- A fresh code reviewer checks completed implementation before branch handoff.

## Builder decisions left open

- Exact temporary directories, display numbers, ports, test account names, and
  evidence filenames, provided they are private, deterministic, and recorded.
- Whether an existing script should be extended or a focused new script added
  for a qualification step, based on keeping each file single-purpose.
- Ordering of independent locally actionable workflows within a stage to
  minimize repeated native builds.
- Which safe independent RDP/X11/service endpoints are available locally;
  unavailable endpoint classes must be documented rather than emulated as
  native proof.
- Minor UI automation mechanics, provided the observation is bounded,
  reproducible, and tied to the exact artifact.

## Completion condition

The branch is ready for handoff when all locally actionable issue #1 work has
passing tests and exact evidence; all affected parity documents and issue #1
reflect those facts; and every unavailable native-host, hardware,
original-product, signing, or remote-fixture requirement is explicitly listed
without being counted as complete.
