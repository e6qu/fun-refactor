"""Save checked Python declarations and startup writes between separate questions."""
from __future__ import annotations

from dataclasses import dataclass
import hashlib
import json
from pathlib import Path
import re
import tempfile
from typing import Any

from .context import ObjectStore, restore_stored_value, store_merkle_value
from .flow import FlowAnalysis
from .flow_standard_library import contract as standard_contract
from .runtime import FrClient, FrReport, FrRuntimeError


def _encode(value: Any) -> str:
    try:
        return json.dumps(value, ensure_ascii=False, sort_keys=True,
                          separators=(',', ':'), allow_nan=False)
    except (TypeError, ValueError) as error:
        raise FrRuntimeError('saved startup requires finite JSON') from error


def _digest(value: Any) -> str:
    return hashlib.sha256(_encode(value).encode('utf-8')).hexdigest()


def _record(value: Any) -> dict[str, Any]:
    if (not isinstance(value, dict) or set(value) != {
            'schema', 'inputs', 'input_digest', 'functions', 'function_aliases',
            'inert_bindings', 'dictionary_statements', 'dictionary_omitted'}
            or value['schema'] != 'fr-python-startup'
            or not isinstance(value['inputs'], dict)
            or set(value['inputs']) != {'analyzer', 'source', 'configuration', 'modules'}
            or value['input_digest'] != _digest(value['inputs'])
            or not isinstance(value['functions'], list)
            or not isinstance(value['function_aliases'], dict) or len(value['function_aliases']) > 128
            or not isinstance(value['inert_bindings'], list)
            or not isinstance(value['dictionary_statements'], list)
            or len(value['dictionary_statements']) > 16
            or type(value['dictionary_omitted']) is not int or value['dictionary_omitted'] < 0
            or value['dictionary_omitted'] > 0 and len(value['dictionary_statements']) != 16):
        raise FrRuntimeError('unsupported saved startup record')
    if len(_encode(value).encode('utf-8')) > 1_048_576:
        raise FrRuntimeError('saved startup exceeds 1 MiB')
    return value


def _store(store: ObjectStore, value: dict[str, Any]) -> str:
    encoded = _encode(_record(value))
    chunks = [encoded[index:index + 32_768] for index in range(0, len(encoded), 32_768)]
    return store_merkle_value(store, {'schema': 'fr-startup-record', 'chunks': chunks}).digest


def _restore(store: ObjectStore, digest: str) -> dict[str, Any]:
    value = restore_stored_value(store, digest)
    if (not isinstance(value, dict) or set(value) != {'schema', 'chunks'}
            or value['schema'] != 'fr-startup-record'
            or not isinstance(value['chunks'], list) or not 1 <= len(value['chunks']) <= 32
            or any(not isinstance(chunk, str) or not 1 <= len(chunk) <= 32_768
                   or index < len(value['chunks']) - 1 and len(chunk) != 32_768
                   for index, chunk in enumerate(value['chunks']))):
        raise FrRuntimeError('unsupported saved startup chunks')
    encoded = ''.join(value['chunks'])
    if len(encoded.encode('utf-8')) > 1_048_576:
        raise FrRuntimeError('saved startup exceeds 1 MiB')
    try:
        record = json.loads(encoded)
    except (ValueError, RecursionError) as error:
        raise FrRuntimeError('saved startup contains invalid JSON') from error
    if _encode(record) != encoded:
        raise FrRuntimeError('saved startup JSON is not canonical')
    return _record(record)


