# Windows embedded Browser source candidate

The Windows candidate connects the existing embedded Browser worker to a
64-bit Google Chrome or Chromium installation. It discovers `chrome.exe`
in the Windows Program Files and Local AppData known folders and checks the
PE machine type, executable flag and header bounds. The candidate accepts
x86-64 and ARM64 images matching the app architecture. The Windows loader
and local policy govern the installed binary. TermCraft does not bundle an
engine or select programs from PATH. [Microsoft PE format](https://learn.microsoft.com/en-us/windows/win32/debug/pe-format)

Each Browser tab starts Chrome or Chromium headless into a new directory
with a protected DACL granting access to the current user. TermCraft uses
a local fixed volume with persistent ACLs. The child receives the Windows
system directory and a small explicit environment with its home, AppData
and temp variables pointing inside the private profile. Startup supplies a
Unicode executable path and argument block directly to Windows. The
provider retains Chromium's sandbox and standard TLS validation.

The private channel consists of two one-client, local, randomly named
byte-stream pipes. TermCraft creates both ends with inheritance disabled.
It starts a short-lived copy of its executable in the browser job without
inheriting handles. Both processes verify the peer's process ID through
the local control pipe. TermCraft then duplicates the two browser ends
as inheritable handles directly into the helper's handle table. Only the
helper creates Chromium, using a whitelist of those two synchronous pipe
handles and a shared NUL handle for standard I/O. This keeps the pipe
handles out of unrelated children launched by TermCraft. The switches
`--remote-debugging-pipe` and `--remote-debugging-io-pipes` pass the
inherited browser input and output handle values as comma-separated
unsigned integers. The worker uses overlapped I/O on its own ends, limits
each message to 12 MiB, and requests cancellation at deadlines. Windows
requires completion to be collected before releasing the I/O buffers.
There is no DevTools TCP listener. [Chromium pipe handle adoption](https://chromium.googlesource.com/chromium/src/+/lkgr/content/browser/devtools/devtools_agent_host_impl.cc) · [Chromium pipe reader](https://chromium.googlesource.com/chromium/src/+/main/content/browser/devtools/devtools_pipe_handler.cc) · [Microsoft CancelIoEx](https://learn.microsoft.com/en-us/windows/win32/api/ioapiset/nf-ioapiset-cancelioex)

Windows assigns the suspended helper to a kill-on-close job at process
creation. Chromium and its descendants join that job through the helper.
The job handle stays with the worker. Closing a tab signals cancellation without waiting in the UI.
The worker stops the job, confirms its processes have exited, and removes
the profile. Normal workspace exit also joins those workers. A forced app
exit can leave an owner-private profile directory. If Windows cannot confirm
job teardown, the worker leaves the directory in place. [Microsoft process creation attributes](https://learn.microsoft.com/en-us/windows/win32/api/processthreadsapi/nf-processthreadsapi-updateprocthreadattribute)

## Verification on the Linux development host

- `component-check.log` records a passing cross-target Rust check for
  `x86_64-pc-windows-msvc`. The checking crate in
  `target/browser-windows-component` uses the application dependency
  versions and includes the production Browser modules, the actual runtime
  discovery module, and the Windows transport fixtures. Its small facade
  supplies the enclosing catalog API. This is a source and type check.
- `pe-tests.log` records two passing PE parser tests against the
  production file. These tests run without Windows system calls.
- `linux-regression.log` records three passing real Chromium fixture tests
  and one Browser settings test on Linux.
- `full-check.log` records the full Windows application check blocked
  during native dependency compilation because this Linux host lacks
  `lib.exe`. The application was not linked or run on Windows.

## Native qualification pending

On a Windows 10 or later machine with the supported Rust and Windows SDK
tooling, run the native transport fixtures and then the loopback Chromium
fixtures with a maintained native Chrome or Chromium installation.

```powershell
cargo test --lib browser::pipe::tests::
cargo test --test browser_session -- --ignored
```

The transport fixtures use an owned test process. They exercise a pipe
round trip, a timed-out read, cancellation of a blocked write, UTF-16
argument quoting, directory ACLs, restricted handle inheritance, and
stopping a job that owns both a helper and a descendant. While the private
handles are live, the fixture launches an unrelated process through
ordinary Rust process creation. The owned descendant echoes input over
the intended pipes, and the output pipe reaches EOF after the job stops
while the unrelated process and profile remain intact. The browser fixtures require the real
provider and exercise Chromium pixels, input, history and resize, HTTP
authentication, proxy handling and rejection of an untrusted certificate
using loopback endpoints.

Windows execution, GUI input and DPI behavior, sandbox nesting under host
job policies, and cleanup with real Chrome descendants remain unmeasured.
There is no qualified Windows package, measured engine version or Windows
binary checksum yet. P01/P08 packaged runtime closure and the corresponding
native demonstrations remain open. [Browser runtime packaging](../../../../packaging/browser/README.md)
