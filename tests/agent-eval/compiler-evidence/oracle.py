"""Independent rustc execution and fixture byte coordinates."""
import json
from pathlib import Path
import shutil
import subprocess
import tempfile


def oracle():
    compiler = subprocess.check_output(["rustup", "which", "rustc"], text=True).strip()
    fixture = Path(__file__).with_name("subject.rs")
    source = fixture.read_bytes()
    start = source.index(b"{ value }") + 2
    with tempfile.TemporaryDirectory(prefix="fr-compiler-oracle-") as temporary:
        root = Path(temporary)
        shutil.copyfile(fixture, root / "subject.rs")
        base = [compiler, "--crate-type", "lib", "--emit", "metadata", "--out-dir", temporary, "--error-format", "json"]
        default = subprocess.run([*base, "subject.rs"], cwd=root, capture_output=True)
        strict = subprocess.run([*base, "--cfg", "fr_strict", "subject.rs"], cwd=root, capture_output=True)
        assert default.returncode == 0 and strict.returncode != 0
        diagnostics = [json.loads(line) for line in strict.stderr.splitlines()]
        error = next(row for row in diagnostics if (row.get("code") or {}).get("code") == "E0308")
        assert any(span["is_primary"] and [span["byte_start"], span["byte_end"]] == [start, start + 5] for span in error["spans"])
        (root / "driver.rs").write_text('include!("subject.rs");\nfn main() { for value in ["", "a", "hello", "café", "☀"] { assert_eq!(width(value), value.as_bytes().len()); } assert_eq!(repeated(), 10); }\n')
        subprocess.run([compiler, "driver.rs", "-o", str(root / "driver")], cwd=root, check=True, capture_output=True)
        subprocess.run([str(root / "driver")], check=True, capture_output=True)
    return {"default_accepted": True, "strict_rejected": True, "code": "E0308", "primary_span": [start, start + 5], "runtime_cases": 6}


if __name__ == "__main__":
    print(json.dumps(oracle(), sort_keys=True))
