"""Execute real upstream imports with two independent source values."""
import importlib.util
import json
from pathlib import Path
import sys


def main():
    request = json.load(sys.stdin)
    root = Path(request['root'])
    sys.path.insert(0, str(root))
    events = []

    def emit_value():
        events.append(['source', request['value']])
        return request['value']

    def record_value(value):
        events.append(['sink', value])
        return value

    spec = importlib.util.spec_from_file_location('repository_probe', root / request['path'])
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    module.emit_value = emit_value
    module.record_value = record_value
    module.opaque = lambda value: value
    returned = getattr(module, request['entry'])()
    print(json.dumps({'events': events, 'returned': returned}, sort_keys=True))


if __name__ == '__main__':
    main()
