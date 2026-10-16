"""Independent executable examples for the declared scalar Python contract."""
import ast
import io
import tokenize


RULES = {
    'operands': 'Scalar operand origins join; evaluation follows source order and ignores comments.',
    'assignments': 'Evaluate scalar RHS leaves once, then write literal-shaped targets from left to right. Flat module tuples/lists read literal/function/builtin values before binding fresh ASCII names.',
    'selectors': 'Boolean and None literals select expression paths; unknown truth retains alternatives.',
    'comparisons': 'Evaluate ordered comparison operands; comparison truth remains unknown.',
    'statements': 'Join if/elif alternatives and loop fixed points; preserve break, continue and loop else.',
    'calls': 'Evaluate explicit arguments in source order, then bind positions and exact keyword names.',
    'defaults': 'Omitted immutable literals and captured prior local scalar defaults carry no source origins; explicit arguments override them. Uncalled module functions may capture earlier values/functions or unshadowed builtins without calling them.',
    'returns': 'Substitute scalar helper summaries; return and explicit raise terminate their paths.',
    'recursion': 'Recursive scalar summaries grow to a fixed point; each call substitutes its own origins.',
    'imports': 'Resolve static local modules within one root; check names in cyclic initialization order and preserve defining-file identities and lookup evidence.',
    'rules': 'Sources, sinks, sanitizers and propagators are caller-supplied contracts.',
    'names': 'Local bindings and shadowing prevent guessed external calls or unbound value claims.',
    'locations': 'Witnesses retain source revisions, UTF-8 byte spans and one-based Unicode columns.',
    'boundaries': 'Unsupported constructs and exhausted budgets remain incomplete; invalid entry syntax and inputs can refuse analysis.',
}


def example(key, rules, body, events=(), returned=None, raised=None, witness=False,
            prelude='', complete=True, cutoff=None, difference=None, steps=4096, source_index=0,
            normal=True, may_raise=False, files=None, path='subject.py', phase='entry', depth=8,
            bytes_limit=262144, refusal=None, module_files=None, module_lookup=None, sink_index=0):
    return {'id': key, 'rules_checked': rules.split(),
            'source': prelude + 'def entry():\n' + ''.join('    ' + line + '\n' for line in body.splitlines()),
            'expected_runtime': {'events': list(events), 'returned': returned, 'raised': raised, 'phase': phase},
            'expected_model': ({'outcome': 'refused', 'exit_code': 1,
                                'error': {'error': {'kind': 'error', 'message': refusal}}} if refusal else
                              {'outcome': 'report', 'witness': witness, 'pairs': [[source_index, sink_index]] if witness else [],
                               'complete': complete, 'cutoff': cutoff,
                               'completion': {'normal_return': normal, 'may_raise': may_raise} if complete else None}),
            'difference': difference, 'steps': steps, 'depth': depth, 'bytes': bytes_limit,
            'module_files': module_files, 'module_lookup': module_lookup, 'files': files or {}, 'path': path,
            'imports': files is not None}


