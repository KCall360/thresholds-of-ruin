"""Opt-in, lossless stdout chunks with independent storage ownership.

Only fixed-size staging is bounded. Retained archives, pinned raw chunks and a
producer outrunning compression still consume storage; callers need their
existing resource guard. Nothing here drops records or throttles the producer.
"""
import codecs
import gzip
import hashlib
import json
import os
from pathlib import Path
import queue
import stat
import threading
import time


_BLOCK = 64 * 1024
MAX_SEGMENT_BYTES = 64 * 1024 * 1024
_MANIFEST = "stdout-segments.json"
_TOP = {"version", "complete", "segment_bytes", "total_bytes", "total_lines", "segments"}
_ROW = {"index", "start", "bytes", "sha256", "raw", "archive", "archive_bytes",
        "archive_sha256", "mode", "mtime_ns"}


class CaptureError(RuntimeError):
    """Capture evidence is incomplete, inconsistent or no longer exclusively owned."""


def _integer(value, minimum=0):
    return type(value) is int and value >= minimum


def _digest(value):
    return isinstance(value, str) and len(value) == 64 and all(c in "0123456789abcdef" for c in value)


def _remaining(deadline):
    return None if deadline is None else max(0.0, deadline - time.monotonic())


def _identity(info):
    return info.st_dev, info.st_ino


def _names(index):
    raw = f"stdout-segment-{index:08d}.raw"
    return raw, raw + ".gz"


def _root(path):
    path = Path(path)
    parent = path.parent.absolute()
    if (path.name != "stdout.log" or parent.resolve() != parent
            or parent.is_symlink() or not parent.is_dir()):
        raise CaptureError("Segmented capture requires an existing real diagnostics directory and stdout.log name")
    return parent


def _directory_identity(root):
    info = root.lstat()
    if root.resolve() != root or not stat.S_ISDIR(info.st_mode):
        raise CaptureError("Capture directory is no longer canonical and regular")
    return info.st_dev, info.st_ino


def _fingerprint(info):
    return (info.st_dev, info.st_ino, info.st_size, stat.S_IMODE(info.st_mode), info.st_mtime_ns)


def _regular_open(path, *, allow_growth=False, writable=False):
    """No symlink following, including a path replaced between lstat and open."""
    before = path.lstat()
    if not stat.S_ISREG(before.st_mode):
        raise CaptureError(f"Not a regular capture file: {path.name}")
    fd = os.open(path, (os.O_RDWR if writable else os.O_RDONLY)
                 | getattr(os, "O_BINARY", 0) | getattr(os, "O_NOFOLLOW", 0))
    stream = os.fdopen(fd, "r+b" if writable else "rb")
    try:
        after = os.fstat(stream.fileno())
        named = path.lstat()
        identity = (after.st_dev, after.st_ino) == (before.st_dev, before.st_ino)
        if (not stat.S_ISREG(after.st_mode) or not stat.S_ISREG(named.st_mode) or not identity
                or (named.st_dev, named.st_ino) != (after.st_dev, after.st_ino)
                or (not allow_growth and _fingerprint(after) != _fingerprint(before))):
            raise CaptureError(f"Capture file changed while opening: {path.name}")
    except BaseException:
        stream.close()
        raise
    return stream, before


def _same_file(path, stream, before):
    if (_fingerprint(os.fstat(stream.fileno())) != _fingerprint(before)
            or _fingerprint(path.lstat()) != _fingerprint(before)
            or not stat.S_ISREG(path.lstat().st_mode)):
        raise CaptureError(f"Capture file changed while reading: {path.name}")


def _exclusive_writer(path):
    fd = os.open(path, os.O_WRONLY | os.O_CREAT | os.O_EXCL | getattr(os, "O_BINARY", 0)
                 | getattr(os, "O_NOFOLLOW", 0), 0o600)
    return os.fdopen(fd, "wb")


