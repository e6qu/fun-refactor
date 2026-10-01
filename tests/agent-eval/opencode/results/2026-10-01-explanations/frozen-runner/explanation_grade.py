"""Finite factual rubrics and disclosed source anchors; no application code runs."""

import hashlib
import json
from pathlib import Path
import sys


def unique(pairs):
    value = {}
    for key, item in pairs:
        if key in value:
            raise ValueError("duplicate answer key")
        value[key] = item
    return value


def grade(root, payload):
    checks = 0
    try:
        rubric, answer, spans = payload["criteria"], payload["answer"], payload["disclosed"]
        assert isinstance(rubric, dict) and 1 <= len(rubric) <= 12
        if isinstance(answer, str):
            answer = json.loads(answer, object_pairs_hook=unique)
        assert isinstance(answer, dict) and set(answer) == set(rubric)
        for name, expected in rubric.items():
            assert isinstance(expected["evidence"], list) and 1 <= len(expected["evidence"]) <= 6
            assert all(isinstance(group, list) and 1 <= len(group) <= 6 for group in expected["evidence"])
            claim = answer[name]
            assert set(claim) == {"value", "citations"}
            assert type(claim["value"]) is type(expected["value"]) and claim["value"] == expected["value"]
            checks += 1
            assert isinstance(claim["citations"], list) and 1 <= len(claim["citations"]) <= 6
            citations = []
            for citation in claim["citations"]:
                assert set(citation) == {"path", "quote"}
                path, quote = citation["path"], citation["quote"]
                assert isinstance(quote, str) and 16 <= len(quote.encode()) <= 2048
                target = (root / path).resolve()
                assert target.is_relative_to(root.resolve()) and not (root / path).is_symlink()
                raw = target.read_bytes()
                digest = hashlib.sha256(raw).hexdigest()
                # A quote must have reached the agent in one actual disclosure.
                assert any(span["path"] == path and span["sha256"] == digest and quote in span["text"] for span in spans)
                assert quote.encode() in raw
                citations.append((path, quote))
            for alternatives in expected["evidence"]:
                assert any(path == anchor["path"] and anchor["contains"] in quote
                           for anchor in alternatives for path, quote in citations)
                checks += 1
        return {"passed": True, "checks": checks,
                "scope": "All predeclared factual answers and disclosed source anchors match; no free-form semantic judgment or proof."}
    except (AssertionError, KeyError, TypeError, ValueError, OSError):
        return {"passed": False, "checks": checks,
                "scope": "A factual answer or required disclosed source anchor is missing or incorrect; no proof."}


if __name__ == "__main__":
    print(json.dumps(grade(Path(sys.argv[1]), json.load(sys.stdin, object_pairs_hook=unique))))
