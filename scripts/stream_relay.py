"""Test-only loopback relay: pause delivery or omit one server observation.

The real server and clients keep their normal protocol, queue sizes and clocks.
Only one server frame is held; no unbounded test queue hides backpressure.
"""
import json
import socket
import struct
import threading


def exact(stream, size):
    result = bytearray()
    while len(result) < size:
        data = stream.recv(size - len(result))
        if not data:
            raise EOFError()
        result.extend(data)
    return bytes(result)


class StreamRelay:
    def __init__(self, upstream):
        self.upstream = upstream
        self.gate = threading.Event()
        self.gate.set()
        self.held = threading.Event()
        self.drop_observation = threading.Event()
        self.dropped = threading.Event()
        self.listener = socket.socket()
        self.listener.bind(('127.0.0.1', 0))
        self.listener.listen(1)
        self.address = '127.0.0.1:' + str(self.listener.getsockname()[1])
        self.sockets = [self.listener]
        self.errors = []
        self.thread = threading.Thread(target=self.run, daemon=True)
        self.thread.start()

    def run(self):
        try:
            downstream, _ = self.listener.accept()
            self.sockets.append(downstream)
            host, port = self.upstream.rsplit(':', 1)
            upstream = socket.create_connection((host, int(port)), timeout=10)
            upstream.settimeout(None)
            self.sockets.append(upstream)
            sender = threading.Thread(target=self.forward, args=(downstream, upstream), daemon=True)
            sender.start()
            # Forward the HTTP upgrade before parsing unmasked server frames.
            header = bytearray()
            while not header.endswith(b'\r\n\r\n'):
                header.extend(exact(upstream, 1))
                if len(header) > 65536:
                    raise ValueError('Oversized handshake')
            downstream.sendall(header)
            while True:
                prefix = exact(upstream, 2)
                length = prefix[1] & 127
                if prefix[1] & 128:
                    raise ValueError('Masked server frame')
                extra = b''
                if length in (126, 127):
                    extra = exact(upstream, 2 if length == 126 else 8)
                    length = struct.unpack('!H' if length == 126 else '!Q', extra)[0]
                if length > 16 * 1024 * 1024:
                    raise ValueError('Oversized test frame')
                payload = exact(upstream, length)
                if not self.gate.is_set():
                    self.held.set()
                self.gate.wait()
                if prefix[0] == 0x81 and self.drop_observation.is_set():
                    message = json.loads(payload)
                    if message.get('type') == 'update' and message['update']['body']['type'] in ('observation', 'observation_delta'):
                        self.drop_observation.clear()
                        self.dropped.set()
                        continue
                downstream.sendall(prefix + extra + payload)
        except (EOFError, OSError):
            pass
        except Exception as error:
            self.errors.append(error)
        finally:
            self.close_sockets()

    def forward(self, source, target):
        try:
            while data := source.recv(65536):
                target.sendall(data)
        except OSError:
            pass
        finally:
            self.close_sockets()

    def close_sockets(self):
        for stream in list(self.sockets):
            try:
                stream.shutdown(socket.SHUT_RDWR)
            except OSError:
                pass
            stream.close()

    def close(self):
        self.gate.set()
        self.close_sockets()
        self.thread.join(timeout=10)
        if self.thread.is_alive():
            raise AssertionError('Relay failed to stop')
        if self.errors:
            raise AssertionError(self.errors)
