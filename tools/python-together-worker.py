"""Measure two fresh helper questions inside one installed Python process."""
import time

BOOT = time.perf_counter()
import argparse
import json
import os
from pathlib import Path
import sys

parser = argparse.ArgumentParser(description=__doc__)
parser.add_argument('--sdk', type=Path, required=True)
parser.add_argument('--project', type=Path, required=True)
parser.add_argument('--fr', required=True)
parser.add_argument('--cache', type=Path, required=True)
args = parser.parse_args()
sys.path.insert(0, str(args.sdk.resolve()))
os.environ['FUN_REFACTOR_CACHE'] = str(args.cache)
from fr_ir.flow import FlowAnalysis
from fr_ir.flow_together import analyze_together
from fr_ir.runtime import FrClient


class TimedClient(FrClient):
    def __init__(self):
        super().__init__(args.project, executable=args.fr, timeout=20,
                         max_output_bytes=1_048_576)
        self.calls = []

    def project(self, *arguments):
        started = time.perf_counter()
        report = super().project(*arguments)
        seconds = time.perf_counter() - started
        self.calls.append({'operation': arguments[0], 'seconds': seconds,
                           'report_json_bytes': len(json.dumps(report.to_data(), ensure_ascii=False,
                                                               separators=(',', ':')).encode())})
        return report


def modules():
    return {name: {'file': module.__file__, 'cached': module.__cached__}
            for name, module in tuple(sys.modules.items())
            if name == 'fr_ir' or name.startswith('fr_ir.')}


def emit(value):
    print(json.dumps(value, separators=(',', ':'), allow_nan=False), flush=True)


emit({'ready': True, 'modules': modules(), 'bootstrap_seconds': time.perf_counter() - BOOT})
for line in sys.stdin:
    request = json.loads(line)
    if request == {'stop': True}:
        emit({'stopped': True, 'modules': modules()})
        break
    if set(request) != {'condition', 'names', 'steps', 'depth', 'bytes', 'imports', 'context'}:
        raise ValueError('supply the approach, names and every analysis setting')
    started = time.perf_counter()
    client = TimedClient()
    handles = [client.project('find', name).definition_target().handle for name in request['names']]
    if request['condition'] == 'together':
        result = analyze_together(client, handles, rules=args.project / 'rules.json',
                                  context=request['context'], steps=request['steps'], depth=request['depth'],
                                  max_bytes=request['bytes'], imports=request['imports'])
        reports = [result.report.to_data()]
    elif request['condition'] == 'separate':
        reports = []
        for target in handles:
            arguments = ['dataflow', target, '--rules', str(args.project / 'rules.json'),
                         '--context', request['context'], '--steps', str(request['steps']),
                         '--depth', str(request['depth']), '--bytes', str(request['bytes']), '--summaries']
            if request['imports']:
                arguments.append('--imports')
            answer = FlowAnalysis(client.project(*arguments))
            answer.initialization
            answer.graphs
            answer.witnesses
            if request['imports']:
                answer.dependencies
            if answer.report.at('/function_summaries'):
                answer.summaries
            reports.append(answer.report.to_data())
    else:
        raise ValueError('unknown approach')
    emit({'reports': reports, 'calls': client.calls, 'body_seconds': time.perf_counter() - started})
