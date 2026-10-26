"""Run the public regex behavior checks against the two actual Rust libraries."""
import argparse
import math
import os
from pathlib import Path
import signal
import shutil
import subprocess
import sys
import time

CHECKER = '.fr/check-escape'


PROBE = r'''
use std::alloc::{GlobalAlloc, Layout, System};
use std::sync::atomic::{AtomicUsize, Ordering};

struct Count;
static ALLOCATIONS: AtomicUsize = AtomicUsize::new(0);
unsafe impl GlobalAlloc for Count {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        ALLOCATIONS.fetch_add(1, Ordering::SeqCst);
        unsafe { System.alloc(layout) }
    }
    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        unsafe { System.dealloc(ptr, layout) }
    }
    unsafe fn realloc(&self, ptr: *mut u8, layout: Layout, size: usize) -> *mut u8 {
        ALLOCATIONS.fetch_add(1, Ordering::SeqCst);
        unsafe { System.realloc(ptr, layout, size) }
    }
}
#[global_allocator]
static GLOBAL: Count = Count;

fn check(input: &str, expected: &str) {
    for length in [regex::escape_len as fn(&str) -> usize, regex_syntax::escape_len] {
        let before = ALLOCATIONS.load(Ordering::SeqCst);
        let actual = length(input);
        let allocations = ALLOCATIONS.load(Ordering::SeqCst) - before;
        assert_eq!(allocations, 0, "escape_len must allocate nothing");
        assert_eq!(actual, expected.len(), "escaped UTF-8 byte count");
    }
    for escape in [regex::escape as fn(&str) -> String, regex_syntax::escape] {
        let before = ALLOCATIONS.load(Ordering::SeqCst);
        let actual = escape(input);
        let allocations = ALLOCATIONS.load(Ordering::SeqCst) - before;
        assert!(allocations <= usize::from(!input.is_empty()),
            "escape allocated {allocations} times; empty input permits zero, nonempty input at most one");
        assert_eq!(actual, expected, "escaping result");
    }
}

fn main() {
    check("", "");
    check("a.b", "a\\.b");
    check("é", "é");
    check("é.", "é\\.");
    let input = ".".repeat(256);
    let expected = "\\.".repeat(256);
    check(&input, &expected);
    let mut output = String::from("prefix/π:");
    regex_syntax::escape_into("a.b", &mut output);
    regex_syntax::escape_into("é", &mut output);
    assert_eq!(output, "prefix/π:a\\.bé", "append behavior");
    println!("Both APIs passed the public byte, allocation, escaping and append checks.");
}
'''


def arguments(tools, deadline_unix):
    return [tools['python'], CHECKER, '--project', '.', '--target', 'target/public-escape-check',
            '--cargo', tools['cargo'], '--rustc', tools['rustc'], '--deadline-unix', str(deadline_unix)]


def install(project):
    destination = project / CHECKER
    destination.parent.mkdir(parents=True, exist_ok=True)
    shutil.copyfile(__file__, destination)
    return destination


def command(project, target, name, argv, deadline):
    remaining = deadline - time.monotonic()
    if remaining <= 0:
        raise RuntimeError('public check deadline expired before ' + name)
    started = time.monotonic()
    stdout_path, stderr_path = target / (name + '.stdout'), target / (name + '.stderr')
    with stdout_path.open('wb') as stdout, stderr_path.open('wb') as stderr:
        process = subprocess.Popen(argv, cwd=project, stdout=stdout, stderr=stderr, start_new_session=True)
        expired = False
        try:
            code = process.wait(timeout=remaining)
        except subprocess.TimeoutExpired:
            expired, code = True, None
        finally:
            try:
                os.killpg(process.pid, signal.SIGKILL)
            except ProcessLookupError:
                pass
            process.wait(timeout=5)
    if expired or code:
        with stderr_path.open('rb') as stream:
            stream.seek(max(0, stderr_path.stat().st_size - 4096))
            detail = stream.read().decode(errors='replace')
        reason = 'deadline expired' if expired else 'command exited ' + str(code)
        raise RuntimeError(name + ': ' + reason + '\n' + detail)
    print(name + ': passed (' + format(time.monotonic() - started, '.3f') + ' seconds)', flush=True)


def check(project, target, cargo, rustc, deadline_unix):
    project, target = project.resolve(), target.resolve()
    if not target.is_relative_to(project) or target == project:
        raise ValueError('keep public check output inside a separate project target directory')
    if not math.isfinite(deadline_unix):
        raise ValueError('supply the explicit finite attempt checking deadline')
    if os.environ.get('CARGO_PROFILE_DEV_OPT_LEVEL') != '0' or os.environ.get('CARGO_BUILD_JOBS') != '1':
        raise ValueError('public behavior checks require explicit development optimization 0 and one compiler job')
    deadline = time.monotonic() + max(0, deadline_unix - time.time())
    target.mkdir(parents=True, exist_ok=True)
    source, binary = target / 'public_probe.rs', target / 'public_probe'
    source.write_text(PROBE)
    build = [str(cargo), 'build', '-p', 'regex', '-p', 'regex-syntax', '--lib', '--locked', '--offline',
             '--target-dir', str(target)]
    command(project, target, 'build-default', build, deadline)
    command(project, target, 'compile-public-probe', [str(rustc), '--edition=2021', '--crate-name', 'public_probe', str(source),
            '--extern', 'regex=' + str(target / 'debug/libregex.rlib'),
            '--extern', 'regex_syntax=' + str(target / 'debug/libregex_syntax.rlib'),
            '-L', 'dependency=' + str(target / 'debug/deps'), '-o', str(binary)], deadline)
    command(project, target, 'public-behavior', [str(binary)], deadline)
    command(project, target, 'check-no-default-features', [str(cargo), 'check', '-p', 'regex', '-p', 'regex-syntax',
            '--no-default-features', '--locked', '--offline', '--target-dir', str(target)], deadline)


if __name__ == '__main__':
    parser = argparse.ArgumentParser(description=__doc__)
    for name in ('project', 'target', 'cargo', 'rustc'):
        parser.add_argument('--' + name, type=Path, required=True)
    parser.add_argument('--deadline-unix', type=float, required=True)
    args = parser.parse_args()
    try:
        check(args.project, args.target, args.cargo, args.rustc, args.deadline_unix)
    except (OSError, RuntimeError, ValueError, subprocess.SubprocessError) as error:
        print(str(error), file=sys.stderr)
        raise SystemExit(1)
