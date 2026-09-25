"""Independent finite behavior oracle for the admitted Boolean input domain."""
import json
from pathlib import Path
import runpy
import sys

source = Path(sys.argv[1]) if len(sys.argv) > 1 else Path(__file__).with_name("subject.py")
allowed = runpy.run_path(str(source))["allowed"]
values = [allowed(value) for value in (False, True)]
assert values == [False, True]
assert all(type(value) is bool for value in values)
print(json.dumps(values))
