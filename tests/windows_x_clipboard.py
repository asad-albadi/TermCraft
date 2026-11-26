"""Opt-in VcXsrv clipboard test for an elevated disposable Windows runner.

The controller and X server use a newly created noninteractive window station.
Supply the manifest hash from the exact candidate build and the two pinned
wheel files in a private directory. This script never switches the desktop.
"""

import argparse
from contextlib import contextmanager
import csv
import ctypes as c
from ctypes import wintypes as w
import json
import os
from pathlib import Path
import queue
import secrets
import socket
import struct
import subprocess
import sys
import tempfile
import threading
import time

from windows_xdmcp_candidate import candidate_environment, digest, field, x11_setup


WHEELS = {
    "python_xlib-0.33-py2.py3-none-any.whl": "c3534038d42e0df2f1392a1b30a15a4ff5fdc2b86cfa94f072bf11b10a164398",
    "six-1.17.0-py2.py3-none-any.whl": "4721f391ed90541fddacab5acf947aa0d3dc7d27b2e1e8eda2be8970586c3274",
}
CF_UNICODETEXT = 13
OWNER_TEXT = "X selection to Windows — café 日本語 🚀"
NATIVE_TEXT = "Windows to X selection — ελληνικά 中 😀"


def api(dll, name, result, *arguments):
    function = getattr(dll, name)
    function.restype = result
    function.argtypes = arguments
    return function


def require(result, operation):
    if not result:
        code = c.get_last_error()
        raise OSError(f"{operation} failed: Win32 {code}: {c.FormatError(code).strip()}")
    return result


class SecurityAttributes(c.Structure):
    _fields_ = [("length", w.DWORD), ("descriptor", c.c_void_p), ("inherit", w.BOOL)]


class StartupInfo(c.Structure):
    _fields_ = [("size", w.DWORD), ("reserved", w.LPWSTR), ("desktop", w.LPWSTR), ("title", w.LPWSTR)] + [
        (field_name, w.DWORD) for field_name in ("x", "y", "width", "height", "xchars", "ychars", "fill", "flags")
    ] + [("show", w.WORD), ("reserved_size", w.WORD), ("reserved_buf", c.c_void_p),
         ("stdin", w.HANDLE), ("stdout", w.HANDLE), ("stderr", w.HANDLE)]


class ProcessInfo(c.Structure):
    _fields_ = [("process", w.HANDLE), ("thread", w.HANDLE), ("pid", w.DWORD), ("tid", w.DWORD)]


class JobLimits(c.Structure):
    _fields_ = [("process_time", c.c_int64), ("job_time", c.c_int64), ("flags", w.DWORD),
                ("min_space", c.c_size_t), ("max_space", c.c_size_t), ("active", w.DWORD),
                ("affinity", c.c_size_t), ("priority", w.DWORD), ("scheduling", w.DWORD)]


class ExtendedJobLimits(c.Structure):
    _fields_ = [("base", JobLimits), ("io", c.c_uint64 * 6), ("memory", c.c_size_t * 4)]


