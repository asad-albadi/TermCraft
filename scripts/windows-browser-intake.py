#!/usr/bin/env python3
"""Run one explicitly supplied diagnostic child inside an owned Windows job.

The executable is created suspended, assigned before its first instruction,
and receives only the three explicitly listed standard handles. No processes
are discovered by name and no existing browser/profile is selected.
"""
import ctypes as c
from ctypes import wintypes as w
import math
import os
from pathlib import Path
import subprocess
import time


def command(executable: Path, profile: Path) -> list[str]:
    return [str(executable), '--headless', '--allow-chrome-scheme-url', '--dump-dom',
            '--no-first-run', '--no-default-browser-check',
            '--disable-background-networking', '--disable-component-update',
            '--timeout=20000', '--user-data-dir=' + str(profile), 'chrome://credits/']


def _run_owned(argv, stdout, stderr, timeout):
    if os.name != 'nt':
        raise OSError('Windows native process ownership is required')
    if not isinstance(timeout, (int, float)) or not math.isfinite(timeout) or not 0 < timeout <= 180:
        raise ValueError('Expected a bounded timeout between zero and 180 seconds')
    import msvcrt

    class Startup(c.Structure):
        _fields_ = [('cb', w.DWORD), ('reserved', w.LPWSTR), ('desktop', w.LPWSTR),
                    ('title', w.LPWSTR), ('x', w.DWORD), ('y', w.DWORD),
                    ('xsize', w.DWORD), ('ysize', w.DWORD), ('xchars', w.DWORD),
                    ('ychars', w.DWORD), ('fill', w.DWORD), ('flags', w.DWORD),
                    ('show', w.WORD), ('reserved_size', w.WORD), ('reserved_bytes', c.c_void_p),
                    ('stdin', w.HANDLE), ('stdout', w.HANDLE), ('stderr', w.HANDLE)]

    class StartupEx(c.Structure):
        _fields_ = [('startup', Startup), ('attributes', c.c_void_p)]

    class Process(c.Structure):
        _fields_ = [('process', w.HANDLE), ('thread', w.HANDLE), ('pid', w.DWORD), ('tid', w.DWORD)]

    class BasicLimits(c.Structure):
        _fields_ = [('process_time', c.c_longlong), ('job_time', c.c_longlong),
                    ('flags', w.DWORD), ('min_working', c.c_size_t), ('max_working', c.c_size_t),
                    ('active_limit', w.DWORD), ('affinity', c.c_size_t),
                    ('priority', w.DWORD), ('scheduling', w.DWORD)]

    class IoCounters(c.Structure):
        _fields_ = [(name, c.c_ulonglong) for name in
                    ('read_ops', 'write_ops', 'other_ops', 'read_bytes', 'write_bytes', 'other_bytes')]

    class ExtendedLimits(c.Structure):
        _fields_ = [('basic', BasicLimits), ('io', IoCounters),
                    ('process_memory', c.c_size_t), ('job_memory', c.c_size_t),
                    ('peak_process', c.c_size_t), ('peak_job', c.c_size_t)]

    class Accounting(c.Structure):
        _fields_ = [(name, c.c_longlong) for name in
                    ('user', 'kernel', 'period_user', 'period_kernel')] + [
                    (name, w.DWORD) for name in ('faults', 'total', 'active', 'terminated')]

    kernel = c.WinDLL('kernel32', use_last_error=True)

    def api(name, result, *args):
        fn = getattr(kernel, name)
        fn.restype, fn.argtypes = result, args
        return fn

    close = api('CloseHandle', w.BOOL, w.HANDLE)
    create_job = api('CreateJobObjectW', w.HANDLE, c.c_void_p, w.LPCWSTR)
    limits = api('SetInformationJobObject', w.BOOL, w.HANDLE, c.c_int, c.c_void_p, w.DWORD)
    query = api('QueryInformationJobObject', w.BOOL, w.HANDLE, c.c_int, c.c_void_p, w.DWORD, c.c_void_p)
    terminate = api('TerminateJobObject', w.BOOL, w.HANDLE, w.UINT)
    initialize = api('InitializeProcThreadAttributeList', w.BOOL, c.c_void_p, w.DWORD, w.DWORD,
                     c.POINTER(c.c_size_t))
    update = api('UpdateProcThreadAttribute', w.BOOL, c.c_void_p, w.DWORD, c.c_size_t,
                 c.c_void_p, c.c_size_t, c.c_void_p, c.c_void_p)
    delete = api('DeleteProcThreadAttributeList', None, c.c_void_p)
    create = api('CreateProcessW', w.BOOL, w.LPCWSTR, w.LPWSTR, c.c_void_p, c.c_void_p,
                 w.BOOL, w.DWORD, c.c_void_p, w.LPCWSTR, c.POINTER(StartupEx), c.POINTER(Process))
    assign = api('AssignProcessToJobObject', w.BOOL, w.HANDLE, w.HANDLE)
    resume = api('ResumeThread', w.DWORD, w.HANDLE)
    wait = api('WaitForSingleObject', w.DWORD, w.HANDLE, w.DWORD)
    exit_code = api('GetExitCodeProcess', w.BOOL, w.HANDLE, c.POINTER(w.DWORD))
    terminate_process = api('TerminateProcess', w.BOOL, w.HANDLE, w.UINT)

    def checked(value, operation):
        if not value:
            raise c.WinError(c.get_last_error(), operation)
        return value

    job = create_job(None, None)
    checked(job, 'CreateJobObjectW')
    process = Process()
    descriptors = []
    attributes = None
    initialized = False
    assigned = False
    try:
        policy = ExtendedLimits()
        policy.basic.flags = 0x2000  # JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE
        checked(limits(job, 9, c.byref(policy), c.sizeof(policy)), 'SetInformationJobObject')
        # Exclusive creation prevents replacement of previous diagnostic outputs.
        descriptors.append(os.open(stdout, os.O_WRONLY | os.O_CREAT | os.O_EXCL | os.O_BINARY, 0o600))
        descriptors.append(os.open(stderr, os.O_WRONLY | os.O_CREAT | os.O_EXCL | os.O_BINARY, 0o600))
        descriptors.append(os.open(os.devnull, os.O_RDONLY | os.O_BINARY))
        handles = (w.HANDLE * 3)(*(msvcrt.get_osfhandle(fd) for fd in descriptors))
        for fd in descriptors:
            os.set_inheritable(fd, True)
        length = c.c_size_t()
        initialize(None, 1, 0, c.byref(length))
        if length.value == 0:
            raise c.WinError(c.get_last_error(), 'InitializeProcThreadAttributeList size')
        attributes = c.create_string_buffer(length.value)
        checked(initialize(attributes, 1, 0, c.byref(length)), 'InitializeProcThreadAttributeList')
        initialized = True
        checked(update(attributes, 0, 0x00020002, handles, c.sizeof(handles), None, None),
                'UpdateProcThreadAttribute HANDLE_LIST')
        start = StartupEx()
        start.startup.cb = c.sizeof(start)
        start.startup.flags = 0x100  # STARTF_USESTDHANDLES
        start.startup.stdout, start.startup.stderr, start.startup.stdin = handles
        start.attributes = c.cast(attributes, c.c_void_p)
        line = c.create_unicode_buffer(subprocess.list2cmdline(argv))
        checked(create(argv[0], line, None, None, True, 0x00080000 | 0x4 | 0x08000000,
                       None, None, c.byref(start), c.byref(process)), 'CreateProcessW suspended')
        for fd in descriptors:
            os.set_inheritable(fd, False)
        checked(assign(job, process.process), 'AssignProcessToJobObject')
        assigned = True
        if resume(process.thread) == 0xffffffff:
            raise c.WinError(c.get_last_error(), 'ResumeThread')
        outcome = wait(process.process, math.ceil(timeout * 1000))
        if outcome not in (0, 258):
            raise c.WinError(c.get_last_error(), 'WaitForSingleObject')
        timed_out = outcome == 258
        checked(terminate(job, 1 if timed_out else 0), 'TerminateJobObject')
        deadline = time.monotonic() + 10
        while True:
            accounting = Accounting()
            checked(query(job, 1, c.byref(accounting), c.sizeof(accounting), None),
                    'QueryInformationJobObject accounting')
            if accounting.active == 0:
                break
            if time.monotonic() >= deadline:
                raise RuntimeError('Owned job did not drain within cleanup deadline')
            time.sleep(0.025)
        checked(wait(process.process, 1000) == 0, 'Owned root process did not exit')
        code = w.DWORD()
        checked(exit_code(process.process, c.byref(code)), 'GetExitCodeProcess')
        return {'pid': process.pid, 'exit_code': code.value, 'timed_out': timed_out,
                'assigned_before_resume': True, 'inherited_handle_count': 3,
                'cleanup': {'active_processes': 0, 'job_terminated': True, 'kill_on_close': True},
                'argv': list(argv)}
    finally:
        # Only this exact suspended root can exist outside the job if assignment failed.
        if process.process:
            if assigned:
                terminate(job, 1)
            else:
                terminate_process(process.process, 1)
            wait(process.process, 10000)
        if process.thread:
            close(process.thread)
        if process.process:
            close(process.process)
        close(job)
        if initialized:
            delete(attributes)
        for fd in descriptors:
            os.close(fd)


