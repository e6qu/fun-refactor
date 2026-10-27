"""Run the declared source programs independently of their generated Lean models."""
import argparse
import hashlib
import json
import os
from pathlib import Path

from . import proof_writing_tasks as tasks
from .selected_plan import ROOT, require


def check(case, output):
    require(os.environ.get('GITHUB_ACTIONS') == 'true', 'run the source-toolchain checks on GitHub')
    require(case in tasks.CASES and not output.exists(), 'check one named source case in a fresh folder')
    output.mkdir(parents=True)
    entry = tasks.CASES[case]
    source = (ROOT / tasks.DIRECTORY / 'fixtures' / entry['path']).read_text()
    commands = []
    count = 4
    language = entry['language']
    if language == 'Rust':
        if case == 'page-count':
            driver = '''
fn main() {
    let items = [i64::MIN, -1, 0, 1, 2, 5, 11, i64::MAX - 1, i64::MAX];
    let sizes = [i64::MIN, -1, 0, 1, 2, 5, 10, i64::MAX - 1, i64::MAX];
    for n in items { for d in sizes {
        let expected = if n < 0 || d <= 0 { -1 } else {
            ((n as i128 + d as i128 - 1) / d as i128) as i64
        };
        assert_eq!(page_count(n, d), expected, "items={n}, size={d}");
    }}
}
'''
            count = 81
        elif case == 'fits-window':
            driver = '''
fn main() {
    let values = [0, 1, 2, u64::MAX - 1, u64::MAX];
    for offset in values { for count in values { for total in values {
        let expected = (offset as u128) + (count as u128) <= (total as u128);
        assert_eq!(fits_window(offset, count, total), expected,
                   "offset={offset}, count={count}, total={total}");
    }}}
}
'''
            count = 125
        else:
            driver = '\nfn main() { for staff in [false, true] { for owner in [false, true] { assert_eq!(allowed(staff, owner), staff || owner); } } }\n'
        (output / 'check.rs').write_text(source + driver)
        commands = [['rustc', '--edition=2024', '-C', 'overflow-checks=on', '-C', 'opt-level=0', 'check.rs', '-o', 'check'],
                    [str((output / 'check').resolve())]]
    elif language == 'Python':
        (output / 'allowed.py').write_text(source)
        (output / 'check.py').write_text('from allowed import allowed\nfor staff in (False, True):\n    for owner in (False, True):\n        assert allowed(staff, owner) is (staff or owner)\n')
        commands = [['python3', '-B', 'check.py']]
    elif language == 'Go':
        (output / 'allowed.go').write_text(source)
        (output / 'allowed_test.go').write_text('package access\nimport "testing"\nfunc TestAccess(t *testing.T) { for _,staff := range []bool{false,true} { for _,owner := range []bool{false,true} { if allowed(staff,owner) != (staff || owner) { t.Fatal("wrong access rule") } } } }\n')
        commands = [['go', 'test', 'allowed.go', 'allowed_test.go']]
    elif language == 'Java':
        (output / 'Allowed.java').write_text(source)
        (output / 'Check.java').write_text('class Check { public static void main(String[] args) { for (boolean staff : new boolean[]{false,true}) { for (boolean owner : new boolean[]{false,true}) { if (Allowed.allowed(staff,owner) != (staff || owner)) throw new AssertionError("wrong access rule"); } } } }\n')
        commands = [['javac', '-d', 'classes', 'Allowed.java', 'Check.java'], ['java', '-cp', 'classes', 'Check']]
    elif language in {'TypeScript', 'TSX'}:
        name = Path(entry['path']).name
        (output / name).write_text(source)
        checker = ROOT / 'tests/typesafety/typescript/node_modules/typescript/bin/tsc'
        require(checker.is_file(), 'install the pinned TypeScript checker before source controls')
        (output / 'check.cjs').write_text('const assert = require("node:assert/strict"); const {allowed}=require("./compiled/allowed.js"); for(const staff of [false,true]) for(const owner of [false,true]) assert.equal(allowed(staff,owner),staff||owner);\n')
        commands = [['node', str(checker), '--strict', '--target', 'ES2022', '--module', 'commonjs',
                     '--lib', 'ES2022', '--jsx', 'react', '--skipLibCheck', '--outDir', 'compiled', name], ['node', 'check.cjs']]
    elif language == 'Zig':
        (output / 'check.zig').write_text(source + '\nconst std = @import("std");\ntest "access rule" { for ([_]bool{false,true}) |staff| { for ([_]bool{false,true}) |owner| { try std.testing.expectEqual(staff or owner, allowed(staff, owner)); } } }\n')
        commands = [['zig', 'test', 'check.zig', '--cache-dir', 'zig-cache', '--global-cache-dir', 'zig-global-cache']]
    else:
        require(language == 'Lean', 'select an explicitly admitted source language')
        table = '\n'.join(f'example : allowed {staff} {owner} = {str(staff == "true" or owner == "true").lower()} := by decide'
                          for staff in ('false', 'true') for owner in ('false', 'true'))
        (output / 'Check.lean').write_text(source + '\n' + table + '\n')
        commands = [['lean', 'Check.lean']]
    versions = {'Rust': [['rustc', '--version']], 'Python': [['python3', '--version']],
                'Go': [['go', 'version']], 'Java': [['javac', '--version'], ['java', '--version']],
                'TypeScript': [['node', '--version'], ['node', str(ROOT / 'tests/typesafety/typescript/node_modules/typescript/bin/tsc'), '--version']],
                'TSX': [['node', '--version'], ['node', str(ROOT / 'tests/typesafety/typescript/node_modules/typescript/bin/tsc'), '--version']],
                'Zig': [['zig', 'version']], 'Lean': [['lean', '--version']]}
    toolchains = [tasks.common.command(argv, output, output / f'toolchain-{index}.json')
                  for index, argv in enumerate(versions[language])]
    require(all(row['returncode'] == 0 for row in toolchains), 'record every actual source toolchain version')
    checks = [tasks.common.command(argv, output, output / f'command-{index}.json') for index, argv in enumerate(commands)]
    result = {'case': case, 'language': language, 'passed': all(row['returncode'] == 0 for row in checks),
              'input_cases': count, 'source_sha256': hashlib.sha256(source.encode()).hexdigest(),
              'source_implementation_proved': False, 'live_model_calls': 0, 'checks': checks, 'toolchains': toolchains,
              'scope': 'Execute the declared source; finite input observations are separate from a Lean model proof.'}
    tasks.common.save(output / 'result.json', result)
    require(result['passed'], 'source behavior disagrees with its declared cases')
    return result


if __name__ == '__main__':
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--case', required=True, choices=tasks.CASES)
    parser.add_argument('--output', type=Path, required=True)
    arguments = parser.parse_args()
    result = check(arguments.case, arguments.output.resolve())
    print(json.dumps({name: result[name] for name in ('case', 'passed', 'input_cases', 'source_implementation_proved', 'live_model_calls')}))
