"""Own one private OpenCode2 v2 server and bound every HTTP exchange."""
import base64
import http.client
import json
import os
from pathlib import Path
import secrets
import signal
import socket
import subprocess
import time

from .selected_plan import require


class OpenCode2Server:
    def __init__(self, binary, workspace, env, output, *, version, startup_seconds, call_seconds, reply_bytes):
        self.binary, self.workspace = Path(binary), Path(workspace)
        self.env, self.output = dict(env), Path(output)
        self.version, self.startup_seconds = version, startup_seconds
        self.call_seconds, self.reply_bytes = call_seconds, reply_bytes
        require(startup_seconds > 0 and call_seconds > 0 and reply_bytes > 0, 'positive protocol limits required')
        self.password = secrets.token_urlsafe(32)
        self.process = None
        self.streams = []
        self.info = None
        self.shutdown = None

    def __enter__(self):
        self.output.mkdir(parents=True, exist_ok=False)
        with socket.socket() as reservation:
            reservation.bind(('127.0.0.1', 0))
            self.port = reservation.getsockname()[1]
        self.streams = [(self.output / name).open('xb') for name in ('server.stdout', 'server.stderr')]
        self.env['OPENCODE_PASSWORD'] = self.password
        self.process = subprocess.Popen([str(self.binary), 'serve', '--hostname', '127.0.0.1', '--port', str(self.port)],
            cwd=self.workspace, env=self.env, stdin=subprocess.DEVNULL,
            stdout=self.streams[0], stderr=self.streams[1], start_new_session=True)
        deadline = time.monotonic() + self.startup_seconds
        try:
            while True:
                require(self.process.poll() is None, 'private OpenCode2 server exited during startup')
                require(time.monotonic() < deadline, 'private OpenCode2 startup exceeded its deadline')
                try:
                    self.info = self.request('GET', '/api/info', None, 200, deadline - time.monotonic())
                    break
                except (ConnectionError, OSError):
                    time.sleep(0.05)
            require(self.info['version'] == self.version, 'OpenCode2 server release differs from its pin')
            require(os.getpgid(self.info['pid']) == self.process.pid, 'connected server is outside the private process group')
            (self.output / 'server-info.json').write_text(json.dumps(self.info, indent=2) + '\n')
            return self
        except BaseException:
            self.close()
            raise

    def request(self, method, path, body, expected_status, timeout_seconds):
        require(path == '/openapi.json' or path.startswith('/api/'), 'only explicit v2 server routes are admitted')
        require(timeout_seconds > 0, 'v2 request deadline expired')
        connection = http.client.HTTPConnection('127.0.0.1', self.port, timeout=min(self.call_seconds, timeout_seconds))
        auth = base64.b64encode(('opencode:' + self.password).encode()).decode()
        payload = None if body is None else json.dumps(body, allow_nan=False).encode()
        try:
            connection.request(method, path, body=payload,
                headers={'Authorization': 'Basic ' + auth, 'Content-Type': 'application/json'})
            reply = connection.getresponse()
            raw = reply.read(self.reply_bytes + 1)
            require(len(raw) <= self.reply_bytes, 'v2 reply exceeds its byte limit')
            require(reply.status == expected_status,
                    f'{method} {path}: expected {expected_status}, got {reply.status}: {raw[:512]!r}')
            if expected_status == 204:
                require(not raw, '204 reply contains a body')
                return None
            return json.loads(raw)
        finally:
            connection.close()

    def close(self):
        if self.process is None:
            return
        forced = False
        try:
            os.killpg(self.process.pid, signal.SIGTERM)
        except ProcessLookupError:
            pass
        try:
            code = self.process.wait(timeout=3)
        except subprocess.TimeoutExpired:
            forced = True
            os.killpg(self.process.pid, signal.SIGKILL)
            code = self.process.wait(timeout=3)
        finally:
            for stream in self.streams:
                stream.close()
        self.shutdown = {'exit_code': code, 'forced': forced}
        (self.output / 'shutdown.json').write_text(json.dumps(self.shutdown) + '\n')
        self.process = None

    def __exit__(self, *_):
        self.close()