class Windows:
    def __init__(self):
        self.user = c.WinDLL("user32", use_last_error=True)
        self.kernel = c.WinDLL("kernel32", use_last_error=True)
        self.security = c.WinDLL("advapi32", use_last_error=True)

    def object_name(self, handle):
        query = api(self.user, "GetUserObjectInformationW", w.BOOL, w.HANDLE, c.c_int,
                    c.c_void_p, w.DWORD, c.POINTER(w.DWORD))
        name = c.create_unicode_buffer(256)
        require(query(handle, 2, name, c.sizeof(name), c.byref(w.DWORD())), "GetUserObjectInformationW")
        return name.value

    def current(self):
        station = require(api(self.user, "GetProcessWindowStation", w.HANDLE)(), "GetProcessWindowStation")
        tid = api(self.kernel, "GetCurrentThreadId", w.DWORD)()
        desktop = require(api(self.user, "GetThreadDesktop", w.HANDLE, w.DWORD)(tid), "GetThreadDesktop")
        return station, desktop

    def check_isolated(self, station_name, desktop_name):
        station, desktop = self.current()
        if station_name.casefold() == "winsta0" or self.object_name(station) != station_name or self.object_name(desktop) != desktop_name:
            raise RuntimeError("Clipboard test is outside its private station and desktop")

    @contextmanager
    def private_desktop(self):
        station_name = "TermCraftClipboard" + secrets.token_hex(8)
        desktop_name = "Fixture"
        old_station, old_desktop = self.current()
        whoami = Path(os.environ["SystemRoot"]) / "System32/whoami.exe"
        account = subprocess.check_output([whoami, "/user", "/fo", "csv", "/nh"], text=True, timeout=5)
        sid = next(csv.reader(account.splitlines()))[1]
        if not sid.startswith("S-1-") or not all(part.isdigit() for part in sid.split("-")[1:]):
            raise RuntimeError("Could not identify the fixture account SID")
        descriptor = c.c_void_p()
        convert = api(self.security, "ConvertStringSecurityDescriptorToSecurityDescriptorW", w.BOOL,
                      w.LPCWSTR, w.DWORD, c.POINTER(c.c_void_p), c.c_void_p)
        require(convert(f"D:P(A;;GA;;;{sid})", 1, c.byref(descriptor), None), "Create fixture security descriptor")
        free = api(self.kernel, "LocalFree", c.c_void_p, c.c_void_p)
        create_station = api(self.user, "CreateWindowStationW", w.HANDLE, w.LPCWSTR, w.DWORD, w.DWORD,
                             c.POINTER(SecurityAttributes))
        create_desktop = api(self.user, "CreateDesktopW", w.HANDLE, w.LPCWSTR, w.LPCWSTR, c.c_void_p,
                             w.DWORD, w.DWORD, c.POINTER(SecurityAttributes))
        set_station = api(self.user, "SetProcessWindowStation", w.BOOL, w.HANDLE)
        set_desktop = api(self.user, "SetThreadDesktop", w.BOOL, w.HANDLE)
        close_station = api(self.user, "CloseWindowStation", w.BOOL, w.HANDLE)
        close_desktop = api(self.user, "CloseDesktop", w.BOOL, w.HANDLE)
        attributes = SecurityAttributes(c.sizeof(SecurityAttributes), descriptor, False)
        station = desktop = None
        try:
            station = require(create_station(station_name, 1, 0x37F, c.byref(attributes)), "CreateWindowStationW (private create-only station)")
            require(set_station(station), "SetProcessWindowStation (private station)")
            desktop = require(create_desktop(desktop_name, None, None, 0, 0xC3, c.byref(attributes)), "CreateDesktopW (private desktop)")
            require(set_desktop(desktop), "SetThreadDesktop (private desktop)")
            self.check_isolated(station_name, desktop_name)
            yield station_name, desktop_name, desktop
        finally:
            free(descriptor)
            if station:
                set_station(old_station)
                set_desktop(old_desktop)
                if desktop:
                    close_desktop(desktop)
                close_station(station)

    @contextmanager
    def server(self, arguments, cwd, env, station_name, desktop_name):
        self.check_isolated(station_name, desktop_name)
        start = StartupInfo()
        start.size = c.sizeof(start)
        start.desktop = f"{station_name}\\{desktop_name}"
        process = ProcessInfo()
        close = api(self.kernel, "CloseHandle", w.BOOL, w.HANDLE)
        terminate = api(self.kernel, "TerminateProcess", w.BOOL, w.HANDLE, w.UINT)
        wait = api(self.kernel, "WaitForSingleObject", w.DWORD, w.HANDLE, w.DWORD)
        job = require(api(self.kernel, "CreateJobObjectW", w.HANDLE, c.c_void_p, w.LPCWSTR)(None, None), "CreateJobObjectW")
        try:
            limits = ExtendedJobLimits()
            limits.base.flags = 0x2000  # JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE
            configure = api(self.kernel, "SetInformationJobObject", w.BOOL, w.HANDLE, c.c_int, c.c_void_p, w.DWORD)
            require(configure(job, 9, c.byref(limits), c.sizeof(limits)), "SetInformationJobObject")
            create = api(self.kernel, "CreateProcessW", w.BOOL, w.LPCWSTR, w.LPWSTR, c.c_void_p,
                         c.c_void_p, w.BOOL, w.DWORD, c.c_void_p, w.LPCWSTR,
                         c.POINTER(StartupInfo), c.POINTER(ProcessInfo))
            command = c.create_unicode_buffer(subprocess.list2cmdline([str(arg) for arg in arguments]))
            environment = c.create_unicode_buffer("\0".join(f"{key}={env[key]}" for key in sorted(env, key=str.upper)) + "\0\0")
            require(create(str(arguments[0]), command, None, None, False, 0x404, environment, str(cwd),
                           c.byref(start), c.byref(process)), "CreateProcessW (VcXsrv on private desktop)")
            assign = api(self.kernel, "AssignProcessToJobObject", w.BOOL, w.HANDLE, w.HANDLE)
            require(assign(job, process.process), "AssignProcessToJobObject")
            resume = api(self.kernel, "ResumeThread", w.DWORD, w.HANDLE)
            require(resume(process.thread) != 0xFFFFFFFF, "ResumeThread (VcXsrv)")
            yield process, wait
        finally:
            close(job)
            if process.process:
                if wait(process.process, 2000) == 0x102:
                    terminate(process.process, 1)
                    wait(process.process, 2000)
                close(process.process)
                close(process.thread)

    @contextmanager
    def clipboard(self, station_name, desktop_name, window=None):
        self.check_isolated(station_name, desktop_name)
        open_clipboard = api(self.user, "OpenClipboard", w.BOOL, w.HWND)
        deadline = time.monotonic() + 2
        while not open_clipboard(window):
            if time.monotonic() >= deadline:
                require(False, "OpenClipboard (private station)")
            time.sleep(0.025)
        try:
            yield
        finally:
            require(api(self.user, "CloseClipboard", w.BOOL)(), "CloseClipboard (private station)")

    @contextmanager
    def clipboard_window(self, station_name, desktop_name):
        self.check_isolated(station_name, desktop_name)
        create = api(self.user, "CreateWindowExW", w.HWND, w.DWORD, w.LPCWSTR, w.LPCWSTR, w.DWORD,
                     c.c_int, c.c_int, c.c_int, c.c_int, w.HWND, w.HMENU, w.HINSTANCE, c.c_void_p)
        hwnd = require(create(0, "STATIC", "TermCraft private clipboard", 0, 0, 0, 0, 0,
                              w.HWND(-3), None, None, None), "CreateWindowExW (private clipboard owner)")
        try:
            yield hwnd
        finally:
            api(self.user, "DestroyWindow", w.BOOL, w.HWND)(hwnd)

    def write_text(self, station_name, desktop_name, window, text):
        payload = (text + "\0").encode("utf-16-le")
        allocate = api(self.kernel, "GlobalAlloc", w.HANDLE, w.UINT, c.c_size_t)
        lock = api(self.kernel, "GlobalLock", c.c_void_p, w.HANDLE)
        unlock = api(self.kernel, "GlobalUnlock", w.BOOL, w.HANDLE)
        free = api(self.kernel, "GlobalFree", w.HANDLE, w.HANDLE)
        memory = require(allocate(2, len(payload)), "GlobalAlloc")
        try:
            address = require(lock(memory), "GlobalLock")
            c.memmove(address, payload, len(payload))
            unlock(memory)
            with self.clipboard(station_name, desktop_name, window):
                require(api(self.user, "EmptyClipboard", w.BOOL)(), "EmptyClipboard (private station)")
                put = api(self.user, "SetClipboardData", w.HANDLE, w.UINT, w.HANDLE)
                require(put(CF_UNICODETEXT, memory), "SetClipboardData (private station)")
                memory = None  # Clipboard now owns this allocation.
        finally:
            if memory:
                free(memory)

    def read_text(self, station_name, desktop_name):
        with self.clipboard(station_name, desktop_name):
            get = api(self.user, "GetClipboardData", w.HANDLE, w.UINT)
            handle = get(CF_UNICODETEXT)
            if not handle:
                return None
            size = api(self.kernel, "GlobalSize", c.c_size_t, w.HANDLE)(handle)
            if not 0 < size <= 8192 or size % 2 != 0:
                raise RuntimeError("Private clipboard returned invalid UTF-16 size")
            address = require(api(self.kernel, "GlobalLock", c.c_void_p, w.HANDLE)(handle), "GlobalLock")
            try:
                return c.string_at(address, size).decode("utf-16-le").split("\0", 1)[0]
            finally:
                api(self.kernel, "GlobalUnlock", w.BOOL, w.HANDLE)(handle)