def _sync_directory(root):
    # Windows does not expose POSIX directory descriptors through os.open.
    # File contents are fsynced on both platforms; replacement is atomic.
    if os.name == "nt":
        return
    fd = os.open(root, os.O_RDONLY | getattr(os, "O_DIRECTORY", 0))
    try:
        os.fsync(fd)
    finally:
        os.close(fd)


def _hash_stream(stream, limit=None):
    digest = hashlib.sha256()
    size = 0
    while block := stream.read(_BLOCK):
        if limit is not None and size + len(block) > limit:
            raise CaptureError("Capture data exceeds its declared bounded length")
        digest.update(block)
        size += len(block)
    return size, digest.hexdigest()


def _hash_regular(path, *, durable=False):
    stream, before = _regular_open(path, writable=durable)
    with stream:
        size, digest = _hash_stream(stream, before.st_size)
        if durable:
            os.fsync(stream.fileno())
        _same_file(path, stream, before)
    return size, digest, before


def _checked_raw(root, row):
    path = root / row["raw"]
    stream, before = _regular_open(path)
    try:
        if (before.st_size != row["bytes"] or stat.S_IMODE(before.st_mode) != row["mode"]
                or before.st_mtime_ns != row["mtime_ns"]):
            raise CaptureError("Raw segment metadata mismatch")
        if row["sha256"] is not None:
            size, digest = _hash_stream(stream, row["bytes"])
            if (size, digest) != (row["bytes"], row["sha256"]):
                raise CaptureError("Raw segment identity mismatch")
            _same_file(path, stream, before)
        stream.seek(0)
        return stream
    except BaseException:
        stream.close()
        raise


class _ArchiveReader:
    """Own both gzip and its underlying descriptor (GzipFile does not)."""
    def __init__(self, raw, decoded):
        self.raw, self.decoded = raw, decoded

    def readline(self, size):
        return self.decoded.readline(size)

    def read(self, size):
        return self.decoded.read(size)

    def seek(self, offset):
        return self.decoded.seek(offset)

    def close(self):
        try:
            self.decoded.close()
        finally:
            self.raw.close()


def _checked_archive(root, row):
    path = root / row["archive"]
    stream, before = _regular_open(path)
    decoded = None
    try:
        # This deliberately loose envelope easily accommodates this writer's
        # gzip output, while rejecting attacker-sized headers/trailers. Decode
        # is independently stopped after the exact expected raw chunk length.
        if before.st_size != row["archive_bytes"] or before.st_size > 2 * row["bytes"] + 1024:
            raise CaptureError("Archive size exceeds its chunk envelope")
        size, digest = _hash_stream(stream, row["archive_bytes"])
        if (size, digest) != (row["archive_bytes"], row["archive_sha256"]):
            raise CaptureError("Compressed segment identity mismatch")
        _same_file(path, stream, before)
        stream.seek(0)
        decoded = gzip.GzipFile(fileobj=stream, mode="rb")
        size, digest = _hash_stream(decoded, row["bytes"])
        if (size, digest) != (row["bytes"], row["sha256"]):
            raise CaptureError("Decoded segment identity mismatch")
        _same_file(path, stream, before)
        decoded.seek(0)
        return _ArchiveReader(stream, decoded)
    except BaseException:
        if decoded is not None:
            decoded.close()
        stream.close()
        raise


class _LogicalReader:
    """One pinned chunk at a time, preserving efficient sequential FIFO reads."""
    def __init__(self, owner, start=0):
        self.owner, self.position = owner, start
        self.closed = False
        self.stream = None
        self.row = None

    def _release(self):
        if self.stream is not None:
            try:
                self.stream.close()
            finally:
                self.stream = None
                self.owner._release(self.row)
                self.row = None

    def close(self):
        if not self.closed:
            self._release()
            self.closed = True

    def tell(self):
        return self.position

    def readline(self, stop, *, deadline=None):
        if self.closed:
            raise CaptureError("Read from closed capture reader")
        pieces = []
        while self.position < stop:
            if self.stream is None:
                self.row, self.stream = self.owner._acquire(self.position, deadline=deadline)
                self.stream.seek(self.position - self.row["start"])
            with self.owner.condition:
                end = self.row["start"] + self.row["bytes"]
            length = min(end, stop) - self.position
            if length <= 0:
                raise CaptureError("Published capture offset exceeds its chunk")
            piece = self.stream.readline(length)
            if not piece:
                raise CaptureError("Truncated capture chunk")
            pieces.append(piece)
            self.position += len(piece)
            # Close even if LF is the last byte: sealing no longer pins a
            # completed FIFO chunk until the consumer requests another line.
            if self.position == self.row["start"] + self.owner.segment_bytes:
                self._release()
            if piece.endswith(b"\n"):
                break
        return b"".join(pieces)


