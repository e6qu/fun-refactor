"""Check number bounds, recovery messages and unexpected errors independently."""
import importlib.util
from pathlib import Path
import sys

root = Path(sys.argv[1]).resolve()
sys.path.insert(0, str(root))
spec = importlib.util.spec_from_file_location('customer_application', root / 'app.py')
app = importlib.util.module_from_spec(spec)
spec.loader.exec_module(app)
for value, expected in [(-5, 0), (17, 17), (70, 50)]:
    assert app.clamp(value, 0, 50) == expected
    assert app.prepare(value, 0, 50) == 'ACCEPTED'
for message in ['Customer', '', ' hello ', 'café']:
    assert app.prepare(message, 50, 0) == message
try:
    app.prepare('Customer', 'invalid-bound', 50)
except TypeError:
    pass
else:
    raise AssertionError('unexpected TypeError must leave the application')
print('number bounds, four recovery messages and unexpected TypeError passed')
