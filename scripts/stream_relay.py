"""Test-only loopback relay: pause, omit an observation, or corrupt a delta.

The real server and clients keep their normal protocol, queue sizes and clocks.
Only one server frame is held, including a separately gated repair snapshot; no unbounded test queue hides backpressure.
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
    def __init__(self, upstream, receive_buffer=None, *, large_retained_base=False):
        """Relay a client to the server at `upstream`. A small `receive_buffer`
        (bytes) on the relay's server connection keeps a paused relay from
        absorbing the server's output in socket buffers, which Linux grows to
        megabytes, so the server's own queue fills instead."""
        self.large_retained_base = large_retained_base
        self.upstream = upstream
        self.receive_buffer = receive_buffer
        self.gate = threading.Event()
        self.gate.set()
        self.held = threading.Event()
        self.repair_gate = threading.Event()
        self.repair_gate.set()
        self.repair_held = threading.Event()
        self.attachments = 0
        self.repairs = 0
        self.drop_observation = threading.Event()
        self.dropped = threading.Event()
        self.overflow_delta = threading.Event()
        self.invalid_inventory = threading.Event()
        self.invalid_collection_range = threading.Event()
        self.oversized_retained_state = threading.Event()
        self.overdeep = threading.Event()
        self.corrupted = threading.Event()
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
            upstream = socket.socket()
            if self.receive_buffer:
                # Set before connecting so the advertised window starts small.
                upstream.setsockopt(socket.SOL_SOCKET, socket.SO_RCVBUF, self.receive_buffer)
            upstream.settimeout(10)
            upstream.connect((host, int(port)))
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
                message = json.loads(payload) if prefix[0] == 0x81 else None
                if message is not None and self.drop_observation.is_set():
                    if message.get('type') == 'update' and message['update']['body']['type'] in ('observation', 'observation_delta'):
                        self.drop_observation.clear()
                        self.dropped.set()
                        continue
                if message is not None and self.corrupt_observation(message):
                    payload = json.dumps(message, separators=(',', ':')).encode()
                    length = len(payload)
                    if length > 16 * 1024 * 1024:
                        raise ValueError('Corrupted test frame exceeds the frame ceiling')
                    if length < 126:
                        prefix, extra = bytes((0x81, length)), b''
                    elif length <= 65535:
                        prefix, extra = bytes((0x81, 126)), struct.pack('!H', length)
                    else:
                        prefix, extra = bytes((0x81, 127)), struct.pack('!Q', length)
                    self.corrupted.set()
                if message is not None:
                    if message.get('type') == 'snapshot':
                        if message['request_id'] == 'attach':
                            self.attachments += 1
                        else:
                            self.repairs += 1
                            if not self.repair_gate.is_set():
                                self.repair_held.set()
                            self.repair_gate.wait()
                downstream.sendall(prefix + extra + payload)
        except (EOFError, OSError):
            pass
        except Exception as error:
            self.errors.append(error)
        finally:
            self.close_sockets()

    def corrupt_observation(self, message):
        """Apply one requested test corruption while preserving its envelope."""
        if (self.large_retained_base and message.get('type') == 'snapshot'
                and message['request_id'] == 'attach'):
            message['snapshot']['state']['observation']['inventory'].append({
                'id':'18446744073709551614', 'quantity':'1', 'name':'retained test item',
                'appearance':'stone', 'identified':False, 'description':'x' * (8 * 1024 * 1024)})
            self.large_retained_base = False
            return True
        if message.get('type') != 'update':
            return False
        body = message['update']['body']
        if body['type'] not in ('observation', 'observation_delta'):
            return False
        state = body['state']
        if self.overdeep.is_set():
            ignored = None
            for _ in range(65):
                ignored = [ignored]
            state['ignored'] = ignored
        elif body['type'] != 'observation_delta':
            return False
        elif self.overflow_delta.is_set():
            state['cells']['shift']['x'] = 2147483647
        elif self.invalid_inventory.is_set():
            state['inventory'].append({'start':0, 'remove':0, 'insert':[{'id':'123', 'quantity':'0', 'name':'invalid test fixture', 'appearance':'stone', 'identified':False}]})
        elif self.oversized_retained_state.is_set():
            state['ground_items'].append({'start':0, 'remove':0, 'insert':[{
                'reachable':False, 'position':{'x':0, 'y':0, 'z':0},
                'item':{'id':'18446744073709551615', 'quantity':'1', 'name':'inserted test item',
                        'appearance':'stone', 'identified':False,
                        'description':'y' * (8 * 1024 * 1024)}}]})
        elif self.invalid_collection_range.is_set():
            state['places'].append({'start':4294967295, 'remove':1, 'insert':[]})
        else:
            return False
        self.overflow_delta.clear()
        self.invalid_inventory.clear()
        self.invalid_collection_range.clear()
        self.oversized_retained_state.clear()
        self.overdeep.clear()
        return True

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
        self.repair_gate.set()
        self.close_sockets()
        self.thread.join(timeout=10)
        if self.thread.is_alive():
            raise AssertionError('Relay failed to stop')
        if self.errors:
            raise AssertionError(self.errors)