S = ['source', 17]
T = ['sink', 17]
ZERO = ['sink', 0]
EXAMPLES = [
    example('module-collection-empty-dictionary', 'imports names returns boundaries', 'return sink(relay(source()))', [S, T], 17, witness=True,
            prelude='from helper import relay\n', files={'helper.py': 'registry = {}\ndef relay(value):\n    return value\n'}),
    example('module-collection-module-names', 'imports names returns boundaries', 'return sink(relay(source()))', [S, ZERO], 0,
            prelude='from helper import relay\n', files={'helper.py': "modules = ('calc', 'colls', 'tree')\ndef relay(value):\n    return 0\n"}),
    example('module-collection-nested', 'names boundaries', 'return sink(source())', [S, T], 17, witness=True,
            prelude="metadata = ({(0, 'café'): [None]}, {1, 2}, [])\n"),
    example('module-collection-global-read', 'names boundaries', 'return sink(metadata[0])', [T], 17,
            prelude='metadata = [17]\n', complete=False, cutoff='unsupported-expression:subscript'),
    example('module-collection-mutated', 'names boundaries', 'return sink(0)', [S, ZERO], 0,
            prelude='metadata = []\nmetadata.append(source())\n', complete=False, cutoff='module-effects-unchecked'),
    example('module-collection-evaluated-element', 'names boundaries', 'return sink(0)', [S, ZERO], 0,
            prelude="metadata = {'x': source()}\n", complete=False, cutoff='module-effects-unchecked'),
    example('module-collection-hash-callback', 'names boundaries', 'return sink(0)', [S, T, ZERO], 0,
            prelude='class Key:\n    def __hash__(self):\n        sink(source())\n        return 0\nmetadata = {Key(): 0}\n', complete=False, cutoff='module-effects-unchecked'),
    example('module-collection-unhashable-key', 'names boundaries', 'return sink(0)', raised='TypeError', phase='definition',
            prelude='metadata = {([] ,): 0}\n', complete=False, cutoff='module-effects-unchecked',
            difference='Python rejects the key during initialization; the model keeps that initialization unchecked.'),
    example('module-collection-unpacked', 'names boundaries', 'return sink(0)', [ZERO], 0,
            prelude='metadata = [*(0, 1)]\n', complete=False, cutoff='module-effects-unchecked'),
    example('module-collection-over-budget', 'names boundaries', 'return sink(source())', [S, T], 17, witness=True,
            prelude='metadata = [' + ','.join('0' for _ in range(128)) + ']\n', complete=False, cutoff='module-effects-unchecked'),
    example('module-metadata-forward', 'imports names returns boundaries', 'return sink(relay(source()))', [S, T], 17, witness=True,
            prelude='from helper import relay\n', files={'helper.py': "__version__ = '8.1.0'\n__author__ = 'owner'\n__license__ = 'MIT'\n__copyright__ = 'owner'\ndef relay(value):\n    return value\n"}),
    example('module-metadata-blocked', 'imports names returns boundaries', 'return sink(relay(source()))', [S, ZERO], 0,
            prelude='from helper import relay\n', files={'helper.py': "__version__ = '8.1.0'\ndef relay(value):\n    return 0\n"}),
    example('module-metadata-evaluated', 'imports names boundaries', 'return sink(0)', [S, ZERO], 0,
            prelude='from helper import relay\n', files={'helper.py': '__version__ = source()\ndef relay(value):\n    return value\n'},
            complete=False, cutoff='module-effects-unchecked'),
    example('module-metadata-mutable', 'names boundaries', 'return sink(source())', [S, T], 17, witness=True,
            prelude='__version__ = []\n', complete=False, cutoff='module-effects-unchecked'),
    example('module-metadata-repeated', 'names boundaries', 'return sink(source())', [S, T], 17, witness=True,
            prelude="__version__ = '8'\n__version__ = '9'\n", complete=False, cutoff='module-effects-unchecked'),
    example('module-metadata-global-read', 'names boundaries', 'return sink(__version__)', [['sink', '8.1.0']], '8.1.0',
            prelude="__version__ = '8.1.0'\n", complete=False, cutoff='unknown-binding:__version__'),
    example('module-metadata-import-control', 'imports names boundaries', 'return sink(source())', [S, T], 17, witness=True,
            prelude="__path__ = ''\n", files={}, complete=False, cutoff='module-effects-unchecked'),
    example('postponed-annotations-forward', 'imports calls returns boundaries', 'return sink(relay(source()))', [S, T], 17, witness=True,
            prelude='from helper import relay\n', files={'helper.py': 'from __future__ import annotations\ndef relay(value: Missing) -> Missing:\n    return value\n'}),
    example('postponed-annotations-blocked', 'imports calls returns boundaries', 'return sink(relay(source()))', [S, ZERO], 0,
            prelude='from helper import relay\n', files={'helper.py': 'from __future__ import annotations\ndef relay(value: Missing) -> Missing:\n    return 0\n'}),
    example('postponed-annotations-fixed-slots', 'imports calls defaults returns', 'return sink(relay(source(), flag=0))', [S, T], 17, witness=True,
            prelude='from helper import relay\n', files={'helper.py': 'from __future__ import annotations\ndef relay(value: Missing, /, *, flag: Missing = 0):\n    return value\n'}),
    example('postponed-annotations-unused-variadics', 'imports calls returns boundaries', 'return sink(relay(source()))', [S, T], 17, witness=True,
            prelude='from helper import relay\n', files={'helper.py': 'from __future__ import annotations\ndef relay(value: Missing, *unused: Missing, **kwargs: Missing):\n    return value\n'}),
    example('postponed-annotations-not-evaluated', 'imports calls returns locations', 'return sink(relay(source()))', [S, T], 17, witness=True,
            prelude='from helper import relay\n', files={'helper.py': 'from __future__ import annotations\ndef relay(value: source()) -> source():\n    return value\n'}, source_index=2),
    example('postponed-annotations-module-local', 'imports boundaries', 'return sink(0)', [ZERO], 0,
            prelude='from __future__ import annotations\nfrom helper import relay\n',
            files={'helper.py': 'def relay(value: Missing):\n    return value\n'}, complete=False, cutoff='module-effects-unchecked',
            difference='The caller directive does not apply to the helper. Python 3.14 also defers unflagged annotations, which this model still leaves unchecked.'),
    example('postponed-annotations-default-still-evaluated', 'imports defaults boundaries', 'return sink(0)', [S, ZERO], 0,
            prelude='from __future__ import annotations\ndef unused(value: Missing = source()):\n    return value\n', files={},
            complete=False, cutoff='module-effects-unchecked'),
    example('postponed-annotations-plain-method', 'imports names boundaries', 'return sink(source())', [S, T], 17, witness=True,
            prelude='from __future__ import annotations\nclass Token:\n    def unused(self, value: Missing) -> Missing:\n        return value\nmarker = Token()\n', files={}),
    example('postponed-annotations-local-shadow', 'imports names boundaries', 'return sink(0)', [S, T, ZERO], 0,
            prelude='from __future__ import annotations\n', files={'__future__.py': 'annotations = 0\nsink(source())\n'},
            complete=False, cutoff='module-effects-unchecked',
            difference='Python executes the local __future__ module; the standard-library exception must not hide its effects.'),
    example('plain-class-unused', 'names boundaries', 'return sink(source())', [S, T], 17, witness=True,
            prelude="class Token:\n    'metadata'\n    code = 0\n"),
    example('plain-class-default-instance', 'names boundaries', 'return sink(source())', [S, T], 17, witness=True,
            prelude='class Token:\n    pass\nmarker = Token()\n'),
    example('plain-class-default-instance-blocked', 'names returns boundaries', 'return sink(relay(source()))', [S, ZERO], 0,
            prelude='class Token:\n    pass\nmarker = Token()\ndef relay(value):\n    return 0\n'),
    example('plain-class-method-deferred', 'names boundaries', 'return sink(source())', [S, T], 17, witness=True,
            prelude='class Token:\n    def __repr__(self):\n        return str(source())\nmarker = Token()\n', source_index=1),
    example('plain-class-constructor-deferred', 'names boundaries', 'return sink(source())', [S, T], 17, witness=True,
            prelude='class Token:\n    def __init__(self):\n        source()\n', source_index=1),
    example('plain-class-imported-helper', 'imports names returns', 'return sink(relay(source()))', [S, T], 17, witness=True,
            prelude='from helper import *\n', files={'helper.py': "__all__ = ['relay']\nclass Token:\n    def __repr__(self):\n        return 'token'\nmarker = Token()\ndef relay(value):\n    return value\n"}),
    example('plain-class-global-instance-read', 'names boundaries comparisons', 'return sink(marker is None)', [['sink', False]], False,
            prelude='class Token:\n    pass\nmarker = Token()\n', complete=False, cutoff='unknown-binding:marker'),
    example('plain-class-body-effect', 'names boundaries', 'return sink(source())', [S, S, T], 17, witness=True,
            prelude='class Token:\n    source()\n', complete=False, cutoff='module-effects-unchecked', source_index=1),
    example('plain-class-constructor-effect', 'names boundaries', 'return sink(source())', [S, S, T], 17, witness=True,
            prelude='class Token:\n    def __init__(self):\n        source()\nmarker = Token()\n',
            complete=False, cutoff='module-effects-unchecked', source_index=1),
    example('plain-class-constructor-arguments', 'names boundaries', 'return sink(source())', raised='TypeError', phase='definition', witness=True,
            prelude='class Token:\n    pass\nmarker = Token(0)\n', complete=False, cutoff='module-effects-unchecked',
            difference='Python stops at invalid constructor arguments; an incomplete model retains the entry call.'),
    example('plain-class-explicit-base', 'names boundaries', 'return sink(source())', [S, T], 17, witness=True,
            prelude='class Token(object):\n    pass\n'),
    example('builtin-base-dict-deferred', 'names boundaries', 'return sink(source())', [S, T], 17, witness=True,
            prelude='class Cache(dict):\n    def __init__(self, timeout):\n        source()\n', source_index=1),
    example('builtin-base-exception-deferred', 'names boundaries', 'return sink(source())', [S, T], 17, witness=True,
            prelude='class Skip(Exception):\n    pass\n'),
    example('builtin-base-object-instance', 'names boundaries', 'return sink(source())', [S, T], 17, witness=True,
            prelude='class Token(object):\n    pass\nmarker = Token()\n'),
    example('builtin-base-import', 'imports names defaults boundaries', 'return sink(relay(source()))', [S, T], 17, witness=True,
            prelude='from helper import Token, relay\ndef unused(key=Token):\n    return key\n',
            files={'helper.py': 'class Token(dict):\n    pass\ndef relay(value):\n    return value\n'}),
    example('builtin-base-reexport', 'imports names boundaries', 'return sink(relay(source()))', [S, ZERO], 0,
            prelude='from bridge import Token, relay\n', files={'helper.py': 'class Token(Exception):\n    pass\ndef relay(value):\n    return 0\n',
            'bridge.py': 'from helper import Token, relay\n'}),
    example('builtin-base-shadow-callback', 'names boundaries', 'return sink(0)', [S, T, ZERO], 0,
            prelude='class dict:\n    def __init_subclass__(cls):\n        sink(source())\nclass Token(dict):\n    pass\n',
            complete=False, cutoff='module-effects-unchecked'),
    example('builtin-base-computed-callback', 'names boundaries', 'return sink(0)', [S, T, ZERO], 0,
            prelude='def base():\n    sink(source())\n    return object\nclass Token(base()):\n    pass\n',
            complete=False, cutoff='module-effects-unchecked'),
    example('builtin-base-custom-constructor', 'names boundaries', 'return sink(0)', [S, T, ZERO], 0,
            prelude='class Token(dict):\n    def __init__(self):\n        sink(source())\nmarker = Token()\n',
            complete=False, cutoff='module-effects-unchecked'),
    example('builtin-base-inherited-construction', 'names boundaries', 'return sink(source())', [S, T], 17, witness=True,
            prelude='class Token(Exception):\n    pass\nmarker = Token()\n', complete=False, cutoff='module-effects-unchecked'),
    example('builtin-base-metaclass', 'names boundaries', 'return sink(source())', [S, T], 17, witness=True,
            prelude='class Token(object, metaclass=type):\n    pass\n', complete=False, cutoff='module-effects-unchecked'),
    example('builtin-base-unpacked', 'names boundaries', 'return sink(source())', [S, T], 17, witness=True,
            prelude='class Token(*[]):\n    pass\n', complete=False, cutoff='module-effects-unchecked'),
    example('builtin-base-shadowed-invalid', 'names boundaries', 'return sink(source())', raised='TypeError', phase='definition', witness=True,
            prelude='Exception = 0\nclass Token(Exception):\n    pass\n', complete=False, cutoff='module-effects-unchecked',
            difference='Python rejects the shadowed base before the entry call; the incomplete model retains that call.'),
    example('plain-class-descriptor-installation', 'names boundaries', 'return sink(0)', [S, T, ZERO], 0,
            prelude='class Descriptor:\n    def __set_name__(self, owner, name):\n        sink(source())\ndescriptor = Descriptor()\nclass Owner:\n    field = descriptor\n',
            complete=False, cutoff='module-effects-unchecked',
            difference='Python invokes a descriptor during class creation; an incomplete empty trace cannot establish no connection.'),
    example('module-literal-unused', 'names boundaries', 'return sink(source())', [S, T], 17,
            witness=True, prelude='metadata = -17\n'),
    example('module-literal-global-read', 'names boundaries', 'return sink(metadata)', [['sink', 5]], 5,
            prelude='metadata = 5\n', complete=False, cutoff='unknown-binding:metadata'),
    example('module-literal-repeated', 'names boundaries', 'return sink(source())', [S, T], 17,
            witness=True, prelude='metadata = 0\nmetadata = 1\n', complete=False, cutoff='module-effects-unchecked'),
    example('module-literal-shadow-function', 'names boundaries calls', 'return sink(relay(source()))', [S], raised='TypeError',
            witness=True, prelude='def relay(value):\n    return value\nrelay = 0\n',
            complete=False, cutoff='module-effects-unchecked',
            difference='A literal replaces the helper at runtime; an unchecked module binding cannot establish execution.'),
    example('module-literal-import-selected', 'imports names returns', 'return sink(helper.relay(source()))', [S, T], 17,
            witness=True, prelude='import helper\n', files={'helper.py': 'metadata = 0\ndef relay(value):\n    return value\n'}),
    example('module-literal-import-blocked', 'imports names returns', 'return sink(helper.relay(source()))', [S, ZERO], 0,
            prelude='import helper\n', files={'helper.py': 'metadata = "constant"\ndef relay(value):\n    return 0\n'}),
    example('arithmetic', 'operands locations', 'return sink(+source() + 2)', [S, ['sink', 19]], 19, witness=True),
    example('commented-operands', 'operands locations', 'return sink(source() + # café 🦀\n    0)', [S, T], 17, witness=True),
    example('commented-unary', 'operands locations', 'return sink(+ # café 🦀\n    source())', [S, T], 17, witness=True),
    example('arithmetic-cancellation', 'operands', 'value = source()\nreturn sink(value - value)', [S, ZERO], 0,
            witness=True, difference='Operand origins survive arithmetic cancellation; values are not interpreted.'),
    example('overwrite', 'assignments', 'value = source()\nvalue = 0\nreturn sink(value)', [S, ZERO], 0),
    example('destructure', 'assignments', '(left, [value, right]) = (0, [source(), 3])\nreturn sink(value)', [S, T], 17, witness=True),
    example('chain', 'assignments', 'left = right = source()\nreturn sink(left)', [S, T], 17, witness=True),
    example('repeat-target', 'assignments', 'value, value = source(), 0\nreturn sink(value)', [S, ZERO], 0),
    example('swap', 'assignments', 'left = source()\nright = 0\nleft, right = right, left\nreturn sink(right)', [S, T], 17, witness=True),
    example('and-skip', 'selectors', 'return False and sink(source())', returned=False),
    example('or-skip', 'selectors', 'return True or sink(source())', returned=True),
    example('none-select', 'selectors', 'return None or sink(source())', [S, T], 17, witness=True),
    example('conditional-order', 'selectors', 'return sink(source()) if not False else sink(0)', [S, T], 17, witness=True),
    example('comparison-order', 'comparisons', 'return source() < sink(source()) < 30', [S, S, T], False,
            witness=True, source_index=1),
    example('comparison-stop', 'comparisons', 'return 3 < 2 < sink(source())', returned=False, witness=True,
            difference='Comparison truth remains unknown, so later operands may be retained.'),
    example('if-join', 'statements', 'value = 0\nif False:\n    value = source()\nreturn sink(value)', [ZERO], 0, witness=True,
            difference='Statement conditions retain both CFG edges, including literal conditions.'),
    example('elif', 'statements', 'value = 0\nif False:\n    value = 1\nelif True:\n    value = source()\nelse:\n    value = 2\nreturn sink(value)', [S, T], 17, witness=True),
    example('loop-break', 'statements', 'value = 0\nwhile True:\n    value = source()\n    break\nelse:\n    value = 0\nreturn sink(value)', [S, T], 17, witness=True),
    example('loop-continue', 'statements', 'value = 0\nflag = True\nwhile flag:\n    value = source()\n    flag = False\n    continue\nelse:\n    return sink(value)', [S, T], 17, witness=True),
    example('positional', 'calls returns', 'return sink(relay(source(), 0))', [S, T], 17, witness=True,
            prelude='def relay(value, other, /):\n    return value\n'),
    example('keyword-order', 'calls returns', 'return sink(relay(other=sink(0), value=source()))', [ZERO, S, T], 17, witness=True,
            prelude='def relay(value, *, other):\n    return value\n'),
    example('literal-default', 'defaults calls', 'return sink(relay())', [ZERO], 0,
            prelude='def relay(value=0):\n    return value\n'),
    example('override-default', 'defaults calls', 'return sink(relay(source()))', [S, T], 17, witness=True,
            prelude='def relay(value=0):\n    return value\n'),
    example('early-return', 'returns', 'return 0\nsink(source())', returned=0),
    example('raise-stops-arguments', 'returns calls', 'return sink(relay(source(), sink(0)))', [S, ZERO], raised='TypeError',
            prelude='def relay(value, other):\n    raise value\n', normal=False, may_raise=True,
            difference='Explicit raise ends the helper path; argument effects occur before entry.'),
    example('raise-before-next-argument', 'returns calls', 'return relay(stop(), sink(source()))', raised='TypeError',
            prelude='def stop():\n    raise 0\ndef relay(value, other):\n    return other\n', normal=False, may_raise=True),
    example('sanitize', 'rules', 'return sink(clean(source()))', [S, ['clean', 17], ZERO], 0),
    example('propagate', 'rules', 'return sink(forward(source()))', [S, ['forward', 17], T], 17, witness=True),
    example('unicode-local', 'names locations', 'café = source(); return sink(café)', [S, T], 17, witness=True),
    example('unbound-local', 'names boundaries', 'sink(value)\nvalue = source()', raised='UnboundLocalError',
            complete=False, cutoff='unknown-binding'),
    example('shadow-source', 'names boundaries', 'sink(source())\nsource = 0', raised='UnboundLocalError',
            complete=False, cutoff='ambiguous-call'),
    example('implicit-exception', 'boundaries', 'value = 1 // 0\nreturn sink(source())', raised='ZeroDivisionError', witness=True,
            difference='Implicit exceptions are outside the contract, even when complete is true.'),
    example('handler', 'boundaries', 'try:\n    return 1 // 0\nexcept ZeroDivisionError:\n    return 7', returned=7,
            complete=False, cutoff='unsupported'),
    example('container', 'boundaries', 'value = [source()]\nreturn 0', [S], 0,
            complete=False, cutoff='assignment'),
    example('bare-raise', 'boundaries returns', 'raise', raised='RuntimeError', complete=False, cutoff='raise-contract-unchecked'),
    example('budget', 'boundaries', 'value = source()\nreturn sink(value)', [S, T], 17,
            complete=False, cutoff='budget', steps=1),
]

# Both directions are authored separately from analyzer output: replacing the selected
# argument with zero must remove its origin even when another argument is tainted.
for key, parameters, positive, negative in [
    ('ordinary-positional', 'left, right', 'source(), 0', '0, source()'),
    ('ordinary-keyword', 'left, right', 'right=0, left=source()', 'right=source(), left=0'),
    ('ordinary-mixed', 'left, right', 'source(), right=0', '0, right=source()'),
    ('positional-only', 'left, right, /', 'source(), 0', '0, source()'),
    ('positional-ordinary', 'left, /, right', 'source(), right=0', '0, right=source()'),
    ('ordinary-keyword-only', 'left, *, right', 'source(), right=0', '0, right=source()'),
    ('keyword-only', '*, left, right', 'right=0, left=source()', 'right=source(), left=0'),
    ('positional-keyword-only', 'left, /, *, right', 'source(), right=0', '0, right=source()'),
    ('all-kinds', 'left, /, value, *, right', 'source(), 0, right=0', '0, source(), right=0'),
]:
    for direction, arguments, tainted in [('selected', positive, True), ('discarded', negative, False)]:
        EXAMPLES.append(example('binding-' + key + '-' + direction, 'calls returns',
            'return sink(relay(' + arguments + '))', [S, T if tainted else ZERO], 17 if tainted else 0,
            witness=tainted, prelude='def relay(' + parameters + '):\n    return left\n'))

for key, parameters in [
    ('positional-tail', 'left, *tail'),
    ('keyword-tail', 'left, **tail'),
    ('both-tails', 'left, *tail, **options'),
]:
    for direction, argument, tainted in [('selected', 'source()', True), ('blocked', '0', False)]:
        EXAMPLES.append(example('fixed-variadic-' + key + '-' + direction, 'calls returns boundaries',
            'return sink(relay(' + argument + '))', [S, T] if tainted else [ZERO], 17 if tainted else 0,
            witness=tainted, prelude='def relay(' + parameters + '):\n    return left\n'))

