"""Pinned multi-crate source and a caller-side oracle for the public buffer API task."""

import hashlib
import os
from pathlib import Path
import subprocess
import tarfile
import tempfile

ROOT = Path(__file__).resolve().parents[2]
DATA = ROOT / "tests/agent-eval/regex"
COMMIT = "2b527599eb9eea0dcc288c704584f242f26a5c61"
ARCHIVE_SHA = "7f8beace6ed6c94b2eec3c5aa92219e10e73bd8ad696035877bed01d3ee47255"
LOCK_SHA = "7ae225f5fbac82509d5c259dd3613a809c75a8bd4e2124b9cb69ed29d98de3a0"
TASK = "regex-escape-into"
DESCRIPTION = "Add a documented public regex::escape_into(pattern: &str, buf: &mut alloc::string::String) API. Append the regex-escaped pattern to buf, preserving its existing contents. Match regex::escape for every Unicode input and every regex metacharacter. Do not allocate an intermediate escaped String. Reuse workspace functionality where suitable, keep existing APIs unchanged and retain support for builds without default features."
CHECKS = [
    {"name": "upstream", "argv": ["cargo", "test", "-p", "regex", "-p", "regex-syntax", "--lib", "--locked", "--offline"],
     "cwd": ".", "timeout_seconds": 120, "covers": ["Upstream regex and regex-syntax library tests, including missing-docs lint; excludes integration and documentation tests"]},
    {"name": "minimal", "argv": ["cargo", "check", "-p", "regex", "--no-default-features", "--locked", "--offline"],
     "cwd": ".", "timeout_seconds": 120, "covers": ["regex library compilation without default features"]},
]

ORACLE = r'''
extern crate regex;
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
    let reference = |s: &str| {
        let mut result = String::new();
        for c in s.chars() {
            if "\\.+*?()|[]{}^$#&-~".contains(c) { result.push('\\'); }
            result.push(c);
        }
        result
    };
    let mut inputs = vec![String::new()];
    for a in alphabet {
        inputs.push(a.to_string());
        for b in alphabet { inputs.push(format!("{a}{b}")); }
    }
    inputs.push("λ🙂é.*".repeat(100));
    let prefixes = ["", "prefix[already\\escaped]", "π🙂\0"];
    let mut cases = 0;
    for input in &inputs {
        let escaped = reference(input);
        assert_eq!(regex::escape(input), escaped);
        for prefix in prefixes {
            let mut buf = String::with_capacity(prefix.len() + escaped.len() * 2);
            buf.push_str(prefix);
            let before = ALLOCATIONS.load(Ordering::SeqCst);
            regex::escape_into(input, &mut buf);
            assert_eq!(ALLOCATIONS.load(Ordering::SeqCst), before, "unexpected allocation");
            assert_eq!(buf, format!("{prefix}{escaped}"));
            let before = ALLOCATIONS.load(Ordering::SeqCst);
            regex::escape_into(input, &mut buf);
            assert_eq!(ALLOCATIONS.load(Ordering::SeqCst), before, "unexpected allocation");
            assert_eq!(buf, format!("{prefix}{escaped}{escaped}"));
            cases += 1;
        }
    }
    println!("{cases} independent append cases passed");
}
'''


def unpack(destination):
    archive = DATA / "workspace.tar.gz"
    if hashlib.sha256(archive.read_bytes()).hexdigest() != ARCHIVE_SHA:
        raise ValueError("Pinned regex workspace checksum mismatch")
    destination.mkdir()
    with tarfile.open(archive, "r:gz") as source:
        for member in source.getmembers():
            parts = Path(member.name).parts
            if not parts or parts[0] != f"regex-{COMMIT}" or ".." in parts or member.issym() or member.islnk():
                raise ValueError("Unsafe regex archive member")
            path = destination.joinpath(*parts[1:])
            if member.isdir():
                path.mkdir(parents=True, exist_ok=True)
            elif member.isfile():
                path.parent.mkdir(parents=True, exist_ok=True)
                path.write_bytes(source.extractfile(member).read())
                path.chmod(0o755 if member.mode & 0o111 else 0o644)
            else:
                raise ValueError("Unsupported regex archive member")
    lock = (DATA / "Cargo.lock").read_bytes()
    if hashlib.sha256(lock).hexdigest() != LOCK_SHA:
        raise ValueError("Pinned regex dependency lock checksum mismatch")
    (destination / "Cargo.lock").write_bytes(lock)


def verify(root):
    env = os.environ.copy()
    env.update(CARGO_HOME=str(ROOT / "target/cargo-home"), CARGO_NET_OFFLINE="true")
    target = root / "target"
    build = subprocess.run(["cargo", "build", "-p", "regex", "--lib", "--locked", "--offline", "--target-dir", str(target)],
                           cwd=root, env=env, capture_output=True, timeout=180)
    if build.returncode:
        return {"passed": False, "stage": 0, "detail": build.stderr.decode(errors="replace")[-4096:]}
    with tempfile.TemporaryDirectory(prefix="fr-regex-oracle-") as tmp:
        source, binary = Path(tmp) / "oracle.rs", Path(tmp) / "oracle"
        source.write_text(ORACLE)
        compiled = subprocess.run(["rustc", "--edition=2021", str(source), "--extern", f"regex={target / 'debug/libregex.rlib'}",
                                   "-L", f"dependency={target / 'debug/deps'}", "-o", str(binary)],
                                  env=env, capture_output=True, timeout=60)
        if compiled.returncode:
            missing = "cannot find function `escape_into` in crate `regex`" in compiled.stderr.decode(errors="replace")
            return {"passed": False, "stage": 1 if missing else 0, "detail": compiled.stderr.decode(errors="replace")[-4096:]}
        result = subprocess.run([str(binary)], capture_output=True, timeout=30)
        return {"passed": result.returncode == 0, "stage": 2,
                "detail": (result.stdout + result.stderr).decode(errors="replace")[-4096:]}
