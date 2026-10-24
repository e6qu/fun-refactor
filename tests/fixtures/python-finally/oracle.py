"""Independent requirements for the customer-text repair, including cleanup failure."""
from pathlib import Path
import runpy
import sys

root = Path(sys.argv[1])
for value in ('Customer', '', 'café', 'PUBLIC'):
    closed = []

    def finish():
        closed.append('closed')
        return 'PUBLIC'

    namespace = runpy.run_path(str(root / 'delivery.py'), init_globals={'finish': finish})
    actual = namespace['prepare'](value)
    assert actual == value, (value, actual)
    assert closed == ['closed'], closed

closed = []


def stop_cleanup():
    closed.append('closed')
    raise ValueError('cleanup failed')


namespace = runpy.run_path(str(root / 'delivery.py'), init_globals={'finish': stop_cleanup})
try:
    namespace['prepare']('Customer')
except ValueError as error:
    assert str(error) == 'cleanup failed'
else:
    raise AssertionError('the repair hid a cleanup failure')
assert closed == ['closed'], closed
