import inspect
import hashlib
import json
import re

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
            "theorem": "theorem keep_identity (x : Bool) : keep x = x",
            "source_anchor": "-- fr:spec src/lib.rs::keep @ " + "b" * 64,
            "signature_map": "-- fr:signature value: bool => value: Bool",
            "proof_region": "keep_identity", "object_digest": "a" * 64,
            "prove_template": ["spec", "prove", "specs/Keep.lean::keep_identity"],
            "verify": ["spec", "verify", "specs/Keep.lean"],
        }
        contract = {
            "author": "agent", "format": "utf8-lean-tactics",
            "insertion_point": "inside-existing-by-block",
            "normalization": "trim-outer-whitespace-indent-two-spaces", "forbidden": ["sorry"],
            "checker": "leanprover/lean4:v4.28.0",
        }
        templates = [{"kind": "direct", "lines": ["<agent-written-tactics>"]}]
        task = {
            "schema": "fr-proof-task-1", "goal": goal, "contract": contract,
            "templates": templates,
            "object_digest": merkle_object_digest({
                "schema": "fr-proof-task-1", "goal": goal, "contract": contract,
                "templates": templates,
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
        changed_task = json.loads(json.dumps(task))
        changed_task["goal"]["theorem"] = "changed"
        with pytest.raises(IrError, match="Merkle content address"):
            ProofTask.from_data(changed_task)

        proof_digest = merkle_object_digest("rfl")
        receipt_core = {
            "schema": "fr-proof-receipt-1", "goal_id": goal["id"],
            "proof_digest": proof_digest, "checker": contract["checker"],
        }
        attempt = {
            "schema": "fr-proof-attempt-1", "goal_id": goal["id"],
            "proof_digest": proof_digest, "checker": contract["checker"], "passed": True,
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