class _WriterState:
    def __init__(self, owner):
        self.owner = owner
        self.closed = False

    def close(self):
        self.owner._finish_writer()


class SegmentedOutputQueue:
    """Existing OutputQueue contract with an explicitly opted-in storage owner."""
    def __init__(self, path, segment_bytes):
        if not _integer(segment_bytes, 1) or segment_bytes > MAX_SEGMENT_BYTES:
            raise ValueError("Segment size must be an integer in 1..64 MiB")
        self.path = Path(path)
        self.root = _root(path)
        self._root_identity = _directory_identity(self.root)
        if self.path.exists() or self.path.is_symlink():
            raise CaptureError("Refusing existing stdout capture")
        if any(p.name.startswith("stdout-segment") for p in self.root.iterdir()):
            raise CaptureError("Refusing an existing segment namespace")
        self.segment_bytes = segment_bytes
        self.condition = threading.Condition()
        self._writer_lock = threading.Lock()
        self._consumer_lock = threading.Lock()
        self._rows = []
        self._file = None
        self._current = None
        self._raw_digest = None
        self._total_bytes = self._published_bytes = self._total_lines = 0
        self.offset = self.count = 0
        self.eof = False
        self._error = None
        self._stopping = self._closed = False
        self._manifest_identity = None
        self.writer = _WriterState(self)
        self.reader = _LogicalReader(self)
        self._worker = threading.Thread(target=self._compress, name="stdout-segment-compressor", daemon=True)
        self._worker.start()

    def _raise_error(self):
        if self._error is not None:
            raise CaptureError("Segmented stdout retention failed") from self._error

    def fail(self, error):
        """Keep a failed source prefix incomplete, even if its reader then ends."""
        self._fail(error)

    def _fail(self, error):
        with self.condition:
            if self._error is None:
                self._error = error
            self.condition.notify_all()

    def _check_root(self):
        if _directory_identity(self.root) != self._root_identity:
            raise CaptureError("Owned capture directory was replaced")

    def _new_chunk(self):
        self._check_root()
        index = len(self._rows)
        raw, _archive = _names(index)
        stream = _exclusive_writer(self.root / raw)
        identity = _identity(os.fstat(stream.fileno()))
        row = dict(index=index, start=self._total_bytes, bytes=0, sha256=None, raw=raw,
                   archive=None, archive_bytes=None, archive_sha256=None, mode=None, mtime_ns=None,
                   _status="open", _refs=0, _identity=identity)
        raw_digest = hashlib.sha256()
        with self.condition:
            self._rows.append(row)
            self._current, self._file = row, stream
            self._raw_digest = raw_digest

    def _seal(self):
        if self._file is None:
            return
        if _identity(os.fstat(self._file.fileno())) != self._current["_identity"]:
            raise CaptureError("Original raw writer descriptor changed identity")
        self._file.close()
        self._check_root()
        info = (self.root / self._current["raw"]).lstat()
        if (not stat.S_ISREG(info.st_mode) or info.st_size != self._current["bytes"]
                or _identity(info) != self._current["_identity"]):
            raise CaptureError("Sealed stdout chunk identity mismatch")
        digest = self._raw_digest.hexdigest()
        with self.condition:
            self._current.update(mode=stat.S_IMODE(info.st_mode), mtime_ns=info.st_mtime_ns,
                                 sha256=digest, _status="raw")
            self._file = self._current = None
            self._raw_digest = None
            self.condition.notify_all()

    def _write(self, data):
        pending = memoryview(data)
        while pending:
            if self._file is None:
                self._new_chunk()
            available = self.segment_bytes - self._current["bytes"]
            written = self._file.write(pending[:available])
            if not written:
                raise CaptureError("Short stdout chunk write")
            # Original capture authority, established by the producer before
            # any asynchronous archival. Hashing never owns the Condition.
            self._raw_digest.update(pending[:written])
            # Publish file contents before any corresponding logical offset.
            self._file.flush()
            with self.condition:
                self._current["bytes"] += written
                self._total_bytes += written
            pending = pending[written:]
            if self._current["bytes"] == self.segment_bytes:
                self._seal()

    def put(self, line):
        if line is None:
            self._finish_writer()
            return
        with self._writer_lock:
            with self.condition:
                self._raise_error()
                if self.writer.closed:
                    raise CaptureError("Write after stdout EOF")
            try:
                # Do not duplicate an arbitrarily large Unicode line merely
                # to encode it; each encode/write has bounded staging.
                for start in range(0, len(line), _BLOCK // 4):
                    self._write(line[start:start + _BLOCK // 4].encode("utf-8"))
                self._write(b"\n")
                with self.condition:
                    self._published_bytes = self._total_bytes
                    self._total_lines += 1
                    self.count += 1
                    self.condition.notify_all()
            except BaseException as error:
                self._fail(error)
                raise

    def _finish_writer(self):
        with self._writer_lock:
            if not self.writer.closed:
                try:
                    self._seal()
                except BaseException as error:
                    self._fail(error)
                    if self._file is not None:
                        self._file.close()
                finally:
                    with self.condition:
                        self.writer.closed = True
                        self.eof = True
                        self.condition.notify_all()

    def _acquire(self, position, *, deadline=None):
        self._check_root()
        index = position // self.segment_bytes
        with self.condition:
            self._raise_error()
            if index >= len(self._rows):
                raise CaptureError("Capture read exceeds published chunks")
            row = self._rows[index]
            if not self.condition.wait_for(
                    lambda: row["_status"] != "archiving" or self._error is not None,
                    _remaining(deadline)):
                raise queue.Empty
            self._raise_error()
            row["_refs"] += 1
            snapshot = {key: row[key] for key in _ROW}
            status = row["_status"]
        try:
            if status == "archived":
                stream = _checked_archive(self.root, snapshot)
            elif status == "raw":
                stream = _checked_raw(self.root, snapshot)
            else:
                stream, _before = _regular_open(self.root / row["raw"], allow_growth=True)
            if status != "archived" and _identity(os.fstat(stream.fileno())) != row["_identity"]:
                stream.close()
                raise CaptureError("Original raw capture inode was replaced")
            return row, stream
        except BaseException as error:
            self._release(row)
            self._fail(error)
            raise

    def _release(self, row):
        with self.condition:
            if row["_refs"] <= 0:
                raise CaptureError("Capture reader reference underflow")
            row["_refs"] -= 1
            self.condition.notify_all()

    def get(self, timeout):
        deadline = None if timeout is None else time.monotonic() + timeout
        acquired = (self._consumer_lock.acquire() if deadline is None
                    else self._consumer_lock.acquire(timeout=_remaining(deadline)))
        if not acquired:
            raise queue.Empty
        try:
            with self.condition:
                if not self.condition.wait_for(lambda: self.count or self.eof or self._error is not None,
                                               _remaining(deadline)):
                    raise queue.Empty
                self._raise_error()
                if not self.count:
                    return None
                stop = self._published_bytes
            try:
                raw = self.reader.readline(stop, deadline=deadline)
                if not raw.endswith(b"\n"):
                    raise CaptureError("Published stdout record is incomplete")
                line = raw[:-1].decode("utf-8")
                with self.condition:
                    self.offset = self.reader.tell()
                    self.count -= 1
                return line
            except queue.Empty:
                # A record can straddle a readable chunk and a later chunk
                # being archived. Timeout is not corruption or consumption:
                # release any pin and retry from the last committed offset.
                self.reader.close()
                self.reader = _LogicalReader(self, self.offset)
                raise
            except BaseException as error:
                self._fail(error)
                raise
        finally:
            self._consumer_lock.release()

    def iter_lines(self, start=0, stop=None, *, deadline=None):
        with self.condition:
            self._raise_error()
            if stop is None:
                stop = self._published_bytes
            if not _integer(start) or not _integer(stop) or not 0 <= start <= stop <= self._published_bytes:
                raise CaptureError("Invalid logical stdout interval")
        reader = _LogicalReader(self, start)
        try:
            while reader.position < stop:
                line = reader.readline(stop, deadline=deadline)
                if not line.endswith(b"\n"):
                    raise CaptureError("Stdout interval does not end at a record boundary")
                yield line[:-1].decode("utf-8")
        finally:
            reader.close()

    def read_between(self, start, stop, *, deadline=None):
        return "\n".join(self.iter_lines(start, stop, deadline=deadline))

    @property
    def queue(self):
        with self.condition:
            start, stop, eof = self.offset, self._published_bytes, self.eof
        return [*self.iter_lines(start, stop), *([None] if eof else [])]

    def _snapshot(self, complete=False):
        with self.condition:
            return dict(version=1, complete=complete, segment_bytes=self.segment_bytes,
                        total_bytes=self._total_bytes, total_lines=self._total_lines,
                        segments=[{key: row[key] for key in sorted(_ROW)} for row in self._rows])

    def _publish(self, complete=False):
        # Only the one compressor (or close, after joining it) publishes.
        self._check_root()
        manifest = self.root / _MANIFEST
        temporary = self.root / (_MANIFEST + ".partial")
        data = (json.dumps(self._snapshot(complete), sort_keys=True, separators=(",", ":")) + "\n").encode("utf-8")
        def check_manifest_owner():
            if manifest.exists() or manifest.is_symlink():
                info = manifest.lstat()
                if (not stat.S_ISREG(info.st_mode) or self._manifest_identity is None
                        or _fingerprint(info) != self._manifest_identity):
                    raise CaptureError("Unsafe or replaced capture manifest destination")
            elif self._manifest_identity is not None:
                raise CaptureError("Owned capture manifest disappeared")
        check_manifest_owner()
        with _exclusive_writer(temporary) as stream:
            stream.write(data)
            stream.flush()
            os.fsync(stream.fileno())
            temporary_info = os.fstat(stream.fileno())
        check_manifest_owner()
        self._check_root()
        if (_fingerprint(temporary.lstat()) != _fingerprint(temporary_info)
                or not stat.S_ISREG(temporary.lstat().st_mode)):
            raise CaptureError("Owned manifest staging inode was replaced")
        os.replace(temporary, manifest)
        if (_fingerprint(manifest.lstat()) != _fingerprint(temporary_info)
                or not stat.S_ISREG(manifest.lstat().st_mode)):
            raise CaptureError("Manifest inode changed during publication")
        _sync_directory(self.root)
        self._manifest_identity = _fingerprint(temporary_info)

    def _archive(self, row):
        self._check_root()
        raw_path = self.root / row["raw"]
        _raw_name, archive_name = _names(row["index"])
        archive_path = self.root / archive_name
        partial = self.root / (archive_name + ".partial")
        source, before = _regular_open(raw_path, writable=True)
        digest = hashlib.sha256()
        size = 0
        with source, _exclusive_writer(partial) as destination:
            if (_identity(before) != row["_identity"]
                    or before.st_size != row["bytes"] or stat.S_IMODE(before.st_mode) != row["mode"]
                    or before.st_mtime_ns != row["mtime_ns"]):
                raise CaptureError("Raw chunk changed before archival")
            os.fsync(source.fileno())
            with gzip.GzipFile(filename="", fileobj=destination, mode="wb", compresslevel=1, mtime=0) as zipped:
                while block := source.read(_BLOCK):
                    digest.update(block)
                    size += len(block)
                    zipped.write(block)
            destination.flush()
            os.fsync(destination.fileno())
            _same_file(raw_path, source, before)
        if (size, digest.hexdigest()) != (row["bytes"], row["sha256"]):
            raise CaptureError("Raw chunk differs from original captured bytes")
        archive_size, archive_sha, archive_info = _hash_regular(partial)
        trial = {**row, "archive": partial.name,
                 "archive_bytes": archive_size, "archive_sha256": archive_sha}
        verified = _checked_archive(self.root, trial)
        verified.close()
        # A hard-link publication is atomic and refuses collisions rather than
        # overwriting another file. The partial and final are our same inode.
        self._check_root()
        if _fingerprint(partial.lstat()) != _fingerprint(archive_info) or not stat.S_ISREG(partial.lstat().st_mode):
            raise CaptureError("Verified archive staging identity changed")
        os.link(partial, archive_path)
        if _fingerprint(archive_path.lstat()) != _fingerprint(archive_info) or not stat.S_ISREG(archive_path.lstat().st_mode):
            raise CaptureError("Archive identity changed during publication")
        partial.unlink()
        _sync_directory(self.root)
        with self.condition:
            row.update(archive=archive_name, archive_bytes=archive_size, archive_sha256=archive_sha)
        self._publish()
        # Durable mapping precedes the only raw unlink. Readers still wait on
        # the archiving state; no raw descriptor can have been acquired here.
        self._check_root()
        if _fingerprint(archive_path.lstat()) != _fingerprint(archive_info) or not stat.S_ISREG(archive_path.lstat().st_mode):
            raise CaptureError("Published archive identity changed before raw unlink")
        if _fingerprint(raw_path.lstat()) != _fingerprint(before) or not stat.S_ISREG(raw_path.lstat().st_mode):
            raise CaptureError("Raw capture identity changed before unlink")
        raw_path.unlink()
        _sync_directory(self.root)
        with self.condition:
            row["_status"] = "archived"
            self.condition.notify_all()

    def _compress(self):
        try:
            while True:
                with self.condition:
                    self.condition.wait_for(lambda: self._stopping or self._error is not None
                        or any(row["_status"] == "raw" and row["_refs"] == 0 for row in self._rows))
                    if self._error is not None:
                        return
                    row = next((row for row in self._rows if row["_status"] == "raw" and row["_refs"] == 0), None)
                    if row is None:
                        if self._stopping:
                            return
                        continue
                    row["_status"] = "archiving"
                self._archive(row)
        except BaseException as error:
            self._fail(error)

    def close(self):
        if self._closed:
            self._raise_error()
            return
        self._finish_writer()
        with self._consumer_lock:
            self.reader.close()
        with self.condition:
            self._stopping = True
            self.condition.notify_all()
        self._worker.join()
        try:
            self._raise_error()
            if self._published_bytes != self._total_bytes:
                raise CaptureError("Stdout stopped with an unpublished partial record")
            # A caller may retain a diagnostic generator at a yielded line.
            # Do not wait for its reference: leave verified raw bytes instead.
            self._check_root()
            for row in self._rows:
                if row["_status"] == "raw":
                    size, digest, info = _hash_regular(self.root / row["raw"], durable=True)
                    if (_identity(info) != row["_identity"]
                            or (size, digest) != (row["bytes"], row["sha256"]) or stat.S_IMODE(info.st_mode) != row["mode"]
                            or info.st_mtime_ns != row["mtime_ns"]):
                        raise CaptureError("Pinned raw segment identity changed")
                elif row["_status"] != "archived":
                    raise CaptureError("Unsettled stdout segment at close")
            self._publish(complete=True)
        except BaseException as error:
            self._fail(error)
            raise
        finally:
            self._closed = True

    def __enter__(self):
        return self

    def __exit__(self, *_error):
        self.close()


def _manifest(path):
    root = _root(path)
    stream, before = _regular_open(root / _MANIFEST)
    with stream:
        def unique(pairs):
            result = {}
            for key, value in pairs:
                if key in result:
                    raise CaptureError("Duplicate capture manifest field")
                result[key] = value
            return result
        manifest = json.load(stream, object_pairs_hook=unique)
        _same_file(root / _MANIFEST, stream, before)
    if (not isinstance(manifest, dict) or set(manifest) != _TOP
            or type(manifest["version"]) is not int or manifest["version"] != 1
            or manifest["complete"] is not True or not _integer(manifest["segment_bytes"], 1)
            or manifest["segment_bytes"] > MAX_SEGMENT_BYTES
            or not _integer(manifest["total_bytes"]) or not _integer(manifest["total_lines"])
            or not isinstance(manifest["segments"], list)):
        raise CaptureError("Unsupported or incomplete stdout capture manifest")
    offset = 0
    allowed = {_MANIFEST}
    for index, row in enumerate(manifest["segments"]):
        raw, archive = _names(index)
        if (not isinstance(row, dict) or set(row) != _ROW or not _integer(row["index"])
                or row["index"] != index or not _integer(row["start"]) or row["start"] != offset
                or not _integer(row["bytes"], 1) or row["bytes"] > manifest["segment_bytes"]
                or not _digest(row["sha256"]) or row["raw"] != raw
                or not _integer(row["mode"]) or row["mode"] > 0o7777 or not _integer(row["mtime_ns"])):
            raise CaptureError("Invalid stdout segment descriptor")
        if index + 1 < len(manifest["segments"]) and row["bytes"] != manifest["segment_bytes"]:
            raise CaptureError("Nonfinal stdout chunk is not full")
        if row["archive"] is None:
            if row["archive_bytes"] is not None or row["archive_sha256"] is not None:
                raise CaptureError("Raw-only segment claims archive metadata")
        elif (row["archive"] != archive or not _integer(row["archive_bytes"], 1)
                or not _digest(row["archive_sha256"])):
            raise CaptureError("Invalid stdout archive descriptor")
        allowed.add(raw)
        if row["archive"]:
            allowed.add(archive)
        offset += row["bytes"]
    if offset != manifest["total_bytes"]:
        raise CaptureError("Stdout manifest total does not match ordered segments")
    for child in root.iterdir():
        if child.name.startswith("stdout-segment") and child.name not in allowed:
            raise CaptureError("Unowned file in capture namespace")
    return root, manifest


def _retained_bytes(root, manifest):
    for row in manifest["segments"]:
        if row["archive"]:
            stream = _checked_archive(root, row)
        else:
            stream = _checked_raw(root, row)
        try:
            if row["archive"]:
                raw_path = root / row["raw"]
                if raw_path.exists() or raw_path.is_symlink():
                    # A verified crash prefix may have mapping plus both files.
                    raw = _checked_raw(root, row)
                    raw.close()
            while block := stream.read(_BLOCK):
                yield block
        finally:
            stream.close()


def validate_retained_capture(path):
    """Validate a complete capture without trusting any compressed metadata."""
    root, manifest = _manifest(path)
    decoder = codecs.getincrementaldecoder("utf-8")()
    size = lines = 0
    last = b""
    chunks = _retained_bytes(root, manifest)
    try:
        for block in chunks:
            decoder.decode(block)
            size += len(block)
            lines += block.count(b"\n")
            last = block[-1:]
    finally:
        chunks.close()
    decoder.decode(b"", final=True)
    if ((size, lines) != (manifest["total_bytes"], manifest["total_lines"])
            or (size and last != b"\n")):
        raise CaptureError("Retained stdout records do not match manifest totals")
    return manifest


def iter_retained_lines(path):
    """Strict source-free reopen of a complete normalized stdout transcript."""
    manifest = validate_retained_capture(path)
    root = _root(path)
    decoder = codecs.getincrementaldecoder("utf-8")()
    pending = ""
    chunks = _retained_bytes(root, manifest)
    try:
        for block in chunks:
            pieces = (pending + decoder.decode(block)).split("\n")
            pending = pieces.pop()
            yield from pieces
    finally:
        chunks.close()
    pending += decoder.decode(b"", final=True)
    if pending:
        raise CaptureError("Retained stdout ends in an incomplete record")
