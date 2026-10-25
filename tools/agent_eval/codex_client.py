"""Own one pinned Codex app-server and retain bounded native RPC replies."""
import json
import os
from pathlib import Path
import selectors
import signal
import subprocess
import time

from .selected_plan import require


class CodexServer:
    def __init__(self, binary, workspace, env, output, *, call_seconds, transcript_bytes):
        require(call_seconds > 0 and transcript_bytes > 0, 'positive Codex protocol limits required')
        self.binary, self.workspace = Path(binary), Path(workspace)
        self.env, self.output = dict(env), Path(output)
        self.call_seconds, self.transcript_bytes = call_seconds, transcript_bytes
        self.process, self.stderr, self.selector = None, None, None
        self.buffer, self.transcript, self.serial, self.bytes = b'', [], 0, 0
        self.shutdown = None

    def __enter__(self):
        self.output.mkdir(parents=True, exist_ok=False)
        self.stderr = (self.output / 'server.stderr').open('xb')
        self.process = subprocess.Popen([str(self.binary), 'app-server', '--strict-config', '--listen', 'stdio://'],
            cwd=self.workspace, env=self.env, stdin=subprocess.PIPE, stdout=subprocess.PIPE,
            stderr=self.stderr, start_new_session=True)
        self.selector = selectors.DefaultSelector()
        self.selector.register(self.process.stdout, selectors.EVENT_READ)
        try:
            self.initialized = self.request('initialize', {
                'clientInfo': {'name': 'fr_m1_control', 'version': '0.84.1'},
                'capabilities': {'experimentalApi': True}})
            self.notify('initialized', {})
            return self
        except BaseException:
            self.close()
            raise

    def notify(self, method, params):
        self.process.stdin.write((json.dumps({'method': method, 'params': params}, allow_nan=False) + '\n').encode())
        self.process.stdin.flush()

    def receive(self, deadline):
        while b'\n' not in self.buffer:
            remaining = deadline - time.monotonic()
            require(remaining > 0 and self.selector.select(remaining), 'Codex RPC deadline expired')
            block = os.read(self.process.stdout.fileno(), 65536)
            require(block, 'Codex exited before completing the RPC')
            self.bytes += len(block)
            require(self.bytes <= self.transcript_bytes, 'Codex transcript exceeds its byte limit')
            self.buffer += block
        line, self.buffer = self.buffer.split(b'\n', 1)
        value = json.loads(line)
        self.transcript.append(value)
        require(not ('method' in value and 'id' in value), 'unexpected Codex approval or client request')
        return value

    def request(self, method, params):
        self.serial += 1
        identity = self.serial
        self.process.stdin.write((json.dumps({'id': identity, 'method': method, 'params': params}, allow_nan=False) + '\n').encode())
        self.process.stdin.flush()
        deadline = time.monotonic() + self.call_seconds
        while True:
            require(time.monotonic() < deadline, 'Codex RPC deadline expired')
            value = self.receive(deadline)
            if value.get('id') == identity:
                require('error' not in value, 'Codex RPC refused: ' + json.dumps(value))
                return value['result']

    def close(self):
        if self.process is None:
            return
        self.process.stdin.close()
        forced = False
        try:
            code = self.process.wait(timeout=3)
        except subprocess.TimeoutExpired:
            forced = True
            os.killpg(self.process.pid, signal.SIGKILL)
            code = self.process.wait(timeout=3)
        finally:
            self.selector.close()
            self.process.stdout.close()
            self.stderr.close()
        self.shutdown = {'exit_code': code, 'forced': forced}
        for name, value in [('shutdown.json', self.shutdown), ('transcript.json', self.transcript)]:
            (self.output / name).write_text(json.dumps(value, indent=2, allow_nan=False) + '\n')
        self.process = None

    def __exit__(self, *_):
        self.close()
