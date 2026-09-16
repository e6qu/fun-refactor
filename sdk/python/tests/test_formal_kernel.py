import copy

import pytest

from fr_ir.formal_kernel import KernelRequest, KernelResult, KernelTerm, KernelValue, SEMANTICS, kernel_limits_admitted
from fr_ir.ir import IrError, merkle_object_digest


def test_kernel_requests_follow_the_native_ir_shape():
    request = KernelRequest(KernelTerm.let(KernelTerm.literal(KernelValue("int", 3)), KernelTerm.binary("add", KernelTerm.bound(0), KernelTerm.bound(1))), (KernelValue("int", 7),))
    data = request.to_data()
    assert data["term"]["body"]["right"] == {"kind": "bound", "index": 1}
    assert data["environment"] == [{"kind": "int", "value": 7}]
    assert KernelTerm.from_data(data["term"]).to_data() == data["term"]


@pytest.mark.parametrize("value", [True, 2**63, -(2**63) - 1, 3.0, "1"])
def test_integer_values_refuse_coercion_and_overflow(value):
    with pytest.raises(IrError):
        KernelValue("int", value).to_data()


@pytest.mark.parametrize("value", [True, -1, 2**64, 3.0])
def test_bound_variables_require_native_indices(value):
    with pytest.raises(IrError):
        KernelTerm.bound(value)


def test_kernel_values_preserve_nested_data_structures():
    data = {"kind": "record", "value": {
        "pair": {"kind": "tuple", "value": [{"kind": "bool", "value": True}, {"kind": "unit"}]},
        "option": {"kind": "option", "value": None},
        "result": {"kind": "result", "value": {"ok": False, "value": {"kind": "string", "value": "failed"}}},
    }}
    assert KernelValue.from_data(data).to_data() == data


def test_result_verification_binds_the_request_semantics_and_outcome():
    request = KernelRequest(KernelTerm.literal(KernelValue("bool", True)))
    data = {"schema": "fr-pure-kernel-result-1", "request_digest": merkle_object_digest(request.to_data()), "semantics": dict(SEMANTICS), "passed": True, "value": {"kind": "bool", "value": True}, "failure": None}
    data["object_digest"] = merkle_object_digest(data)
    assert KernelResult.from_data(data, request).passed
    for field, replacement in [("request_digest", "0" * 64), ("passed", 1), ("failure", "overflow"), ("value", None)]:
        changed = copy.deepcopy(data)
        changed[field] = replacement
        changed["object_digest"] = merkle_object_digest({key: value for key, value in changed.items() if key != "object_digest"})
        with pytest.raises(IrError):
            KernelResult.from_data(changed, request)


@pytest.mark.parametrize("data", [
    {"kind": "bound", "index": 0, "extra": True},
    {"kind": "binary", "operator": {}, "left": {"kind": "bound", "index": 0}, "right": {"kind": "bound", "index": 1}},
    {"kind": "result", "ok": 1, "value": {"kind": "bound", "index": 0}},
])
def test_unknown_or_ill_typed_term_fields_refuse(data):
    with pytest.raises(IrError):
        KernelTerm.from_data(data)


def test_kernel_limits_do_not_accept_python_bool_indices():
    assert kernel_limits_admitted(64, 64, 4096, 64)
    assert not kernel_limits_admitted(True, 0, 0, 0)
    assert not kernel_limits_admitted(257, 0, 0, 0)
    assert not kernel_limits_admitted(64, 65, 0, 0)


def test_result_semantics_require_a_boolean_correspondence_boundary():
    request = KernelRequest(KernelTerm.literal(KernelValue("bool", True)))
    data = {"schema": "fr-pure-kernel-result-1", "request_digest": merkle_object_digest(request.to_data()), "semantics": dict(SEMANTICS, source_correspondence=0), "passed": True, "value": {"kind": "bool", "value": True}, "failure": None}
    data["object_digest"] = merkle_object_digest(data)
    with pytest.raises(IrError):
        KernelResult.from_data(data, request)


def test_formal_evaluation_binds_each_independent_material_and_signature():
    from fr_ir.formal_kernel import KernelEvidence, SourceBinding
    from fr_ir.ir import FormalKernel
    definition = "def keepModel (value : Bool) : Bool := value"
    evidence = KernelEvidence("go", KernelTerm.bound(0), (SourceBinding("value", "bool", "Bool"), SourceBinding("return", "bool", "Bool")), merkle_object_digest([]), merkle_object_digest({"kind": "bound", "index": 0}), merkle_object_digest(definition))
    data = {"module": "Keep", "model": "keepModel", "inputs": [{"name": "value", "rust_type": "bool", "lean_type": "Bool"}], "output": {"name": "return", "rust_type": "bool", "lean_type": "Bool"}, "semantic_ir": [], "lean_definition": definition, "evaluation": evidence.to_data()}
    assert FormalKernel.from_data(data).to_data() == data
    for field in ("ir_digest", "term_digest", "model_digest"):
        changed = copy.deepcopy(data)
        changed["evaluation"][field] = "0" * 64
        with pytest.raises(IrError):
            FormalKernel.from_data(changed)
    changed = copy.deepcopy(data)
    changed["evaluation"]["bindings"][0]["source_type"] = "int"
    with pytest.raises(IrError):
        FormalKernel.from_data(changed)
    changed["evaluation"] = None
    with pytest.raises(IrError):
        FormalKernel.from_data(changed)
