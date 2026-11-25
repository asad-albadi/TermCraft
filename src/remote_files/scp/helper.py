# Independent POSIX helper: framed enumeration and single-file legacy SCP staging.
# Filename bytes are hexadecimal arguments, never shell expressions or SCP header names.
import os, sys, stat, struct, ctypes, secrets, json, hashlib
out = sys.stdout.buffer
inp = sys.stdin.buffer

def emit(data):
    out.write(data)
    out.flush()

def line():
    value = inp.readline(4097)
    if len(value) > 4096 or (value and not value.endswith(b'\n')):
        raise ValueError('SCP control line exceeds its bound')
    return value.rstrip(b'\n')

def exact(length):
    value = inp.read(length)
    if len(value) != length: raise EOFError('SCP peer closed before staging completed')
    return value

def identity(info): return (info.st_dev, info.st_ino)

def fd_path(fd):
    try:
        if os.fstat(fd).st_nlink == 0: return None
        return os.readlink(b'/proc/self/fd/' + str(fd).encode())
    except OSError:
        try:
            import fcntl
            # Darwin's F_GETPATH writes MAXPATHLEN bytes. Python's fcntl
            # bytes interface rejects buffers larger than 1024 bytes.
            return fcntl.fcntl(fd, fcntl.F_GETPATH, bytes(1024)).split(b'\0', 1)[0]
        except (OSError, AttributeError, ImportError): return None

def no_replace(source_dir, source, target_dir, target):
    libc = ctypes.CDLL(None, use_errno=True)
    if sys.platform.startswith('linux') and hasattr(libc, 'renameat2'):
        result = libc.renameat2(ctypes.c_int(source_dir), ctypes.c_char_p(source), ctypes.c_int(target_dir), ctypes.c_char_p(target), ctypes.c_uint(1))
    elif sys.platform == 'darwin' and hasattr(libc, 'renameatx_np'):
        result = libc.renameatx_np(ctypes.c_int(source_dir), ctypes.c_char_p(source), ctypes.c_int(target_dir), ctypes.c_char_p(target), ctypes.c_uint(4))
    else: raise OSError('Atomic no-replace is unavailable; select explicit overwrite or SFTP')
    if result:
        code = ctypes.get_errno()
        raise OSError(code, os.strerror(code))

def fail(message):
    emit(b'\x02SCP helper: ' + str(message).encode('utf-8', 'backslashreplace').replace(b'\n', b' ')[:2048] + b'\n')

def metadata(info):
    return {'permissions': info.st_mode, 'uid': info.st_uid, 'gid': info.st_gid, 'mtime': int(info.st_mtime)}

def signature(name, parent):
    try: fd = os.open(name, os.O_RDONLY | os.O_NOFOLLOW | os.O_NONBLOCK, dir_fd=parent)
    except FileNotFoundError: return None
    with os.fdopen(fd, 'rb') as current:
        before = os.fstat(current.fileno())
        if not stat.S_ISREG(before.st_mode) or before.st_size > 2 * 1024 * 1024:
            raise ValueError('Remote edit target is not a regular file within the 2 MiB limit')
        content = current.read(2 * 1024 * 1024 + 1)
        after = os.fstat(current.fileno())
        if len(content) != before.st_size or metadata(before) != metadata(after):
            raise ValueError('Remote edit target changed while checking its version')
        return {'sha256': hashlib.sha256(content).hexdigest(), 'metadata': metadata(after)}

def get(path, editing=False):
    if exact(1) != b'\0': raise ValueError('SCP receiver did not acknowledge readiness')
    fd = os.open(path, os.O_RDONLY | os.O_NOFOLLOW | os.O_NONBLOCK)
    before = os.fstat(fd)
    if stat.S_ISDIR(before.st_mode) and not editing: return get_tree(path, fd, True)
    with os.fdopen(fd, 'rb') as source:
        before = os.fstat(source.fileno())
        if not stat.S_ISREG(before.st_mode): raise ValueError('Select a regular file for SCP transfer')
        emit(('C%04o %d payload\n' % (stat.S_IMODE(before.st_mode) & 0o777, before.st_size)).encode())
        if editing: emit(b'TCMETA:' + json.dumps(metadata(before)).encode().hex().encode() + b'\n')
        if exact(1) != b'\0': raise ValueError('SCP receiver rejected the header')
        remaining = before.st_size
        while remaining:
            block = source.read(min(32768, remaining))
            if not block: raise ValueError('Source became shorter while transferring')
            emit(block)
            remaining -= len(block)
        # Virtual files can have st_size == 0 while still returning bytes.
        if source.read(1): raise ValueError('Readable data exceeds advertised source size; destination was not committed')
        after = os.fstat(source.fileno())
        if (before.st_size, before.st_mtime_ns, metadata(before)) != (after.st_size, after.st_mtime_ns, metadata(after)):
            raise ValueError('Source changed while transferring')
    emit(b'\0')
    if exact(1) != b'\0': raise ValueError('SCP receiver rejected the staged data')
    emit(b'TCOK\n')