EXAMPLES.extend([
    example('uncalled-variadic-body', 'calls returns boundaries', 'return sink(source())', [S, T], 17,
            witness=True, prelude='def unused(*items):\n    return opaque(items[0])\n'),
    example('uncalled-keyword-body', 'calls returns boundaries', 'return sink(0)', [ZERO], 0,
            prelude='def unused(**options):\n    return options["value"]\n'),
    example('uncalled-generator-body', 'calls returns boundaries', 'return sink(source())', [S, T], 17,
            witness=True, sink_index=1, prelude='def unused(*items):\n    yield sink(items[0])\n'),
    example('import-uncalled-variadic-body', 'imports calls returns defaults', 'return sink(relay(source()))',
            [S, T], 17, witness=True, prelude='from helper import relay\n',
            files={'helper.py': 'def unused(*items):\n    return opaque(items)\ndef relay(value=0):\n    return value\n'}),
    example('called-generator-body', 'calls returns boundaries', 'deferred(source())\nreturn 0', [S], 0,
            complete=False, cutoff='generator-function',
            prelude='def deferred(value):\n    yield sink(value)\n'),
    example('called-skipped-yield', 'calls returns selectors boundaries', 'deferred(source())\nreturn 0', [S], 0,
            complete=False, cutoff='generator-function',
            prelude='def deferred(value):\n    return False and (yield sink(value))\n'),
    example('called-lambda-default-yield', 'calls returns selectors boundaries', 'deferred(source())\nreturn 0',
            [S], 0, complete=False, cutoff='generator-function',
            prelude='def deferred(value):\n    return False and (lambda ignored=(yield sink(value)): 0)\n'),
    example('skipped-nested-generator', 'calls returns selectors', 'return sink(relay(source()))', [S, ZERO], 0,
            prelude='def relay(value):\n    return 0 if True else (lambda: (yield value))\n'),
    example('fixed-variadic-default', 'calls defaults returns', 'return sink(relay())', [ZERO], 0,
            prelude='def relay(left=0, *tail):\n    return left\n'),
    example('fixed-variadic-extra-effect', 'calls boundaries', 'return relay(0, sink(source()))', [S, T], 0,
            witness=True, complete=False, cutoff='invalid-call-binding',
            prelude='def relay(left, *tail):\n    return left\n'),
    example('fixed-variadic-extra-keyword', 'calls boundaries', 'return relay(0, unknown=sink(source()))', [S, T], 0,
            witness=True, complete=False, cutoff='invalid-call-binding',
            prelude='def relay(left, **tail):\n    return left\n'),
    example('fixed-variadic-referenced', 'calls boundaries', 'return relay(source())', [S], 17,
            complete=False, cutoff='unsupported-parameter-contract',
            prelude='def relay(left, *tail):\n    tail = 0\n    return left\n'),
])

for key, parameters, arguments in [
    ('missing', 'left, right', 'sink(source())'),
    ('excess', 'left, right', 'sink(source()), 0, 1'),
    ('duplicate', 'left, right', 'sink(source()), left=0, right=0'),
    ('unknown', 'left, right', 'left=sink(source()), unknown=0'),
    ('positional-only-keyword', 'left, /, right', 'left=sink(source()), right=0'),
    ('keyword-only-position', 'left, *, right', 'sink(source()), 0'),
    ('missing-keyword-only', 'left, /, *, right', 'sink(source())'),
]:
    EXAMPLES.append(example('binding-invalid-' + key, 'calls boundaries',
        'return relay(' + arguments + ')', [S, T], raised='TypeError', witness=True,
        prelude='def relay(' + parameters + '):\n    return left\n',
        complete=False, cutoff='invalid-call-binding'))

for key, parameters, arguments in [
    ('positional', 'value=0, /', ''),
    ('keyword-only', '*, value=0', ''),
    ('mixed', 'left, /, value=0, *, right', 'source(), right=0'),
    ('required-after-default', 'value=0, *, right', 'right=source()'),
]:
    effects = [S] if 'source()' in arguments else []
    EXAMPLES.append(example('default-' + key, 'defaults calls', 'return sink(relay(' + arguments + '))',
        effects + [ZERO], 0, prelude='def relay(' + parameters + '):\n    return value\n'))

for key, literal in [('none', 'None'), ('boolean', 'False'), ('signed', '-3'),
                     ('float', '+1.5'), ('string', '"café"'), ('bytes', 'b"x"'), ('parentheses', '(0)')]:
    EXAMPLES.append(example('default-literal-' + key, 'defaults calls', 'return sink(relay())', [ZERO], 0,
        prelude='def relay(value=' + literal + '):\n    return 0\n'))

EXAMPLES.extend([
    example('default-keyword-override', 'defaults calls', 'return sink(relay(value=source()))', [S, T], 17,
            witness=True, prelude='def relay(*, value=0):\n    return value\n'),
    example('binding-independent-callers', 'calls returns',
            'relay(left=source(), right=0)\nreturn sink(relay(left=0, right=source()))', [S, S, ZERO], 0,
            prelude='def relay(left, right):\n    return left\n'),
    example('binding-keyword-raise-order', 'calls returns',
            'return relay(right=stop(sink(source())), left=sink(0))', [S, T], raised='TypeError', witness=True,
            prelude='def stop(value):\n    raise value\ndef relay(left, right):\n    return left\n',
            normal=False, may_raise=True),
    example('binding-normalized-keyword', 'calls names boundaries', 'return sink(relay(left=source(), K=0))',
            [S, T], 17, prelude='def relay(left, K):\n    return left\n', complete=False,
            cutoff='non-ascii-keyword-binding-unchecked'),
    example('binding-varargs', 'calls boundaries', 'return relay(source())', [S], 0,
            prelude='def relay(*value):\n    return 0\n', complete=False, cutoff='invalid-call-binding'),
    example('binding-kwargs', 'calls boundaries', 'return relay(value=source())', [S], 0,
            prelude='def relay(**value):\n    return 0\n', complete=False, cutoff='invalid-call-binding'),
    example('binding-unpack', 'calls boundaries', 'return relay(*source())', [S], raised='TypeError',
            prelude='def relay(value):\n    return value\n', complete=False, cutoff='argument-unpacking-unchecked'),
    example('binding-unpack-keywords', 'calls boundaries', 'return relay(**source())', [S], raised='TypeError',
            prelude='def relay(value):\n    return value\n', complete=False, cutoff='argument-unpacking-unchecked'),
    example('binding-annotation', 'calls boundaries', 'return relay(source())', [S], 17,
            prelude='def relay(value: int):\n    return value\n', complete=False, cutoff='module-effects-unchecked'),
    example('default-evaluated-override', 'defaults boundaries', 'return relay(value=source())', [S, S], 17,
            prelude='def relay(value=source()):\n    return value\n', complete=False, cutoff='module-effects-unchecked'),
    example('default-definition-failure', 'defaults boundaries', 'return sink(source())', raised='ZeroDivisionError',
            phase='definition', prelude='def relay(value=1 // 0):\n    return value\n',
            complete=False, cutoff='module-effects-unchecked', witness=True,
            difference='Definition-time exceptions are refused; any retained entry flow remains incomplete.'),
    example('recursive-return', 'recursion returns', 'return sink(relay(source(), True))', [S, T], 17, witness=True,
            prelude='def relay(value, flag):\n    if flag:\n        return relay(value, False)\n    return value\n'),
    example('recursive-constant', 'recursion returns', 'return sink(relay(source(), True))', [S, ZERO], 0,
            prelude='def relay(value, flag):\n    if flag:\n        return relay(value, False)\n    return 0\n'),
    example('recursive-keyword', 'recursion calls', 'return sink(relay(source(), flag=True))', [S, T], 17, witness=True,
            prelude='def relay(value, /, *, flag):\n    if flag:\n        return relay(value, flag=False)\n    return value\n'),
    example('recursive-independent', 'recursion calls',
            'relay(source(), True)\nreturn sink(relay(0, True))', [S, ZERO], 0,
            prelude='def relay(value, flag):\n    if flag:\n        return relay(value, False)\n    return value\n'),
    example('recursive-no-return', 'recursion boundaries', 'return sink(relay(source()))', [S], raised='RecursionError',
            prelude='def relay(value):\n    return relay(value)\n', normal=False,
            difference='No modeled normal return is not a proof of divergence; CPython raises at its recursion limit.'),
    example('recursive-raise', 'recursion returns', 'return sink(relay(source(), True))', [S], raised='TypeError',
            prelude='def relay(value, flag):\n    if flag:\n        return relay(value, False)\n    raise value\n',
            normal=False, may_raise=True),
    example('mutual-return', 'recursion returns', 'return sink(relay(source(), True))', [S, T], 17, witness=True,
            prelude='def relay(value, flag):\n    if flag:\n        return bounce(value)\n    return value\n'
                    'def bounce(value):\n    return relay(value, False)\n'),
    example('mutual-no-return', 'recursion boundaries', 'return sink(relay(source()))', [S], raised='RecursionError',
            prelude='def relay(value):\n    return bounce(value)\ndef bounce(value):\n    return relay(value)\n',
            normal=False, difference='The fixed point has no normal return; runtime stack exhaustion is outside the model.'),
    example('recursive-budget', 'recursion boundaries', 'return sink(relay(source(), True))', [S, T], 17,
            prelude='def relay(value, flag):\n    if flag:\n        return relay(value, False)\n    return value\n',
            complete=False, cutoff='step-budget', steps=1),
])

for key, statement, call, entry_path, modules in [
    ('absolute', 'import helper', 'helper.relay', 'subject.py', {'helper.py': 'def relay(value):\n    return value\n'}),
    ('alias', 'import helper as bridge', 'bridge.relay', 'subject.py', {'helper.py': 'def relay(value):\n    return value\n'}),
    ('from-alias', 'from helper import relay as bridge', 'bridge', 'subject.py', {'helper.py': 'def relay(value):\n    return value\n'}),
    ('dotted', 'import pack.helper', 'pack.helper.relay', 'subject.py', {'pack/__init__.py': '', 'pack/helper.py': 'def relay(value):\n    return value\n'}),
    ('continued', 'import pack\\\n    .helper', 'pack.helper.relay', 'subject.py', {'pack/__init__.py': '', 'pack/helper.py': 'def relay(value):\n    return value\n'}),
    ('relative', 'from .helper import relay', 'relay', 'pack/subject.py', {'pack/__init__.py': '', 'pack/helper.py': 'def relay(value):\n    return value\n'}),
    ('relative-continued', 'from .\\\n    helper import relay', 'relay', 'pack/subject.py', {'pack/__init__.py': '', 'pack/helper.py': 'def relay(value):\n    return value\n'}),
    ('namespace', 'from pack.helper import relay', 'relay', 'subject.py', {'pack/helper.py': 'def relay(value):\n    return value\n'}),
    ('mixed-tree', 'from pack.inner.helper import relay', 'relay', 'subject.py', {'pack/inner/__init__.py': '', 'pack/inner/helper.py': 'def relay(value):\n    return value\n'}),
    ('reexport', 'from pack import relay', 'relay', 'subject.py', {'pack/__init__.py': 'from .helper import relay\n', 'pack/helper.py': 'def relay(value):\n    return value\n'}),
    ('child-fallback', 'from pack import helper', 'helper.relay', 'subject.py', {'pack/__init__.py': '', 'pack/helper.py': 'def relay(value):\n    return value\n'}),
    ('module-alias-chain', 'import bridge', 'bridge.helper.relay', 'subject.py', {'bridge.py': 'import helper\n', 'helper.py': 'def relay(value):\n    return value\n'}),
]:
    for direction, tainted in [('selected', True), ('discarded', False)]:
        contents = modules if tainted else {path: text.replace('return value', 'return 0') for path, text in modules.items()}
        EXAMPLES.append(example('import-' + key + '-' + direction, 'imports returns locations',
            'return sink(' + call + '(source()))', [S, T if tainted else ZERO], 17 if tainted else 0,
            witness=tainted, prelude=statement + '\n', files=contents, path=entry_path))