@dataclass(frozen=True)
class PreparedFlow:
    """The new answer and the work repeated before answering it."""
    report: FrReport
    answer: FlowAnalysis
    reused: bool
    checks: int
    import_checks: int

    @classmethod
    def from_report(cls, report: FrReport) -> PreparedFlow:
        try:
            data = report.to_data()
            preparation, answer = data['preparation'], data['answer']
            if (report.schema != 'fr-dataflow-prepared'
                    or set(data) != {'schema', 'revision', 'handle_prefix', 'coverage', 'context_basis',
                                     'preparation', 'startup', 'answer'}
                    or not isinstance(preparation, dict) or set(preparation) != {
                        'kind', 'checks', 'import_checks', 'reason', 'trust'}
                    or preparation['kind'] not in ('checked', 'retained')
                    or type(preparation['checks']) is not int
                    or preparation['checks'] != int(preparation['kind'] == 'checked')
                    or type(preparation['import_checks']) is not int
                    or preparation['import_checks'] not in (0, 1)
                    or preparation['reason'] not in ('matching-inputs', 'changed-inputs', 'no-saved-startup')
                    or (preparation['reason'] == 'matching-inputs') != (preparation['kind'] == 'retained')
                    or preparation['trust'] != 'caller-supplied digest binds saved work; no execution attestation'
                    or not isinstance(answer, dict) or answer['schema'] != 'fr-dataflow-1'
                    or answer['revision'] != data['revision']
                    or answer['handle_prefix'] != data['handle_prefix']
                    or data['handle_prefix'] != 'frp1:' + data['revision'][:32] + ':'
                    or answer['coverage'] != data['coverage']
                    or data['context_basis'] != 'frcb1:' + _digest([
                        'fr-context-1', data['revision'], data['handle_prefix'], data['coverage']])
                    or type(answer['complete']) is not bool or answer['execution']['kind'] != 'analyzed'
                    or answer['inputs']['summary_mode'] is not True
                    or preparation['import_checks'] != int('modules' in answer['inputs'])
                    or answer['input_digest'] != _digest(answer['inputs'])):
                raise FrRuntimeError('unsupported prepared flow report')
            limits, work = answer['inputs']['budget'], answer['execution']['analysis_steps']
            if (not isinstance(limits, dict) or set(limits) != {'steps', 'depth', 'bytes'}
                    or any(type(value) is not int for value in limits.values())
                    or not 1 <= limits['steps'] <= 4096 or not 1 <= limits['depth'] <= 32
                    or not 2048 <= limits['bytes'] <= 1_048_576
                    or type(work) is not int or not 0 <= work <= limits['steps']
                    or not isinstance(answer['budget'], dict)
                    or any(type(value) is not int for value in answer['budget'].values())
                    or answer['budget'] != {'steps': limits['steps'], 'used': work,
                                            'call_depth': limits['depth'], 'response_bytes': limits['bytes']}
                    or len(_encode(data).encode('utf-8')) > limits['bytes']):
                raise FrRuntimeError('prepared answer exceeds or disagrees with its work limits')
            analysis = FlowAnalysis(FrReport(answer, report.arguments))
            analysis.initialization
            analysis.graphs
            analysis.witnesses
            if preparation['import_checks']:
                analysis.dependencies
            if answer['function_summaries']:
                analysis.summaries
            if data['startup'] is not None:
                saved = _record(data['startup'])
                basis = {key: answer['inputs'][key] for key in ('analyzer', 'source', 'configuration')}
                basis['modules'] = answer['inputs']['modules'] if preparation['import_checks'] else None
                if not answer['complete'] or saved['inputs'] != basis:
                    raise FrRuntimeError('saved startup differs from the checked answer')
            return cls(report, analysis, preparation['kind'] == 'retained',
                       preparation['checks'], preparation['import_checks'])
        except (KeyError, TypeError, ValueError) as error:
            raise FrRuntimeError('missing or malformed prepared flow report') from error


class StartupCache:
    """Keep one checked startup record; every question still resolves its imports."""

    def __init__(self, store: ObjectStore) -> None:
        self.store = store
        self._record: str | None = None

    def persist(self) -> str:
        return store_merkle_value(self.store, {'schema': 'fr-startup-cache', 'record': self._record}).digest

    @classmethod
    def restore(cls, store: ObjectStore, digest: str) -> StartupCache:
        value = restore_stored_value(store, digest)
        if (not isinstance(value, dict) or set(value) != {'schema', 'record'}
                or value['schema'] != 'fr-startup-cache'):
            raise FrRuntimeError('unsupported startup cache manifest')
        record = value['record']
        if record is not None and (not isinstance(record, str) or re.fullmatch(r'[0-9a-f]{64}', record) is None):
            raise FrRuntimeError('saved startup needs a content digest')
        cache = cls(store)
        cache._record = record
        return cache

    def analyze(self, client: FrClient, target: str, *, standard_library: tuple[str, ...], rules: Path | None,
                context: str, steps: int, depth: int,
                max_bytes: int, imports: bool) -> PreparedFlow:
        standard_contract(standard_library)
        if standard_library and not imports:
            raise FrRuntimeError('standard-library models require imported flow')
        arguments = ['dataflow', target, '--summaries', '--with-startup', '--steps', str(steps),
                     '--depth', str(depth), '--bytes', str(max_bytes), '--context', context]
        if rules is not None:
            arguments.extend(['--rules', str(rules)])
        if imports:
            arguments.append('--imports')
        if standard_library:
            arguments.extend(['--standard-library', ','.join(standard_library)])
        if self._record is None:
            report = client.project(*arguments)
        else:
            saved = _restore(self.store, self._record)
            with tempfile.TemporaryDirectory(prefix='fr-startup-') as directory:
                path = Path(directory) / 'startup.json'
                path.write_text(_encode(saved), encoding='utf-8')
                report = client.project(*arguments, '--reuse-startup', str(path), '--startup-digest', _digest(saved))
        result = PreparedFlow.from_report(report)
        if result.answer.report.at('/target') != target:
            raise FrRuntimeError('prepared answer differs from the requested question')
        saved = report.at('/startup')
        self._record = None if saved is None else _store(self.store, saved)
        return result