def put(path, policy, expected=None, folder=False):
    if not os.path.isabs(path): raise ValueError('SCP uploads require an absolute target')
    parent_path, name = os.path.split(path)
    if not name or name in (b'.', b'..'): raise ValueError('Invalid SCP target basename')
    parent = os.open(parent_path, os.O_RDONLY | os.O_DIRECTORY | os.O_NOFOLLOW)
    parent_id = identity(os.fstat(parent))
    directory_name = b'.termcraft-scp-' + secrets.token_hex(16).encode()
    directory = None
    payload = None
    tree = None
    bulk_finished = False
    retain = False
    published = False
    announced = False
    response = None
    try:
        try: existing = os.stat(name, dir_fd=parent, follow_symlinks=False)
        except FileNotFoundError: existing = None
        if expected is not None:
            if signature(name, parent) != expected: emit(b'\x01TC_CONFLICT\n'); return
            if existing is None or existing.st_mode & 0o7000: raise ValueError('Atomic editing cannot preserve special permission bits')
        if existing is not None:
            if policy != 'overwrite': emit(b'\x01TC_CONFLICT\n'); return
            if folder and not stat.S_ISDIR(existing.st_mode): raise ValueError('Folder target must be a directory, without following links')
            if not folder and not stat.S_ISREG(existing.st_mode): raise ValueError('Refusing to overwrite a non-regular SCP target')
        os.mkdir(directory_name, 0o700, dir_fd=parent)
        directory = os.open(directory_name, os.O_RDONLY | os.O_DIRECTORY | os.O_NOFOLLOW, dir_fd=parent)
        directory_id = identity(os.fstat(directory))
        # Check exclusive-publication support on this filesystem before receiving file data.
        if policy != 'overwrite':
            probe = os.open(b'probe-from', os.O_WRONLY | os.O_CREAT | os.O_EXCL, 0o600, dir_fd=directory)
            os.close(probe)
            try: no_replace(directory, b'probe-from', directory, b'probe-to')
            finally:
                for item in (b'probe-from', b'probe-to'):
                    try: os.unlink(item, dir_fd=directory)
                    except FileNotFoundError: pass
        payload = os.open(b'payload', os.O_RDWR | os.O_CREAT | os.O_EXCL | os.O_NOFOLLOW, 0o600, dir_fd=directory)
        payload_id = identity(os.fstat(payload))
        emit(b'\0TCSTAGING:' + (fd_path(payload) or b'').hex().encode() + b'\n')
        header = line().split(b' ')
        if len(header) != 3 or len(header[0]) != 5 or header[0][0:1] != b'C' or header[2] != b'payload':
            raise ValueError('Invalid SCP payload header')
        mode = int(header[0][1:], 8) & 0o777
        size = int(header[1])
        if size < 0 or size > (1 << 63) - 1: raise ValueError('Invalid SCP size')
        emit(b'\0')
        remaining = size
        retain = True
        while remaining:
            block = exact(min(32768, remaining))
            offset = 0
            while offset < len(block): offset += os.write(payload, block[offset:])
            remaining -= len(block)
        if exact(1) != b'\0': raise ValueError('SCP sender rejected its source')
        os.fsync(payload)
        if folder:
            os.mkdir(b'tree', 0o700, dir_fd=directory)
            tree = os.open(b'tree', os.O_RDONLY | os.O_DIRECTORY | os.O_NOFOLLOW, dir_fd=directory)
            tree_id = identity(os.fstat(tree))
            with os.fdopen(os.dup(payload), 'rb') as archive:
                archive.seek(0)
                with Preparing() as stopped: tree_modes = tree_read(archive, tree, stopped)
            if existing is None: tree_finalize(tree, tree_modes)
        recovery = fd_path(directory if folder else payload)
        emit(b'\0TCSTAGED:' + (recovery or b'').hex().encode() + b'\n')
        announced = True
        if folder:
            emit(b'TC_BULK\n' if existing is not None else b'TC_SINGLE\n')
            if existing is not None:
                bulk_finished = merge_tree(tree, parent, name, parent_path, parent_id, directory, directory_name, directory_id, payload, payload_id, tree_modes, identity(existing))
                return
        # EOF is cancellation. Completed SCP data alone never grants publication.
        if line() != b'TC_COMMIT': return
        if identity(os.stat(parent_path, follow_symlinks=False)) != parent_id:
            raise ValueError('Selected destination directory moved or changed; staged bytes are retained in the original directory')
        if identity(os.stat(directory_name, dir_fd=parent, follow_symlinks=False)) != directory_id:
            raise ValueError('Owned staging directory changed; no publication or unrelated cleanup attempted')
        if identity(os.stat(b'payload', dir_fd=directory, follow_symlinks=False)) != payload_id:
            raise ValueError('Owned staging file changed; no publication attempted')
        if expected is not None:
            if signature(name, parent) != expected:
                emit(b'TC_CONFLICT:' + (fd_path(payload) or b'').hex().encode() + b'\n')
                return
            live = os.stat(name, dir_fd=parent, follow_symlinks=False)
            staged = os.fstat(payload)
            if (staged.st_uid, staged.st_gid) != (live.st_uid, live.st_gid): os.fchown(payload, live.st_uid, live.st_gid)
            os.fchmod(payload, stat.S_IMODE(live.st_mode) & 0o777)
            staged = os.fstat(payload)
            if (staged.st_uid, staged.st_gid, staged.st_mode & 0o7777) != (live.st_uid, live.st_gid, live.st_mode & 0o7777):
                raise ValueError('Cannot preserve remote file security metadata')
            os.fsync(payload)
        elif not folder:
            os.fchmod(payload, (stat.S_IMODE(existing.st_mode) & 0o777) if existing is not None else mode)
        if folder and identity(os.stat(b'tree', dir_fd=directory, follow_symlinks=False)) != tree_id:
            raise ValueError('Owned folder staging root changed; no publication attempted')
        publish_name = b'tree' if folder else b'payload'
        try:
            if policy == 'overwrite': os.replace(publish_name, name, src_dir_fd=directory, dst_dir_fd=parent)
            else: no_replace(directory, publish_name, parent, name)
        except FileExistsError:
            emit(b'TC_CONFLICT:' + (fd_path(payload) or b'').hex().encode() + b'\n')
            return
        published = True
        retain = False
        response = b'TCOK\n'
        saved_metadata = metadata(os.fstat(payload)) if expected is not None else None
    except Exception as error:
        recovery = fd_path(payload) if payload is not None and retain else None
        if payload is not None:
            emit((b'' if announced else b'\x02') + b'TC_FAILED:' + (recovery or b'').hex().encode() + b':' + str(error).encode('utf-8', 'backslashreplace')[:1024].hex().encode() + b'\n')
        else: fail(error)
    finally:
        if published and folder:
            try:
                if identity(os.stat(b'payload', dir_fd=directory, follow_symlinks=False)) == payload_id: os.unlink(b'payload', dir_fd=directory)
            except OSError as error: response = b'TCWARN:' + str(error).encode().hex().encode() + b'\n'
        if tree is not None: os.close(tree)
        if payload is not None: os.close(payload)
        if directory is not None and not bulk_finished:
            # Only remove an owned, identity-matching payload. Never recursively remove a path.
            if not retain and not published:
                try:
                    if identity(os.stat(b'payload', dir_fd=directory, follow_symlinks=False)) == payload_id:
                        os.unlink(b'payload', dir_fd=directory)
                except (FileNotFoundError, UnboundLocalError): pass
            if not retain:
                try:
                    if identity(os.stat(directory_name, dir_fd=parent, follow_symlinks=False)) == directory_id:
                        os.rmdir(directory_name, dir_fd=parent)
                except OSError as error:
                    if published: response = b'TCWARN:' + ('Published; owned staging-directory cleanup failed: ' + repr(fd_path(directory)) + ': ' + str(error)).encode('utf-8', 'backslashreplace').hex().encode() + b'\n'
        if directory is not None: os.close(directory)
        os.close(parent)
    if response is not None:
        if expected is not None and published:
            warning = bytes.fromhex(response[7:].strip().decode()).decode('utf-8', 'replace') if response.startswith(b'TCWARN:') else None
            emit(b'TC_EDITOR_SAVED:' + json.dumps({'metadata': saved_metadata, 'warning': warning}).encode().hex().encode() + b'\n')
        else: emit(response)

