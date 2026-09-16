from itertools import product

from fr_ir.application import (
    adapter_supports, adapters_compatible, dispositions_complete,
    endpoint_agreement, json_status_admitted,
)

for adapter, feature in product(range(7), range(5)):
    print(str(adapter_supports(adapter, feature)).lower())
for source, target, feature in product(range(7), range(7), range(5)):
    print(str(adapters_compatible(source, target, feature)).lower())
for status in range(602):
    print(str(json_status_admitted(status)).lower())
for case in product([0, 1, 2, 256, 4096], [0, 1, 2, 256, 4096], [False, True], [False, True]):
    print(str(dispositions_complete(*case)).lower())
for case in product([False, True], repeat=4):
    print(str(endpoint_agreement(*case)).lower())
