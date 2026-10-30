import inspect
import hashlib
import json
import re
from unittest.mock import patch

from fr_ir.ir import (
    CHANGE_SCHEMA,
    EXPRESSION_KINDS,
    STATEMENT_KINDS,
    TEMPLATE_KINDS,
    TYPE_KINDS,
    BinaryOp,
    AgentProperty,
    Change,
    DisclosedEditRequest,
    DisclosedIrEditRequest,
    Expr,
    FormalPlan,
    ProofAttempt,
    ProofTask,
    PropertyProposition,
    PropertyTask,
    PropertyTerm,
    ProjectReference,
    ProjectRequest,
    Intent,
    IrError,
    LocatorStep,
    NodeCategory,
    Role,
    ScalarRequest,
    SemanticBody,
    SemanticChange,
    SemanticIntent,
    Stmt,
    TemplatePart,
    TaskChange,
    TaskDelivery,
    TaskTarget,
    Type,
    merkle_object_digest,
    merkle_object_pack,
    restore_merkle_object,
    verify_disclosure_commitment,
    verify_disclosure_proof,
)
import pytest


def kinds(namespace):
    names = (name for name, value in inspect.getmembers(namespace, inspect.isfunction) if not name.startswith("_"))
    return tuple(re.sub(r"(?<!^)(?=[A-Z])", "-", name).lower() for name in names)


