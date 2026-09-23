"""Independent behavioral requirements, intentionally separate from discovery."""
import importlib.util
import sys

spec = importlib.util.spec_from_file_location("subject", sys.argv[1])
module = importlib.util.module_from_spec(spec)
spec.loader.exec_module(module)
for price, count, member, expected in [
    (3, 2, True, 0), (20, 2, True, 30), (3, 2, False, 6), (0, 1, True, 0),
]:
    assert module.checkout(price, count, member) == expected
if "--feature" in sys.argv:
    for price, count, member, expected in [(3, 2, True, 0), (20, 2, True, 30)]:
        assert module.quote(price, count, member) == {"total": expected, "currency": "USD"}
