"""Independent executable examples for the declared scalar Python contract."""
import io
import tokenize


RULES = {
    'operands': 'Scalar operand origins join; evaluation follows source order and ignores comments.',
    'assignments': 'Evaluate scalar RHS leaves once, then write literal-shaped targets from left to right.',
    'selectors': 'Boolean and None literals select expression paths; unknown truth retains alternatives.',
    'comparisons': 'Evaluate ordered comparison operands; comparison truth remains unknown.',
    'statements': 'Join if/elif alternatives and loop fixed points; preserve break, continue and loop else.',
    'calls': 'Evaluate explicit arguments in source order, then bind positions and exact keyword names.',
    'defaults': 'Omitted immutable literal defaults carry no source origins; explicit arguments override them.',
    'returns': 'Substitute scalar helper summaries; return and explicit raise terminate their paths.',
    'recursion': 'Recursive scalar summaries grow to a fixed point; each call substitutes its own origins.',
    'imports': 'Resolve static local modules within one root; preserve defining-file identities and lookup evidence.',
    'rules': 'Sources, sinks, sanitizers and propagators are caller-supplied contracts.',
    'names': 'Local bindings and shadowing prevent guessed external calls or unbound value claims.',
    'locations': 'Witnesses retain source revisions, UTF-8 byte spans and one-based Unicode columns.',
    'boundaries': 'Unsupported syntax and exhausted budgets remain incomplete; implicit exceptions are outside the model.',
}


def example(key, rules, body, events=(), returned=None, raised=None, witness=False,
            prelude='', complete=True, cutoff=None, difference=None, steps=4096, source_index=0,
            normal=True, may_raise=False, files=None, path='subject.py', phase='entry'):
    return {'id': key, 'rules_checked': rules.split(),
            'source': prelude + 'def entry():\n' + ''.join('    ' + line + '\n' for line in body.splitlines()),
            'expected_runtime': {'events': list(events), 'returned': returned, 'raised': raised, 'phase': phase},
            'expected_model': {'witness': witness, 'pairs': [[source_index, 0]] if witness else [],
                               'complete': complete, 'cutoff': cutoff,
                               'completion': {'normal_return': normal, 'may_raise': may_raise} if complete else None},
            'difference': difference, 'steps': steps, 'files': files or {}, 'path': path,
            'imports': files is not None}


S = ['source', 17]
T = ['sink', 17]
ZERO = ['sink', 0]
EXAMPLES = [
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
            prelude='def relay(*value):\n    return 0\n', complete=False, cutoff='module-effects-unchecked'),
    example('binding-kwargs', 'calls boundaries', 'return relay(value=source())', [S], 0,
            prelude='def relay(**value):\n    return 0\n', complete=False, cutoff='module-effects-unchecked'),
    example('binding-unpack', 'calls boundaries', 'return relay(*source())', [S], raised='TypeError',
            prelude='def relay(value):\n    return value\n', complete=False, cutoff='unsupported-call-syntax'),
    example('binding-unpack-keywords', 'calls boundaries', 'return relay(**source())', [S], raised='TypeError',
            prelude='def relay(value):\n    return value\n', complete=False, cutoff='unsupported-call-syntax'),
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
            witness=True, complete=False, cutoff='cyclic-module-initialization'),
    example('import-missing', 'imports boundaries', 'return sink(helper.relay(source()))', raised='ModuleNotFoundError',
            prelude='import helper\n', files={}, phase='definition', complete=False, cutoff='unresolved-local-import'),
])
RENAMES = {'entry': 'compute', 'relay': 'transfer', 'stop': 'halt', 'source': 'read_value',
           'sink': 'emit', 'clean': 'escape_value', 'forward': 'pass_value',
           'value': 'payload', 'other': 'extra', 'left': 'first', 'right': 'second', 'café': 'thé', 'flag': 'condition',
           'bounce': 'recur', 'helper': 'engine', 'pack': 'parcel', 'inner': 'inside', 'bridge': 'gateway',
           'alpha': 'cedar', 'beta': 'maple', 'subject': 'renamed'}


def cases():
    result = []
    for example in EXAMPLES:
        for moved in (False, True):
            def transform(source):
                if not moved:
                    return source
                tokens = tokenize.generate_tokens(io.StringIO(source).readline)
                return '# relocated π 🦀\n\n' + tokenize.untokenize(
                    token._replace(string=RENAMES.get(token.string, token.string))
                    if token.type == tokenize.NAME else token for token in tokens)
            def relocate(path):
                if not moved:
                    return path
                if not example['imports']:
                    return 'nested/renamed/module.py'
                return '/'.join(RENAMES.get(part, part) for part in path[:-3].split('/')) + '.py'
            source = transform(example['source'])
            path = relocate(example['path'])
            files = {relocate(name): transform(text) for name, text in example['files'].items()}
            files[path] = source
            names = {name: RENAMES[name] if moved else name for name in ('entry', 'source', 'sink', 'clean', 'forward')}
            result.append({**example, 'id': example['id'] + ('/moved' if moved else '/original'),
                'source': source, 'path': path, 'files': files,
                'names': names, 'variant': 'moved' if moved else 'original'})
    return result