def owns_loopback_listener(pid, port):
    ip_helper = c.WinDLL("iphlpapi", use_last_error=True)
    query = api(ip_helper, "GetExtendedTcpTable", w.DWORD, c.c_void_p,
                c.POINTER(w.DWORD), w.BOOL, w.ULONG, c.c_int, w.ULONG)
    size = w.DWORD()
    query(None, c.byref(size), False, socket.AF_INET, 3, 0)
    for _ in range(3):
        if not 4 <= size.value <= 4 * 1024 * 1024:
            raise RuntimeError("Invalid Windows TCP ownership table size")
        table = c.create_string_buffer(size.value)
        status = query(table, c.byref(size), False, socket.AF_INET, 3, 0)
        if status == 122:  # ERROR_INSUFFICIENT_BUFFER
            continue
        if status:
            raise OSError(f"GetExtendedTcpTable failed: Win32 {status}")
        (rows,) = struct.unpack_from("<I", table)
        if 4 + rows * 24 > size.value:
            raise RuntimeError("Invalid Windows TCP ownership table row count")
        listeners = [
            (socket.inet_ntoa(struct.pack("<I", local_ip)), socket.ntohs(local_port & 0xFFFF))
            for _, local_ip, local_port, _, _, owner in struct.iter_unpack("<6I", table.raw[4:4 + rows * 24])
            if owner == pid
        ]
        if listeners and listeners != [("127.0.0.1", port)]:
            raise RuntimeError("VcXsrv did not bind exactly the selected loopback listener")
        return bool(listeners)
    raise RuntimeError("Windows TCP ownership table repeatedly changed")


