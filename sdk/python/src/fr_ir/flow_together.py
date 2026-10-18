"""Ask several questions after one startup check, within one shared work budget."""
from __future__ import annotations

from dataclasses import dataclass
import hashlib
import json
from pathlib import Path
from typing import Sequence

from .flow import FlowAnalysis
from .runtime import FrClient, FrReport, FrRuntimeError, Occurrence


@dataclass(frozen=True)
class FlowTogether:
    report: FrReport
    answers: tuple[FlowAnalysis, ...]
    names: tuple[str, ...]
    dictionary_statements: tuple[Occurrence, ...]

    @classmethod
    def from_report(cls, report: FrReport) -> FlowTogether:
        try:
            data = report.to_data()
            startup, budget = data['startup'], data['budget']
            answers = data['answers']
            if (report.schema != 'fr-dataflow-together'
                    or data['handle_prefix'] != 'frp1:' + data['revision'][:32] + ':'
                    or not isinstance(startup, dict) or set(startup) != {
                        'checks', 'questions', 'input_digest', 'inputs', 'names', 'dictionary_statements'}
                    or type(startup['checks']) is not int or startup['checks'] != 1
                    or not isinstance(answers, list) or not 2 <= len(answers) <= 16
                    or type(startup['questions']) is not int or startup['questions'] != len(answers)
                    or not isinstance(budget, dict) or set(budget) != {'steps', 'used', 'response_bytes'}
                    or any(type(value) is not int for value in budget.values())
                    or not 0 <= budget['used'] <= budget['steps'] <= 4096
                    or budget['steps'] < 1 or not 2048 <= budget['response_bytes'] <= 1_048_576):
                raise FrRuntimeError('unsupported shared startup report')
            inputs = startup['inputs']
            if (not isinstance(inputs, dict) or 'selection' in inputs or inputs['summary_mode'] is not True
                    or inputs['budget']['steps'] != budget['steps']
                    or inputs['budget']['bytes'] != budget['response_bytes']):
                raise FrRuntimeError('shared startup inputs disagree with their budget')
            encoded = json.dumps(inputs, sort_keys=True, ensure_ascii=False, separators=(',', ':'))
            if hashlib.sha256(encoded.encode('utf-8')).hexdigest() != startup['input_digest']:
                raise FrRuntimeError('shared startup input digest mismatch')
            names, statements = startup['names'], startup['dictionary_statements']
            if (not isinstance(names, list) or any(not isinstance(name, str) or not name for name in names)
                    or names != sorted(set(names)) or not isinstance(statements, list)):
                raise FrRuntimeError('malformed shared startup names or writes')
            files = inputs['modules']['files'] if 'modules' in inputs else {
                inputs['source']['path']: inputs['source']['digest']}
            occurrences = tuple(Occurrence.from_data(item) for item in statements)
            keys = [(site.path, site.location.span.start, site.location.span.end) for site in occurrences]
            if (keys != sorted(set(keys)) or any(
                    site.revision != data['revision'] or site.path not in files
                    or site.role != 'checked-dictionary-startup' for site in occurrences)):
                raise FrRuntimeError('shared startup writes escape their checked files')
            analyses = []
            used = 0
            targets = set()
            for value in answers:
                answer = FlowAnalysis(FrReport(value, report.arguments))
                dependencies = dict(value['inputs'])
                dependencies.pop('selection')
                work = value['execution']['analysis_steps']
                if (answer.report.schema != 'fr-dataflow-1' or value['revision'] != data['revision']
                        or value['handle_prefix'] != data['handle_prefix'] or value['coverage'] != data['coverage']
                        or dependencies != inputs or value['target'] in targets
                        or type(value['complete']) is not bool or value['execution']['kind'] != 'analyzed'
                        or type(work) is not int or work < 0
                        or type(value['budget']['used']) is not int or value['budget']['used'] != work
                        or type(value['budget']['steps']) is not int or value['budget']['steps'] != budget['steps']
                        or type(value['budget']['response_bytes']) is not int
                        or value['budget']['response_bytes'] != budget['response_bytes']):
                    raise FrRuntimeError('answer differs from its shared startup inputs or work')
                answer_inputs = json.dumps(value['inputs'], sort_keys=True, ensure_ascii=False, separators=(',', ':'))
                if hashlib.sha256(answer_inputs.encode('utf-8')).hexdigest() != value['input_digest']:
                    raise FrRuntimeError('answer input digest mismatch')
                targets.add(value['target'])
                used += work
                answer.initialization
                answer.graphs
                answer.witnesses
                if 'modules' in inputs:
                    answer.dependencies
                if value['function_summaries']:
                    answer.summaries
                analyses.append(answer)
            if used != budget['used']:
                raise FrRuntimeError('shared work count disagrees with its answers')
            return cls(report, tuple(analyses), tuple(names), occurrences)
        except (KeyError, TypeError, ValueError) as error:
            raise FrRuntimeError('missing or malformed shared startup report') from error


def analyze_together(client: FrClient, targets: Sequence[str], *, rules: Path | None = None,
                     context: str = 'generic', steps: int = 256, depth: int = 8,
                     max_bytes: int = 65_536, imports: bool = False) -> FlowTogether:
    """Analyze 2–16 distinct functions in one file; startup stays in this request."""
    if (isinstance(targets, str) or not 2 <= len(targets) <= 16
            or any(not isinstance(target, str) or not target for target in targets)
            or len(set(targets)) != len(targets)):
        raise FrRuntimeError('shared startup needs 2–16 distinct function handles')
    arguments = ['dataflow', targets[0], '--summaries', '--steps', str(steps),
                 '--depth', str(depth), '--bytes', str(max_bytes), '--context', context]
    for target in targets[1:]:
        arguments.extend(['--also', target])
    if rules is not None:
        arguments.extend(['--rules', str(rules)])
    if imports:
        arguments.append('--imports')
    result = FlowTogether.from_report(client.project(*arguments))
    if tuple(answer.report.at('/target') for answer in result.answers) != tuple(targets):
        raise FrRuntimeError('shared startup answers differ from the requested questions')
    return result