def parent_handle(path):
    if not os.path.isabs(path): raise ValueError('Remote mutations require absolute paths')
    parent_path, name = os.path.split(path)
    if not name or name in (b'.', b'..'): raise ValueError('Refusing root or navigation entry mutation')
    fd = os.open(parent_path, os.O_RDONLY | os.O_DIRECTORY | os.O_NOFOLLOW)
    return fd, parent_path, name, identity(os.fstat(fd))

def control(operation, path, policy, other=None):
    parent, parent_path, name, parent_id = parent_handle(path)
    target = None
    response = None
    try:
        current = None
        if operation != 'mkdir': current = os.stat(name, dir_fd=parent, follow_symlinks=False)
        if operation == 'rename':
            target, target_path, target_name, target_id = parent_handle(other)
            try: exists = os.stat(target_name, dir_fd=target, follow_symlinks=False)
            except FileNotFoundError: exists = None
            if exists is not None and policy != 'overwrite': emit(b'TC_CONFLICT\n'); return
        emit(b'TC_READY\n')
        if line() != b'TC_COMMIT': return
        try:
            if identity(os.stat(parent_path, follow_symlinks=False)) != parent_id:
                raise ValueError('Selected parent directory moved or changed; mutation stopped')
            if current is not None and identity(os.stat(name, dir_fd=parent, follow_symlinks=False)) != identity(current):
                raise ValueError('Selected source changed; refresh and confirm again')
            if operation == 'mkdir': os.mkdir(name, 0o755, dir_fd=parent)
            elif operation == 'remove':
                if stat.S_ISDIR(current.st_mode): os.rmdir(name, dir_fd=parent)
                else: os.unlink(name, dir_fd=parent)
            elif operation == 'rename':
                if identity(os.stat(target_path, follow_symlinks=False)) != target_id:
                    raise ValueError('Rename destination directory moved or changed')
                if policy == 'overwrite': os.replace(name, target_name, src_dir_fd=parent, dst_dir_fd=target)
                else: no_replace(parent, name, target, target_name)
            else: raise ValueError('Unsupported remote mutation')
            response = b'TCOK\n'
        except FileExistsError:
            response = b'TC_CONFLICT\n'
        except Exception as error:
            response = b'TC_FAILED::' + str(error).encode('utf-8', 'backslashreplace')[:1024].hex().encode() + b'\n'
    finally:
        os.close(parent)
        if target is not None: os.close(target)
    if response is not None: emit(response)

