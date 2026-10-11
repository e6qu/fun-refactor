"""Execute only the declared contract programs, with isolated imports and named hooks."""
import builtins
import importlib
import json
from pathlib import Path
import sys
import tempfile


def execute(case, namespace):
    events = []
    def source():
        events.append(['source', 17])
        return 17
    def sink(value):
        events.append(['sink', value])
        return value
    def clean(value):
        events.append(['clean', value])
        return 0
    def forward(value):
        events.append(['forward', value])
        return value
    hooks = {'source': source, 'sink': sink, 'clean': clean, 'forward': forward}
    namespace.update({case['names'][name]: function for name, function in hooks.items()})
    returned, raised, phase = None, None, 'definition'
    try:
        if case['imports']:
            for name, function in namespace.items():
                setattr(builtins, name, function)
            module = importlib.import_module(case['path'][:-3].replace('/', '.'))
            namespace = vars(module)
        else:
            exec(compile(case['source'], case['path'], 'exec'), namespace)
        phase = 'entry'
        returned = namespace[case['names']['entry']]()
    except Exception as error:
        raised = type(error).__name__
    return {'events': events, 'returned': returned, 'raised': raised, 'phase': phase}


def main():
    case = json.load(sys.stdin)
    with tempfile.TemporaryDirectory(prefix='fr-contract-python-') as temporary:
        root = Path(temporary)
        for name, source in case['files'].items():
            path = root / name
            path.parent.mkdir(parents=True, exist_ok=True)
            path.write_bytes(source.encode())
        sys.path.insert(0, str(root))
        print(json.dumps(execute(case, {}), ensure_ascii=False))


if __name__ == '__main__':
    main()
