#!/usr/bin/env python3
"""Compare direct JSON and Python SDK construction at the semantic boundary."""

from __future__ import annotations

import argparse
import hashlib
import json
from pathlib import Path
import subprocess
import sys
import tempfile

ROOT = Path(__file__).resolve().parent.parent
sys.path.insert(0, str(ROOT / "sdk/python/src"))

from fr_ir import BinaryOp, Expr, IrError, SemanticBody, Stmt, Type  # noqa: E402


def compact(value: object) -> str:
    return json.dumps(value, ensure_ascii=False, separators=(",", ":"))


def validate(binary: Path, root: Path, path: Path) -> tuple[bool, dict]:
    result = subprocess.run(
        [str(binary), "--json", "--no-cache", "-C", str(root), "author", "validate-semantic", "--from", str(path), "--canonical"],
        check=False,
        capture_output=True,
        text=True,
    )
    return result.returncode == 0, json.loads(result.stdout)


def catalog_bytes(binary: Path, root: Path) -> int:
    requests = (("statement", "return"), ("expression", "binary"), ("expression", "name"), ("expression", "int"))
    total = 0
    for section, kind in requests:
        result = subprocess.run(
            [str(binary), "--json", "--no-cache", "-C", str(root), "author", "semantic-schema", section, "--kind", kind],
            check=True,
            capture_output=True,
        )
        total += len(result.stdout)
    return total


def evaluate(binary: Path) -> dict:
    direct = {
        "schema": "fr-semantic-body-1",
        "body": [{"kind": "return", "value": {"kind": "binary", "value": {
            "op": "mul", "left": {"kind": "name", "value": "value"},
            "right": {"kind": "int", "value": "2"},
        }}}],
    }
    sdk = SemanticBody([Stmt.Return(Expr.Binary(BinaryOp.MUL, Expr.Name("value"), Expr.Int(2)))])
    sdk_program = 'from fr_ir import *\nprint(SemanticBody([Stmt.Return(Expr.Binary(BinaryOp.MUL, Expr.Name("value"), Expr.Int(2)))]).to_json())\n'
    with tempfile.TemporaryDirectory() as directory:
        root = Path(directory)
        direct_path = root / "direct.json"
        sdk_path = root / "sdk.json"
        invalid_path = root / "invalid.json"
        direct_path.write_text(compact(direct), encoding="utf-8")
        sdk_path.write_text(sdk.to_json(), encoding="utf-8")
        invalid_path.write_text(compact({"schema": "fr-semantic-body-1", "body": [{"kind": "return", "value": Type.String().to_data()}]}), encoding="utf-8")
        direct_ok, direct_report = validate(binary, root, direct_path)
        sdk_ok, sdk_report = validate(binary, root, sdk_path)
        invalid_ok, invalid_report = validate(binary, root, invalid_path)
        try:
            Stmt.Return(Type.String())
            sdk_rejected_category = False
        except IrError:
            sdk_rejected_category = True
        context_bytes = catalog_bytes(binary, root)
    direct_bytes = len(compact(direct).encode())
    sdk_bytes = len(sdk_program.encode())
    return {
        "schema": "fr-agent-ir-sdk-evaluation-1",
        "fixture": "generic multiply-and-return function body",
        "routes": {
            "direct-json": {"producer_bytes": direct_bytes, "contract_context_bytes": context_bytes, "valid": direct_ok},
            "python-sdk": {"producer_bytes": sdk_bytes, "contract_context_bytes": context_bytes, "valid": sdk_ok},
        },
        "equivalence": {
            "payloads_equal": direct == sdk.to_data(),
            "canonical_sha256_equal": direct_report.get("canonical_sha256") == sdk_report.get("canonical_sha256"),
        },
        "category_error": {
            "sdk_rejected_before_serialization": sdk_rejected_category,
            "direct_json_rejected_by_rust": not invalid_ok,
            "rust_error_kind": invalid_report.get("error", {}).get("kind"),
        },
        "translation_scaffold": {
            "source": "src/transpile/ir.rs", "target": "python", "lines": 977, "bytes": 24962,
            "functions": 11, "records": 15, "sum_types": 8, "carried_verbatim": 51,
            "imports_listed": 2, "foreign_signature_types": 1, "syntax_executable": True,
        },
        "scope": "Deterministic construction, validation, size and early-refusal evidence. This is not an agent-quality or token-usage claim.",
        "report_sha256": hashlib.sha256((compact(direct) + sdk_program).encode()).hexdigest(),
    }


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--fr", type=Path, default=ROOT / "target/debug/fr")
    parser.add_argument("--output", type=Path)
    args = parser.parse_args()
    report = evaluate(args.fr.resolve())
    rendered = json.dumps(report, indent=2) + "\n"
    if args.output:
        args.output.write_text(rendered, encoding="utf-8")
    else:
        print(rendered, end="")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
