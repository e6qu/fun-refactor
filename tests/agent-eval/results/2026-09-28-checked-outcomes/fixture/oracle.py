"""Compile the public API and compare 49 outputs with independent Python sums."""
import json
from pathlib import Path
import subprocess
import tempfile

root = Path.cwd()
with tempfile.TemporaryDirectory(prefix="fr-outcome-oracle-") as temporary:
    work = Path(temporary)
    harness = '#[path = '+json.dumps(str(root/"src/lib.rs"))+'] mod subject;\n'
    harness += 'fn main() { for a in -3..=3 { for b in -3..=3 { println!("{}", subject::quote(a,b)); } } }\n'
    (work/"main.rs").write_text(harness)
    subprocess.run(["rustc",str(work/"main.rs"),"-o",str(work/"oracle")],check=True,capture_output=True)
    result = subprocess.run([str(work/"oracle")],check=True,capture_output=True,text=True)
    actual = [int(line) for line in result.stdout.splitlines()]
    expected = [sum((a,b,3)) for a in range(-3,4) for b in range(-3,4)]
    assert actual == expected, "quote must sum both inputs and add the required fee"
    print(json.dumps({"passed":True,"cases":len(expected),"outputs":actual}))
