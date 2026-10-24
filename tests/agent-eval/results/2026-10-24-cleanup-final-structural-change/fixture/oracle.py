"""Compile the public program and compare every output with independent arithmetic."""
import json
from pathlib import Path
import subprocess
import tempfile

with tempfile.TemporaryDirectory(prefix="fr-structural-oracle-") as temporary:
    binary = Path(temporary)/"public-results"
    build = subprocess.run(["rustc", "--edition=2021", "main.rs", "-o", str(binary)], capture_output=True, text=True)
    assert build.returncode == 0, build.stderr
    values = json.loads(subprocess.check_output([str(binary)], text=True))
    expected = [sum((a, b)) for a in range(-3, 4) for b in range(-3, 4)]
    assert values == expected, (values, expected)
    print(json.dumps({"passed": True, "results": values, "cases": len(values)}))
