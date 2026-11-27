"""Check the Python interface while collecting independent analyzer reports."""
import argparse
from concurrent.futures import ThreadPoolExecutor
import os
from pathlib import Path
import subprocess
import sys

ROOT = Path(__file__).resolve().parents[1]
SDK_TESTS = (
    'expression_control', 'flow_imports', 'call_binding', 'literal_defaults',
    'flow_summaries', 'scalar_assignments', 'flow_storage', 'package_flow',
    'namespace_packages', 'function_declarations', 'module_literals',
    'function_defaults', 'parallel_bindings', 'scalar_named_defaults',
    'imported_scalar_defaults', 'value_aliases', 'default_replacements', 'startup_registration', 'flow_together',
    'saved_startup', 'registration_loops', 'registration_arguments',
    'finally_flow', 'exception_handlers', 'graph_analysis', 'list_sequences', 'list_exceptions', 'sequence_unpacking', 'mapping_calls', 'mapping_operations', 'standard_library',
)


def check(sdk_check, collect, audit):
    failures = []
    with ThreadPoolExecutor(max_workers=1) as work:
        sdk = work.submit(sdk_check)
        try:
            collect()
        except Exception as error:
            failures.append(error)
        try:
            sdk.result()
        except Exception as error:
            failures.append(error)
    if failures:
        raise ExceptionGroup('Python interface or report collection failed', failures)
    audit()


def sdk_checks(environment, log):
    logs = [log.with_name(f'{log.stem}-{slot}.log') for slot in range(2)]

    def shard(slot):
        with logs[slot].open('w') as output:
            subprocess.run([sys.executable, '-B', '-m', 'pytest', '-q', *[
                f'sdk/python/tests/test_{name}.py' for name in SDK_TESTS[slot::2]]],
                cwd=ROOT, env=environment, stdout=output, stderr=subprocess.STDOUT,
                check=True, timeout=600)

    try:
        check(lambda: shard(0), lambda: shard(1), lambda: None)
    finally:
        text = '\n'.join(f'SDK shard {slot + 1}\n' +
                         (path.read_text() if path.exists() else 'Did not start.\n')
                         for slot, path in enumerate(logs))
        log.write_text(text)
        print(text, end='', flush=True)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--output', type=Path, required=True)
    args = parser.parse_args()
    if os.environ.get('GITHUB_ACTIONS') != 'true':
        raise ValueError('Run collection checks on GitHub runners.')
    environment = dict(os.environ, PYTHONPATH=str(ROOT / 'sdk/python/src'),
                       RAYON_NUM_THREADS='1', PYTHONDONTWRITEBYTECODE='1')
    log = Path(os.environ['RUNNER_TEMP']) / 'python-contract-sdk.log'

    def collect():
        subprocess.run([sys.executable, '-B', 'tools/python-contract.py', '--output', str(args.output)],
                       cwd=ROOT, env=environment, check=True, timeout=600)

    def audit():
        subprocess.run([sys.executable, '-B', 'tools/test-python-contract.py'], cwd=ROOT,
                       env=dict(environment, FR_PYTHON_CONTRACT_REPORT=str(args.output.resolve())),
                       check=True, timeout=120)

    check(lambda: sdk_checks(environment, log), collect, audit)


if __name__ == '__main__':
    main()
