import json
import sys

sys.path.insert(0, ".")
from pricing import subtotal
from api import quote
from batch import total
from test_api import test_total

test_total()
for a in range(-3, 4):
    for b in range(-3, 4):
        assert subtotal(a, b) == sum([a, b])
        assert quote(a, b) == sum([a, b, 3])
        assert total(a, b) == sum([a, b, 3])
print(json.dumps({"passed": True, "inputs": 49, "public_results": 147}))