EXAMPLES.extend([
    example('import-keyword-default', 'imports calls defaults', 'return sink(helper.relay(right=source()))', [S, ZERO], 0,
            prelude='import helper\n', files={'helper.py': 'def relay(left=0, *, right):\n    return left\n'}),
    example('import-keyword-selected', 'imports calls locations', 'return sink(helper.relay(right=0, left=source()))', [S, T], 17,
            witness=True, prelude='import helper\n', files={'helper.py': 'def relay(*, left, right):\n    return left\n'}),
    example('import-helper-source', 'imports rules locations', 'return sink(helper.relay())', [S, T], 17,
            witness=True, prelude='import helper\n', files={'helper.py': 'def relay():\n    return source()\n'}),
    example('import-helper-sink', 'imports rules locations', 'return helper.relay(source())', [S, T], 0,
            witness=True, prelude='import helper\n', files={'helper.py': 'def relay(value):\n    sink(value)\n    return 0\n'}),
    example('import-same-named-helpers', 'imports names',
            'alpha.relay(source())\nreturn sink(beta.relay(source()))', [S, S, ZERO], 0,
            prelude='import alpha\nimport beta\n', files={'alpha.py': 'def relay(value):\n    return value\n',
                                                           'beta.py': 'def relay(value):\n    return 0\n'}),
    example('import-explicit-raise', 'imports returns', 'return sink(helper.relay(source()))', [S], raised='TypeError',
            prelude='import helper\n', files={'helper.py': 'def relay(value):\n    raise value\n'}, normal=False, may_raise=True),
    example('import-effect', 'imports boundaries', 'return sink(helper.relay(source()))', [S, S, T], 17,
            prelude='import helper\n', files={'helper.py': 'value = source()\ndef relay(value):\n    return value\n'},
            witness=True, complete=False, cutoff='module-effects-unchecked', source_index=1),
    example('import-cycle', 'imports boundaries', 'return sink(helper.relay(source()))', [S, T], 17,
            prelude='import helper\n', files={'helper.py': 'import subject\ndef relay(value):\n    return value\n'},
            witness=True),
    example('import-missing', 'imports boundaries', 'return sink(helper.relay(source()))', raised='ModuleNotFoundError',
            prelude='import helper\n', files={}, phase='definition', complete=False, cutoff='unresolved-local-import',
            witness=True, difference='Unresolved calls retain argument origins in an incomplete report; Python fails during import.'),
])
multiple_sinks = example('import-distinct-sink-files', 'imports rules locations',
    'value = source()\nalpha.relay(value)\nreturn beta.relay(value)', [S, T, T], 0, witness=True,
    prelude='import alpha\nimport beta\n', files={
        'alpha.py': 'def relay(value):\n    sink(value)\n    return 0\n',
        'beta.py': 'def relay(value):\n    sink(value)\n    return 0\n'})
multiple_sinks['expected_model']['pairs'] = [[0, 0], [0, 1]]
EXAMPLES.append(multiple_sinks)

for kind, exports in [('list', "['relay']"), ('tuple', "('relay',)"), ('repeated', "['relay', 'relay']")]:
    EXAMPLES.append(example('star-' + kind + '-selected', 'imports calls names locations',
        'return sink(relay(source()))', [S, T], 17, witness=True, prelude='from helper import *\n',
        files={'helper.py': '__all__ = ' + exports + '\ndef relay(value):\n    return value\n'}))
EXAMPLES.extend([
    example('star-discarded', 'imports returns', 'return sink(relay(source()))', [S, ZERO], 0,
            prelude='from helper import *\n', files={'helper.py': "__all__ = ['relay']\ndef relay(value):\n    return 0\n"}),
    example('star-empty', 'imports names', 'return sink(source())', [S, T], 17, witness=True,
            prelude='from helper import *\n', files={'helper.py': "__all__ = []\ndef relay(value):\n    return value\n"}),
    example('star-private', 'imports calls names', 'return sink(_relay(source()))', [S, T], 17, witness=True,
            prelude='from helper import *\n', files={'helper.py': "__all__ = ['_relay']\ndef _relay(value):\n    return value\n"}),
    example('star-child', 'imports calls', 'return sink(inner.relay(source()))', [S, T], 17, witness=True,
            prelude='from pack import *\n', files={'pack/__init__.py': "__all__ = ['inner']\n",
                                                 'pack/inner.py': 'def relay(value):\n    return value\n'}),
    example('star-unlisted', 'imports names boundaries', 'return sink(relay(source()))', raised='NameError',
            prelude='from helper import *\n', files={'helper.py': "__all__ = []\ndef relay(value):\n    return value\n"},
            witness=True, complete=False, cutoff='unknown-external-call',
            difference='An unlisted name raises before argument evaluation; incomplete analysis retains argument origins.'),
    example('star-computed', 'imports boundaries', 'return sink(relay(source()))', [S, T], 17,
            prelude='from helper import *\n', files={'helper.py': "__all__ = list(['relay'])\ndef relay(value):\n    return value\n"},
            witness=True, complete=False, cutoff='star-exports-unchecked'),
    example('star-mutated', 'imports names boundaries', 'return sink(relay(source()))', raised='NameError',
            prelude='from helper import *\n', files={'helper.py': "__all__ = ['relay']\n__all__.clear()\ndef relay(value):\n    return value\n"},
            witness=True, complete=False, cutoff='star-exports-unchecked',
            difference='Mutated exports stay unchecked; Python has no imported helper and never evaluates the argument.'),
    example('star-missing', 'imports boundaries', 'return sink(relay(source()))', raised='AttributeError', phase='definition',
            prelude='from helper import *\n', files={'helper.py': "__all__ = ['missing']\ndef relay(value):\n    return value\n"},
            witness=True, complete=False, cutoff='missing-or-ambiguous-import-member',
            difference='A listed missing attribute stops import; the incomplete model retains the later argument origin.'),
    example('star-rebound', 'imports names boundaries', 'return sink(relay(source()))', [S], raised='TypeError',
            prelude='from helper import *\nrelay = 0\n', files={'helper.py': "__all__ = ['relay']\ndef relay(value):\n    return value\n"},
            witness=True, complete=False, cutoff='ambiguous-module-binding',
            difference='Rebinding stays incomplete; Python evaluates the argument and then rejects the call.'),
])

# Boundary expectations are authored before collection. A stopped search is never
# an absence proof, and a CLI error has no analysis completeness or witness answer.
for depth in (2, 3):
    for direction, argument, events, returned in [('selected', 'source()', [S, T], 17),
                                                 ('discarded', '0', [ZERO], 0)]:
        EXAMPLES.append(example(f'discovery-depth-{depth}-{direction}', 'recursion returns boundaries',
            f'return sink(relay({argument}))', events, returned, witness=depth == 3 and direction == 'selected',
            prelude='def bounce(value):\n    return value\ndef relay(value):\n    return bounce(value)\n',
            depth=depth, complete=depth == 3, cutoff='call-depth-budget' if depth == 2 else None,
            difference='Discovery stops before the second helper; an incomplete report cannot rule out its return.'
                       if depth == 2 else None))

EXAMPLES.extend([
    example('refuse-entry-syntax', 'boundaries', 'return source() +', raised='SyntaxError', phase='definition',
            refusal='dataflow refuses syntax errors'),
    example('refuse-local-rule', 'rules names boundaries', 'return sink(source())', [T], 17,
            prelude='def source():\n    return 17\n', refusal='rule overlaps another rule or local definition.'),
    example('refuse-import-rule', 'imports rules names boundaries', 'return sink(source())', [T], 17,
            prelude='from helper import relay as source\n', files={'helper.py': 'def relay():\n    return 17\n'},
            refusal='rule overlaps another rule or local definition.'),
    example('refuse-import-entry', 'imports boundaries', 'return sink(source())', [S, T], 17,
            files={'subject.pyi': ''}, refusal='imported flow requires an unambiguous root-local Python source entry.'),
    example('refuse-step-budget', 'boundaries', 'return sink(source())', [S, T], 17, steps=0,
            refusal='invalid analysis budget'),
    example('refuse-depth-budget', 'boundaries', 'return sink(source())', [S, T], 17, depth=0,
            refusal='invalid analysis budget'),
    example('refuse-byte-budget', 'boundaries', 'return sink(source())', [S, T], 17, bytes_limit=1,
            refusal='invalid response byte budget'),
])

for direction, argument, events, returned in [('selected', 'source()', [S, T], 17),
                                             ('discarded', '0', [ZERO], 0)]:
    tainted = direction == 'selected'
    for kind, modules in [
        ('package', {'helper.py': 'def relay(value):\n    return 0\n',
                     'helper/__init__.py': 'def relay(value):\n    return value\n'}),
        ('stub', {'helper.py': 'def relay(value):\n    return value\n', 'helper.pyi': ''}),
    ]:
        EXAMPLES.append(example('import-ambiguous-' + kind + '-' + direction, 'imports names boundaries',
            f'return sink(helper.relay({argument}))', events, returned, witness=tainted,
            prelude='import helper\n', files=modules, module_files=['subject.py'],
            complete=False, cutoff='unresolved-local-import',
            difference='Python selects a runtime module; the static contract refuses competing candidates and retains argument origins.'))

    EXAMPLES.append(example('import-parameter-shadow-' + direction, 'imports names calls boundaries',
        f'return sink(relay(0, {argument}))', [S] if tainted else [], raised='AttributeError', witness=tainted,
        prelude='import helper\ndef relay(helper, value):\n    return helper.relay(value)\n',
        files={'helper.py': 'def relay(value):\n    return value\n'},
        complete=False, cutoff='ambiguous-call',
        difference='The parameter hides the imported module; an unknown call can retain arguments without a runtime sink call.'))
    EXAMPLES.append(example('import-assignment-shadow-' + direction, 'imports names assignments boundaries',
        f'value = helper.relay({argument})\nhelper = 0\nreturn sink(value)', raised='UnboundLocalError',
        witness=tainted, prelude='import helper\n', files={'helper.py': 'def relay(value):\n    return value\n'},
        complete=False, cutoff='ambiguous-call',
        difference='A later local assignment hides the module throughout the function; Python fails before evaluating arguments.'))
    EXAMPLES.append(example('import-global-shadow-' + direction, 'imports names boundaries',
        f'return sink(helper.relay({argument}))', raised='AttributeError', witness=tainted,
        prelude='import helper\nhelper = 0\n', files={'helper.py': 'def relay(value):\n    return value\n'},
        complete=False, cutoff='module-effects-unchecked',
        difference='Module initialization overwrites the import; unchecked initialization cannot justify runtime execution.'))
    EXAMPLES.append(example('import-binding-conflict-' + direction, 'imports names boundaries',
        f'return sink(relay({argument}))', events, returned, witness=tainted,
        prelude='import helper as relay\ndef relay(value):\n    return value\n',
        files={'helper.py': 'def bounce(value):\n    return value\n'},
        complete=False, cutoff='ambiguous-module-binding'))

    # The first helper stays admitted when unrelated imports exhaust the closure.
    # The control below fits the same boundary, without changing the analyzer.
    for count in (15, 17):
        modules = {f'unit{number}.py': 'def relay(value):\n    return value\n' for number in range(count)}
        EXAMPLES.append(example(f'import-module-limit-{count}-{direction}', 'imports boundaries locations',
            f'return sink(unit0.relay({argument}))', events, returned, witness=tainted,
            prelude=''.join(f'import unit{number}\n' for number in range(count)), files=modules,
            module_files=['subject.py', *(f'unit{number}.py' for number in range(min(count, 15)))],
            complete=count == 15, cutoff='import-module-budget' if count == 17 else None))
    EXAMPLES.append(example('import-byte-limit-' + direction, 'imports boundaries',
        f'return sink(helper.relay({argument}))', events, returned, witness=tainted,
        prelude='import helper\n', files={'helper.py': '# ' + 'x' * 262144 + '\ndef relay(value):\n    return value\n'},
        module_files=['subject.py'], complete=False, cutoff='import-source-byte-budget',
        difference='The helper exceeds the source budget; unknown call arguments survive only in an incomplete report.'))
    EXAMPLES.append(example('import-lookup-limit-' + direction, 'imports boundaries',
        f'return sink(alias0.relay({argument}))', events, returned, witness=tainted,
        prelude=''.join(f'import helper as alias{number}\n' for number in range(129)),
        files={'helper.py': 'def relay(value):\n    return value\n'},
        complete=False, cutoff='import-lookup-budget'))

    even, odd = [], []
    for number in range(20):
        (even if number % 2 == 0 else odd).append(
            f'from {"beta" if number % 2 == 0 else "alpha"} import link{number + 1} as link{number}\n')
    even.append('def link20(value):\n    return value\n')
    EXAMPLES.append(example('import-binding-limit-' + direction, 'imports boundaries',
        f'return sink(bridge({argument}))', raised='ImportError', phase='definition', witness=tainted,
        prelude='from alpha import link0 as bridge\n', files={'alpha.py': ''.join(even), 'beta.py': ''.join(odd)},
        module_lookup={'importer': 'subject.py', 'alias': 'bridge', 'resolution': 'budget', 'binding_chain_length': 16},
        complete=False, cutoff='missing-or-ambiguous-import-member',
        difference='Binding-chain search stops after sixteen hops; Python fails while loading the cyclic re-export chain.'))

    parts = [f'level{number}' for number in range(17)]
    modules = {'/'.join(parts[:count]) + '/__init__.py': '' for count in range(1, 17)}
    modules['/'.join(parts) + '.py'] = 'def relay(value):\n    return value\n'
    EXAMPLES.append(example('import-component-limit-' + direction, 'imports boundaries',
        f'return sink(bridge.relay({argument}))', events, returned, witness=tainted,
        prelude='import ' + '.'.join(parts) + ' as bridge\n', files=modules, module_files=['subject.py'],
        complete=False, cutoff='unsupported-import-form',
        difference='Module names beyond sixteen components are outside the static import contract.'))

    EXAMPLES.append(example('summary-function-limit-' + direction, 'calls returns boundaries',
        f'sink({argument})\n' + ''.join(f'if True:\n    hop{number}()\n' for number in range(65)) + 'return 0',
        events, 0, witness=tainted,
        prelude=''.join(f'def hop{number}():\n    return {number}\n' for number in range(65)),
        complete=False, cutoff='summary-function-budget'))

