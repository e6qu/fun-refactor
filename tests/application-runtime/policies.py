from itertools import product

from fr_ir.application import (
    adapter_reads, adapter_supports, adapter_writes, adapters_compatible, dispositions_complete,
    endpoint_agreement, json_status_admitted, static_resources_admitted,
    request_input_admitted, validated_endpoint_agreement,
)

for adapter, feature in product(range(7), range(5)):
    print(str(adapter_reads(adapter, feature)).lower())
    print(str(adapter_writes(adapter, feature)).lower())
    print(str(adapter_supports(adapter, feature)).lower())
for source, target, feature in product(range(7), range(7), range(5)):
    print(str(adapters_compatible(source, target, feature)).lower())
for status in range(602):
    print(str(json_status_admitted(status)).lower())
for method, source, scalar in product(range(8), range(4), range(5)):
    print(str(request_input_admitted(method, source, scalar)).lower())
for case in product([0, 1, 2, 256, 4096], [0, 1, 2, 256, 4096], [False, True], [False, True]):
    print(str(dispositions_complete(*case)).lower())
for method, path in product([False, True], repeat=2):
    for inputs, status, response in product([False, True], repeat=3):
        print(str(validated_endpoint_agreement(method, path, inputs, status, response)).lower())
    for status, response in product([False, True], repeat=2):
        print(str(endpoint_agreement(method, path, status, response)).lower())
for case in product([0, 1, 1024, 1025], [0, 32, 33], [0, 1_048_576, 1_048_577]):
    print(str(static_resources_admitted(*case)).lower())