# Application folder payload. All paths are raw bytes and all filesystem operations are dir-fd relative.
def tree_parts(path):
    parts = path.split(b'/')
    if not path or len(path) > 32768 or b'\0' in path or len(parts) > 64 or any(p in (b'', b'.', b'..') for p in parts):
        raise ValueError('Unsafe or oversized folder entry path')
    return parts

def tree_link(value):
    if not value or len(value) > 32768 or value.startswith(b'/') or b'\0' in value or b'\\' in value or b':' in value or b'..' in value.split(b'/'):
        raise ValueError('Folder link must stay inside selected tree')

def tree_parent(root, parts):
    fd = os.dup(root)
    try:
        for part in parts:
            child = os.open(part, os.O_RDONLY | os.O_DIRECTORY | os.O_NOFOLLOW, dir_fd=fd)
            os.close(fd)
            fd = child
        return fd
    except BaseException:
        os.close(fd)
        raise

class TreeBudget:
    def __init__(self): self.count, self.paths = 0, 0
    def add(self, path):
        tree_parts(path)
        self.count += 1
        self.paths += len(path)
        if self.count > 10000 or self.paths > 4 * 1024 * 1024:
            raise ValueError('Folder exceeds 10000 entries / 4 MiB path budget')

def tree_write(root, output, stopped):
    budget = TreeBudget()
    output.write(b'TCAR1\0' + struct.pack('>I', os.fstat(root).st_mode & 0o777))
    def walk(directory, prefix):
        with os.scandir(directory) as entries:
            for entry in entries:
                if stopped.is_set(): raise EOFError('Folder peer disconnected during preparation')
                name = os.fsencode(entry.name)
                path = prefix + (b'/' if prefix else b'') + name
                budget.add(path)
                before = os.stat(name, dir_fd=directory, follow_symlinks=False)
                if stat.S_ISLNK(before.st_mode):
                    value = os.readlink(name, dir_fd=directory)
                    if isinstance(value, str): value = os.fsencode(value)
                    tree_link(value)
                    after = os.stat(name, dir_fd=directory, follow_symlinks=False)
                    if (identity(before), before.st_mtime_ns) != (identity(after), after.st_mtime_ns): raise ValueError('Folder source link changed')
                    output.write(struct.pack('>BIIQ', 3, 0o777, len(path), len(value)) + path + value)
                    continue
                fd = os.open(name, os.O_RDONLY | os.O_NOFOLLOW | os.O_NONBLOCK, dir_fd=directory)
                opened = os.fstat(fd)
                if (identity(opened), opened.st_mode) != (identity(before), before.st_mode):
                    os.close(fd)
                    raise ValueError('Folder source changed')
                if stat.S_ISDIR(opened.st_mode):
                    try:
                        output.write(struct.pack('>BIIQ', 1, opened.st_mode & 0o777, len(path), 0) + path)
                        walk(fd, path)
                    finally: os.close(fd)
                    continue
                with os.fdopen(fd, 'rb') as source:
                    opened = os.fstat(source.fileno())
                    if (identity(opened), opened.st_mode) != (identity(before), before.st_mode): raise ValueError('Folder source changed')
                    if stat.S_ISDIR(opened.st_mode):
                        output.write(struct.pack('>BIIQ', 1, opened.st_mode & 0o777, len(path), 0) + path)
                        walk(source.fileno(), path)
                    elif stat.S_ISREG(opened.st_mode):
                        output.write(struct.pack('>BIIQ', 2, opened.st_mode & 0o777, len(path), opened.st_size) + path)
                        remaining = opened.st_size
                        while remaining:
                            if stopped.is_set(): raise EOFError('Folder preparation cancelled')
                            block = source.read(min(32768, remaining))
                            if not block: raise ValueError('Folder source became shorter')
                            output.write(block)
                            remaining -= len(block)
                        if source.read(1): raise ValueError('Folder source exceeds its advertised size')
                        after = os.fstat(source.fileno())
                        if (opened.st_size, opened.st_mtime_ns, opened.st_mode) != (after.st_size, after.st_mtime_ns, after.st_mode): raise ValueError('Folder source changed while reading')
                    else: raise ValueError('Folder contains unsupported special entry')
    walk(root, b'')
    output.write(b'\0')
    output.flush()
    os.fsync(output.fileno())
    output.seek(0)

