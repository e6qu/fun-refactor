"""The explicit scalar exits understood by the checker, including finally cleanup."""
from dataclasses import dataclass
from typing import Any

from .runtime import FrReport, FrRuntimeError


_CONTRACT: dict[str, Any] = {
    'schema': 'fr-scalar-completion',
    'finally': 'Capture values before cleanup; cleanup may replace exits; nest separately.',
    'except': 'Ordered unbound builtin handlers; explicit raises only.',
    'classes': ['ValueError', 'TypeError', 'Exception', 'BaseException'],
    'reraise': 'Lexically active known error.',
    'local_call_raises': True,
    'block_limit': 512,
    'implicit_exceptions': False,
}


@dataclass(frozen=True)
class CompletionControl:
    finally_rule: str
    local_call_raises: bool
    block_limit: int
    implicit_exceptions: bool
    except_rule: str
    exception_classes: tuple[str, ...]
    reraise_rule: str

    @classmethod
    def from_report(cls, report: FrReport) -> 'CompletionControl':
        data = report.to_data()
        try:
            value = data['inputs']['completion_control']
            if ('completion_control' in data or not isinstance(value, dict) or value != _CONTRACT
                    or type(value['block_limit']) is not int
                    or value['local_call_raises'] is not True
                    or value['implicit_exceptions'] is not False):
                raise FrRuntimeError('unsupported scalar completion contract')
            return cls(_CONTRACT['finally'], True, _CONTRACT['block_limit'], False,
                _CONTRACT['except'], tuple(_CONTRACT['classes']), _CONTRACT['reraise'])
        except (KeyError, TypeError) as error:
            raise FrRuntimeError('missing or malformed scalar completion contract') from error
