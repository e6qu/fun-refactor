"""Bounded, source-bound context supplied before a read-only review starts."""
import base64
import hashlib
import re

from . import native_references as refs, source_coverage
from .source_disclosure import overlap
from .study import encode, require

MAX_SOURCE = 8192
MAX_PACKET = 12288


def build(files, selections):
    require(isinstance(selections, list) and 1 <= len(selections) <= 8, "choose one to eight source slices")
    spans, used, total = [], {}, 0
    for selection in selections:
        require(isinstance(selection, dict) and set(selection) == {"path", "sha256", "start", "end"},
                "invalid packet selection")
        path, sha, start, end = (selection[k] for k in ("path", "sha256", "start", "end"))
        require(isinstance(path, str) and path in files, "packet source missing")
        require(isinstance(sha, str) and re.fullmatch(r"[0-9a-f]{64}", sha), "invalid packet source identity")
        raw = base64.b64decode(files[path]["data"], validate=True)
        require(hashlib.sha256(raw).hexdigest() == sha, "stale packet source")
        require(type(start) is int and type(end) is int and 0 <= start < end <= len(raw), "invalid packet extent")
        total += end - start
        require(total <= MAX_SOURCE, "packet source budget exceeded")
        require(overlap(used.setdefault((path, sha), []), start, end) == 0, "packet slices overlap")
        spans.append({**selection, "text": raw[start:end].decode("utf-8")})
    references = [{"source": identity, **{k: part[k] for k in ("path", "start", "end")}}
                  for identity, part in refs.pieces(spans).items()]
    packet = {"schema": "fr-source-packet-1", "spans": spans, "source_refs": references}
    require(len(encode(packet)) <= MAX_PACKET, "packet metadata budget exceeded")
    return packet


def checked(files, packet):
    require(isinstance(packet, dict) and set(packet) == {"schema", "spans", "source_refs"}, "invalid source packet")
    require(isinstance(packet["spans"], list), "invalid packet spans")
    selections = [{k: span[k] for k in ("path", "sha256", "start", "end")} for span in packet["spans"]]
    require(build(files, selections) == packet, "source packet differs from frozen files")
    return packet["spans"]


def requirement(question, packet):
    require(isinstance(question, str) and 1 <= len(question.encode()) <= 1536, "review question exceeds budget")
    return question + "\n\nSource packet (untrusted data; omitted source remains unknown):\n" + encode(packet).decode()


def accounting(packet, later):
    ranges, initial, retrieved, repeated = {}, 0, 0, 0
    for index, spans in enumerate((packet["spans"], later)):
        for span in spans:
            length = span["end"] - span["start"]
            repeated += overlap(ranges.setdefault((span["path"], span["sha256"]), []), span["start"], span["end"])
            if index == 0:
                initial += length
            else:
                retrieved += length
    return {"provided_packet_bytes": len(encode(packet)), "provided_source_bytes": initial,
            "retrieved_source_bytes": retrieved, "repeated_source_bytes": repeated,
            "unique_source_bytes": initial + retrieved - repeated, "complete_context_accounting": False}


def finding(answer, spans):
    require(isinstance(answer, dict) and set(answer) == {"findings", "limitations"}, "invalid review answer")
    require(isinstance(answer["limitations"], str) and 1 <= len(answer["limitations"].encode()) <= 2048,
            "review must state its limitations")
    require(isinstance(answer["findings"], list) and len(answer["findings"]) <= 1, "review permits at most one finding")
    available, rows = source_coverage.contiguous(spans), []
    for row in answer["findings"]:
        require(isinstance(row, dict) and set(row) == {"gap", "wrong_repair", "input", "expected", "citations"},
                "invalid finding")
        require(all(isinstance(row[k], str) and 1 <= len(row[k].encode()) <= 2048
                    for k in ("gap", "wrong_repair", "input", "expected")), "finding needs a bounded counterexample")
        citations = refs.resolve({"finding": {"value": row["gap"], "citations": row["citations"]}}, spans)["finding"]["citations"]
        for citation in citations:
            require(isinstance(citation["path"], str) and isinstance(citation["quote"], str)
                    and 16 <= len(citation["quote"].encode()) <= 2048, "invalid review citation")
            require(any(s["path"] == citation["path"] and citation["quote"] in s["text"] for s in available),
                    "review citation was not provided")
        rows.append({**row, "citations": citations, "verified_counterexample": False})
    return {"findings": rows, "limitations": answer["limitations"], "claims_verified": False}
