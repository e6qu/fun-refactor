"""Check original project behavior through the explicitly requested public name."""
import argparse
import importlib
import json
from pathlib import Path
import subprocess
import sys
import tempfile
import re


def check(project, root, name):
    if project == 'boltons':
        sys.path.insert(0, str(root))
        module = importlib.import_module('boltons.mathutils')
        function = getattr(module, name)
        actual = [function(value, lower=0, upper=50) for value in (-5, 17, 70)]
        assert actual == [0, 17, 50], actual
        obsolete = 'clamp_value' if name == 'clamp' else 'clamp'
        assert not hasattr(module, obsolete), obsolete
        return {'results': actual, 'original_import': 'boltons.mathutils'}
    assert project == 'glob'
    with tempfile.TemporaryDirectory(prefix='fr-original-glob-oracle-') as temporary:
        work = Path(temporary)
        tree = work / 'tree'
        (tree / 'sub').mkdir(parents=True)
        for relative in ('a.txt', 'B.TXT', 'sub/c.txt'):
            (tree / relative).write_text(relative)
        driver = work / 'driver.rs'
        driver.write_text('''use glob::{NAME, MatchOptions};
fn main() {
    let base = std::env::args().nth(1).unwrap();
    let options = MatchOptions { case_sensitive: true,
        require_literal_separator: true, require_literal_leading_dot: true };
    let count = |suffix: &str| NAME(&format!("{base}/{suffix}"), options)
        .unwrap().map(|path| path.unwrap()).count();
    assert_eq!(count("a.txt"), 1);
    assert_eq!(count("*.txt"), 1);
    assert_eq!(count("**/*.txt"), 2);
    assert_eq!(count("*.missing"), 0);
    assert!(NAME("[", options).is_err());
    let insensitive = MatchOptions { case_sensitive: false, ..options };
    assert_eq!(NAME(&format!("{base}/*.txt"), insensitive).unwrap().count(), 2);
    println!("[1,1,2,0,2]");
}
'''.replace('NAME', name))
        commands = [
            ['rustc', '--edition=2021', '--crate-name', 'glob', '--crate-type', 'rlib',
             str(root / 'src/lib.rs'), '-o', str(work / 'libglob.rlib')],
            ['rustc', '--edition=2021', str(driver), '--extern', 'glob=' + str(work / 'libglob.rlib'),
             '-o', str(work / 'driver')],
            [str(work / 'driver'), str(tree)],
        ]
        for argv in commands:
            result = subprocess.run(argv, capture_output=True, text=True, timeout=30)
            if result.returncode:
                raise AssertionError((argv, result.stdout, result.stderr))
        return {'results': json.loads(result.stdout), 'complete_library': 'src/lib.rs'}


def documentation(project, root):
    assert project == 'boltons'
    sys.path.insert(0, str(root))
    objects = []
    for line in (root / 'docs/mathutils.rst').read_text().splitlines():
        match = re.fullmatch(r'\.\. auto(?:function|class):: (\S+)', line)
        if match is not None:
            qualified = match[1]
            module, attribute = qualified.rsplit('.', 1)
            value = getattr(importlib.import_module(module), attribute)
            assert callable(value), qualified
            objects.append(qualified)
    assert len(objects) == 4, objects
    return {'resolved_documented_objects': objects}


if __name__ == '__main__':
    parser = argparse.ArgumentParser(description=__doc__)
    commands = parser.add_subparsers(dest='operation', required=True)
    behavior = commands.add_parser('behavior')
    behavior.add_argument('project', choices=('glob', 'boltons'))
    behavior.add_argument('root', type=Path)
    behavior.add_argument('--name', required=True)
    docs = commands.add_parser('documentation')
    docs.add_argument('project', choices=('boltons',))
    docs.add_argument('root', type=Path)
    args = parser.parse_args()
    if args.operation == 'behavior':
        result = check(args.project, args.root.resolve(), args.name)
    else:
        result = documentation(args.project, args.root.resolve())
    print(json.dumps(result))