def _private_profile(path):
    """Create a fresh directory with a protected DACL for the current token user."""
    if os.name != 'nt':
        raise OSError('Windows profile security is required')
    kernel = c.WinDLL('kernel32', use_last_error=True)
    security = c.WinDLL('advapi32', use_last_error=True)

    class Attributes(c.Structure):
        _fields_ = [('size', w.DWORD), ('descriptor', c.c_void_p), ('inherit', w.BOOL)]

    class TokenUser(c.Structure):
        _fields_ = [('sid', c.c_void_p), ('attributes', w.DWORD)]

    def bind(lib, name, result, *arguments):
        fn = getattr(lib, name)
        fn.restype, fn.argtypes = result, arguments
        return fn

    current = bind(kernel, 'GetCurrentProcess', w.HANDLE)
    open_token = bind(security, 'OpenProcessToken', w.BOOL, w.HANDLE, w.DWORD, c.POINTER(w.HANDLE))
    information = bind(security, 'GetTokenInformation', w.BOOL, w.HANDLE, c.c_int,
                       c.c_void_p, w.DWORD, c.POINTER(w.DWORD))
    sid_string = bind(security, 'ConvertSidToStringSidW', w.BOOL, c.c_void_p, c.POINTER(w.LPWSTR))
    descriptor = bind(security, 'ConvertStringSecurityDescriptorToSecurityDescriptorW', w.BOOL,
                      w.LPCWSTR, w.DWORD, c.POINTER(c.c_void_p), c.c_void_p)
    create = bind(kernel, 'CreateDirectoryW', w.BOOL, w.LPCWSTR, c.POINTER(Attributes))
    free = bind(kernel, 'LocalFree', c.c_void_p, c.c_void_p)
    close = bind(kernel, 'CloseHandle', w.BOOL, w.HANDLE)
    token, text, sd = w.HANDLE(), w.LPWSTR(), c.c_void_p()
    try:
        if not open_token(current(), 0x8, c.byref(token)):
            raise c.WinError(c.get_last_error())
        length = w.DWORD()
        information(token, 1, None, 0, c.byref(length))
        buffer = c.create_string_buffer(length.value)
        if not information(token, 1, buffer, length, c.byref(length)):
            raise c.WinError(c.get_last_error())
        user = c.cast(buffer, c.POINTER(TokenUser)).contents
        if not sid_string(user.sid, c.byref(text)):
            raise c.WinError(c.get_last_error())
        # OI/CI applies this same owner-only policy to subsequently created files.
        sddl = 'D:P(A;OICI;FA;;;' + text.value + ')'
        if not descriptor(sddl, 1, c.byref(sd), None):
            raise c.WinError(c.get_last_error())
        attributes = Attributes(c.sizeof(Attributes), sd, False)
        if not create(str(path), c.byref(attributes)):
            raise c.WinError(c.get_last_error())
    finally:
        if sd:
            free(sd)
        if text:
            free(c.cast(text, c.c_void_p))
        if token:
            close(token)


def capture(executable: Path, profile: Path, stdout: Path, stderr: Path, timeout=90) -> dict:
    if os.name != 'nt':
        raise OSError('Windows native process ownership is required')
    executable = Path(executable).resolve(strict=True)
    if executable.name.lower() != 'chrome.exe' or not executable.is_file():
        raise ValueError('Expected the exact supplied chrome.exe')
    profile = Path(profile).absolute()
    # CreateDirectoryW refuses an existing profile, including reparse targets.
    _private_profile(profile)
    return _run_owned(command(executable, profile), Path(stdout), Path(stderr), timeout)