def accept_owned_listener(win, process, wait, port, cookie, isolated):
    deadline = time.monotonic() + 8
    while time.monotonic() < deadline:
        if wait(process.process, 0) != 0x102:
            raise RuntimeError("VcXsrv exited before opening its private X11 listener; see server.log")
        if not owns_loopback_listener(process.pid, port):
            time.sleep(0.025)
            continue
        get_desktop = api(win.user, "GetThreadDesktop", w.HANDLE, w.DWORD)
        child_desktop = require(get_desktop(process.tid), "GetThreadDesktop (VcXsrv)")
        if win.object_name(child_desktop) != isolated[1]:
            raise RuntimeError("VcXsrv is not on the designated private desktop")
        try:
            client = socket.create_connection(("127.0.0.1", port), 0.25)
        except (ConnectionRefusedError, TimeoutError):
            time.sleep(0.025)
            continue
        with client, client.makefile("rb") as stream:
            if x11_setup(client, stream, cookie)[0] != 1:
                raise RuntimeError("The private server rejected the generated Xauthority cookie")
        with socket.create_connection(("127.0.0.1", port), 1) as client, client.makefile("rb") as stream:
            if x11_setup(client, stream, b"")[0] != 0:
                raise RuntimeError("The private server accepted a connection without its cookie")
        return
    raise RuntimeError("Timed out waiting for VcXsrv on the private desktop; see server.log")


