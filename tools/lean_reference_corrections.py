"""Check unchanged source bytes and the exact independently reviewed Lean facts."""
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
    inventory = json.loads(review_path(task_path).read_text())
    for name, digest in inventory['lean_sources'].items():
        assert hashlib.sha256((snapshot / name).read_bytes()).hexdigest() == digest, name


def review_path(task_path):
    return task_path.parent / json.loads(task_path.read_text())['reader_review']


def check_review(task_path, before, after):
    path = review_path(task_path)
    inventory = json.loads(path.read_text())
    previous = inventory['previous_review']
    assert previous['path'] == 'lean-reader-review.json'
    assert hashlib.sha256((path.parent / previous['path']).read_bytes()).hexdigest() == previous['sha256']
    corpus = inventory['corpora'][json.loads(task_path.read_text())['baseline']]
    assert corpus['original_counts'] == {key: before[key] for key in ('symbols', 'references')}
    assert corpus['before']['symbols'] == before['symbols']
    assert corpus['before']['references'] == before['references'] - len(wildcard_sites(task_path))
    changes = inventory['changes']
    assert corpus['after']['symbols'] == corpus['before']['symbols'] + len(changes['added_symbols']) - len(changes['removed_symbols'])
    assert corpus['after']['references'] == corpus['before']['references'] + len(changes['added_references']) - len(changes['removed_references'])
    for key in ('symbols', 'references'):
        assert after[key] == corpus['after'][key], (key, after[key], corpus['after'][key])
    assert re.fullmatch(r'[0-9a-f]{64}', inventory['lean_facts_sha256'])
    assert after['lean_reader_review'] == {
        'facts_sha256': inventory['lean_facts_sha256'],
        'inventory_sha256': hashlib.sha256(path.read_bytes()).hexdigest()}
