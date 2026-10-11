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
    'rules': 'Sources, sinks, sanitizers and propagators are caller-supplied contracts.',
    'names': 'Local bindings and shadowing prevent guessed external calls or unbound value claims.',
    'locations': 'Witnesses retain source revisions, UTF-8 byte spans and one-based Unicode columns.',
    'boundaries': 'Unsupported syntax and exhausted budgets remain incomplete; implicit exceptions are outside the model.',
}


def example(key, rules, body, events=(), returned=None, raised=None, witness=False,
            prelude='', complete=True, cutoff=None, difference=None, steps=4096):
    return {'id': key, 'rules_checked': rules.split(),
            'source': prelude + 'def entry():\n' + ''.join('    ' + line + '\n' for line in body.splitlines()),
            'expected_runtime': {'events': list(events), 'returned': returned, 'raised': raised},
            'expected_model': {'witness': witness, 'complete': complete, 'cutoff': cutoff},
            'difference': difference, 'steps': steps}


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
    example('comparison-order', 'comparisons', 'return source() < sink(source()) < 30', [S, S, T], False, witness=True),
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
            prelude='def relay(value, other):\n    raise value\n', difference='Explicit raise ends the helper path; argument effects occur before entry.'),
    example('raise-before-next-argument', 'returns calls', 'return relay(stop(), sink(source()))', raised='TypeError',
            prelude='def stop():\n    raise 0\ndef relay(value, other):\n    return other\n'),
    example('sanitize', 'rules', 'return sink(clean(source()))', [S, ['clean', 17], ZERO], 0),
    example('propagate', 'rules', 'return sink(forward(source()))', [S, ['forward', 17], T], 17, witness=True),
    example('unicode-local', 'names locations', 'café = source(); return sink(café)', [S, T], 17, witness=True),
    example('unbound-local', 'names boundaries', 'sink(value)\nvalue = source()', raised='UnboundLocalError',
            complete=False, cutoff='unbound'),
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
RENAMES = {'entry': 'compute', 'relay': 'transfer', 'stop': 'halt', 'source': 'read_value',
           'sink': 'emit', 'clean': 'escape_value', 'forward': 'pass_value',
           'value': 'payload', 'other': 'extra', 'left': 'first', 'right': 'second', 'café': 'thé', 'flag': 'condition'}


def cases():
    result = []
    for example in EXAMPLES:
        for moved in (False, True):
            source = example['source']
            if moved:
                tokens = tokenize.generate_tokens(io.StringIO(source).readline)
                source = tokenize.untokenize(token._replace(string=RENAMES.get(token.string, token.string))
                    if token.type == tokenize.NAME else token for token in tokens)
                source = '# relocated π 🦀\n\n' + source
            names = {name: RENAMES[name] if moved else name for name in ('entry', 'source', 'sink', 'clean', 'forward')}
            result.append({**example, 'id': example['id'] + ('/moved' if moved else '/original'),
                'source': source, 'path': 'nested/renamed/module.py' if moved else 'subject.py',
                'names': names, 'variant': 'moved' if moved else 'original'})
    return result