EXAMPLES.extend([
    example('value-import-scalar', 'imports names calls returns locations', 'return sink(relay(source()))', [S, T], 17,
            witness=True, prelude='from helper import marker, relay\n',
            files={'helper.py': 'marker = 0\ndef relay(value):\n    return value\n'}),
    example('value-import-blocked', 'imports names returns', 'return sink(relay(source()))', [S, ZERO], 0,
            prelude='from helper import marker, relay\n',
            files={'helper.py': 'marker = {}\ndef relay(value):\n    return 0\n'}),
    example('value-import-class', 'imports names calls returns', 'return sink(relay(source()))', [S, T], 17,
            witness=True, source_index=1, sink_index=1, prelude='from helper import Token, relay\n',
            files={'helper.py': 'class Token:\n    def __init__(self):\n        sink(source())\ndef relay(value):\n    return value\n'}),
    example('value-import-instance', 'imports names calls returns locations', 'return sink(relay(source()))', [S, T], 17,
            witness=True, source_index=1, prelude='from helper import marker, relay\n',
            files={'helper.py': 'class Token:\n    def __repr__(self):\n        return str(source())\nmarker = Token()\ndef relay(value):\n    return value\n'}),
    example('value-import-reexport', 'imports names calls locations', 'return sink(relay(source()))', [S, T], 17,
            witness=True, prelude='from alpha import marker, relay\n', files={
                'alpha.py': 'from helper import marker, relay\n',
                'helper.py': 'marker = (0,)\ndef relay(value):\n    return value\n'}),
    example('value-import-star', 'imports names calls', 'return sink(relay(source()))', [S, T], 17,
            witness=True, prelude='from helper import *\n',
            files={'helper.py': "__all__ = ['marker', 'relay']\nmarker = 0\ndef relay(value):\n    return value\n"}),
    example('value-import-read', 'imports names boundaries', 'return sink(marker)', [T], 17,
            prelude='from helper import marker\n', files={'helper.py': 'marker = 17\n'},
            complete=False, cutoff='unknown-binding:marker'),
    example('value-import-call', 'imports names boundaries', 'return sink(marker(source()))', [S], raised='TypeError',
            witness=True, prelude='from helper import marker\n', files={'helper.py': 'marker = 0\n'},
            complete=False, cutoff='unknown-external-call:marker',
            difference='A value import is not a callable helper; Python raises before the sink and the trace stays incomplete.'),
    example('value-import-constructor-call', 'imports names boundaries', 'Token()\nreturn sink(0)', [S, T, ZERO], 0,
            prelude='from helper import Token\n',
            files={'helper.py': 'class Token:\n    def __init__(self):\n        sink(source())\n'},
            complete=False, cutoff='unknown-external-call:Token'),
    example('value-import-named-default', 'imports defaults names boundaries', 'return sink(source())', [S, T], 17,
            witness=True, prelude='from helper import marker\ndef unused(value=marker):\n    return value\n',
            files={'helper.py': 'marker = 0\n'}),
    example('value-import-computed', 'imports names boundaries', 'return sink(0)', [S, ZERO], 0,
            prelude='from helper import marker\n', files={'helper.py': 'marker = source()\n'},
            complete=False, cutoff='missing-or-ambiguous-import-member'),
    example('value-import-package-precedence', 'imports names boundaries', 'return sink(marker.relay(source()))',
            raised='AttributeError', witness=True, prelude='from pack import marker\n', files={
                'pack/__init__.py': 'marker = 0\n',
                'pack/marker.py': 'def relay(value):\n    return value\n'}, module_files=['subject.py', 'pack/__init__.py'],
            complete=False, cutoff='unknown-external-call:marker.relay',
            difference='The existing package value prevents child loading; Python fails before evaluating the argument.'),
    example('function-doc-forward', 'calls names returns locations', 'return sink(relay(source()))', [S, T], 17,
            witness=True, prelude='def relay(value):\n    return value\nrelay.__doc__ = "helper"\n'),
    example('function-doc-blocked', 'calls names returns', 'return sink(relay(source()))', [S, ZERO], 0,
            prelude='def relay(value):\n    return 0\nrelay.__doc__ = None\n'),
    example('function-doc-import', 'imports calls names returns locations', 'return sink(relay(source()))', [S, T], 17,
            witness=True, prelude='from helper import relay\n',
            files={'helper.py': 'def relay(value):\n    return value\nrelay.__doc__ = "first"\nrelay.__doc__ = None\n'}),
    example('function-doc-uncalled-body', 'calls names boundaries', 'return sink(source())', [S, T], 17,
            witness=True, prelude='def relay(value):\n    return value.attribute\nrelay.__doc__ = "helper"\n'),
    example('function-doc-before-definition', 'names boundaries', 'return sink(source())', raised='NameError',
            phase='definition', witness=True, prelude='relay.__doc__ = "helper"\ndef relay(value):\n    return value\n',
            complete=False, cutoff='module-effects-unchecked',
            difference='A later definition cannot provide a function for an earlier documentation write.'),
    example('function-doc-computed', 'calls names boundaries', 'return sink(0)', [S, ZERO], 0,
            prelude='def relay(value):\n    return value\nrelay.__doc__ = source()\n',
            complete=False, cutoff='module-effects-unchecked'),
    example('function-doc-alias', 'names boundaries', 'return sink(source())', [S, T], 17,
            witness=True, prelude='def relay(value):\n    return value\npublic = relay\npublic.__doc__ = "helper"\n',
            complete=False, cutoff='module-effects-unchecked'),
    example('function-doc-imported-target', 'imports names boundaries', 'return sink(source())', [S, T], 17,
            witness=True, prelude='from helper import relay\nrelay.__doc__ = "helper"\n',
            files={'helper.py': 'def relay(value):\n    return value\n'},
            complete=False, cutoff='module-effects-unchecked'),
    example('function-doc-read', 'names boundaries', 'return sink(relay.__doc__)', [['sink', 'helper']], 'helper',
            prelude='def relay(value):\n    return value\nrelay.__doc__ = "helper"\n',
            complete=False, cutoff='unsupported-expression:attribute'),
    example('function-doc-descriptor', 'names boundaries', 'return sink(0)', [S, T, ZERO], 0,
            prelude='class Descriptor:\n    def __setattr__(self, name, value):\n        sink(source())\n'
                    'relay = Descriptor()\nrelay.__doc__ = "helper"\n',
            complete=False, cutoff='module-effects-unchecked'),
    example('function-defaults-write', 'defaults names boundaries', 'return sink(relay())', [ZERO], 0,
            prelude='def relay(value=17):\n    return value\nrelay.__defaults__ = (0,)\n',
            complete=False, cutoff='module-effects-unchecked'),
    example('builtin-references-unused', 'names boundaries', 'return sink(source())', [S, T], 17,
            witness=True, prelude='sorted = sorted\nmap = map\nfilter = filter\n'),
    example('builtin-reference-import-forward', 'imports names returns', 'return sink(relay(source()))', [S, T], 17,
            witness=True, prelude='from helper import relay\n',
            files={'helper.py': 'stored = sorted\ndef relay(value):\n    return value\n'}),
    example('builtin-reference-import-blocked', 'imports names returns', 'return sink(relay(source()))', [S, ZERO], 0,
            prelude='from helper import relay\n',
            files={'helper.py': 'stored = map\ndef relay(value):\n    return 0\n'}),
    example('builtin-reference-value-shadow', 'names boundaries', 'return sink(source())', [S, T], 17,
            witness=True, prelude='sorted = 0\nstored = sorted\n', complete=False, cutoff='module-effects-unchecked'),
    example('builtin-reference-function-shadow', 'calls names returns', 'return sink(stored(source()))', [S, ZERO], 0,
            prelude='def sorted(value):\n    return 0\nstored = sorted\n'),
    example('builtin-reference-repeated', 'names boundaries', 'return sink(source())', [S, T], 17,
            witness=True, prelude='sorted = sorted\nsorted = sorted\n', complete=False, cutoff='module-effects-unchecked'),
    example('builtin-reference-evaluated', 'calls boundaries', 'return sink(source())', [S, T], 17,
            witness=True, prelude='stored = abs(0)\n', complete=False, cutoff='module-effects-unchecked'),
    example('builtin-reference-parenthesized', 'names boundaries', 'return sink(source())', [S, T], 17,
            witness=True, prelude='stored = (sorted)\n', complete=False, cutoff='module-effects-unchecked'),
    example('builtin-reference-site-name', 'names boundaries', 'return sink(source())', [S, T], 17,
            witness=True, prelude='stored = help\n', complete=False, cutoff='module-effects-unchecked'),
    example('builtin-reference-special-target', 'names boundaries', 'return sink(source())', [S, T], 17,
            witness=True, prelude='__custom__ = sorted\n', complete=False, cutoff='module-effects-unchecked'),
    example('builtin-reference-custom-namespace', 'names boundaries', 'return sink(source())', [S, T], 17,
            witness=True, prelude='def unused(__builtins__):\n    return 0\nstored = sorted\n',
            complete=False, cutoff='module-effects-unchecked'),
    example('function-alias-selected', 'calls returns names locations', 'return sink(public(source()))', [S, T], 17,
            witness=True, prelude='def relay(value):\n    return value\npublic = relay\n'),
    example('function-alias-discarded', 'calls returns names', 'return sink(public(source()))', [S, ZERO], 0,
            prelude='def relay(value):\n    return 0\npublic = relay\n'),
    example('function-alias-keyword-discarded', 'calls names', 'return sink(public(right=source(), left=0))', [S, ZERO], 0,
            prelude='def relay(*, left, right):\n    return left\npublic = relay\n'),
    example('function-alias-default', 'calls defaults names', 'return sink(public())', [ZERO], 0,
            prelude='def relay(value=0):\n    return value\npublic = relay\n'),
    example('function-alias-import', 'imports calls names locations', 'return sink(public(source()))', [S, T], 17,
            witness=True, prelude='from helper import public\n',
            files={'helper.py': 'def relay(value):\n    return value\npublic = relay\n'}),
    example('function-alias-module', 'imports calls names locations', 'return sink(helper.public(source()))', [S, T], 17,
            witness=True, prelude='import helper\n',
            files={'helper.py': 'def relay(value):\n    return value\npublic = relay\n'}),
    example('function-alias-reexport', 'imports calls names locations', 'return sink(public(source()))', [S, T], 17,
            witness=True, prelude='from alpha import public\n', files={
                'alpha.py': 'from beta import relay\npublic = relay\n',
                'beta.py': 'def relay(value):\n    return value\n'}),
    example('function-alias-before-definition', 'names boundaries', 'return sink(source())', raised='NameError',
            phase='definition', witness=True, prelude='public = relay\ndef relay(value):\n    return value\n',
            complete=False, cutoff='module-effects-unchecked',
            difference='A later declaration cannot supply an earlier alias; Python fails before entry execution.'),
    example('function-alias-evaluated', 'calls boundaries', 'return sink(source())', [S, T], 17,
            witness=True, prelude='def relay():\n    return 0\npublic = relay()\n',
            complete=False, cutoff='module-effects-unchecked'),
    example('function-alias-rebound-source', 'names boundaries', 'return sink(public(source()))', [S, T], 17,
            witness=True, prelude='def relay(value):\n    return value\npublic = relay\nrelay = 0\n',
            complete=False, cutoff='module-effects-unchecked'),
    example('function-alias-local-shadow', 'names boundaries', 'public = 0\nreturn sink(public(source()))', [S],
            raised='TypeError', witness=True, prelude='def relay(value):\n    return value\npublic = relay\n',
            complete=False, cutoff='ambiguous-call:public',
            difference='The shadowed call preserves argument origins only in an incomplete report; Python raises before the sink.'),
    example('function-alias-hop-budget', 'names boundaries', 'return sink(public(source()))', [S, T], 17,
            witness=True, prelude='def relay(value):\n    return value\n' +
                'alias0 = relay\n' + ''.join(f'alias{n} = alias{n - 1}\n' for n in range(1, 15)) + 'public = alias14\n',
            complete=False, cutoff='module-effects-unchecked'),
    example('function-alias-before-import', 'imports names boundaries', 'return sink(source())', raised='NameError',
            phase='definition', witness=True, prelude='public = relay\nfrom helper import relay\n',
            files={'helper.py': 'def relay(value):\n    return value\n'},
            complete=False, cutoff='module-effects-unchecked',
            difference='A later import cannot initialize an earlier alias; Python raises before entry execution.'),
    example('function-alias-class-attribute', 'names boundaries', 'return sink(source())', [S, T], 17,
            witness=True, prelude='def relay(value):\n    return value\nclass Token:\n    public = relay\n',
            complete=False, cutoff='module-effects-unchecked'),
])

