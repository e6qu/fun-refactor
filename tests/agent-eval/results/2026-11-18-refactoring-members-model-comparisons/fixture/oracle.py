import importlib.util
import itertools
from pathlib import Path
import subprocess
import tempfile

root = Path.cwd()
values = tuple(itertools.product((False, True), repeat=3))
expected = [str(a and not b).lower() for a, b, extra in values]
with tempfile.TemporaryDirectory() as temporary:
    binary = Path(temporary)/"probe"
    subprocess.run(["rustc", "--edition=2021", "main.rs", "-o", str(binary)], check=True, capture_output=True)
    assert subprocess.check_output([str(binary)], text=True).splitlines() == expected
spec = importlib.util.spec_from_file_location("subject", root/"translation.py")
assert spec and spec.loader
module = importlib.util.module_from_spec(spec)
spec.loader.exec_module(module)
for a, b, extra in values:
    assert module.permits(b, a) == (a and not b)
    assert module.restricted(a, b, extra) == (a and not b and extra)
    assert not module.restricted(a, b, extra) or module.permits(b, a)
print("8 Rust outputs, 8 translated outputs, 8 refinement outcomes passed")
