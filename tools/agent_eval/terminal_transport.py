"""Bounded OpenCode server capture shared by controls and source reviews."""
import base64
import http.client
import socket
import subprocess
import threading
import time

from . import native_mcp as mcp, structured_submission as protocol
from .study import encode, require

MAX_BYTES = 1024**2


def capture(binary, root, env, body):
    """Run beneath bounded_host; retain partial files when the process stops."""
    with socket.socket() as reservation:
        reservation.bind(("127.0.0.1", 0))
        port = reservation.getsockname()[1]
    auth = "Basic " + base64.b64encode((env["OPENCODE_SERVER_USERNAME"] + ":" + env["OPENCODE_SERVER_PASSWORD"]).encode()).decode()
    deadline = time.monotonic() + 100
    def request(method, route, data=None, timeout=None):
        connection = http.client.HTTPConnection("127.0.0.1", port, timeout=min(timeout or 100, max(0.1, deadline - time.monotonic())))
        try:
            connection.request(method, route, body=encode(data) if data is not None else None,
                               headers={"Authorization": auth, "Content-Type": "application/json"})
            reply = connection.getresponse()
            raw = reply.read(MAX_BYTES + 1)
            require(reply.status == 200 and len(raw) <= MAX_BYTES, f"invalid server response: {reply.status} {raw[:256]!r}")
            return mcp.decode(raw)
        finally:
            connection.close()
    with (root / "server.stdout").open("wb") as out, (root / "server.stderr").open("wb") as err:
        child = subprocess.Popen([str(binary), "--pure", "--print-logs", "--log-level", "WARN", "serve", "--hostname", "127.0.0.1", "--port", str(port)],
                                 cwd=root, env=env, stdin=subprocess.DEVNULL, stdout=out, stderr=err)
        stream = None
        try:
            while True:
                require(child.poll() is None and time.monotonic() < deadline, "server did not start")
                try:
                    health = request("GET", "/global/health", timeout=2)
                    break
                except (ConnectionError, OSError) as error:
                    (root / "startup-error.json").write_bytes(encode({"error": str(error), "type": type(error).__name__}))
                    time.sleep(0.05)
            require(health["version"] == protocol.VERSION, "OpenCode version differs")
            session = request("POST", "/session", {"title": "Bounded terminal submission"})["id"]
            stream = http.client.HTTPConnection("127.0.0.1", port, timeout=100)
            stream.request("GET", "/event", headers={"Authorization": auth})
            incoming = stream.getresponse()
            require(incoming.status == 200, "event stream failed")
            ready, idle = threading.Event(), threading.Event()
            errors = []
            def collect():
                size = 0
                try:
                    with (root / "events.jsonl").open("xb") as retained:
                        while not idle.is_set():
                            line = incoming.readline(MAX_BYTES + 1)
                            require(line and len(line) <= MAX_BYTES, "incomplete or oversized event stream")
                            size += len(line)
                            require(size <= MAX_BYTES, "event stream exceeds budget")
                            if not line.startswith(b"data: "):
                                continue
                            event = mcp.decode(line[6:])
                            retained.write(encode(event) + b"\n")
                            retained.flush()
                            ready.set()
                            if event["type"] == "session.status" and event["properties"].get("sessionID") == session:
                                if event["properties"]["status"]["type"] == "idle":
                                    idle.set()
                except (ValueError, KeyError, OSError) as error:
                    errors.append(str(error))
                    ready.set()
                    idle.set()
            reader = threading.Thread(target=collect, daemon=True)
            reader.start()
            require(ready.wait(5) and not errors, "event stream never became ready")
            (root / "request.json").write_bytes(encode(body))
            terminal = request("POST", f"/session/{session}/message", body)
            (root / "terminal.json").write_bytes(encode(terminal))
            require(idle.wait(5) and not errors, "event stream did not finish cleanly")
            reader.join(timeout=1)
            # The pinned HTTP message encoder rejects its persisted structured format.
            # Export the unchanged database through the client after stopping the server.
            child.terminate()
            child.wait(timeout=3)
            with (root / "export.json").open("xb") as saved, (root / "export.stderr").open("wb") as errors_out:
                exported = subprocess.run([str(binary), "--pure", "export", session],
                    cwd=root, env=env, stdin=subprocess.DEVNULL, stdout=saved, stderr=errors_out,
                    timeout=max(0.1, deadline - time.monotonic()))
            require(exported.returncode == 0, "session export failed")
            require((root / "export.json").stat().st_size <= MAX_BYTES, "session export exceeds budget")
            exported = mcp.decode((root / "export.json").read_bytes())
            require(exported["info"]["id"] == session, "export session differs")
            messages = exported["messages"]
            (root / "messages.json").write_bytes(encode(messages))
            return {"opencode_version": health["version"], "session": session}
        finally:
            if stream:
                stream.close()
            child.terminate()
            try:
                child.wait(timeout=3)
            except subprocess.TimeoutExpired:
                child.kill()
                child.wait(timeout=3)
