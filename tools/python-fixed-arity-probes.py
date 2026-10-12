#!/usr/bin/env python3
"""Replay already-seen utility helpers after the fixed-slot summary correction."""
import argparse
import copy
import importlib.util
import json
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
spec = importlib.util.spec_from_file_location('flow', ROOT / 'tools/python-repository-flow.py')
flow = importlib.util.module_from_spec(spec)
spec.loader.exec_module(flow)
flow.COHORT = copy.deepcopy(flow.COHORT)
flow.COHORT['analyzer_revision'] = 'e176c24ea078687e9eb1645b8006215a87653967'
flow.COHORT['holdout'] = 'All three libraries are already-seen coverage. Boltons remains unopened and does not participate in this regression collection.'
for project in flow.COHORT['projects']:
    project['role'] = 'coverage'
original_bindings = flow.bindings


def bindings():
    value = original_bindings()
    value['tools/python-fixed-arity-probes.py'] = flow.sha(Path(__file__).read_bytes())
    return value


flow.bindings = bindings


def check(value):
    for project in flow.COHORT['projects']:
        name = str(Path(project['path']).relative_to(project['python_root'])) + '::' + project['function']
        for row in value['cases']:
            if row['project'] != project['id'] or row['case'] in ('steps', 'bytes'):
                continue
            report = row['invocations'][1]['report']
            flow.require(report['semantics'] == 'python-scalar-summaries-6'
                         and 'unsupported-parameter-contract:' + name not in report['cutoffs'],
                         'fixed-slot helper still hits the parameter boundary')
            summary = report['function_summaries']['functions'].get(name)
            flow.require(summary is not None and summary['normal_return'] is True,
                         'upstream fixed-slot helper did not enter the summary solver')
    flow.require(value['summary']['acceptance_demonstrated'] is False,
                 'identity-adapter regressions cannot close repository acceptance')
    return value['summary']


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--fr', type=Path, default=ROOT / 'target/debug/fr')
    parser.add_argument('--output', type=Path)
    parser.add_argument('--audit', type=Path)
    args = parser.parse_args()
    if args.audit:
        value = json.loads(args.audit.read_bytes())
        flow.audit(value, args.audit.parent)
    else:
        flow.require(args.output, 'choose collection or an offline audit')
        args.phase, args.coverage_run = 'coverage', None
        flow.measure(args)
        value = json.loads((args.output / 'result.json').read_bytes())
    print(json.dumps(check(value), sort_keys=True))


if __name__ == '__main__':
    main()
