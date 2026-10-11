"""Keep the retained cross-cutting Python contract in the normal SDK gate."""
from pathlib import Path
import subprocess
import sys


def test_retained_python_contract_and_corruption_controls():
    root = Path(__file__).resolve().parents[3]
    result = subprocess.run([sys.executable, '-B', 'tools/test-python-contract.py'],
                            cwd=root, capture_output=True, text=True, timeout=120)
    assert result.returncode == 0, result.stdout + result.stderr
