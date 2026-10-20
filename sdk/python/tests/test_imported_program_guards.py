"""Compare imported program guards and mandatory Python 3 directives with Python."""
import pytest

from fr_ir.context import MemoryObjectStore
from fr_ir.flow import FlowCache
from test_flow_summaries import analyze, evidence
from test_postponed_annotations import install, observe


@pytest.mark.parametrize('directive', [
    'division', 'print_function', 'absolute_import', 'unicode_literals',
    'division, print_function', 'division\nfrom __future__ import annotations',
])
@pytest.mark.parametrize('value,reaches', [('source()', True), ('0', False)])
def test_python3_directives_preserve_original_imports(tmp_path, directive, value, reaches):
    helper = 'from __future__ import ' + directive + '\ndef relay(value):\n    return value\n'
    client, rules = install(tmp_path, helper, value)
    result = analyze(client, rules, 'entry', imports=True, standard_library=())
    assert result.report.at('/complete'), result.report.at('/cutoffs')
    assert bool(result.witnesses) is reaches
    assert observe(tmp_path)['result'] == (17 if reaches else 0)


def test_mandatory_directive_does_not_enable_postponed_annotations(tmp_path):
    client, rules = install(tmp_path, 'from __future__ import division\ndef relay(value: Missing):\n    return value\n')
    assert not analyze(client, rules, 'entry', imports=True, standard_library=()).report.at('/complete')


@pytest.mark.parametrize('prefix', ['from __future__ import barry_as_FLUFL\n', 'from __future__ import division as alternate\n'])
def test_other_directives_and_aliases_remain_unchecked(tmp_path, prefix):
    client, rules = install(tmp_path, prefix + 'def relay(value):\n    return value\n')
    assert not analyze(client, rules, 'entry', imports=True, standard_library=()).report.at('/complete')


def test_local_future_candidate_invalidates_the_saved_directive(tmp_path):
    client, rules = install(tmp_path, 'from __future__ import division\ndef relay(value):\n    return value\n')
    cache = FlowCache(MemoryObjectStore())
    first = analyze(client, rules, 'entry', cache, imports=True, standard_library=())
    assert first.report.at('/complete')
    (tmp_path / '__future__.py').write_text('division = 0\n')
    changed = analyze(client, rules, 'entry', cache, imports=True, standard_library=())
    assert not changed.reused and not changed.report.at('/complete')
    assert evidence(changed) == evidence(analyze(client, rules, 'entry', imports=True, standard_library=()))


@pytest.mark.parametrize('literal', ["'__main__'", '"__main__"'])
@pytest.mark.parametrize('value,reaches', [('source()', True), ('0', False)])
def test_imported_program_guard_cannot_replace_the_helper(tmp_path, literal, value, reaches):
    helper = ('def relay(value):\n    return value\n'
              f'if __name__ == {literal}:\n    def relay(value):\n        return 0\n')
    client, rules = install(tmp_path, helper, value)
    result = analyze(client, rules, 'entry', imports=True, standard_library=())
    assert result.report.at('/complete'), result.report.at('/cutoffs')
    assert bool(result.witnesses) is reaches
    assert observe(tmp_path)['result'] == (17 if reaches else 0)


@pytest.mark.parametrize('prefix,guard', [
    ("__name__ = '__main__'\n", "if __name__ == '__main__':"),
    ('from other import __name__\n', "if __name__ == '__main__':"),
    ('', "if __name__ != '__main__':"),
    ('', "if __name__ == '__ main__':"),
    ('', "if __name__ == '__main__':"),
])
def test_rebinding_other_conditions_and_else_are_not_skipped(tmp_path, prefix, guard):
    helper = prefix + 'def relay(value):\n    return value\n' + guard + '\n    relay = unknown()\n'
    if not prefix and guard == "if __name__ == '__main__':":
        helper += 'else:\n    relay = unknown()\n'
    client, rules = install(tmp_path, helper)
    assert not analyze(client, rules, 'entry', imports=True, standard_library=()).report.at('/complete')


def test_entry_file_program_guard_stays_unchecked(tmp_path):
    client, rules = install(tmp_path, 'def relay(value):\n    return value\n', entry_prefix="if __name__ == '__main__':\n    unknown()\n")
    assert not analyze(client, rules, 'entry', imports=True, standard_library=()).report.at('/complete')


def test_unused_math_member_does_not_grant_call_support(tmp_path):
    client, rules = install(tmp_path, 'from math import modf\ndef relay(value):\n    return value\n')
    assert analyze(client, rules, 'entry', imports=True, standard_library=('math',)).report.at('/complete')
    (tmp_path / 'helpers.py').write_text('from math import modf\ndef relay(value):\n    return modf(value)\n')
    assert not analyze(client, rules, 'entry', imports=True, standard_library=('math',)).report.at('/complete')


@pytest.mark.parametrize('value,reaches,expected', [('source()', True, True), ('0', False, False)])
def test_boolean_arithmetic_keeps_a_value_route_through_imported_guards(tmp_path, value, reaches, expected):
    helper = ("from __future__ import division, print_function\nfrom math import modf\n"
              "def relay(value):\n    return value % 2 == 1 and value > 0\n"
              "if __name__ == '__main__':\n    unknown()\n")
    client, rules = install(tmp_path, helper, value)
    result = analyze(client, rules, 'entry', imports=True, standard_library=('math',))
    assert result.report.at('/complete'), result.report.at('/cutoffs')
    assert bool(result.witnesses) is reaches
    assert observe(tmp_path)['result'] is expected