def tree_read(source, root, stopped):
    def read(n):
        value = source.read(n)
        if len(value) != n: raise ValueError('Truncated folder payload')
        return value
    if read(6) != b'TCAR1\0': raise ValueError('Unsupported folder payload')
    root_mode = struct.unpack('>I', read(4))[0] & 0o777
    budget, seen, modes = TreeBudget(), set(), []
    while True:
        if stopped.is_set(): raise EOFError('Folder extraction cancelled')
        kind = read(1)[0]
        if kind == 0:
            if source.read(1): raise ValueError('Trailing folder payload')
            break
        mode, length, size = struct.unpack('>IIQ', read(16))
        if length == 0 or length > 32768: raise ValueError('Folder path exceeds 32 KiB')
        path = read(length)
        budget.add(path)
        if path in seen: raise ValueError('Duplicate folder entry')
        seen.add(path)
        parts = tree_parts(path)
        parent = tree_parent(root, parts[:-1])
        try:
            if kind == 1 and size == 0:
                os.mkdir(parts[-1], 0o700, dir_fd=parent)
                modes.append((path, mode & 0o777, identity(os.stat(parts[-1], dir_fd=parent, follow_symlinks=False))))
            elif kind == 2:
                fd = os.open(parts[-1], os.O_WRONLY | os.O_CREAT | os.O_EXCL | os.O_NOFOLLOW, 0o600, dir_fd=parent)
                with os.fdopen(fd, 'wb') as target:
                    remaining = size
                    while remaining:
                        if stopped.is_set(): raise EOFError('Folder extraction cancelled')
                        block = read(min(32768, remaining))
                        target.write(block)
                        remaining -= len(block)
                    target.flush()
                    os.fchmod(target.fileno(), mode & 0o777)
                    os.fsync(target.fileno())
            elif kind == 3 and size <= 32768:
                value = read(size)
                tree_link(value)
                os.symlink(value, parts[-1], dir_fd=parent)
            else: raise ValueError('Unsupported folder payload entry')
        finally: os.close(parent)
    return root_mode, modes

class Preparing:
    def __enter__(self):
        import threading
        self.stop = threading.Event()
        self.disconnected = threading.Event()
        def run():
            while not self.stop.wait(2):
                try: emit(b'TC_PREPARING\n')
                except (BrokenPipeError, OSError): self.disconnected.set(); return
        self.thread = threading.Thread(target=run, daemon=True)
        self.thread.start()
        return self.disconnected
    def __exit__(self, *args): self.stop.set(); self.thread.join()

def get_tree(path, root=None, ready=False):
    import tempfile
    if not ready and exact(1) != b'\0': raise ValueError('Folder receiver not ready')
    if root is None: root = os.open(path, os.O_RDONLY | os.O_DIRECTORY | os.O_NOFOLLOW)
    try:
        with tempfile.TemporaryFile() as archive:
            with Preparing() as stopped: tree_write(root, archive, stopped)
            size = os.fstat(archive.fileno()).st_size
            emit(('D0600 %d payload\n' % size).encode())
            if exact(1) != b'\0': raise ValueError('Folder receiver rejected header')
            while True:
                block = archive.read(32768)
                if not block: break
                emit(block)
            emit(b'\0')
            if exact(1) != b'\0': raise ValueError('Folder receiver rejected data')
            emit(b'TCOK\n')
    finally: os.close(root)

