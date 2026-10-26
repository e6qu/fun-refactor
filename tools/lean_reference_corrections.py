"""Check exact ignored-wildcard sites in the retained public Lean source."""
import hashlib
import json
import re


def wildcard_sites(task_path):
    correction = json.loads(task_path.read_text())['reference_correction']
    source = (task_path.parent / correction['fixture']).read_bytes()
    assert hashlib.sha256(source).hexdigest() == correction['sha256']
    pattern = re.compile(rb'(?m)\brcases\b[^\n]*?\bwith[ \t]+(_)[ \t]*\|')
    return [{'file': correction['file'], 'span': {'start': match.start(1), 'end': match.end(1)}}
            for match in pattern.finditer(source)]


def check_snapshot(task_path, snapshot):
    correction = json.loads(task_path.read_text())['reference_correction']
    assert (snapshot / correction['file']).read_bytes() == (task_path.parent / correction['fixture']).read_bytes()
