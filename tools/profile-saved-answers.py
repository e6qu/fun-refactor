"""Locate Python storage costs using the answers retained by the project trial."""
import argparse
import cProfile
import io
import json
import os
from pathlib import Path
import pstats
import sys
import tempfile
import time
import zipfile

ROOT = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(ROOT / 'sdk/python/src'))
from fr_ir.context import DirectoryObjectStore, MemoryObjectStore, restore_stored_value, store_merkle_value


def profile(answer, store):
    timings = {}
    profiler = cProfile.Profile()
    started = time.perf_counter()
    profiler.enable()
    saved = store_merkle_value(store, answer)
    profiler.disable()
    timings['store_profiled_seconds'] = time.perf_counter() - started
    started = time.perf_counter()
    profiler.enable()
    restored = restore_stored_value(store, saved.digest)
    profiler.disable()
    timings['restore_profiled_seconds'] = time.perf_counter() - started
    assert restored == answer
    text = io.StringIO()
    pstats.Stats(profiler, stream=text).strip_dirs().sort_stats('cumulative').print_stats(25)
    return {**timings, 'objects': saved.objects, 'encoded_bytes': saved.encoded_bytes,
            'profile': text.getvalue()}


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--output', type=Path, required=True)
    args = parser.parse_args()
    assert os.environ.get('GITHUB_ACTIONS') == 'true', 'run profiling on GitHub runners'
    record = json.loads((ROOT / 'tests/agent-eval/saved-work-cost/record.json').read_text())
    results = {}
    with zipfile.ZipFile(ROOT / record['directory'] / 'measurements.zip') as archive:
        for project in ('glob', 'boltons', 'jmespath'):
            answer = json.loads(archive.read(project + '/0-saved/first/result.json'))['answer']
            with tempfile.TemporaryDirectory(prefix='fr-answer-profile-') as temporary:
                results[project] = {
                    'memory': profile(answer, MemoryObjectStore()),
                    'directory': profile(answer, DirectoryObjectStore(Path(temporary) / 'objects'))}
    args.output.parent.mkdir(parents=True, exist_ok=True)
    args.output.write_text(json.dumps(results, indent=2) + '\n')
    for project, stores in results.items():
        for name, result in stores.items():
            print(project, name, json.dumps({key: value for key, value in result.items() if key != 'profile'}))
            print(result['profile'])


if __name__ == '__main__':
    main()
