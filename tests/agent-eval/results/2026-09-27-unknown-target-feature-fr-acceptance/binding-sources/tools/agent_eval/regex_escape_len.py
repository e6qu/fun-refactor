"""A coordinated regex/regex-syntax task and independent byte/allocation oracle."""

import json
import os
from pathlib import Path
import subprocess
import tempfile

from agent_eval import regex_workspace

TASK = "regex-escape-len"
PATHS = ("regex-syntax/src/lib.rs", "src/lib.rs")
DESCRIPTION = "Add documented public escape_len(pattern: &str) -> usize APIs to both regex and regex-syntax. Return the UTF-8 byte length of the regex-escaped pattern, without allocating. Keep the facade consistent with the lower crate. Update regex-syntax::escape to reserve sufficient capacity before appending, so escaping a nonempty pattern uses at most one allocation and escaping empty input allocates nothing. Preserve all existing escaping results and APIs, including append behavior, and retain builds without default features."
CHECKS = regex_workspace.CHECKS

ORACLE = r'''
extern crate regex;
extern crate regex_syntax;
use std::alloc::{GlobalAlloc, Layout, System};
use std::sync::atomic::{AtomicUsize, Ordering};
struct Count;
static ALLOCATIONS: AtomicUsize = AtomicUsize::new(0);
unsafe impl GlobalAlloc for Count {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        ALLOCATIONS.fetch_add(1, Ordering::SeqCst);
        System.alloc(layout)
    }
    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) { System.dealloc(ptr, layout) }
    unsafe fn realloc(&self, ptr: *mut u8, layout: Layout, size: usize) -> *mut u8 {
        ALLOCATIONS.fetch_add(1, Ordering::SeqCst);
        System.realloc(ptr, layout, size)
    }
}
#[global_allocator]
static GLOBAL: Count = Count;
fn main() {
    let alphabet = ['a', '\\', '.', '+', '*', '?', '(', ')', '|', '[', ']', '{', '}', '^', '$', '#', '&', '-', '~', '%', ':', '/', '!', '=', '<', '>', '_', ' ', '\n', '\0', 'é', '🙂'];
    let mut inputs = vec![String::new()];
    for a in alphabet {
        inputs.push(a.to_string());
        for b in alphabet { inputs.push(format!("{a}{b}")); }
    }
    inputs.extend(["λ🙂é.*".repeat(100), ".".repeat(4096), "x".repeat(4096)]);
    for input in &inputs {
        let mut expected = String::new();
        for c in input.chars() {
            if "\\.+*?()|[]{}^$#&-~".contains(c) { expected.push('\\'); }
            expected.push(c);
        }
        for length in [regex::escape_len as fn(&str) -> usize, regex_syntax::escape_len] {
            let before = ALLOCATIONS.load(Ordering::SeqCst);
            let size = length(input);
            assert_eq!(ALLOCATIONS.load(Ordering::SeqCst), before, "length calculation allocated");
            assert_eq!(size, expected.len(), "escaped byte count");
        }
        for escape in [regex::escape as fn(&str) -> String, regex_syntax::escape] {
            let before = ALLOCATIONS.load(Ordering::SeqCst);
            let output = escape(input);
            let allocations = ALLOCATIONS.load(Ordering::SeqCst) - before;
            assert!(allocations <= usize::from(!input.is_empty()), "escaping allocated {allocations} times");
            assert_eq!(output, expected);
        }
        let mut buffer = String::from("prefix[π]\\");
        regex_syntax::escape_into(input, &mut buffer);
        regex_syntax::escape_into(input, &mut buffer);
        assert_eq!(buffer, format!("prefix[π]\\{expected}{expected}"));
    }
    println!("{} independent byte/allocation cases passed in both crates", inputs.len());
}
'''


def missing_api_only(stderr):
    errors = []
    for line in stderr.decode(errors="replace").splitlines():
        try:
            diagnostic = json.loads(line)
        except ValueError:
            return False
        if not isinstance(diagnostic, dict):
            return False
        if diagnostic.get("level") == "error" and not diagnostic.get("message", "").startswith("aborting due to"):
            errors.append(diagnostic)
    expected = {f"cannot find {kind} `escape_len` in crate `{crate}`" for crate in ("regex", "regex_syntax") for kind in ("function", "value")}
    return bool(errors) and all((error.get("code") or {}).get("code") == "E0425" and error.get("message") in expected for error in errors)


def verify(root):
    env = os.environ.copy()
    env.update(CARGO_HOME=str(regex_workspace.ROOT / "target/cargo-home"), CARGO_NET_OFFLINE="true")
    target = root / "target"
    build = subprocess.run(["cargo", "build", "-p", "regex", "-p", "regex-syntax", "--lib", "--locked", "--offline", "--target-dir", str(target)],
                           cwd=root, env=env, capture_output=True, timeout=180)
    if build.returncode:
        return {"passed": False, "stage": 0, "detail": build.stderr.decode(errors="replace")[-4096:]}
    with tempfile.TemporaryDirectory(prefix="fr-escape-len-oracle-") as tmp:
        source, binary = Path(tmp) / "oracle.rs", Path(tmp) / "oracle"
        source.write_text(ORACLE)
        compiled = subprocess.run(["rustc", "--edition=2021", "--error-format=json", str(source),
                                   "--extern", f"regex={target / 'debug/libregex.rlib'}",
                                   "--extern", f"regex_syntax={target / 'debug/libregex_syntax.rlib'}",
                                   "-L", f"dependency={target / 'debug/deps'}", "-o", str(binary)],
                                  env=env, capture_output=True, timeout=60)
        if compiled.returncode:
            return {"passed": False, "stage": 1 if missing_api_only(compiled.stderr) else 0,
                    "detail": compiled.stderr.decode(errors="replace")[-4096:]}
        result = subprocess.run([str(binary)], capture_output=True, timeout=30)
        return {"passed": result.returncode == 0, "stage": 2,
                "detail": (result.stdout + result.stderr).decode(errors="replace")[-4096:]}
