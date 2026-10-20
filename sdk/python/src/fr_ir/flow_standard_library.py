"""Explicit trusted standard-library interfaces, with competing imports retained."""
from __future__ import annotations

from typing import Any

from .runtime import FrRuntimeError


NAMESPACE_DIGEST = 'd566d3f7ccca2afddbade52d5b5afb71254a383076b78a995dcbc9430b422eb7'

MEMBERS = {
    'builtins': ['abs', 'int', 'float', 'bool', 'round', 'min', 'max', 'ValueError', 'TypeError'],
    'math': ['ceil', 'floor', 'trunc', 'sqrt', 'fabs', 'isfinite', 'isinf', 'isnan', 'inf', 'nan', 'pi', 'e', 'tau'],
    'bisect': ['bisect', 'bisect_left', 'bisect_right', 'insort', 'insort_left', 'insort_right'],
    'binascii': ['Error', 'Incomplete', 'hexlify', 'unhexlify', 'b2a_hex', 'a2b_hex', 'b2a_base64', 'a2b_base64', 'crc32', 'crc_hqx'],
    'datetime': ['date', 'datetime', 'time', 'timedelta', 'timezone', 'tzinfo', 'MINYEAR', 'MAXYEAR', 'UTC'],
    'sys': ['version_info', 'version', 'platform', 'maxsize', 'implementation', 'modules'],
}


def contract(selection: tuple[str, ...]) -> dict[str, Any] | None:
    if (not isinstance(selection, tuple) or any(not isinstance(name, str) or name not in MEMBERS for name in selection)
            or len(set(selection)) != len(selection)):
        raise FrRuntimeError('standard-library models must be an explicit tuple of unique supported module names')
    if not selection:
        return None
    return {
        'schema': 'fr-python-standard-library', 'modules': {name: MEMBERS[name] for name in selection},
        'runtime': 'CPython 3.14.8',
        'namespace_digest': NAMESPACE_DIGEST,
        'imports': 'Explicit selected standard modules; competing local source, package, stub, directory, bytecode and extension candidates must be absent, including transitive standard-library names.',
        'calls': 'Scalar abs/int/float/bool/round and multi-argument min/max; ValueError/TypeError construction in raise; math scalar functions and datetime.date. Other calls remain unchecked.',
        'boundary': 'Standard CPython library and builtins, scalar arguments and immutable dates; no replacement import hooks, monkey patching, overloaded protocols or implicit exception model.',
    }


def selected_contract(value: Any) -> tuple[str, ...]:
    if value is None:
        return ()
    if not isinstance(value, dict) or not isinstance(value.get('modules'), dict):
        raise FrRuntimeError('missing explicit standard-library contract')
    names = tuple(value['modules'])
    if value != contract(names):
        raise FrRuntimeError('unsupported standard-library contract')
    return names