WIDE_NAMES = ['relay', *[f'helper{index}' for index in range(127)]]
WIDE_PROVIDER = '__all__ = ' + repr(WIDE_NAMES) + '\n' + ''.join(
    f'def {name}(value):\n    return value\n' for name in WIDE_NAMES)
MARKER_NAMES = ['EMPTY', *WIDE_NAMES[:-1]]
VALUE_MODULES = {name + '.py': '__all__ = ' + repr([f'{name}{index}' for index in range(65)]) + '\n' +
                 ''.join(f'{name}{index} = 0\n' for index in range(65)) for name in ('left', 'right')}
EXAMPLES.extend([
    example('shared-star-wide-forward', 'imports calls names locations', 'return sink(relay(source()))', [S, T], 17,
            witness=True, prelude='from helper import *\n', files={'helper.py': WIDE_PROVIDER}),
    example('shared-star-wide-discard', 'imports calls returns', 'return sink(relay(source()))', [S, ZERO], 0,
            prelude='from helper import *\n', files={'helper.py': WIDE_PROVIDER.replace('def relay(value):\n    return value', 'def relay(value):\n    return 0')}),
    example('shared-star-reexport', 'imports calls names locations', 'return sink(relay(source()))', [S, T], 17,
            witness=True, prelude='from bridge import *\n', files={'helper.py': WIDE_PROVIDER,
            'bridge.py': '__all__ = ' + repr(WIDE_NAMES) + '\nfrom helper import *\n'}),
    example('shared-star-marker', 'imports defaults names boundaries', 'return sink(relay(source()))', [S, T], 17,
            witness=True, prelude='from helper import *\ndef unused(key=EMPTY):\n    return key\n',
            files={'helper.py': '__all__ = ' + repr(MARKER_NAMES) + '\nclass Token:\n    pass\nEMPTY = Token()\n' +
                   ''.join(f'def {name}(value):\n    return value\n' for name in WIDE_NAMES[:-1])}),
    example('shared-star-startup', 'imports names boundaries', 'return sink(0)', [S, T, ZERO], 0,
            prelude='from helper import *\n', files={'helper.py': WIDE_PROVIDER + 'sink(source())\n'},
            complete=False, cutoff='module-effects-unchecked'),
    example('shared-star-child', 'imports calls locations', 'return sink(inner.relay(source()))', [S, T], 17,
            witness=True, prelude='from pack import *\n', files={'pack/__init__.py': "__all__ = ['inner', 'leaf']\n",
            'pack/inner.py': 'def relay(value):\n    return value\n', 'pack/leaf.py': 'pass\n'}),
    example('shared-star-over-export-limit', 'imports calls boundaries', 'return sink(relay(source()))', [S, T], 17,
            witness=True, prelude='from helper import *\n', files={'helper.py': WIDE_PROVIDER.replace(repr(WIDE_NAMES), repr([*WIDE_NAMES, 'extra'])) +
            'def extra(value):\n    return value\n'}, complete=False, cutoff='star-exports-unchecked'),
    example('shared-star-value-limit', 'imports names boundaries', 'return sink(source())', [S, T], 17,
            prelude='from left import *\nfrom right import *\n', files=VALUE_MODULES,
            refusal='import value declaration budget exceeds 128'),
])

