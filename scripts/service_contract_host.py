"""Thin adapter to the explicitly built production Service test host.

No build fallback, scenario language, protocol normalization or client model.
"""
import hashlib
import json
from pathlib import Path
import queue
import re
import subprocess
import tempfile
import threading


MAX_INPUT_BYTES = 1024 * 1024
MAX_OUTPUT_BYTES = 2 * 1024 * 1024


def current_protocol():
    source = Path(__file__).resolve().parent.parent / "crates/protocol/src/wire.rs"
    return int(re.search(r"pub const PROTOCOL_VERSION: u32 = (\d+);", source.read_text(encoding="utf-8"))[1])


class HostError(RuntimeError):
    """Host setup/adapter failure, distinct from production protocol rejection."""


class ServiceHost:
    def __init__(self, executable, *, expected_sha256, expected_protocol=None, timeout=30):
        self.executable = Path(executable).resolve(strict=True)
        self.binary_sha256 = hashlib.sha256(self.executable.read_bytes()).hexdigest()
        if self.binary_sha256 != expected_sha256:
            raise HostError("Service host binary SHA-256 differs from the caller-provided artifact identity")
        expected_protocol = current_protocol() if expected_protocol is None else expected_protocol
        self.timeout = timeout
        self._closed = False
        self._stopped = threading.Event()
        self._responses = queue.Queue(maxsize=2)
        self._stderr = tempfile.TemporaryFile(mode="w+b")
        try:
            self.process = subprocess.Popen([str(self.executable)], stdin=subprocess.PIPE,
                stdout=subprocess.PIPE, stderr=self._stderr, text=True, encoding="utf-8", bufsize=1)
        except BaseException:
            self._stderr.close()
            raise
        self._reader = threading.Thread(target=self._read, daemon=True)
        self._reader.start()
        try:
            self.identity = self._receive()
            if (self.identity.get("host_version") != 1
                    or self.identity.get("feature") != "service-contract-host"
                    or self.identity.get("protocol") != expected_protocol):
                raise HostError(f"Unsupported Service host identity: {self.identity}; expected protocol {expected_protocol}")
        except BaseException:
            self.close()
            raise

    def _publish(self, value):
        while not self._stopped.is_set():
            try:
                self._responses.put(value, timeout=.01)
                return
            except queue.Full:
                continue

    def _read(self):
        try:
            while not self._stopped.is_set():
                line = self.process.stdout.readline(MAX_OUTPUT_BYTES + 2)
                if not line:
                    self._publish(None)
                    return
                if len(line.encode("utf-8")) > MAX_OUTPUT_BYTES + 1 or not line.endswith("\n"):
                    self._publish(HostError("Service host response exceeds its bound or is incomplete"))
                    return
                self._publish(line)
        except (OSError, ValueError) as error:
            self._publish(HostError(f"Cannot read Service host response: {error}"))

    def _failed(self, message):
        self.close()
        detail = f"; stderr: {self.stderr[-2000:]}" if self.stderr else ""
        return HostError(f"{message}; exit={self.process.returncode}{detail}")

    def _receive(self):
        try:
            line = self._responses.get(timeout=self.timeout)
        except queue.Empty as error:
            raise self._failed("Service host response timed out; host was stopped") from error
        if line is None:
            raise self._failed("Service host closed its output before responding")
        if isinstance(line, HostError):
            raise self._failed(str(line))
        try:
            value = json.loads(line)
        except json.JSONDecodeError as error:
            raise self._failed(f"Invalid Service host JSON: {line[:200]!r}") from error
        if not isinstance(value, dict):
            raise self._failed("Invalid Service host envelope")
        return value

    def raw(self, line):
        """Send one raw host envelope for adapter validation; never retry."""
        if self._closed:
            raise HostError("Service host is closed")
        if "\n" in line or "\r" in line:
            raise ValueError("A host operation must occupy exactly one line")
        if len(line.encode("utf-8")) + 1 > MAX_INPUT_BYTES:
            raise HostError("Service host input exceeds 1 MiB")
        expired = threading.Event()
        watchdog = self._watchdog(expired)
        try:
            try:
                self.process.stdin.write(line + "\n")
                self.process.stdin.flush()
            except (OSError, ValueError) as error:
                message = ("Service host operation timed out during write" if expired.is_set()
                           else f"Cannot write to Service host: {error}")
                raise self._failed(message) from error
            response = self._receive()
        finally:
            watchdog.cancel()
            watchdog.join()
        if expired.is_set():
            raise self._failed("Service host operation timed out")
        return response

    def _watchdog(self, expired):
        # A pipe write/close can block before a response timeout starts. Killing
        # the child releases the pipe; no helper writer thread is left behind.
        def stop():
            expired.set()
            try:
                self.process.kill()
            except ProcessLookupError:
                pass
        watchdog = threading.Timer(self.timeout, stop)
        watchdog.daemon = True
        watchdog.start()
        return watchdog

    def call(self, op, **fields):
        response = self.raw(json.dumps({"op": op, **fields}))
        if set(response) == {"error"}:
            raise HostError(response["error"])
        if set(response) != {"ok"} or not isinstance(response["ok"], dict):
            raise self._failed(f"Invalid Service host response: {response!r}")
        return response["ok"]

    def request(self, *, case, connection, request_id, request):
        message = json.dumps({"type": "request", "request_id": request_id, "request": request})
        return self.call("request", case=case, connection=connection, message=message)

    def close(self):
        if self._closed:
            return
        self._closed = True
        self._stopped.set()
        watchdog = self._watchdog(threading.Event())
        try:
            try:
                self.process.stdin.close()
            except (OSError, ValueError):
                pass  # Broken pipe must not prevent reaping a failed child.
            try:
                self.process.wait(timeout=self.timeout)
            except subprocess.TimeoutExpired:
                self.process.kill()
                self.process.wait(timeout=self.timeout)
        finally:
            watchdog.cancel()
            watchdog.join()
        self._reader.join(timeout=max(1, self.timeout))
        self.process.stdout.close()
        self._stderr.seek(0)
        self.stderr = self._stderr.read().decode("utf-8", errors="replace")
        self._stderr.close()

    def __enter__(self):
        return self

    def __exit__(self, *_):
        self.close()