class TestIr:
    def test_task_change_builder_mirrors_inline_reviewed_session(self):
        request = ProjectRequest("target", [
            "find", "render", "--signature", "--source", "--bytes", "2048",
        ])
        target = TaskTarget(
            "render-body", ProjectReference("target", "/rows/0/0"), "replace-body",
            fragment="{ value.to_uppercase() }",
        )
        change = TaskChange(
            [request], [target],
            {"files-changed": 1, "edits": 1, "changed-operations": 1,
             "paths-changed": ["src/lib.rs"]},
            ["unit"], TaskDelivery(patch="artifacts/change.patch"),
        )
        data = change.to_data()
        assert data["schema"] == "fr-task-change-1"
        assert data["targets"][0]["fragment"] == "{ value.to_uppercase() }"
        assert data["delivery"]["check-original"] == True
        assert json.loads(change.to_json()) == data
        direct = TaskChange(
            [], [TaskTarget("render-body", "frp1:" + "a" * 32 + ":0001", "replace-body",
                            fragment="{ value.to_uppercase() }")],
            {"files-changed": 1}, ["unit"],
        )
        assert direct.to_data()["requests"] == []
        with pytest.raises(IrError, match="exactly one"):
            TaskTarget("bad", "handle", "replace-body")
        with pytest.raises(IrError, match="safe relative"):
            TaskDelivery(patch="../change.patch")

    def test_property_task_builds_and_validates_agent_authored_ir(self):
        target = {"source": "src/lib.rs", "symbol": "both", "source_hash": "a" * 64}
        kernel = {
            "model": "bothModel",
            "inputs": [
                {"name": "left", "rust_type": "bool", "lean_type": "Bool"},
                {"name": "right", "rust_type": "bool", "lean_type": "Bool"},
            ],
            "output": {"name": "return", "rust_type": "bool", "lean_type": "Bool"},
        }
        contract = {
            "author": "agent", "format": "fr-formal-property-1", "proof_author": "agent",
            "allowed_terms": ["variable", "model", "boolean", "integer", "unary", "binary", "if"],
            "allowed_propositions": [
                "equals", "not-equals", "less-than", "less-or-equal", "greater-than",
                "greater-or-equal", "holds", "not", "and", "or", "implies",
            ],
            "allowed_unary_operators": ["not", "negate"],
            "allowed_binary_operators": ["add", "subtract", "multiply", "and", "or"],
            "max_parameters": 8, "max_nodes": 64, "max_depth": 16,
        }
        templates = [{
            "kind": "model-relation",
            "value": {"schema": "fr-formal-property-1", "task_digest": "<copy>"},
        }]
        core = {
            "schema": "fr-property-task-1", "target": target, "kernel": kernel,
            "contract": contract, "templates": templates,
        }
        data = dict(core)
        data.update({
            "object_digest": merkle_object_digest(core),
            "actions": {
                "plan": ["spec", "plan", "src/lib.rs::both", "--property-from", "<PROPERTY_FILE>"],
                "scaffold": ["spec", "scaffold", "--from", "<PLAN_FILE>", "--write"],
            },
            "token_budget": {
                "limit": 4096, "used_upper_bound": 2048, "measurement": "serialized_utf8_bytes",
            },
        })
        task = PropertyTask.from_data(data)
        x, y = PropertyTerm.variable("x"), PropertyTerm.variable("y")
        property_ = task.property(
            "commutative",
            [{"name": "x", "lean_type": "Bool"}, {"name": "y", "lean_type": "Bool"}],
            PropertyProposition.equals(PropertyTerm.model(x, y), PropertyTerm.model(y, x)),
        )
        assert AgentProperty.from_data(property_.to_data(), task) == property_
        assert AgentProperty.from_json(property_.to_json(), task) == property_
        assert property_.to_data()["task_digest"] == task.object_digest
        assert PropertyProposition.not_equals(x, y)["kind"] == "not-equals"
        assert PropertyProposition.less_than(x, y)["kind"] == "less-than"
        assert PropertyProposition.less_or_equal(x, y)["kind"] == "less-or-equal"
        assert PropertyProposition.greater_than(x, y)["kind"] == "greater-than"
        assert PropertyProposition.greater_or_equal(x, y)["kind"] == "greater-or-equal"
        changed = property_.to_data()
        changed["parameters"][0]["lean_type"] = "String"
        with pytest.raises(IrError, match="outside the disclosed"):
            AgentProperty.from_data(changed, task)
        changed = property_.to_data()
        changed["proposition"]["left"]["arguments"].pop()
        with pytest.raises(IrError, match="wrong arity"):
            AgentProperty.from_data(changed, task)
        changed_task = json.loads(json.dumps(data))
        changed_task["kernel"]["model"] = "otherModel"
        with pytest.raises(IrError, match="Merkle content address"):
            PropertyTask.from_data(changed_task)
        empty_model = json.loads(json.dumps(data))
        empty_model["kernel"]["model"] = ""
        core = {key: empty_model[key] for key in ("schema", "target", "kernel", "contract", "templates")}
        empty_model["object_digest"] = merkle_object_digest(core)
        with pytest.raises(IrError, match="kernel is malformed"):
            PropertyTask.from_data(empty_model)

    def test_proof_task_and_attempt_bind_agent_context_and_tactics(self):
        goal = {
            "id": "a" * 64, "name": "keep_identity", "spec": "specs/Keep.lean", "line": 8,
            "target": "specs/Keep.lean::keep_identity",
            "theorem": "theorem keep_identity (x : Bool) : keep x = x",
            "source_anchor": "-- fr:spec src/lib.rs::keep @ " + "b" * 64,
            "signature_map": "-- fr:signature value: bool => value: Bool",
            "proof_region": "keep_identity", "object_digest": "a" * 64,
            "prove_template": ["spec", "prove", "specs/Keep.lean::keep_identity"],
            "verify": ["spec", "verify", "specs/Keep.lean"],
            "model_context_digest": merkle_object_digest("fixed model and package inputs"),
        }
        goal["id"] = goal["object_digest"] = merkle_object_digest({key: goal[key] for key in (
            "name", "spec", "target", "line", "theorem", "source_anchor", "signature_map", "proof_region", "model_context_digest")})
        contract = {
            "author": "agent", "format": "utf8-lean-tactics",
            "insertion_point": "inside-existing-by-block",
            "normalization": "trim-outer-newlines-indent-two-spaces", "forbidden": ["sorry"],
            "checker": "leanprover/lean4:v4.28.0",
        }
        claim = {"parameters": ["(x : Bool)"],
                 "parts": [{"assumptions": [], "conclusions": ["keep x = x"]}]}
        task = {
            "schema": "fr-proof-task-1", "goal": goal, "contract": contract,
            "claim": claim,
            "object_digest": merkle_object_digest({
                "schema": "fr-proof-task-1", "goal": goal, "contract": contract,
                "claim": claim,
            }),
            "actions": {
                "check": ["spec", "proof-check"], "apply": ["spec", "prove"],
                "verify": ["spec", "verify"],
            },
            "token_budget": {
                "limit": 4096, "used_upper_bound": 1500, "measurement": "serialized_utf8_bytes",
            },
        }
        parsed_task = ProofTask.from_data(task)
        assert parsed_task.to_data() == task
        wrong_target = json.loads(json.dumps(task))
        wrong_target["goal"]["target"] = "specs/Keep.lean::another_claim"
        with pytest.raises(IrError, match="supplied file and theorem"):
            ProofTask.from_data(wrong_target)
        bound = json.loads(json.dumps(task))
        bound["goal"]["model_context_digest"] = merkle_object_digest("current model module")
        addressed = {key: bound["goal"][key] for key in ("name", "spec", "target", "line", "theorem", "source_anchor", "signature_map", "proof_region", "model_context_digest")}
        bound["goal"]["id"] = bound["goal"]["object_digest"] = merkle_object_digest(addressed)
        core = {key: bound[key] for key in ("schema", "goal", "contract", "claim")}
        bound["object_digest"] = merkle_object_digest(core)
        assert ProofTask.from_data(bound).to_data() == bound
        bound["goal"]["model_context_digest"] = merkle_object_digest("changed model module")
        bound["object_digest"] = merkle_object_digest({key: bound[key] for key in ("schema", "goal", "contract", "claim")})
        with pytest.raises(IrError, match="model context identity"):
            ProofTask.from_data(bound)
        changed_task = json.loads(json.dumps(task))
        changed_task["goal"]["theorem"] = "changed"
        with pytest.raises(IrError, match="model context identity"):
            ProofTask.from_data(changed_task)

        changed_claim = json.loads(json.dumps(task))
        changed_claim["claim"]["parts"][0]["conclusions"] = ["False"]
        with pytest.raises(IrError, match="content address"):
            ProofTask.from_data(changed_claim)
        for claim in (
            {"parameters": [], "parts": []},
            {"parameters": [""], "parts": [{"assumptions": [], "conclusions": ["True"]}]},
            {"parameters": [], "parts": [{"assumptions": [""], "conclusions": ["True"]}]},
            {"parameters": [], "parts": [{"assumptions": [], "conclusions": []}]},
            {"parameters": [], "parts": [{"assumptions": ["True"] * 64, "conclusions": ["True"]}]},
        ):
            malformed = json.loads(json.dumps(task))
            malformed["claim"] = claim
            with pytest.raises(IrError, match="proof claim"):
                ProofTask.from_data(malformed)

        proof_digest = merkle_object_digest("rfl")
        checker_context = merkle_object_digest("checked module and exact package inputs")
        receipt_core = {
            "schema": "fr-proof-receipt-1", "goal_id": goal["id"],
            "proof_digest": proof_digest, "checker": contract["checker"],
            "checker_context_digest": checker_context,
        }
        attempt = {
            "schema": "fr-proof-attempt-1", "goal_id": goal["id"],
            "proof_digest": proof_digest, "checker": contract["checker"], "passed": True,
            "checker_context_digest": checker_context,
            "diagnostics": [], "diagnostics_omitted": 0,
            "receipt": merkle_object_digest(receipt_core),
            "actions": {
                "revise": ["spec", "proof-check"], "apply": ["spec", "prove"],
                "verify": ["spec", "verify"],
            },
            "token_budget": {
                "limit": 4096, "used_upper_bound": 900, "measurement": "serialized_utf8_bytes",
            },
        }
        assert ProofAttempt.from_data(attempt).to_data() == attempt
        rejected = json.loads(json.dumps(attempt))
        rejected.update(passed=False, receipt=None)
        rejected["actions"]["apply"] = None
        rejected["diagnostics"] = [{
            "severity": "error", "message": "unknown tactic",
            "draft_location": {"path": "draft.tactics", "line": 2, "column": 3,
                               "source_excerpt": "  unknown_step"},
            "checked_location": {"path": "<PROOF_CHECK>", "line": 42, "column": 5,
                                 "source_excerpt": "    unknown_step"},
            "context": ["⊢ True"], "context_omitted_lines": 12,
        }]
        assert ProofAttempt.from_data(rejected).to_data() == rejected
        for key, value in (("line", 0), ("column", True), ("path", ""), ("source_excerpt", 3)):
            changed = json.loads(json.dumps(rejected))
            changed["diagnostics"][0]["draft_location"][key] = value
            with pytest.raises(IrError):
                ProofAttempt.from_data(changed)
        rejected["diagnostics"][0]["context_omitted_lines"] = -1
        with pytest.raises(IrError, match="omitted context"):
            ProofAttempt.from_data(rejected)
        changed_context = json.loads(json.dumps(attempt))
        changed_context["checker_context_digest"] = merkle_object_digest("changed dependency")
        with pytest.raises(IrError, match="invalid receipt"):
            ProofAttempt.from_data(changed_context)
        missing_context = json.loads(json.dumps(attempt))
        del missing_context["checker_context_digest"]
        with pytest.raises(IrError):
            ProofAttempt.from_data(missing_context)
        attempt["receipt"] = "0" * 64
        with pytest.raises(IrError, match="invalid receipt"):
            ProofAttempt.from_data(attempt)

    def test_formal_plan_mirrors_rust_shape_and_rejects_tampering(self):
        core = {
            "schema": "fr-formal-plan-1",
            "target": {"source": "src/lib.rs", "symbol": "keep", "source_hash": "a" * 64},
            "kernel": {
                "module": "SrcLibRsKeep", "model": "keepModel",
                "inputs": [{"name": "value", "rust_type": "bool", "lean_type": "Bool"}],
                "output": {"name": "return", "rust_type": "bool", "lean_type": "Bool"},
                "semantic_ir": [{"kind": "return", "value": {"kind": "name", "value": "value"}}],
                "lean_definition": "def keepModel (value : Bool) : Bool :=\n  value",
            },
            "properties": [{
                "kind": "identity", "name": "keepModel_identity",
                "proposition": "(x : Bool) : keepModel x = x", "proof_status": "unproved",
            }],
            "correspondence": {
                "source_identity": "sha256-anchored-declaration",
                "signature_surface": "strict-rust-lean-map",
                "model_generation": "deterministic-supported-semantic-ir",
                "implementation_model": "generated-model-with-executable-comparison-required",
            },
            "assumptions": ["parser trusted"],
            "obligations": ["keepModel_identity"],
        }
        data = dict(core)
        data["object_digest"] = merkle_object_digest(core)
        data["actions"] = {
            "scaffold": ["spec", "scaffold", "--from", "<PLAN_FILE>"],
            "goals": ["spec", "goals", "specs"],
            "verify": ["spec", "verify", "specs"],
        }
        plan = FormalPlan.from_data(data)
        assert plan.to_data() == data
        assert FormalPlan.from_json(plan.to_json()).object_digest == data["object_digest"]
        tampered = json.loads(plan.to_json())
        tampered["kernel"]["model"] = "changedModel"
        with pytest.raises(IrError, match="Merkle content address"):
            FormalPlan.from_data(tampered)

    def test_merkle_object_pack_deduplicates_restores_and_detects_corruption(self):
        shared = {"kind": "name", "value": "item"}
        value = {"left": shared, "right": shared, "items": [shared, 1]}
        pack = merkle_object_pack(value)
        assert pack["root"] == merkle_object_digest(value)
        assert restore_merkle_object(pack["root"], pack["objects"]) == value
        fetched = []
        assert restore_merkle_object(
                pack["root"], lambda digest: fetched.append(digest) or pack["objects"].get(digest)
            ) == \
            value
        assert len(fetched) == len(set(fetched))
        assert len(pack["objects"]) < 1 + 3 * len(shared) + len(value["items"])
        broken = json.loads(json.dumps(pack["objects"]))
        scalar = next(key for key, record in broken.items()
                      if record["kind"] == "scalar" and record["value"] == "item")
        broken[scalar]["value"] = "changed"
        with pytest.raises(IrError, match="content verification"):
            restore_merkle_object(pack["root"], broken)
        with pytest.raises(IrError, match="lowercase SHA-256"):
            restore_merkle_object("not-a-digest", pack["objects"])
        with pytest.raises(IrError, match="absent or malformed"):
            restore_merkle_object("f" * 64, pack["objects"])
        cycle = "0" * 64
        with pytest.raises(IrError, match="contains a cycle"):
            restore_merkle_object(cycle, {
                cycle: {
                    "schema": "fr-merkle-object-1",
                    "kind": "array",
                    "children": [cycle],
                }
            })

    def test_merkle_pack_and_restore_preserve_addresses_for_json_shapes(self):
        values = [None, True, False, 0, -17, 1.5, 1e-5, 1e20, "雪", [], {},
                  [None, False, 0, ""], {"雪": [], "a": {"nested": [1, 2, 3]}}]
        for value in values:
            expected = merkle_object_digest(value)
            pack = merkle_object_pack(value)
            assert pack["root"] == expected
            assert restore_merkle_object(expected, pack["objects"]) == value
            for digest, record in pack["objects"].items():
                assert merkle_object_digest(restore_merkle_object(digest, pack["objects"])) == digest
                assert record["schema"] == "fr-merkle-object-1"

    def test_merkle_packing_hash_work_grows_with_input_size(self):
        from fr_ir import ir
        value = "leaf"
        for _ in range(64):
            value = [value]
        expected = merkle_object_digest(value)
        with patch.object(ir, "_tagged_hash", wraps=ir._tagged_hash) as hashed:
            pack = merkle_object_pack(value)
        assert pack["root"] == expected
        assert hashed.call_count < 4 * 65

    def test_merkle_hashes_repeated_separate_structures_once(self):
        from fr_ir import ir

        def tagged(*parts):
            return hashlib.sha256(json.dumps(
                ["fr-merkle-object-1", *parts], ensure_ascii=False,
                separators=(",", ":"), allow_nan=False,
            ).encode()).hexdigest()

        def binary_root(entries):
            if not entries:
                return tagged("empty")
            while len(entries) > 1:
                entries = [tagged("pair", *entries[index:index + 2])
                           if index + 1 < len(entries) else entries[index]
                           for index in range(0, len(entries), 2)]
            return entries[0]

        def reference(value):
            if isinstance(value, dict):
                entries = [tagged("object-entry", index, key, reference(value[key]))
                           for index, key in enumerate(sorted(value))]
                return tagged("object", len(value), binary_root(entries))
            if isinstance(value, list):
                entries = [tagged("array-entry", index, reference(child))
                           for index, child in enumerate(value)]
                return tagged("array", len(value), binary_root(entries))
            if value is None:
                return tagged("null")
            if isinstance(value, bool):
                return tagged("bool", value)
            if isinstance(value, int):
                return tagged("number", str(value))
            return tagged("string", value)

        branch = {"items": ["shared", None, True, {"name": "雪", "count": 17}]}
        value = [json.loads(json.dumps(branch)) for _ in range(257)]
        expected = reference(value)
        with patch.object(ir, "_merkle_record_digest", wraps=ir._merkle_record_digest) as hashed:
            pack = merkle_object_pack(value)
        assert pack["root"] == expected
        assert hashed.call_count == len(pack["objects"])
        with patch.object(ir, "_merkle_record_digest", wraps=ir._merkle_record_digest) as hashed:
            assert merkle_object_digest(value) == expected
        assert hashed.call_count == len(pack["objects"])
        assert restore_merkle_object(expected, pack["objects"]) == value

    def test_merkle_deduplication_keeps_numeric_types_and_signed_zero_distinct(self):
        value = [True, 1, 1.0, False, 0, 0.0, -0.0, "1"]
        pack = merkle_object_pack(value * 8)
        scalars = [record for record in pack["objects"].values() if record["kind"] == "scalar"]
        assert len(scalars) == len(value)
        for original, parts in zip(value, (("bool", True), ("number", "1"),
                ("number", "1.0"), ("bool", False), ("number", "0"),
                ("number", "0.0"), ("number", "-0.0"), ("string", "1"))):
            expected = hashlib.sha256(json.dumps(
                ["fr-merkle-object-1", *parts], separators=(",", ":"),
            ).encode()).hexdigest()
            assert expected in pack["objects"]
            assert merkle_object_digest(original) == expected
        restored = restore_merkle_object(pack["root"], pack["objects"])
        assert json.dumps(restored) == json.dumps(value * 8)

    def test_merkle_deduplication_does_not_reuse_results_between_operations(self):
        shared = {"name": "before"}
        value = [shared, shared]
        first = merkle_object_pack(value)
        shared["name"] = "after"
        second = merkle_object_pack(value)
        assert first["root"] != second["root"]
        assert merkle_object_digest(value) == second["root"]
        assert restore_merkle_object(first["root"], first["objects"]) == [{"name": "before"}] * 2
        assert restore_merkle_object(second["root"], second["objects"]) == [{"name": "after"}] * 2

    @pytest.mark.parametrize("operation", (merkle_object_digest, merkle_object_pack))
    def test_merkle_temporary_hashes_are_released_without_cyclic_collection(self, operation):
        import gc
        import weakref
        from fr_ir import ir

        class TrackedDigest(str):
            pass

        references = []
        original = ir._merkle_record_digest

        def tracked(record):
            digest = TrackedDigest(original(record))
            references.append(weakref.ref(digest))
            return digest

        enabled = gc.isenabled()
        gc.disable()
        try:
            with patch.object(ir, "_merkle_record_digest", tracked):
                result = operation({"items": [{"name": "same", "count": 7}] * 16})
            assert len(references) > 1
            if operation is merkle_object_digest:
                assert all(reference() is None for reference in references[:-1]), \
                    "digest lookup survived the completed operation"
            del result
            assert all(reference() is None for reference in references), \
                "hashing retained records after their caller released the result"
        finally:
            if enabled:
                gc.enable()
            gc.collect()

    def test_merkle_hashing_rejects_cycles_and_nonfinite_numbers(self):
        sequence = []
        sequence.append(sequence)
        mapping = {}
        mapping["self"] = mapping
        for operation in (merkle_object_digest, merkle_object_pack):
            for value in (sequence, mapping):
                with pytest.raises(IrError, match="contains a cycle"):
                    operation(value)
            for number in (float("nan"), float("inf"), float("-inf")):
                with pytest.raises(IrError, match="finite"):
                    operation([number, number])

    def test_merkle_restore_releases_records_without_cyclic_collection(self):
        import gc
        import weakref

        class TrackedScalar(str):
            pass

        enabled = gc.isenabled()
        gc.disable()
        try:
            scalar = TrackedScalar("saved value")
            reference = weakref.ref(scalar)
            digest = merkle_object_digest(scalar)
            objects = {digest: {"schema": "fr-merkle-object-1", "kind": "scalar", "value": scalar}}
            restored = restore_merkle_object(digest, objects)
            del scalar, objects
            assert reference() is restored
            del restored
            assert reference() is None, "restore retained its records after the caller released the result"
        finally:
            if enabled:
                gc.enable()
            gc.collect()

    def test_merkle_restore_verifies_shared_records_once_without_sharing_mutations(self):
        from fr_ir import ir
        value = {"items": [1, {"name": "original"}]}
        for _ in range(8):
            value = [value, value]
        pack = merkle_object_pack(value)
        with patch.object(ir, "_tagged_hash", wraps=ir._tagged_hash) as hashed:
            restored = restore_merkle_object(pack["root"], pack["objects"])
        assert restored == value
        assert hashed.call_count < 8 * len(pack["objects"])
        leaf = restored
        for _ in range(8):
            leaf = leaf[0]
        leaf["items"][1]["name"] = "changed"
        other = restored[1]
        for _ in range(7):
            other = other[0]
        assert other["items"][1]["name"] == "original"
        assert value[0][0][0][0][0][0][0][0]["items"][1]["name"] == "original"

    def test_merkle_restore_rejects_valid_children_under_the_wrong_parent(self):
        pack = merkle_object_pack({"left": [1], "right": [2]})
        record = pack["objects"][pack["root"]]
        record["entries"][0][1] = record["entries"][1][1]
        with pytest.raises(IrError, match="content verification"):
            restore_merkle_object(pack["root"], pack["objects"])

    def test_merkle_restore_rejects_noncanonical_record_shapes(self):
        pack = merkle_object_pack([1])
        objects = pack["objects"]
        root = pack["root"]
        objects[root]["unused"] = True
        with pytest.raises(IrError, match="unsupported shape"):
            restore_merkle_object(root, objects)
        with pytest.raises(IrError, match="non-scalar"):
            restore_merkle_object(root, {root: {
                "schema": "fr-merkle-object-1", "kind": "scalar", "value": [1]}})
        with pytest.raises(IrError, match="keys must be strings"):
            merkle_object_pack({1: "not a JSON object key"})

    def test_disclosure_proofs_reconstruct_the_root_and_reject_tampering(self):
        leaf = {"name": "run", "kind": "function"}
        sibling = merkle_object_digest([1, True, None])
        entry = hashlib.sha256(json.dumps(
            ["fr-merkle-object-1", "object-entry", 0, "left", sibling],
            separators=(",", ":"),
        ).encode()).hexdigest()
        leaf_digest = merkle_object_digest(leaf)
        right = hashlib.sha256(json.dumps(
            ["fr-merkle-object-1", "object-entry", 1, "right", leaf_digest],
            separators=(",", ":"),
        ).encode()).hexdigest()
        pair = hashlib.sha256(json.dumps(
            ["fr-merkle-object-1", "pair", entry, right], separators=(",", ":")
        ).encode()).hexdigest()
        root = hashlib.sha256(json.dumps(
            ["fr-merkle-object-1", "object", 2, pair], separators=(",", ":")
        ).encode()).hexdigest()
        proof = {
            "schema": "fr-merkle-inclusion-1",
            "algorithm": "sha256-tagged-binary-json-tree",
            "leaf": leaf_digest,
            "root": root,
            "path": [{
                "container": "object", "key": "right", "index": 1, "length": 2,
                "branch": [{"side": "left", "digest": entry}],
            }],
        }
        assert verify_disclosure_proof(leaf, proof)
        assert verify_disclosure_commitment(leaf_digest, proof)
        assert not verify_disclosure_proof({"name": "changed", "kind": "function"}, proof)
        tampered = json.loads(json.dumps(proof))
        tampered["path"][0]["branch"][0]["digest"] = "0" * 64
        assert not verify_disclosure_proof(leaf, tampered)
        tampered["path"][0]["branch"][0]["digest"] = "z" * 64
        assert not verify_disclosure_proof(leaf, tampered)

    def test_example_matches_adjacent_tag_shape(self):
        body = SemanticBody([
            Stmt.Return(Expr.Binary(BinaryOp.MUL, Expr.Name("value"), Expr.Int(2)))
        ])
        assert body.to_data() == \
            {
                "schema": "fr-semantic-body-1",
                "body": [{
                    "kind": "return",
                    "value": {
                        "kind": "binary",
                        "value": {
                            "op": "mul",
                            "left": {"kind": "name", "value": "value"},
                            "right": {"kind": "int", "value": "2"},
                        },
                    },
                }],
            }
        assert json.loads(body.to_json()) == body.to_data()

    def test_categories_cannot_cross_constructor_boundaries(self):
        with pytest.raises(IrError, match="must be a expr node"):
            Stmt.Return(Type.Int())
        with pytest.raises(IrError, match="must be a type node"):
            Type.List(Expr.Int(1))
        with pytest.raises(IrError, match="must be a statement node"):
            SemanticBody([Expr.Int(1)])

    def test_unknown_operators_are_rejected(self):
        with pytest.raises(IrError, match="invalid binary operator"):
            Expr.Binary("invented", Expr.Int(1), Expr.Int(2))

    def test_catalog_lists_every_public_constructor(self):
        assert set(kinds(Type)) == set(TYPE_KINDS)
        assert set(kinds(Stmt)) == set(STATEMENT_KINDS)
        assert set(kinds(Expr)) == set(EXPRESSION_KINDS)
        assert set(kinds(TemplatePart)) == set(TEMPLATE_KINDS)

    def test_semantic_changes_infer_typed_replacement_categories(self):
        body = SemanticBody([Stmt.Return(Expr.Name("left"))])
        change = SemanticChange(body, [
            Change.InsertStatement("/body", 0, Stmt.Comment("kept")),
            Change.Replace("/body/1/value", Expr.Name("right")),
            Change.DeleteStatement("/body/0"),
        ])
        data = change.to_data()
        assert data["schema"] == CHANGE_SCHEMA
        assert data["base"] == body.basis()
        assert data["operations"][1]["category"] == "expression"
        assert data["operations"][1]["value"]["kind"] == "name"

    def test_semantic_change_constructors_refuse_wrong_categories_and_paths(self):
        with pytest.raises(IrError, match="must be a statement node"):
            Change.InsertStatement("/body", 0, Expr.Int(1))
        with pytest.raises(IrError, match="canonical RFC 6901"):
            Change.DeleteStatement("/body/~2bad")
        with pytest.raises(IrError, match="1 through 64"):
            SemanticChange("frsb1:" + "0" * 64, [])

    def test_semantic_intents_mirror_roles_scalars_and_body_identity(self):
        body = SemanticBody([
            Stmt.Return(Expr.Binary(BinaryOp.ADD, Expr.Name("value"), Expr.Int(1)))
        ])
        target = [
            LocatorStep(Role.STATEMENT, index=0, category=NodeCategory.STATEMENT, kind="return"),
            LocatorStep(Role.RESULT, category=NodeCategory.EXPRESSION, kind="binary"),
            LocatorStep(Role.RIGHT, category=NodeCategory.EXPRESSION, kind="int"),
        ]
        intent = SemanticIntent(body, [Intent.SetInt(target, "1", "2")])
        assert intent.to_data() == {
            "schema": "fr-semantic-intent-1",
            "base": body.basis(),
            "operations": [{
                "op": "set-int",
                "target": [
                    {"role": "statement", "index": 0, "category": "statement", "kind": "return"},
                    {"role": "result", "category": "expression", "kind": "binary"},
                    {"role": "right", "category": "expression", "kind": "int"},
                ],
                "from": "1",
                "to": "2",
            }],
        }

    def test_semantic_intent_constructors_refuse_invalid_values(self):
        target = [LocatorStep(Role.STATEMENT, index=0)]
        with pytest.raises(IrError, match="portable decimal"):
            Intent.SetInt(target, "1", "-2")
        with pytest.raises(IrError, match="portable identifiers"):
            Intent.SetName(target, "before", "not-portable")
        with pytest.raises(IrError, match="must change"):
            Intent.SetComment(target, "same", "same")
        with pytest.raises(IrError, match="nonnegative"):
            LocatorStep(Role.STATEMENT, index=True)
        with pytest.raises(IrError, match="1 through 64"):
            Intent.SetBool([], False, True)

    def test_scalar_request_matches_the_reviewed_edit_plan_shape(self):
        request = ScalarRequest("set-binary-operator", BinaryOp.ADD, BinaryOp.MUL)
        assert request.to_data() == {
            "operation": "set-binary-operator",
            "from": "add",
            "to": "mul",
        }
        assert json.loads(request.to_json()) == request.to_data()
        with pytest.raises(IrError, match="must change"):
            ScalarRequest("set-int", "1", "1")

    def test_disclosed_edit_request_mirrors_the_opaque_manifest_shape(self):
        request = DisclosedEditRequest("frde1:" + "a" * 64, "7")
        assert request.to_data() == {"edit": "frde1:" + "a" * 64, "to": "7"}
        assert json.loads(request.to_json()) == request.to_data()
        with pytest.raises(IrError, match="exact frde1"):
            DisclosedEditRequest("frde1:short", "7")
        with pytest.raises(IrError, match="string CLI scalar"):
            DisclosedEditRequest("frde1:" + "a" * 64, 7)

    def test_disclosed_ir_edit_request_keeps_typed_ir_adjacent_to_the_wire_shape(self):
        identity = "frdi1:" + "b" * 64
        replacement = DisclosedIrEditRequest(identity, Expr.Int(7))
        assert replacement.to_data() == {
            "edit": identity,
            "value": {"kind": "int", "value": "7"},
        }
        deletion = DisclosedIrEditRequest(identity)
        assert deletion.to_data() == {"edit": identity}
        assert json.loads(replacement.to_json()) == replacement.to_data()
        with pytest.raises(IrError, match="exact frdi1"):
            DisclosedIrEditRequest("frdi1:short", Expr.Int(7))
        with pytest.raises(IrError, match="typed IR node"):
            DisclosedIrEditRequest(identity, {"kind": "int", "value": "7"})