EXAMPLES.extend([
    example('shared-named-wide-forward', 'imports calls names locations', 'return sink(relay(source()))', [S, T], 17,
            witness=True, prelude='from helper import ' + ', '.join(WIDE_NAMES) + '\n', files={'helper.py': WIDE_PROVIDER}),
    example('shared-named-wide-discard', 'imports calls returns', 'return sink(chosen(source()))', [S, ZERO], 0,
            prelude='from helper import relay as chosen, ' + ', '.join(WIDE_NAMES[1:]) + '\n',
            files={'helper.py': WIDE_PROVIDER.replace('def relay(value):\n    return value', 'def relay(value):\n    return 0')}),
    example('shared-named-reexport', 'imports calls names locations', 'return sink(relay(source()))', [S, T], 17,
            witness=True, prelude='from bridge import ' + ', '.join(WIDE_NAMES) + '\n', files={'helper.py': WIDE_PROVIDER,
            'bridge.py': 'from helper import ' + ', '.join(WIDE_NAMES) + '\n'}),
    example('shared-named-marker', 'imports defaults names boundaries', 'return sink(relay(source()))', [S, T], 17,
            witness=True, prelude='from helper import ' + ', '.join(MARKER_NAMES) + '\ndef unused(key=EMPTY):\n    return key\n',
            files={'helper.py': 'class Token:\n    pass\nEMPTY = Token()\n' +
                   ''.join(f'def {name}(value):\n    return value\n' for name in WIDE_NAMES[:-1])}),
    example('shared-named-startup', 'imports names boundaries', 'return sink(0)', [S, T, ZERO], 0,
            prelude='from helper import ' + ', '.join(WIDE_NAMES) + '\n', files={'helper.py': WIDE_PROVIDER + 'sink(source())\n'},
            complete=False, cutoff='module-effects-unchecked'),
    example('shared-named-child', 'imports calls locations', 'return sink(selected.relay(source()))', [S, T], 17,
            witness=True, prelude='from pack import marker, inner as selected\n', files={'pack/__init__.py': 'marker = 0\n',
            'pack/inner.py': 'def relay(value):\n    return value\n'}),
    example('shared-named-over-binding-limit', 'imports names boundaries', 'return sink(source())', [S, T], 17,
            witness=True, prelude='from helper import ' + ', '.join([*WIDE_NAMES, 'extra']) + '\n',
            files={'helper.py': WIDE_PROVIDER + 'def extra(value):\n    return value\n'},
            module_files=['subject.py'], complete=False, cutoff='import-binding-budget'),
    example('shared-named-value-limit', 'imports names boundaries', 'return sink(source())', [S, T], 17,
            prelude=''.join('from ' + module + ' import ' + ', '.join(f'{module}{index}' for index in range(65)) + '\n'
                            for module in ('left', 'right')), files=VALUE_MODULES,
            refusal='import value declaration budget exceeds 128'),
    example('named-default-prior-scalar', 'defaults names boundaries', 'return sink(source())', [S, T], 17,
            witness=True, prelude='marker = 0\ndef unused(key=marker):\n    raise RuntimeError()\n'),
    example('function-default-local', 'defaults names boundaries', 'return sink(source())', [S, T], 17,
            witness=True, prelude='def callback(value):\n    return unknown(value)\ndef unused(follow=callback):\n    return follow(0)\n'),
    example('function-default-local-alias', 'defaults names boundaries', 'return sink(source())', [S, T], 17,
            witness=True, prelude='def callback(value):\n    return unknown(value)\nstored = callback\ndef unused(follow=((stored))):\n    return follow(0)\n'),
    example('function-default-imported', 'imports defaults names boundaries', 'return sink(source())', [S, T], 17,
            witness=True, prelude='from helper import callback\ndef unused(follow=callback):\n    return follow(0)\n',
            files={'helper.py': 'def callback(value):\n    return unknown(value)\n'}),
    example('function-default-reexport', 'imports defaults names boundaries', 'return sink(source())', [S, T], 17,
            witness=True, prelude='from bridge import stored\ndef unused(follow=stored):\n    return follow(0)\n',
            files={'bridge.py': 'from helper import callback as stored\n',
                   'helper.py': 'def callback(value):\n    return unknown(value)\n'}),
    example('function-default-builtin', 'defaults names boundaries', 'return sink(source())', [S, T], 17,
            witness=True, prelude='def unused(value, factory=dict):\n    return factory(value)\n'),
    example('function-default-builtin-alias', 'defaults names boundaries', 'return sink(source())', [S, T], 17,
            witness=True, prelude='factory = dict\ndef unused(value, create=factory):\n    return create(value)\n'),
    example('function-default-dangerous-builtin-reference', 'defaults names boundaries', 'return sink(source())', [S, T], 17,
            witness=True, prelude='def unused(load=__import__, read=open):\n    return read(load("unknown"))\n'),
    example('function-default-forward', 'defaults names boundaries', 'return sink(source())', raised='NameError', phase='definition',
            prelude='def unused(follow=callback):\n    return follow(0)\ndef callback(value):\n    return value\n',
            complete=False, cutoff='module-effects-unchecked', witness=True,
            difference='Python reads the function name before its definition and stops the import; the entry trace cannot establish successful startup.'),
    example('function-default-late-import', 'imports defaults names boundaries', 'return sink(source())', raised='NameError', phase='definition',
            prelude='def unused(follow=callback):\n    return follow(0)\nfrom helper import callback\n',
            files={'helper.py': 'def callback(value):\n    return value\n'},
            complete=False, cutoff='module-effects-unchecked', witness=True,
            difference='A later import cannot provide an earlier default; Python stops before calling the entry function.'),
    example('function-default-evaluated', 'defaults calls boundaries', 'return sink(source())', [S, S, T], 17,
            witness=True, source_index=1,
            prelude='def callback(value):\n    return source()\ndef unused(follow=callback(0)):\n    return follow\n',
            complete=False, cutoff='module-effects-unchecked'),
    example('function-default-builtin-evaluated', 'defaults calls boundaries', 'return sink(source())', [S, T], 17,
            witness=True, prelude='def unused(factory=dict()):\n    return factory\n',
            complete=False, cutoff='module-effects-unchecked'),
    example('function-default-called', 'defaults calls boundaries', 'return sink(relay(source()))', [S, T], 17,
            prelude='def callback(value):\n    return value\ndef relay(value, follow=callback):\n    return follow(value)\n',
            complete=False, cutoff='unsupported-parameter-contract:relay'),
    example('named-default-prior-collection', 'defaults names boundaries', 'return sink(source())', [S, T], 17,
            witness=True, prelude='marker = []\ndef unused(*items, key=((marker)), **options):\n    return opaque(items)\n'),
    example('named-default-prior-class', 'defaults names boundaries', 'return sink(source())', [S, T], 17,
            witness=True, prelude='class marker:\n    def __init__(self):\n        raise RuntimeError()\ndef unused(key=marker):\n    return opaque(key)\n'),
    example('named-default-prior-instance', 'defaults names boundaries', 'return sink(source())', [S, T], 17,
            witness=True, prelude="class Token:\n    def __repr__(self):\n        return 'opaque'\nmarker = Token()\ndef unused(key=marker):\n    return opaque(key)\n"),
    example('named-default-import-alias', 'imports defaults calls names returns', 'return sink(relay(source()))', [S, T], 17,
            witness=True, prelude='from helper import marker as stored, relay\ndef unused(key=stored):\n    return opaque(key)\n',
            files={'helper.py': 'marker = 0\ndef relay(value):\n    return value\n'}),
    example('named-default-import-reexport', 'imports defaults calls names returns', 'return sink(relay(source()))', [S, T], 17,
            witness=True, prelude='from alpha import marker, relay\ndef unused(key=marker):\n    return opaque(key)\n', files={
                'alpha.py': 'from helper import marker, relay\n',
                'helper.py': 'class Token:\n    pass\nmarker = Token()\ndef relay(value):\n    return value\n'}),
    example('named-default-forward', 'defaults names boundaries', 'return sink(source())', raised='NameError', phase='definition',
            witness=True, prelude='def unused(key=marker):\n    return key\nmarker = 0\n',
            complete=False, cutoff='module-effects-unchecked',
            difference='Python reads the missing name before creating the function; an entry trace cannot establish that import succeeded.'),
    example('named-default-deleted', 'defaults names boundaries', 'return sink(source())', raised='NameError', phase='definition',
            witness=True, prelude='marker = 0\ndel marker\ndef unused(key=marker):\n    return key\n',
            complete=False, cutoff='module-effects-unchecked',
            difference='Python reads a deleted name during definition; the report retains unchecked initialization.'),
    example('named-default-rebound', 'defaults names boundaries', 'return sink(source())', [S, T], 17,
            witness=True, prelude='marker = 0\nmarker = 1\ndef unused(key=marker):\n    return key\n',
            complete=False, cutoff='module-effects-unchecked'),
    example('named-default-unknown-setup', 'defaults names boundaries', 'return sink(source())', [S, T], 17,
            witness=True, prelude='marker = 0\ndef setup():\n    pass\nsetup()\ndef unused(key=marker):\n    return key\n',
            complete=False, cutoff='module-effects-unchecked'),
    example('named-default-construction', 'defaults names boundaries', 'return sink(source())', [S, S, T], 17,
            witness=True, source_index=1,
            prelude='class Token:\n    def __init__(self):\n        source()\ndef unused(key=Token()):\n    return key\n',
            complete=False, cutoff='module-effects-unchecked'),
    example('named-default-call', 'defaults names calls boundaries', 'return sink(relay())', [ZERO], 0,
            prelude='marker = 0\ndef relay(value=marker):\n    return value\n'),
    example('scalar-default-override', 'defaults calls names returns', 'return sink(relay(source()))', [S, T], 17,
            witness=True, prelude='marker = 0\ndef relay(value=((marker))):\n    return value\n'),
    example('scalar-default-keyword-only', 'defaults calls names returns', 'return sink(relay())', [ZERO], 0,
            prelude='marker = 0\ndef relay(*, value=marker):\n    return value\n'),
    example('scalar-default-parameter-shadow', 'defaults calls names returns', 'return sink(relay(source()))', [S, ZERO], 0,
            prelude='marker = 0\ndef relay(marker, /, value=marker):\n    return value\n'),
    example('scalar-default-imported-helper', 'imports defaults calls names returns', 'return sink(relay())', [ZERO], 0,
            prelude='from helper import relay\n', files={'helper.py': 'marker = 0\ndef relay(value=marker):\n    return value\n'}),
    example('scalar-default-reexported-helper', 'imports defaults calls names returns', 'return sink(relay(source()))', [S, T], 17,
            witness=True, prelude='from alpha import relay\n', files={
                'alpha.py': 'from helper import relay\n', 'helper.py': 'marker = 0\ndef relay(value=marker):\n    return value\n'}),
    example('scalar-default-later-rebinding', 'defaults calls names boundaries', 'return sink(relay())', [ZERO], 0,
            prelude='marker = 0\ndef relay(value=marker):\n    return value\nmarker = 1\n',
            complete=False, cutoff='module-effects-unchecked'),
    example('scalar-default-later-deletion', 'defaults calls names boundaries', 'return sink(relay())', [ZERO], 0,
            prelude='marker = 0\ndef relay(value=marker):\n    return value\ndel marker\n',
            complete=False, cutoff='module-effects-unchecked'),
    example('scalar-default-imported-value', 'imports defaults calls names boundaries', 'return sink(relay())', [ZERO], 0,
            prelude='from helper import marker\ndef relay(value=marker):\n    return value\n', files={'helper.py': 'marker = 0\n'},
            complete=False, cutoff='unsupported-parameter-contract:'),
    example('type-registry-views', 'names calls boundaries', 'return sink(source())', [S, T], 17, witness=True,
            prelude='registry = {type(object.__dict__): dict, type({}.keys()): list, type({}.values()): list, type({}.items()): list}\n'),
    example('type-registry-direct', 'names boundaries', 'return sink(source())', [S, T], 17, witness=True,
            prelude='registry = {int: str, type(None): object, type([0]): tuple}\n'),
    example('type-registry-import', 'imports names calls returns', 'return sink(relay(source()))', [S, T], 17, witness=True,
            prelude='from helper import registry, relay\n', files={'helper.py': 'registry = {type({}.keys()): list}\ndef relay(value):\n    return value\n'}),
    example('type-registry-reexport', 'imports names calls returns', 'return sink(relay(source()))', [S, T], 17, witness=True,
            prelude='from alpha import registry, relay\n', files={
                'alpha.py': 'from helper import registry, relay\n',
                'helper.py': 'registry = {type(object.__dict__): dict}\ndef relay(value):\n    return value\n'}),
    example('type-registry-keeps-marker', 'defaults names boundaries', 'return sink(source())', [S, T], 17, witness=True,
            prelude='class Token:\n    pass\nmarker = Token()\nregistry = {type({}.keys()): list}\ndef unused(key=marker):\n    return opaque(key)\n'),
    example('type-registry-shadowed-type', 'names calls boundaries', 'return sink(source())', [S, S, T], 17,
            witness=True, source_index=1, prelude='def type(value):\n    source()\n    return int\nregistry = {type({}.keys()): list}\n',
            complete=False, cutoff='module-effects-unchecked'),
    example('type-registry-shadowed-value', 'names boundaries', 'return sink(source())', [S, T], 17,
            witness=True, prelude='list = dict\nregistry = {type({}.keys()): list}\n',
            complete=False, cutoff='module-effects-unchecked'),
    example('type-registry-user-hash', 'names calls boundaries', 'return sink(source())', [S, S, T], 17,
            witness=True, source_index=1, prelude='class Key:\n    def __hash__(self):\n        source()\n        return 0\nregistry = {Key(): list}\n',
            complete=False, cutoff='module-effects-unchecked'),
    example('type-registry-user-view', 'names calls boundaries', 'return sink(source())', [S, S, T], 17,
            witness=True, source_index=1, prelude='class Container:\n    def keys(self):\n        source()\n        return {}\ncustom = Container()\nregistry = {type(custom.keys()): list}\n',
            complete=False, cutoff='module-effects-unchecked'),
    example('type-registry-mutation', 'names boundaries', 'return sink(source())', [S, S, T], 17,
            witness=True, source_index=1, prelude='registry = {int: str}\nregistry[int] = source()\n',
            complete=False, cutoff='module-effects-unchecked'),
    example('type-registry-global-read', 'names boundaries', 'registry\nreturn sink(source())', [S, T], 17,
            witness=True, prelude='registry = {int: str}\n', complete=False, cutoff='unknown-binding:registry'),
    example('type-registry-called-default', 'defaults calls names boundaries', 'return sink(relay())', [ZERO], 0,
            prelude='registry = {int: str}\ndef relay(value=registry):\n    return 0\n',
            complete=False, cutoff='unsupported-parameter-contract:relay'),
    example('type-registry-at-budget', 'names boundaries', 'return sink(source())', [S, T], 17, witness=True,
            prelude='registry = {' + ','.join('int: list' for _ in range(42)) + '}\n'),
    example('type-registry-over-budget', 'names boundaries', 'return sink(source())', [S, T], 17, witness=True,
            prelude='registry = {' + ','.join('int: list' for _ in range(43)) + '}\n',
            complete=False, cutoff='module-effects-unchecked'),
])

for key, setup in [
    ('lambda', 'table = lambda value=0, /, *items, key=None, **options: unknown(value)\n'),
    ('literal', "table = {'send': [lambda value: unknown(value)]}\n"),
    ('constructor', "table = dict(send=lambda value: unknown(value), label='café')\n"),
    ('combined', "table = dict({'send': lambda: missing()}, count=0)\n"),
    ('item', "table = {}\ntable['send'] = lambda value: unknown(value)\n"),
    ('update', 'table = dict()\ntable.update(send=lambda: missing())\n'),
    ('replace', "table = {}\ntable['send'] = lambda: missing()\ntable.update(send=0)\n"),
]:
    for forwarded in (False, True):
        EXAMPLES.append(example('startup-registry-' + key + ('-selected' if forwarded else '-discarded'),
            'imports calls names returns boundaries', 'return sink(relay(source()))', [S, T if forwarded else ZERO],
            17 if forwarded else 0, witness=forwarded, prelude='from helper import table, relay\n',
            files={'helper.py': setup + 'def relay(value):\n    return ' + ('value' if forwarded else '0') + '\n'}))

EXAMPLES.extend([
    example('startup-registry-evaluated-value', 'imports calls names boundaries', 'return sink(relay(source()))',
            [S, S, T], 17, witness=True, source_index=1, prelude='from helper import table, relay\n',
            files={'helper.py': 'table = dict(send=source())\ndef relay(value):\n    return value\n'},
            complete=False, cutoff='module-effects-unchecked'),
    example('startup-registry-evaluated-default', 'defaults names calls boundaries', 'return sink(source())',
            [S, S, T], 17, witness=True, source_index=1, prelude='table = lambda value=source(): value\n',
            complete=False, cutoff='module-effects-unchecked'),
    example('startup-registry-mutation-call', 'names calls boundaries', 'return sink(source())',
            [S, S, T], 17, witness=True, source_index=1, prelude="table = {}\ntable['send'] = source()\n",
            complete=False, cutoff='module-effects-unchecked'),
    example('startup-registry-shared-write', 'imports names boundaries', 'return sink(source())',
            [S, T], 17, witness=True, prelude="from helper import table\ntable['send'] = 0\n",
            files={'helper.py': 'table = {}\n'}, complete=False, cutoff='module-effects-unchecked'),
    example('startup-registry-global-read', 'names boundaries', 'table\nreturn sink(source())',
            [S, T], 17, witness=True, prelude='table = dict(send=lambda: missing())\n',
            complete=False, cutoff='unknown-binding:table'),
    example('startup-registry-called-callback', 'names calls boundaries', "return sink(table['send'](source()))",
            [S, T], 17, prelude="table = {'send': lambda value: value}\n",
            complete=False, cutoff='dynamic-or-attribute-call'),
])

for arrangement, contents in [
    ('module-handles', {'helper.py': 'import neighbor\ndef relay(value):\n    return ANSWER\n',
                        'neighbor.py': 'import helper\n'}),
    ('earlier-function', {'helper.py': 'def relay(value):\n    return ANSWER\nfrom neighbor import flag\n',
                          'neighbor.py': 'from helper import relay\nflag = 0\n'}),
    ('earlier-exports', {'helper.py': "__all__ = ['relay']\ndef relay(value):\n    return ANSWER\nfrom neighbor import flag\n",
                         'neighbor.py': 'from helper import *\nflag = 0\n'}),
    ('earlier-default', {'helper.py': 'LIMIT = 0\nfrom neighbor import unused\ndef relay(value):\n    return ANSWER\n',
                         'neighbor.py': 'from helper import LIMIT\ndef unused(value=LIMIT):\n    return value\n'}),
    ('self-member', {'helper.py': 'def relay(value):\n    return ANSWER\nfrom helper import relay as public\n'}),
    ('parent-member', {'helper.py': 'from pack import relay\n',
                        'pack/__init__.py': 'def relay(value):\n    return ANSWER\nfrom . import inner\n',
                        'pack/inner.py': 'from pack import relay\n'}),
]:
    for selected in (False, True):
        EXAMPLES.append(example('import-order-' + arrangement + ('-selected' if selected else '-discarded'),
            'imports names returns boundaries', 'return sink(relay(source()))', [S, T if selected else ZERO],
            17 if selected else 0, witness=selected, prelude='from helper import relay\n',
            files={path: text.replace('ANSWER', 'value' if selected else '0') for path, text in contents.items()}))

