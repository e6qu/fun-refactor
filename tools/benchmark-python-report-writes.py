#!/usr/bin/env python3
"""Compare report saving alone using the same retained rows on one Linux runner."""
import argparse
import hashlib
import importlib.util
import json
import os
from pathlib import Path
import subprocess
import tempfile
import time

ROOT = Path(__file__).resolve().parents[1]
spec = importlib.util.spec_from_file_location('contract', ROOT / 'tools/python-contract.py')
contract = importlib.util.module_from_spec(spec)
spec.loader.exec_module(contract)


def written_bytes():
    # Linux counts bytes passed to write syscalls, including overwritten bytes.
    # This is application output, not physical device traffic or storage wear.
    counters = dict(line.split(': ', 1) for line in Path('/proc/self/io').read_text().splitlines())
    return int(counters['wchar'])


def compare(report, directory):
    results = {}
    for name in ('rewrite_all_previous_rows', 'append_new_row'):
        path = directory / (name + '.json')
        value = {key: item for key, item in report.items() if key != 'cases'}
        value.update(cases=[], summary={'contract_complete': False})
        before = written_bytes()
        cpu_start, start = time.process_time(), time.perf_counter()
        if name == 'rewrite_all_previous_rows':
            # The previous collector called this unchanged full-report saver
            # after every row. Include its final save in both measurements.
            for row in report['cases']:
                value['cases'].append(row)
                contract.save(value, path)
        else:
            with contract.collecting_report(value, path) as append:
                for row in report['cases']:
                    value['cases'].append(row)
                    append(row)
        value['summary'] = report['summary']
        contract.save(value, path)
        elapsed, cpu = time.perf_counter() - start, time.process_time() - cpu_start
        written = written_bytes() - before
        data = path.read_bytes()
        contract.require(json.loads(data) == report, name + ': saved observations changed')
        results[name] = {'wall_seconds': elapsed, 'cpu_seconds': cpu,
                         'application_bytes_written': written, 'final_bytes': len(data),
                         'final_sha256': hashlib.sha256(data).hexdigest()}
        print(name + ': ' + json.dumps(results[name], sort_keys=True), flush=True)
    contract.require(results['rewrite_all_previous_rows']['final_sha256']
                     == results['append_new_row']['final_sha256'], 'final reports differ')
    return results


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--input', type=Path, required=True)
    parser.add_argument('--output', type=Path, required=True)
    args = parser.parse_args()
    contract.require(os.environ.get('GITHUB_ACTIONS') == 'true', 'benchmark only on GitHub runners')
    data = args.input.read_bytes()
    report = json.loads(data)
    manifest = json.loads(args.input.with_name('manifest.json').read_bytes())
    digest = hashlib.sha256(data).hexdigest()
    contract.require(manifest['files'] == {'result.json': digest}
                     and manifest['collection']['source_commit'] == report['source_commit']
                     and manifest['binary_sha256'] == report['binary_sha256'],
                     'benchmark input receipt changed')
    with tempfile.TemporaryDirectory(prefix='fr-python-report-writes-') as temporary:
        measurements = compare(report, Path(temporary))
    value = {'source_commit': subprocess.check_output(['git', 'rev-parse', 'HEAD'], cwd=ROOT, text=True).strip(),
             'run_url': 'https://github.com/' + os.environ['GITHUB_REPOSITORY']
                        + '/actions/runs/' + os.environ['GITHUB_RUN_ID'],
             'source_bindings': {path: hashlib.sha256((ROOT / path).read_bytes()).hexdigest() for path in (
                 'tools/python-contract.py', 'tools/benchmark-python-report-writes.py')},
             'input': {'path': args.input.as_posix(), 'sha256': digest, 'cases': len(report['cases']),
                       'source_commit': report['source_commit']},
             'measurement': 'One sample per method on the same Linux runner, baseline first. '
                            'Includes JSON encoding, file writes, flushes, row-list appends and the final save. '
                            'Excludes input loading, result comparison, analysis, Python execution, builds and audits. '
                            'No fsync; bytes count application writes, not physical disk traffic. '
                            'Timings include filesystem caching and Python overhead; this does not measure '
                            'the fr language, application speed or complete agent tasks.',
             'measurements': measurements}
    args.output.parent.mkdir(parents=True, exist_ok=True)
    args.output.write_text(json.dumps(value, indent=2) + '\n')


if __name__ == '__main__':
    main()