def tree_manifest(root):
    budget, result, directories = TreeBudget(), [], {b'': identity(os.fstat(root))}
    def walk(fd, prefix):
        with os.scandir(fd) as entries:
            for entry in entries:
                name = os.fsencode(entry.name)
                path = prefix + (b'/' if prefix else b'') + name
                budget.add(path)
                info = os.stat(name, dir_fd=fd, follow_symlinks=False)
                if stat.S_ISDIR(info.st_mode):
                    child = os.open(name, os.O_RDONLY | os.O_DIRECTORY | os.O_NOFOLLOW, dir_fd=fd)
                    try:
                        if identity(os.fstat(child)) != identity(info): raise ValueError('Selected directory changed during preflight')
                        directories[path] = identity(info)
                        walk(child, path)
                    finally: os.close(child)
                result.append((path, identity(info), info.st_mode))
    walk(root, b'')
    return result, directories

def checked_tree_parent(root, parts, expected):
    fd = os.dup(root)
    prefix = b''
    try:
        for name in parts:
            prefix = prefix + (b'/' if prefix else b'') + name
            child = os.open(name, os.O_RDONLY | os.O_DIRECTORY | os.O_NOFOLLOW, dir_fd=fd)
            os.close(fd)
            fd = child
            if identity(os.fstat(fd)) != expected[prefix]: raise ValueError('A selected directory changed; remaining operations stopped')
        return fd
    except BaseException:
        os.close(fd)
        raise

def remove_tree(path):
    parent, parent_path, name, parent_id = parent_handle(path)
    root = None
    try:
        root = os.open(name, os.O_RDONLY | os.O_DIRECTORY | os.O_NOFOLLOW, dir_fd=parent)
        root_id = identity(os.fstat(root))
        with Preparing(): manifest, directories = tree_manifest(root)
        manifest.append((b'', root_id, os.fstat(root).st_mode))
        for i, (relative, expected_id, mode) in enumerate(manifest):
            emit(b'TC_STEP:' + (b'1' if i + 1 == len(manifest) else b'0') + b'\n')
            if line() != b'TC_COMMIT': return
            selected = None
            try:
                if identity(os.stat(parent_path, follow_symlinks=False)) != parent_id or identity(os.stat(name, dir_fd=parent, follow_symlinks=False)) != root_id:
                    raise ValueError('Selected deletion root or parent changed; remaining entries preserved')
                parts = tree_parts(relative) if relative else [name]
                selected = checked_tree_parent(root, parts[:-1], directories) if relative else os.dup(parent)
                live = os.stat(parts[-1], dir_fd=selected, follow_symlinks=False)
                if identity(live) != expected_id or live.st_mode != mode: raise ValueError('Selected deletion entry changed; refresh and confirm again')
                if stat.S_ISDIR(mode): os.rmdir(parts[-1], dir_fd=selected)
                else: os.unlink(parts[-1], dir_fd=selected)
            except Exception as error:
                emit(b'TC_FAILED::' + str(error).encode('utf-8', 'backslashreplace')[:1024].hex().encode() + b'\n')
                return
            finally:
                if selected is not None: os.close(selected)
            emit(b'TC_DONE\n')
    finally:
        if root is not None: os.close(root)
        os.close(parent)

def tree_finalize(root, modes):
    root_mode, directories = modes
    for path, mode, expected in reversed(directories):
        directory = tree_parent(root, tree_parts(path))
        try:
            if identity(os.fstat(directory)) != expected: raise ValueError('Owned staged directory changed')
            os.fchmod(directory, mode)
        finally: os.close(directory)
    os.fchmod(root, root_mode)