def exchange_clipboard(win, isolated, display_number, authority):
    from Xlib import X, Xatom, display, protocol

    station_name, desktop_name, desktop = isolated
    os.environ["XAUTHORITY"] = str(authority)
    connection = display.Display(f"tcp/127.0.0.1:{display_number}")
    window = connection.screen().root.create_window(0, 0, 1, 1, 0, X.CopyFromParent)
    clipboard, utf8, targets, prop = (connection.intern_atom(name) for name in
                                     ("CLIPBOARD", "UTF8_STRING", "TARGETS", "TERMCRAFT_TEST_CLIPBOARD"))
    outcome = queue.Queue(1)

    def read_from_windows():
        try:
            require(api(win.user, "SetThreadDesktop", w.BOOL, w.HANDLE)(desktop), "SetThreadDesktop (clipboard reader)")
            deadline = time.monotonic() + 7
            while time.monotonic() < deadline:
                if win.read_text(station_name, desktop_name) == OWNER_TEXT:
                    outcome.put(None)
                    return
                time.sleep(0.025)
            outcome.put(RuntimeError("Timed out reading the X selection from private CF_UNICODETEXT"))
        except Exception as error:
            outcome.put(error)

    try:
        window.set_selection_owner(clipboard, X.CurrentTime)
        connection.sync()
        reader = threading.Thread(target=read_from_windows, daemon=True)
        reader.start()
        deadline = time.monotonic() + 8
        while outcome.empty():
            if time.monotonic() >= deadline:
                raise RuntimeError("Timed out servicing the X selection request from VcXsrv")
            while connection.pending_events():
                event = connection.next_event()
                if event.type != X.SelectionRequest:
                    continue
                destination = event.property or event.target
                if event.target == targets:
                    event.requestor.change_property(destination, Xatom.ATOM, 32, [targets, utf8])
                elif event.target == utf8:
                    event.requestor.change_property(destination, utf8, 8, OWNER_TEXT.encode())
                else:
                    destination = X.NONE
                event.requestor.send_event(protocol.event.SelectionNotify(
                    time=event.time, requestor=event.requestor, selection=event.selection,
                    target=event.target, property=destination,
                ))
                connection.flush()
            time.sleep(0.025)
        if error := outcome.get_nowait():
            raise error
        reader.join(timeout=1)
        print("X CLIPBOARD → isolated Windows CF_UNICODETEXT passed", flush=True)

        with win.clipboard_window(station_name, desktop_name) as owner:
            win.write_text(station_name, desktop_name, owner, NATIVE_TEXT)
            deadline = time.monotonic() + 8
            while time.monotonic() < deadline:
                selected = connection.get_selection_owner(clipboard)
                if selected and selected != window:
                    break
                time.sleep(0.025)
            else:
                raise RuntimeError("VcXsrv did not claim the X clipboard after the private Windows write")
            window.convert_selection(clipboard, utf8, prop, X.CurrentTime)
            connection.flush()
            deadline = time.monotonic() + 8
            while time.monotonic() < deadline:
                while connection.pending_events():
                    event = connection.next_event()
                    if event.type == X.SelectionNotify and event.selection == clipboard:
                        value = window.get_full_property(prop, utf8)
                        if not value or value.value.decode("utf-8") != NATIVE_TEXT:
                            raise RuntimeError("Private Windows text did not match the X UTF8_STRING reply")
                        print("Isolated Windows CF_UNICODETEXT → X CLIPBOARD passed", flush=True)
                        return
                time.sleep(0.025)
            raise RuntimeError("Timed out reading private Windows clipboard text over X11")
    finally:
        connection.close()


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--candidate", type=Path, required=True)
    parser.add_argument("--inventory", type=Path, required=True)
    parser.add_argument("--inventory-sha256", required=True)
    parser.add_argument("--wheel-directory", type=Path, required=True)
    parser.add_argument("--evidence", type=Path, required=True)
    parser.add_argument("--worker", action="store_true", help=argparse.SUPPRESS)
    args = parser.parse_args()
    if os.name != "nt":
        parser.error("This fixture requires Windows")
    if not args.worker:
        # Keep even a blocked native delayed render or X11 call bounded. The
        # worker's owned job closes if the worker has to be terminated.
        command = [sys.executable, "-E", "-s", "-B", str(Path(__file__).resolve()), *sys.argv[1:], "--worker"]
        try:
            result = subprocess.run(command, timeout=60)
        except subprocess.TimeoutExpired:
            parser.exit(1, "Private clipboard worker timed out after 60 seconds.\n")
        raise SystemExit(result.returncode)
    if digest(args.inventory) != args.inventory_sha256.casefold():
        parser.error("Candidate inventory does not match the supplied SHA-256")
    candidate = args.candidate.resolve(strict=True)
    inventory = json.loads(args.inventory.read_text(encoding="utf-8-sig"))
    actual = {path.relative_to(candidate).as_posix(): digest(path) for path in candidate.rglob("*") if path.is_file()}
    if "vcxsrv.exe" not in actual or actual != inventory["files"]:
        parser.error("The private candidate differs from the pinned build inventory")
    for name, expected in WHEELS.items():
        wheel = (args.wheel_directory / name).resolve(strict=True)
        if digest(wheel) != expected:
            parser.error(f"Dependency does not match its pin: {name}")
        sys.path.insert(0, str(wheel))
    evidence = args.evidence.resolve()
    evidence.mkdir(parents=True, exist_ok=False)
    print(f"VcXsrv SHA-256 {actual['vcxsrv.exe']}; inventory verified for {len(actual)} files", flush=True)
    win = Windows()
    with win.private_desktop() as isolated, tempfile.TemporaryDirectory(prefix="TermCraft clipboard fixture ") as scratch:
        with socket.socket() as reservation:
            reservation.bind(("127.0.0.1", 0))
            port = reservation.getsockname()[1]
        display_number = port - 6000
        if not 1 <= display_number <= 59535:
            parser.error("Windows did not reserve a valid X11 display port")
        auth = Path(scratch) / "Xauthority"
        cookie = secrets.token_bytes(16)
        auth.write_bytes(struct.pack(">H", 0) + field(socket.inet_aton("127.0.0.1")) + field(str(display_number).encode())
                         + field(b"MIT-MAGIC-COOKIE-1") + field(cookie))
        arguments = [candidate / "vcxsrv.exe", f":{display_number}", "-screen", "0", "640x480", "-noreset",
                     "-silent-dup-error", "-nolisten", "inet6", "-nolisten", "hyperv", "-nopn", "-iglx", "-listen", "tcp",
                     "-auth", auth, "-logfile", evidence / "server.log", "-clipboard", "-noprimary"]
        with win.server(arguments, candidate, candidate_environment(candidate, scratch), *isolated[:2]) as (process, wait):
            accept_owned_listener(win, process, wait, port, cookie, isolated)
            exchange_clipboard(win, isolated, display_number, auth)
        if owns_loopback_listener(process.pid, port):
            raise RuntimeError("VcXsrv loopback listener survived its owned job")


if __name__ == "__main__":
    try:
        main()
    except (OSError, RuntimeError) as error:
        sys.exit(str(error))
