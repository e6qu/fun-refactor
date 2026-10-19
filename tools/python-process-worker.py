"""Answer fresh questions through one explicitly selected Python SDK copy."""
import time

BOOT = time.perf_counter()
import argparse
import json
import os
from pathlib import Path
import sys

BASE_READY = time.perf_counter()
parser = argparse.ArgumentParser(description=__doc__)
parser.add_argument('--sdk', type=Path, required=True)
parser.add_argument('--project', type=Path, required=True)
parser.add_argument('--fr', required=True)
parser.add_argument('--cache', type=Path, required=True)
args = parser.parse_args()
PARSED = time.perf_counter()
sys.path.insert(0, str(args.sdk.resolve()))
os.environ['FUN_REFACTOR_CACHE'] = str(args.cache)
IMPORT_STARTED = time.perf_counter()
from fr_ir.flow import FlowAnalysis
from fr_ir.runtime import FrClient
IMPORTED = time.perf_counter()


class TimedClient(FrClient):
    def __init__(self):
        super().__init__(args.project, executable=args.fr, timeout=20,
                         max_output_bytes=1_048_576)
        self.calls = []

    def project(self, *arguments):
        started = time.perf_counter()
        report = super().project(*arguments)
        self.calls.append({'operation': arguments[0], 'seconds': time.perf_counter() - started})
        return report


def modules():
    return {name: {'file': module.__file__, 'cached': module.__cached__}
            for name, module in tuple(sys.modules.items())
            if name == 'fr_ir' or name.startswith('fr_ir.')}


def emit(value):
    print(json.dumps(value, separators=(',', ':'), allow_nan=False), flush=True)


emit({'ready': True, 'modules': modules(), 'timing': {
    'base_import_seconds': BASE_READY - BOOT,
    'arguments_seconds': PARSED - BASE_READY,
    'sdk_import_seconds': IMPORTED - IMPORT_STARTED,
    'bootstrap_seconds': time.perf_counter() - BOOT}})
for line in sys.stdin:
    request = json.loads(line)
    if request == {'stop': True}:
        emit({'stopped': True, 'modules': modules()})
        break
    if set(request) != {'function', 'steps', 'depth', 'bytes', 'imports'}:
        raise ValueError('question needs explicit function and analysis settings')
    started = time.perf_counter()
    client = TimedClient()
    target = client.project('find', request['function']).definition_target().handle
    arguments = ['dataflow', target, '--rules', str(args.project / 'rules.json'),
                 '--context', 'html', '--steps', str(request['steps']),
                 '--depth', str(request['depth']), '--bytes', str(request['bytes']), '--summaries']
    if request['imports']:
        arguments.append('--imports')
    result = FlowAnalysis(client.project(*arguments))
    result.initialization
    result.graphs
    result.witnesses
    if request['imports']:
        result.dependencies
    if result.report.at('/function_summaries'):
        result.summaries
    answer = result.report.to_data()
    steps = answer['execution']['analysis_steps']
    del answer['execution']
    del answer['context_basis']
    emit({'answer': answer, 'analysis_steps': steps, 'calls': client.calls,
          'body_seconds': time.perf_counter() - started})
