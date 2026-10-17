"""Observe one fresh package import and a few ordinary calls on a hosted runner."""
import argparse
import hashlib
import importlib
import json
from pathlib import Path
import sys
import time
from types import ModuleType


class Recorder:
    def __init__(self, root, event_limit=200000):
        self.root = root.resolve()
        self.event_limit = event_limit
        self.events = 0
        self.phase = 'import'
        self.files = {}
        self.order = []
        self.paths = {}

    def relative(self, filename):
        if filename not in self.paths:
            path = Path(filename)
            self.paths[filename] = (path.relative_to(self.root).as_posix()
                                    if path.is_absolute() and path.is_relative_to(self.root) else None)
        return self.paths[filename]

    def trace(self, frame, event, argument):
        path = self.relative(frame.f_code.co_filename)
        if path is None:
            return None
        self.events += 1
        if self.events > self.event_limit:
            raise RuntimeError('package trace event limit reached')
        if event == 'line':
            key = (self.phase, path)
            if key not in self.files:
                self.files[key] = {'lines': set(), 'module_lines': set(), 'calls': {}}
                self.order.append(key)
            record = self.files[key]
            record['lines'].add(frame.f_lineno)
            if frame.f_code.co_name == '<module>':
                record['module_lines'].add(frame.f_lineno)
        elif event == 'call':
            key = (self.phase, path)
            if key not in self.files:
                self.files[key] = {'lines': set(), 'module_lines': set(), 'calls': {}}
                self.order.append(key)
            calls = self.files[key]['calls']
            name = frame.f_code.co_qualname
            calls[name] = calls.get(name, 0) + 1
        return self.trace

    def records(self):
        return [{'phase': phase, 'path': path, 'lines': sorted(self.files[phase, path]['lines']),
                 'module_lines': sorted(self.files[phase, path]['module_lines']),
                 'calls': dict(sorted(self.files[phase, path]['calls'].items()))}
                for phase, path in self.order]


def observe(root, module, function, arguments, traced):
    root = root.resolve()
    if module.split('.')[0] in sys.modules:
        raise ValueError('package was imported before the observation')
    recorder = Recorder(root)
    sys.path.insert(0, str(root))
    started = time.perf_counter()
    if traced:
        sys.settrace(recorder.trace)
    try:
        loaded = importlib.import_module(module)
        selected = getattr(loaded, function)
        imported = time.perf_counter()
        imported_modules = {name: vars(item).get('__file__') for name, item in tuple(sys.modules.items())
                            if isinstance(item, ModuleType)}
        recorder.phase = 'call'
        calls_started = time.perf_counter()
        returned = [selected(*items) for items in arguments]
        finished = time.perf_counter()
    finally:
        sys.settrace(None)
    defining = Path(selected.__code__.co_filename).resolve()
    if not defining.is_relative_to(root):
        raise ValueError('selected helper came from outside the pinned source')
    modules = {}
    for name, filename in imported_modules.items():
        if isinstance(filename, str):
            path = Path(filename).resolve()
            if path.is_relative_to(root) and path.suffix == '.py':
                modules[name] = path.relative_to(root).as_posix()
    paths = sorted(set(modules.values()) | {item['path'] for item in recorder.records()})
    files = {path: {'bytes': (root / path).stat().st_size,
                    'sha256': hashlib.sha256((root / path).read_bytes()).hexdigest()} for path in paths}
    return {'module': module, 'function': function, 'arguments': arguments, 'returned': returned,
            'defining_path': defining.relative_to(root).as_posix(), 'traced': traced,
            'import_seconds': imported - started, 'call_seconds': finished - calls_started,
            'modules': dict(sorted(modules.items())), 'files': files,
            'events': recorder.events, 'event_limit': recorder.event_limit, 'execution': recorder.records()}


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--root', type=Path, required=True)
    parser.add_argument('--module', required=True)
    parser.add_argument('--function', required=True)
    parser.add_argument('--arguments', required=True)
    parser.add_argument('--traced', action='store_true')
    args = parser.parse_args()
    print(json.dumps(observe(args.root, args.module, args.function, json.loads(args.arguments), args.traced)))


if __name__ == '__main__':
    main()