def merge_tree(source, target_parent, target_name, parent_path, parent_id, directory, directory_name, directory_id, payload, payload_id, modes, original_target_id):
    target = os.open(target_name, os.O_RDONLY | os.O_DIRECTORY | os.O_NOFOLLOW, dir_fd=target_parent)
    target_id = identity(os.fstat(target))
    recovery = fd_path(directory) or b''
    warnings = []
    try:
        if target_id != original_target_id: raise ValueError('Selected destination merge root changed during staging')
        source_manifest, source_dirs = tree_manifest(source)
        wanted_modes = {path: mode for path, mode, expected in modes[1]}
        target_dirs = {b'': target_id}
        plan = []
        # Preflight every directory collision before publishing any file.
        for relative, expected, mode in sorted(source_manifest, key=lambda item: len(tree_parts(item[0]))):
            if not stat.S_ISDIR(mode): continue
            parts = tree_parts(relative)
            parent_key = b'/'.join(parts[:-1])
            if target_dirs.get(parent_key) is None:
                target_dirs[relative] = None
                continue
            selected = checked_tree_parent(target, parts[:-1], target_dirs)
            try:
                try: info = os.stat(parts[-1], dir_fd=selected, follow_symlinks=False)
                except FileNotFoundError: info = None
                if info is not None and not stat.S_ISDIR(info.st_mode): raise ValueError('Destination folder component is a symlink or non-directory')
                target_dirs[relative] = identity(info) if info is not None else None
            finally: os.close(selected)
        original_target_dirs = {path for path, value in target_dirs.items() if value is not None}
        # Manifest is postorder: insert a directory-enter before its descendants.
        source_rows = {path: (expected, mode) for path, expected, mode in source_manifest}
        entered = set()
        for relative, expected, mode in source_manifest:
            parts = tree_parts(relative)
            for depth in range(1, len(parts) + (1 if stat.S_ISDIR(mode) else 0)):
                prefix = b'/'.join(parts[:depth])
                if prefix not in entered:
                    entered.add(prefix)
                    plan.append(('directory', prefix, source_rows[prefix][0], source_rows[prefix][1]))
            plan.append(('mode' if stat.S_ISDIR(mode) else 'entry', relative, expected, mode))
        plan.append(('finish', b'', source_dirs[b''], 0))
        for i, (operation, relative, expected, mode) in enumerate(plan):
            emit(b'TC_STEP:' + (b'1' if i + 1 == len(plan) else b'0') + b'\n')
            if line() != b'TC_COMMIT': return False
            selected_source = selected_target = None
            try:
                if identity(os.stat(parent_path, follow_symlinks=False)) != parent_id or identity(os.stat(target_name, dir_fd=target_parent, follow_symlinks=False)) != target_id:
                    raise ValueError('Destination merge root or parent changed; remaining entries preserved')
                if identity(os.stat(directory_name, dir_fd=target_parent, follow_symlinks=False)) != directory_id or identity(os.stat(b'tree', dir_fd=directory, follow_symlinks=False)) != source_dirs[b'']:
                    raise ValueError('Owned staging root changed; remaining entries retained')
                if operation == 'finish':
                    try:
                        if identity(os.stat(b'payload', dir_fd=directory, follow_symlinks=False)) != payload_id: raise ValueError('Owned archive was replaced; cleanup stopped')
                        os.rmdir(b'tree', dir_fd=directory)
                        os.unlink(b'payload', dir_fd=directory)
                        os.rmdir(directory_name, dir_fd=target_parent)
                    except Exception as error: warnings.append(str(error))
                    if warnings: emit(b'TC_DONE_WARN:' + '; '.join(warnings).encode('utf-8', 'backslashreplace')[:2048].hex().encode() + b'\n')
                    else: emit(b'TC_DONE\n')
                    return True
                parts = tree_parts(relative)
                selected_source = checked_tree_parent(source, parts[:-1], source_dirs)
                live = os.stat(parts[-1], dir_fd=selected_source, follow_symlinks=False)
                if identity(live) != expected or live.st_mode != mode: raise ValueError('Staged entry changed; publication stopped')
                selected_target = checked_tree_parent(target, parts[:-1], target_dirs)
                if operation == 'directory':
                    previous = target_dirs[relative]
                    if previous is None:
                        os.mkdir(parts[-1], 0o700, dir_fd=selected_target)
                        target_dirs[relative] = identity(os.stat(parts[-1], dir_fd=selected_target, follow_symlinks=False))
                    elif identity(os.stat(parts[-1], dir_fd=selected_target, follow_symlinks=False)) != previous: raise ValueError('Destination folder changed')
                elif operation == 'mode':
                    # Existing directory modes stay unchanged. New directory modes follow the source.
                    if relative not in original_target_dirs:
                        child = checked_tree_parent(target, parts, target_dirs)
                        try: os.fchmod(child, wanted_modes[relative])
                        finally: os.close(child)
                    try: os.rmdir(parts[-1], dir_fd=selected_source)
                    except OSError as error: warnings.append(str(error))
                else:
                    try: previous = os.stat(parts[-1], dir_fd=selected_target, follow_symlinks=False)
                    except FileNotFoundError: previous = None
                    if previous is not None and stat.S_ISDIR(previous.st_mode): raise ValueError('A file cannot overwrite a destination directory')
                    if stat.S_ISREG(mode):
                        file = os.open(parts[-1], os.O_RDONLY | os.O_NOFOLLOW | os.O_NONBLOCK, dir_fd=selected_source)
                        try:
                            if identity(os.fstat(file)) != expected: raise ValueError('Staged file changed')
                            if previous is not None and stat.S_ISREG(previous.st_mode): os.fchmod(file, previous.st_mode & 0o777)
                            os.fsync(file)
                        finally: os.close(file)
                    elif stat.S_ISLNK(mode):
                        value = os.readlink(parts[-1], dir_fd=selected_source)
                        if isinstance(value, str): value = os.fsencode(value)
                        tree_link(value)
                        probe = os.dup(selected_target)
                        try:
                            for component in [p for p in value.split(b'/') if p not in (b'', b'.')]:
                                try: info = os.stat(component, dir_fd=probe, follow_symlinks=False)
                                except FileNotFoundError: break
                                if stat.S_ISLNK(info.st_mode): raise ValueError('Cannot merge a link through a destination symlink')
                                if stat.S_ISDIR(info.st_mode):
                                    child = os.open(component, os.O_RDONLY | os.O_DIRECTORY | os.O_NOFOLLOW, dir_fd=probe)
                                    os.close(probe)
                                    probe = child
                        finally: os.close(probe)
                    else: raise ValueError('Unsupported staged entry')
                    os.replace(parts[-1], parts[-1], src_dir_fd=selected_source, dst_dir_fd=selected_target)
            except Exception as error:
                emit(b'TC_FAILED:' + (fd_path(directory) or recovery).hex().encode() + b':' + str(error).encode('utf-8', 'backslashreplace')[:1024].hex().encode() + b'\n')
                return False
            finally:
                if selected_source is not None: os.close(selected_source)
                if selected_target is not None: os.close(selected_target)
            emit(b'TC_DONE\n')
    finally: os.close(target)

