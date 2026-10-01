"""Read-only MCP transport for frozen evaluator inputs, not a general fr server."""
from __future__ import annotations

import json
from pathlib import Path
import subprocess
import sys
import tempfile
import time

from . import opencode_rehearsal as legacy
from .study import encode, load, require
from .workspace_bundle import validate

MAX_CALLS = 24
MAX_LINE = 65536
MAX_LOG = 1024**2
PROTOCOL = "2025-06-18"


def schemas(arm, *, read_hint=True):
    def tool(name, description, properties, required=()):
        return {"name": name, "description": description,
                "inputSchema": {"type": "object", "properties": properties,
                                "required": list(required), "additionalProperties": False}}
    string = {"type": "string"}
    offset = {"type": "integer", "minimum": 0, "maximum": 1024**2}
    result = [
        tool("list_files", "List paths in the frozen source snapshot.", {}),
        tool("search_source", "Literal search: at most 20 matches, with byte offsets and source hashes.",
             {"text": {"type": "string", "minLength": 1, "maxLength": 256}}, ("text",)),
        tool("read_source", "Read UTF-8 source. Continue with next_offset and sha256; zero starts a file.",
             {"path": string, "offset": offset, "bytes": {"type": "integer", "minimum": 1, "maximum": 8192},
              "sha256": string}, ("path", "offset", "bytes", "sha256")),
    ]
    if arm == "fr":
        page = {"path": string, "cursor": string}
        result += [
            tool("fr_map", "Public fr project map: bounded directory and declaration names.", page),
            tool("fr_find", "Public fr project find: declaration names, signatures, full handles and continuation.",
                 {**page, "name": string, "contains": {"type": "boolean"}}, ("name",)),
            tool("fr_show", "Public fr project show: 4096 source bytes for a full handle; continue via offset.",
                 {"handle": string, "offset": offset}, ("handle",)),
        ]
    require(arm in {"files", "fr"}, "unknown tool arm")
    if read_hint:
        result[2]["inputSchema"]["properties"]["sha256"] = {"type": "string", "description": "Use the empty string for a first read. For continuation, copy sha256 from the preceding result."}
    return result + [tool("submit_answer", "Submit the requested claim object once. Exact keys and quotations; no notes or ellipses.",
                          {"answer": {"type": "object"}}, ("answer",))]


def decode(raw):
    def unique(pairs):
        obj = {}
        for key, value in pairs:
            require(key not in obj, "duplicate JSON key")
            obj[key] = value
        return obj
    return json.loads(raw, object_pairs_hook=unique,
                      parse_constant=lambda _: require(False, "nonfinite JSON number"))


def arguments(schema, value):
    """Validate the deliberately small schema subset before dispatch."""
    require(isinstance(value, dict), "arguments must be an object")
    require(set(value) <= set(schema["properties"]) and set(schema["required"]) <= set(value), "unexpected or missing arguments")
    types = {"string": str, "integer": int, "boolean": bool, "object": dict}
    for key, item in value.items():
        rule = schema["properties"][key]
        require(type(item) is types[rule["type"]], "invalid argument type")
        for field, test in (("minimum", lambda n: item >= n), ("maximum", lambda n: item <= n),
                            ("minLength", lambda n: len(item) >= n), ("maxLength", lambda n: len(item) <= n)):
            require(field not in rule or test(rule[field]), "argument exceeds bound")


def request(name, args):
    if name.startswith("fr_"):
        return {"action": "fr", "operation": name[3:], **args}
    return {"action": {"list_files": "list", "read_source": "read", "search_source": "search"}[name], **args}


def execute_fr(command, prompt, label):
    """Remain in the enclosing monitored group; never launch a shell or detach."""
    require(label == "fr" and not prompt, "unexpected command")
    with tempfile.TemporaryFile() as out, tempfile.TemporaryFile() as err:
        child = subprocess.Popen(command, stdin=subprocess.DEVNULL, stdout=out, stderr=err)
        deadline = time.monotonic() + 20
        try:
            while child.poll() is None:
                require(time.monotonic() < deadline, "fr deadline exceeded")
                require(out.tell() + err.tell() <= MAX_LINE, "fr stream budget exceeded")
                time.sleep(0.02)
            require(child.returncode == 0, "fr command failed")
            out.seek(0)
            raw = out.read(legacy.MAX_OUTPUT + 1)
            require(len(raw) <= legacy.MAX_OUTPUT, "fr output exceeds budget")
            return raw
        finally:
            if child.poll() is None:
                child.kill()
            child.wait(timeout=5)


