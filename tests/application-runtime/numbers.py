import json
import sys

from fr_ir.ir import merkle_object_digest, merkle_object_pack, restore_merkle_object

cases = json.load(sys.stdin)
for case in cases:
    value = case["value"]
    actual = merkle_object_digest(value)
    assert actual == case["digest"], (value, actual, case["digest"])
    pack = merkle_object_pack({"number": value})
    assert restore_merkle_object(pack["root"], pack["objects"]) == {"number": value}
print(len(cases))
