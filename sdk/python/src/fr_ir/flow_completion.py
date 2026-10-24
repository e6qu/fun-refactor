"""The explicit scalar exits understood by the checker, including finally cleanup."""
from dataclasses import dataclass

from .runtime import FrReport, FrRuntimeError


_CONTRACT = {
    'schema': 'fr-scalar-completion',
    'finally': 'Run cleanup for normal flow, return, explicit raise, break and continue; cleanup can replace the pending completion.',
    'values': 'Evaluate a return or raise value before cleanup. Keep separate pending values for nested protected regions.',
    'calls': "Route explicit raises from local function summaries through the caller's cleanup before leaving the function.",
    'boundary': 'No exception matching, bare reraises, exception groups, implicit runtime exceptions, context managers or shared-object effects.',
    'block_limit': 512,
    'implicit_exceptions': False,
}


@dataclass(frozen=True)
class CompletionControl:
    finally_rule: str
    values: str
    calls: str
    boundary: str
    block_limit: int
    implicit_exceptions: bool

    @classmethod
    def from_report(cls, report: FrReport) -> 'CompletionControl':
        data = report.to_data()
        try:
            for value in (data['completion_control'], data['inputs']['completion_control']):
                if (not isinstance(value, dict) or value != _CONTRACT
                        or type(value['block_limit']) is not int
                        or value['implicit_exceptions'] is not False):
                    raise FrRuntimeError('unsupported scalar completion contract')
            return cls(_CONTRACT['finally'], _CONTRACT['values'], _CONTRACT['calls'],
                       _CONTRACT['boundary'], _CONTRACT['block_limit'], False)
        except (KeyError, TypeError) as error:
            raise FrRuntimeError('missing or malformed scalar completion contract') from error
