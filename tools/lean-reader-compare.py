"""Record exact differences in source facts; keep original benchmark baselines."""
import argparse
from collections import defaultdict
import hashlib
import json
import os
from pathlib import Path
import re
import shutil
import subprocess
import tarfile
import tempfile

ROOT = Path(__file__).resolve().parents[1]


def encoded(value):
    return json.dumps(value, sort_keys=True, ensure_ascii=False, separators=(',', ':'))


def grouped(rows, identity):
    sites = defaultdict(list)
    for row in rows:
        sites[encoded(identity(row))].append(row)
    return sites


def compare(before, after):
    assert before['sources'] == after['sources'], 'both readers must see exactly the same source bytes'
    old = grouped(before['symbols'], lambda row: row['identity'])
    new = grouped(after['symbols'], lambda row: row['identity'])
    result = {'added_symbols': [], 'removed_symbols': [], 'changed_symbols': [],
              'removed_references': [], 'added_references': [], 'changed_references': []}
    for key in sorted(old.keys() | new.keys()):
        left, right = old.get(key, []), new.get(key, [])
        count = min(len(left), len(right))
        result['removed_symbols'].extend(left[count:])
        result['added_symbols'].extend(right[count:])
        result['changed_symbols'].extend({'before': a, 'after': b} for a, b in zip(left, right) if a != b)
    def site(row):
        return {name: row[name] for name in ('file', 'span', 'name', 'kind', 'twin')}
    old_sites, new_sites = grouped(before['references'], site), grouped(after['references'], site)
    for key in sorted(old_sites.keys() | new_sites.keys()):
        left, right = old_sites.get(key, []), new_sites.get(key, [])
        count = min(len(left), len(right))
        result['removed_references'].extend(left[count:])
        result['added_references'].extend(right[count:])
        result['changed_references'].extend({'before': a, 'after': b} for a, b in zip(left, right) if a != b)
    assert len(after['references']) - len(before['references']) == len(result['added_references']) - len(result['removed_references'])
    assert len(after['symbols']) - len(before['symbols']) == len(result['added_symbols']) - len(result['removed_symbols'])
    return result


def collect(args):
    assert os.environ.get('GITHUB_ACTIONS') == 'true', 'compile reader comparisons on GitHub'
    revisions = [args.before_source, args.after_source, args.repository_revision]
    assert all(re.fullmatch('[0-9a-f]{40}', value) for value in revisions)
    args.output.mkdir(parents=True, exist_ok=False)
    driver = ROOT / 'tests/lean_reader_inventory.rs'
    subprocess.run(['git', 'fetch', '--no-tags', '--depth=1', 'origin', *revisions], cwd=ROOT, check=True, timeout=120)
    with tempfile.TemporaryDirectory(prefix='fr-reader-review-') as temporary:
        work = Path(temporary)
        archive, corpus = work / 'corpus.tar', work / 'corpus'
        corpus.mkdir()
        with archive.open('wb') as stream:
            subprocess.run(['git', 'archive', args.repository_revision], cwd=ROOT, stdout=stream, check=True, timeout=30)
        with tarfile.open(archive) as stream:
            stream.extractall(corpus, filter='data')
        facts = {}
        for label, revision in [('before', args.before_source), ('after', args.after_source)]:
            checkout = work / label
            subprocess.run(['git', 'worktree', 'add', '--detach', '--no-checkout', str(checkout), revision], cwd=ROOT, check=True, timeout=30)
            subprocess.run(['git', 'sparse-checkout', 'set', '--no-cone', '/*', '!/tests/agent-eval/results/'], cwd=checkout, check=True, timeout=60)
            shutil.copyfile(driver, checkout / 'tests/lean_reader_inventory.rs')
            with (args.output / (label + '-build.jsonl')).open('w') as stdout, (args.output / (label + '-build.stderr')).open('w') as stderr:
                subprocess.run(['cargo', 'test', '--locked', '--test', 'lean_reader_inventory', '--no-run', '--message-format=json'],
                               cwd=checkout, stdout=stdout, stderr=stderr, check=True, timeout=300)
            messages = [json.loads(line) for line in (args.output / (label + '-build.jsonl')).read_text().splitlines()]
            binaries = [row['executable'] for row in messages if row.get('reason') == 'compiler-artifact' and
                        row['target']['name'] == 'lean_reader_inventory' and row.get('executable')]
            assert len(binaries) == 1
            output = args.output / (label + '.json')
            env = dict(os.environ, FR_READER_ROOT=str(corpus), FR_READER_OUTPUT=str(output))
            subprocess.run([binaries[0], 'record_lean_reader_facts', '--exact', '--ignored', '--test-threads=1'],
                           cwd=checkout, env=env, check=True, timeout=240)
            facts[label] = json.loads(output.read_text())
            subprocess.run(['git', 'worktree', 'remove', '--force', str(checkout)], cwd=ROOT, check=True, timeout=30)
        result = {'before_source': args.before_source, 'after_source': args.after_source,
                  'repository_revision': args.repository_revision,
                  'driver_sha256': hashlib.sha256(driver.read_bytes()).hexdigest(),
                  'stats': {label: value['stats'] for label, value in facts.items()},
                  'kernel_stats': {label: value['kernel_stats'] for label, value in facts.items()},
                  'consumer_query_counts': {label: len(value['consumer_queries']) for label, value in facts.items()},
                  **compare(facts['before'], facts['after'])}
        (args.output / 'comparison.json').write_text(json.dumps(result, indent=2) + '\n')
        print(json.dumps({'stats': result['stats'], **{key: len(result[key]) for key in
              ('added_symbols', 'removed_symbols', 'added_references', 'removed_references', 'changed_references')}}))


if __name__ == '__main__':
    parser = argparse.ArgumentParser(description=__doc__)
    for name in ('before_source', 'after_source', 'repository_revision'):
        parser.add_argument('--' + name.replace('_', '-'), required=True)
    parser.add_argument('--output', type=Path, required=True)
    collect(parser.parse_args())
