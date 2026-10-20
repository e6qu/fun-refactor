"""Check original project behavior through the explicitly requested public name."""
import argparse
import importlib
import json
from pathlib import Path
import subprocess
import sys
import tempfile


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


if __name__ == '__main__':
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('project', choices=('glob', 'boltons'))
    parser.add_argument('root', type=Path)
    parser.add_argument('name')
    args = parser.parse_args()
    print(json.dumps(check(args.project, args.root.resolve(), args.name)))