def main():
    operation = sys.argv[1]
    path = bytes.fromhex(sys.argv[2])
    if not path or b'\0' in path or len(path) > 32768: raise ValueError('Invalid or oversized path')
    if os.name != 'posix': raise ValueError('This compatibility helper requires a POSIX remote environment')
    if operation == 'list':
        canonical = os.path.realpath(path)
        if len(canonical) > 32768: raise ValueError('Directory path exceeds budget')
        rows = []
        budget = len(canonical)
        with os.scandir(canonical) as entries:
            for entry in entries:
                name = entry.name
                budget += len(name) + len(canonical) + 16
                if len(rows) >= 10000 or budget > 4 * 1024 * 1024:
                    raise ValueError('Directory exceeds 10000 entries / 4 MiB; choose a narrower folder')
                info = entry.stat(follow_symlinks=False)
                kind = 1 if stat.S_ISDIR(info.st_mode) else 2 if stat.S_ISREG(info.st_mode) else 3 if stat.S_ISLNK(info.st_mode) else 4
                rows.append(struct.pack('>IBQ', len(name), kind, info.st_size) + name)
        libc = ctypes.CDLL(None, use_errno=True)
        atomic = (sys.platform.startswith('linux') and hasattr(libc, 'renameat2')) or (sys.platform == 'darwin' and hasattr(libc, 'renameatx_np'))
        files = all(operation in os.supports_dir_fd for operation in (os.open, os.mkdir, os.stat, os.unlink, os.rmdir, os.rename)) and hasattr(os, 'O_NOFOLLOW')
        folders = files and atomic and all(operation in os.supports_dir_fd for operation in (os.symlink, os.readlink)) and hasattr(os, "fchmod")
        flags = (32 if files else 0) | (64 if folders else 0) | 1 | (2 if files else 0) | (4 if atomic and files else 0) | (8 if files and hasattr(os, "fchown") else 0) | (16 if files and atomic else 0)
        emit(b'TCFS2\0' + bytes([flags]) + struct.pack('>I', len(canonical)) + canonical + struct.pack('>I', len(rows)) + b''.join(rows))
    elif operation in ('mkdir', 'remove', 'rename'): control(operation, path, sys.argv[3], bytes.fromhex(sys.argv[4]) if operation == 'rename' else None)
    elif operation == 'remove_tree': remove_tree(path)
    elif operation == 'get_tree': get_tree(path)
    elif operation in ('get', 'get_editor'): get(path, operation == 'get_editor')
    elif operation == 'put_tree': put(path, sys.argv[3], folder=True)
    elif operation == 'put': put(path, sys.argv[3])
    elif operation == 'put_editor':
        if len(sys.argv[4]) > 4096: raise ValueError('Remote version descriptor exceeds budget')
        put(path, sys.argv[3], json.loads(bytes.fromhex(sys.argv[4])))
    else: raise ValueError('Unsupported SCP helper operation')
try:
    main()
except (Exception, KeyboardInterrupt) as error:
    try: fail(error)
    except (BrokenPipeError, OSError): pass
    sys.exit(1)
