#!/usr/bin/env python3
"""Locate repeated audit work on a runner within the existing 120-second limit."""
import argparse
import faulthandler
from functools import wraps
import importlib.util
import json
import os
from pathlib import Path
import signal
import time
import unittest

ROOT = Path(__file__).resolve().parents[1]


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--report', type=Path, required=True)
    parser.add_argument('--output', type=Path, required=True)
    args = parser.parse_args()
    if os.environ.get('GITHUB_ACTIONS') != 'true':
        raise SystemExit('Run this diagnostic on GitHub, not the workstation.')
    os.environ['FR_PYTHON_CONTRACT_REPORT'] = str(args.report.resolve())
    started = time.monotonic()
    measurements, tests = {}, {}

    def deadline(signum, frame):
        faulthandler.dump_traceback()
        raise SystemExit('Python audit exceeded its existing 120-second deadline.')

    signal.signal(signal.SIGALRM, deadline)
    signal.alarm(120)
    try:
        spec = importlib.util.spec_from_file_location('audit_controls', ROOT / 'tools/test-python-contract.py')
        module = importlib.util.module_from_spec(spec)
        spec.loader.exec_module(module)

        def measure(owner, name, label):
            original = getattr(owner, name)
            record = measurements.setdefault(label, {'calls': 0, 'seconds': 0.0})

            @wraps(original)
            def measured(*arguments, **keywords):
                before = time.monotonic()
                record['calls'] += 1
                try:
                    return original(*arguments, **keywords)
                finally:
                    record['seconds'] += time.monotonic() - before

            setattr(owner, name, measured)

        measure(module.contract, 'bindings', 'all_source_bindings')
        measure(module.contract.profile, 'native_source_bindings', 'native_source_bindings')
        measure(module.contract, 'audit_case', 'case_checks')
        measure(module.contract, 'ast_locations', 'source_locations')
        measure(module.contract, 'oracle', 'python_observations')

        class TimedResult(unittest.TextTestResult):
            def startTest(self, test):
                self.before = time.monotonic()
                super().startTest(test)

            def stopTest(self, test):
                tests[test.id()] = time.monotonic() - self.before
                super().stopTest(test)

        suite = unittest.defaultTestLoader.loadTestsFromModule(module)
        result = unittest.TextTestRunner(verbosity=2, resultclass=TimedResult).run(suite)
        if not result.wasSuccessful():
            raise SystemExit(1)
    finally:
        signal.alarm(0)
        report = {'deadline_seconds': 120, 'elapsed_seconds': time.monotonic() - started,
                  'measurements': measurements, 'test_seconds': tests}
        args.output.parent.mkdir(parents=True, exist_ok=True)
        args.output.write_text(json.dumps(report, indent=2) + '\n')
        print(json.dumps(report, sort_keys=True), flush=True)


if __name__ == '__main__':
    main()