for arrangement, contents, error in [
    ('late-function', {'helper.py': 'from neighbor import flag\ndef relay(value):\n    return value\n',
                       'neighbor.py': 'from helper import relay\nflag = 0\n'}, 'ImportError'),
    ('late-default', {'helper.py': 'from neighbor import unused\nLIMIT = 0\ndef relay(value):\n    return value\n',
                      'neighbor.py': 'from helper import LIMIT\ndef unused(value=LIMIT):\n    return value\n'}, 'ImportError'),
    ('late-export', {'helper.py': "__all__ = ['relay']\nfrom neighbor import flag\ndef relay(value):\n    return value\n",
                     'neighbor.py': 'from helper import *\nflag = 0\n'}, 'AttributeError'),
    ('late-self-member', {'helper.py': 'from helper import relay as public\ndef relay(value):\n    return value\n'}, 'ImportError'),
]:
    EXAMPLES.append(example('import-order-' + arrangement, 'imports names boundaries', 'return sink(0)',
        raised=error, phase='definition', prelude='from helper import relay\n', files=contents,
        complete=False, cutoff='cyclic-module-initialization',
        difference='Python cannot read a name that has not been created yet; the model keeps the import incomplete.'))

for selected in (False, True):
    EXAMPLES.append(example('import-order-package-entry' + ('-selected' if selected else '-discarded'),
        'imports names returns boundaries', 'return sink(relay(source()))', [S, T if selected else ZERO],
        17 if selected else 0, witness=selected, path='pack/inner.py', prelude='from pack import relay\n',
        files={'pack/__init__.py': 'def relay(value):\n    return ' + ('value' if selected else '0') + '\nfrom . import inner\n'}))
EXAMPLES.append(example('import-order-unknown-effects', 'imports names boundaries', 'return sink(relay(source()))',
    [S, T], 17, witness=True, prelude='from helper import relay\n',
    files={'helper.py': 'import neighbor\ndef relay(value):\n    return value\n',
           'neighbor.py': 'import helper\nstate = 1 + 2\n'}, complete=False, cutoff='module-effects-unchecked'))
for form, statement in [('module', 'import absent_dependency\n'), ('member', 'from absent_dependency import missing\n')]:
    EXAMPLES.append(example('import-order-unresolved-' + form, 'imports names boundaries', 'return sink(0)',
        raised='ModuleNotFoundError', phase='definition', prelude='from helper import relay\n',
        files={'helper.py': statement + 'import neighbor\ndef relay(value):\n    return value\n',
               'neighbor.py': 'import helper\n'}, complete=False, cutoff='unresolved-local-import:absent_dependency',
        difference='Python stops at the missing import; the incomplete analysis must not claim an unavailable cycle member.'))

for names in ('marker, inner', 'inner, marker'):
    EXAMPLES.append(example('import-order-preload-' + names.split(',')[0], 'imports names boundaries',
        'return sink(0)', raised='ImportError', phase='definition', prelude='from pack import ' + names + '\n',
        files={'pack/__init__.py': 'marker = 0\n',
               'pack/inner.py': 'from subject import marker\ndef relay(value):\n    return value\n'},
        complete=False, cutoff='cyclic-module-initialization',
        difference='Python loads the requested child before binding either imported name in the caller.'))
for selected in (False, True):
    EXAMPLES.append(example('import-order-ordinary-sequence-' + str(selected), 'imports names returns boundaries',
        'return sink(inner.relay(source()))', [S, T if selected else ZERO], 17 if selected else 0, witness=selected,
        prelude='import helper as marker, pack.inner as inner\n', files={'helper.py': 'flag = 0\n', 'pack/__init__.py': '',
            'pack/inner.py': 'from subject import marker\ndef relay(value):\n    return ' + ('value' if selected else '0') + '\n'}))
    EXAMPLES.append(example('import-order-written-exports-' + str(selected), 'imports names returns boundaries',
        'return sink(a.relay(source()))', [S, T if selected else ZERO], 17 if selected else 0, witness=selected,
        prelude='from pack import *\n', files={'pack/__init__.py': "__all__ = ['a', 'z']\n",
            'pack/a.py': 'def relay(value):\n    return ' + ('value' if selected else '0') + '\nfrom .z import ready\n',
            'pack/z.py': 'from .a import relay\nready = 0\n'}))
for exports, works in [(['z', 'a'], False), (['a', 'z', 'a'], True), (['z', 'a', 'z'], False)]:
    EXAMPLES.append(example('import-order-written-exports-' + ''.join(exports), 'imports names boundaries',
        'return sink(a.relay(source()))' if works else 'return sink(0)', [S, T] if works else [], 17 if works else None,
        witness=works, raised=None if works else 'ImportError', phase='entry' if works else 'definition',
        prelude='from pack import *\n', files={'pack/__init__.py': '__all__ = ' + repr(exports) + '\n',
            'pack/a.py': 'def relay(value):\n    return value\nfrom .z import ready\n',
            'pack/z.py': 'from .a import relay\nready = 0\n'}, complete=works,
        cutoff=None if works else 'cyclic-module-initialization',
        difference=None if works else 'Changing the written export order starts the failing side of the same cycle first.'))

EXAMPLES.extend([
    example('parallel-builtin-defaults', 'assignments defaults names', 'return sink(relay(source()))', [S, T], 17,
            witness=True, prelude='_map, _filter = map, filter\ndef unused(callback=_map, predicate=_filter):\n    return opaque()\ndef relay(value):\n    return value\n'),
    example('parallel-builtin-before-writes', 'assignments defaults names calls', 'return sink(map(source()))', [S, T], 17,
            witness=True, prelude='def relay(value):\n    return value\nmap, saved = relay, map\ndef unused(callback=saved):\n    return opaque()\n'),
    example('parallel-functions-forward', 'assignments calls names locations', 'return sink(public(source()))', [S, T], 17,
            witness=True, prelude='def relay(value):\n    return value\ndef stop(value):\n    return 0\npublic, discarded = relay, stop\n'),
    example('parallel-functions-stop', 'assignments calls names', 'return sink(public(source()))', [S, ZERO], 0,
            prelude='def relay(value):\n    return value\ndef stop(value):\n    return 0\n[public, discarded] = [stop, relay]\n'),
    example('parallel-functions-reexport', 'imports assignments calls names locations', 'return sink(public(source()))', [S, T], 17,
            witness=True, prelude='from bridge import public\n', files={
                'bridge.py': 'from helper import public\n',
                'helper.py': 'def relay(value):\n    return value\ndef stop(value):\n    return 0\npublic, discarded = relay, stop\n'}),
    example('parallel-imported-functions', 'imports assignments calls names', 'return sink(public(source()))', [S, T], 17,
            witness=True, prelude='from helper import public\n', files={
                'helper.py': 'from provider import relay, stop\npublic, discarded = relay, stop\n',
                'provider.py': 'def relay(value):\n    return value\ndef stop(value):\n    return 0\n'}),
    example('parallel-literal-imports', 'imports assignments names locations', 'return sink(relay(source()))', [S, T], 17,
            witness=True, prelude='from helper import left, right, relay\n',
            files={'helper.py': 'left, right = 0, 1\ndef relay(value):\n    return value\n'}),
    example('parallel-scalar-default', 'assignments defaults calls', 'return sink(relay())', [ZERO], 0,
            prelude='marker, other = 0, 1\ndef relay(value=marker):\n    return value\n'),
    example('parallel-snapshot-missing', 'assignments names boundaries', 'return sink(source())', raised='NameError',
            phase='definition', witness=True, prelude='def relay(value):\n    return value\npublic, other = relay, public\n',
            complete=False, cutoff='module-effects-unchecked',
            difference='The new first target cannot supply a later RHS value; Python raises before writing either name.'),
    example('parallel-length-mismatch', 'assignments boundaries', 'return sink(source())', raised='ValueError',
            phase='definition', witness=True, prelude='public, other = (0,)\n',
            complete=False, cutoff='module-effects-unchecked',
            difference='Python refuses the unpacking length during module setup; the model leaves that setup unchecked.'),
    example('parallel-evaluated-value', 'assignments calls boundaries', 'return sink(source())', [S, S, T], 17,
            witness=True, source_index=1, prelude='def relay(value):\n    return value\npublic, other = relay, source()\n',
            complete=False, cutoff='module-effects-unchecked'),
    example('parallel-duplicate-target', 'assignments names boundaries', 'return sink(source())', [S, T], 17,
            witness=True, prelude='public, public = 0, 1\n', complete=False, cutoff='module-effects-unchecked'),
    example('parallel-nested-pattern', 'assignments boundaries', 'return sink(source())', [S, T], 17,
            witness=True, prelude='public, (other,) = 0, (1,)\n', complete=False, cutoff='module-effects-unchecked'),
    example('parallel-captured-function', 'assignments defaults names', 'return sink(relay(source()))', [S, T], 17,
            witness=True, prelude='def relay(value):\n    return value\ndef stop(value):\n    return 0\npublic, discarded = relay, stop\ndef unused(callback=public):\n    return callback\n'),
    example('parallel-global-value-read', 'assignments names boundaries', 'return sink(public)', [ZERO], 0,
            prelude='public, other = 0, 1\n', complete=False, cutoff='unknown-binding:public'),
])

RENAMES = {'entry': 'compute', 'relay': 'transfer', '_relay': '_transfer', 'stop': 'halt', 'source': 'read_value',
           'sink': 'emit', 'clean': 'escape_value', 'forward': 'pass_value',
           'value': 'payload', 'other': 'extra', 'left': 'first', 'right': 'second', 'café': 'thé', 'flag': 'condition',
           'bounce': 'recur', 'helper': 'engine', 'pack': 'parcel', 'inner': 'inside', 'bridge': 'gateway',
           'alpha': 'cedar', 'beta': 'maple', 'subject': 'renamed', 'Token': 'Tag', 'marker': 'sentinel',
           'Descriptor': 'Notifier', 'Owner': 'Receiver', 'descriptor': 'notifier', 'public': 'exported'}
for prefix, count in [('unit', 17), ('alias', 129), ('link', 21), ('level', 17), ('hop', 65)]:
    RENAMES.update({f'{prefix}{number}': f'renamed_{prefix}{number}' for number in range(count)})


def cases():
    result = []
    for example in EXAMPLES:
        for moved in (False, True):
            def transform(source):
                if not moved:
                    return source
                tokens = tokenize.generate_tokens(io.StringIO(source).readline)
                transformed = []
                export_statement = False
                for token in tokens:
                    if token.type == tokenize.NAME and token.string == '__all__':
                        export_statement = True
                    elif token.type == tokenize.NEWLINE or token.string == ';':
                        export_statement = False
                    if token.type == tokenize.NAME:
                        token = token._replace(string=RENAMES.get(token.string, token.string))
                    elif token.type == tokenize.STRING and export_statement:
                        value = ast.literal_eval(token.string)
                        if isinstance(value, str) and value in RENAMES:
                            token = token._replace(string=repr(RENAMES[value]))
                    transformed.append(token)
                return '# relocated π 🦀\n\n' + tokenize.untokenize(transformed)
            def relocate(path):
                if not moved:
                    return path
                if not example['imports']:
                    return 'nested/renamed/module.py'
                stem, suffix = path.rsplit('.', 1)
                return '/'.join(RENAMES.get(part, part) for part in stem.split('/')) + '.' + suffix
            source = transform(example['source'])
            path = relocate(example['path'])
            files = {relocate(name): transform(text) for name, text in example['files'].items()}
            files[path] = source
            names = {name: RENAMES[name] if moved else name for name in ('entry', 'source', 'sink', 'clean', 'forward')}
            module_files = [relocate(name) for name in example['module_files']] if example['module_files'] is not None else None
            lookup = example['module_lookup']
            if lookup is not None:
                lookup = {**lookup, 'importer': relocate(lookup['importer']),
                          'alias': RENAMES.get(lookup['alias'], lookup['alias']) if moved else lookup['alias']}
            model = example['expected_model']
            cutoff = model.get('cutoff')
            if moved and cutoff and ':' in cutoff:
                prefix, name = cutoff.split(':', 1)
                renamed = RENAMES.get(name, name)
                if '.' in name and all(part.isidentifier() for part in name.split('.')):
                    renamed = '.'.join(RENAMES.get(part, part) for part in name.split('.'))
                model = {**model, 'cutoff': prefix + ':' + renamed}
            result.append({**example, 'id': example['id'] + ('/moved' if moved else '/original'),
                'source': source, 'path': path, 'files': files,
                'module_files': module_files, 'module_lookup': lookup,
                'expected_model': model,
                'names': names, 'variant': 'moved' if moved else 'original'})
    return result