class Server:
    def __init__(self, config, log, execute=execute_fr):
        require(set(config) == {"files", "arm", "binary", "workspace"}, "invalid server configuration")
        validate(config["files"], legacy.MAX_WORKSPACE)
        self.config, self.log, self.execute = config, log, execute
        self.tools = {t["name"]: t for t in schemas(config["arm"])}
        self.calls, self.written, self.finished = 0, 0, False
        self.initialized, self.ready = False, False

    def call(self, params):
        self.calls += 1
        require(self.calls <= MAX_CALLS, "tool call budget exhausted")
        try:
            require(not self.finished, "answer already submitted")
            require(set(params) <= {"name", "arguments", "_meta"}, "unexpected call fields")
            name, args = params["name"], params.get("arguments", {})
            require(name in self.tools, "tool unavailable in this arm")
            arguments(self.tools[name]["inputSchema"], args)
            if name == "submit_answer":
                require(len(encode(args)) <= legacy.MAX_OUTPUT, "answer exceeds budget")
                self.finished = True
                result = {"submitted": True}
            else:
                result = legacy.action(self.config["files"], request(name, args), self.config["arm"],
                                       Path(self.config["binary"]), Path(self.config["workspace"]),
                                       self.execute, read_only=True, materialize=False)
            require(len(encode(result)) <= legacy.MAX_OUTPUT, "tool result exceeds budget")
        except (ValueError, KeyError, TypeError, UnicodeError, OSError, subprocess.SubprocessError) as error:
            result = {"error": str(error)[:256]}
        response = {"content": [{"type": "text", "text": encode(result).decode()}], "isError": "error" in result}
        row = encode({"sequence": self.calls, "params": params, "result": result, "response": response}) + b"\n"
        require(self.written + len(row) <= MAX_LOG, "tool log budget exhausted")
        self.log.write(row)
        self.log.flush()
        self.written += len(row)
        return response

    def dispatch(self, message):
        require(isinstance(message, dict) and message.get("jsonrpc") == "2.0", "invalid JSON-RPC message")
        method = message.get("method")
        if "id" not in message:
            if method == "notifications/initialized":
                require(self.initialized, "initialize first")
                self.ready = True
            return None
        identity = message["id"]
        require(type(identity) in (str, int), "invalid request id")
        if method == "initialize":
            require(not self.initialized, "already initialized")
            require(isinstance(message.get("params", {}).get("protocolVersion"), str), "missing protocol version")
            self.initialized = True
            result = {"protocolVersion": PROTOCOL, "capabilities": {"tools": {}},
                      "serverInfo": {"name": "fr-evaluator", "version": "1"}}
        elif method == "ping":
            result = {}
        elif method in {"tools/list", "tools/call"}:
            require(self.ready, "initialize first")
            result = {"tools": list(self.tools.values())} if method == "tools/list" else self.call(message.get("params", {}))
        else:
            return {"jsonrpc": "2.0", "id": identity, "error": {"code": -32601, "message": "method not found"}}
        return {"jsonrpc": "2.0", "id": identity, "result": result}


def serve(config, log, source, sink):
    server = Server(config, log)
    # Bound even malformed messages and notifications; EOF ends the process.
    for _ in range(128):
        line = source.readline(MAX_LINE + 1)
        if not line:
            return
        require(len(line) <= MAX_LINE and line.endswith(b"\n"), "oversized or unfinished MCP line")
        message = decode(line)
        response = server.dispatch(message)
        if response is not None:
            sink.write(encode(response) + b"\n")
            sink.flush()
    raise ValueError("MCP message budget exhausted")


def main():
    config, destination = sys.argv[1:]
    with Path(destination).open("xb") as log:
        serve(load(Path(config)), log, sys.stdin.buffer, sys.stdout.buffer)


if __name__ == "__main__":
    main()
