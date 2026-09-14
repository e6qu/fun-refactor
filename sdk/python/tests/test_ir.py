import inspect
import hashlib
import json
import re
import unittest

from fr_ir import (
    CHANGE_SCHEMA,
    EXPRESSION_KINDS,
    STATEMENT_KINDS,
    TEMPLATE_KINDS,
    TYPE_KINDS,
    BinaryOp,
    Change,
    DisclosedEditRequest,
    DisclosedIrEditRequest,
    Expr,
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
    Type,
    merkle_object_digest,
    merkle_object_pack,
    restore_merkle_object,
    verify_disclosure_commitment,
    verify_disclosure_proof,
)


def kinds(namespace):
    names = (name for name, value in inspect.getmembers(namespace, inspect.isfunction) if not name.startswith("_"))
    return tuple(re.sub(r"(?<!^)(?=[A-Z])", "-", name).lower() for name in names)


class IrTests(unittest.TestCase):
    def test_merkle_object_pack_deduplicates_restores_and_detects_corruption(self):
        shared = {"kind": "name", "value": "item"}
        value = {"left": shared, "right": shared, "items": [shared, 1]}
        pack = merkle_object_pack(value)
        self.assertEqual(pack["root"], merkle_object_digest(value))
        self.assertEqual(restore_merkle_object(pack["root"], pack["objects"]), value)
        fetched = []
        self.assertEqual(
            restore_merkle_object(
                pack["root"], lambda digest: fetched.append(digest) or pack["objects"].get(digest)
            ),
            value,
        )
        self.assertEqual(len(fetched), len(set(fetched)))
        self.assertLess(len(pack["objects"]), 1 + 3 * len(shared) + len(value["items"]))
        broken = json.loads(json.dumps(pack["objects"]))
        scalar = next(key for key, record in broken.items()
                      if record["kind"] == "scalar" and record["value"] == "item")
        broken[scalar]["value"] = "changed"
        with self.assertRaisesRegex(IrError, "content verification"):
            restore_merkle_object(pack["root"], broken)
        with self.assertRaisesRegex(IrError, "lowercase SHA-256"):
            restore_merkle_object("not-a-digest", pack["objects"])
        with self.assertRaisesRegex(IrError, "absent or malformed"):
            restore_merkle_object("f" * 64, pack["objects"])
        cycle = "0" * 64
        with self.assertRaisesRegex(IrError, "contains a cycle"):
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
        self.assertTrue(verify_disclosure_proof(leaf, proof))
        self.assertTrue(verify_disclosure_commitment(leaf_digest, proof))
        self.assertFalse(verify_disclosure_proof({"name": "changed", "kind": "function"}, proof))
        tampered = json.loads(json.dumps(proof))
        tampered["path"][0]["branch"][0]["digest"] = "0" * 64
        self.assertFalse(verify_disclosure_proof(leaf, tampered))
        tampered["path"][0]["branch"][0]["digest"] = "z" * 64
        self.assertFalse(verify_disclosure_proof(leaf, tampered))

    def test_example_matches_adjacent_tag_shape(self):
        body = SemanticBody([
            Stmt.Return(Expr.Binary(BinaryOp.MUL, Expr.Name("value"), Expr.Int(2)))
        ])
        self.assertEqual(
            body.to_data(),
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
            },
        )
        self.assertEqual(json.loads(body.to_json()), body.to_data())

    def test_categories_cannot_cross_constructor_boundaries(self):
        with self.assertRaisesRegex(IrError, "must be a expr node"):
            Stmt.Return(Type.Int())
        with self.assertRaisesRegex(IrError, "must be a type node"):
            Type.List(Expr.Int(1))
        with self.assertRaisesRegex(IrError, "must be a statement node"):
            SemanticBody([Expr.Int(1)])

    def test_unknown_operators_are_rejected(self):
        with self.assertRaisesRegex(IrError, "invalid binary operator"):
            Expr.Binary("invented", Expr.Int(1), Expr.Int(2))

    def test_catalog_lists_every_public_constructor(self):
        self.assertEqual(set(kinds(Type)), set(TYPE_KINDS))
        self.assertEqual(set(kinds(Stmt)), set(STATEMENT_KINDS))
        self.assertEqual(set(kinds(Expr)), set(EXPRESSION_KINDS))
        self.assertEqual(set(kinds(TemplatePart)), set(TEMPLATE_KINDS))

    def test_semantic_changes_infer_typed_replacement_categories(self):
        body = SemanticBody([Stmt.Return(Expr.Name("left"))])
        change = SemanticChange(body, [
            Change.InsertStatement("/body", 0, Stmt.Comment("kept")),
            Change.Replace("/body/1/value", Expr.Name("right")),
            Change.DeleteStatement("/body/0"),
        ])
        data = change.to_data()
        self.assertEqual(data["schema"], CHANGE_SCHEMA)
        self.assertEqual(data["base"], body.basis())
        self.assertEqual(data["operations"][1]["category"], "expression")
        self.assertEqual(data["operations"][1]["value"]["kind"], "name")

    def test_semantic_change_constructors_refuse_wrong_categories_and_paths(self):
        with self.assertRaisesRegex(IrError, "must be a statement node"):
            Change.InsertStatement("/body", 0, Expr.Int(1))
        with self.assertRaisesRegex(IrError, "canonical RFC 6901"):
            Change.DeleteStatement("/body/~2bad")
        with self.assertRaisesRegex(IrError, "1 through 64"):
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
        self.assertEqual(intent.to_data(), {
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
        })

    def test_semantic_intent_constructors_refuse_invalid_values(self):
        target = [LocatorStep(Role.STATEMENT, index=0)]
        with self.assertRaisesRegex(IrError, "portable decimal"):
            Intent.SetInt(target, "1", "-2")
        with self.assertRaisesRegex(IrError, "portable identifiers"):
            Intent.SetName(target, "before", "not-portable")
        with self.assertRaisesRegex(IrError, "must change"):
            Intent.SetComment(target, "same", "same")
        with self.assertRaisesRegex(IrError, "nonnegative"):
            LocatorStep(Role.STATEMENT, index=True)
        with self.assertRaisesRegex(IrError, "1 through 64"):
            Intent.SetBool([], False, True)

    def test_scalar_request_matches_the_reviewed_edit_plan_shape(self):
        request = ScalarRequest("set-binary-operator", BinaryOp.ADD, BinaryOp.MUL)
        self.assertEqual(request.to_data(), {
            "operation": "set-binary-operator",
            "from": "add",
            "to": "mul",
        })
        self.assertEqual(json.loads(request.to_json()), request.to_data())
        with self.assertRaisesRegex(IrError, "must change"):
            ScalarRequest("set-int", "1", "1")

    def test_disclosed_edit_request_mirrors_the_opaque_manifest_shape(self):
        request = DisclosedEditRequest("frde1:" + "a" * 64, "7")
        self.assertEqual(request.to_data(), {"edit": "frde1:" + "a" * 64, "to": "7"})
        self.assertEqual(json.loads(request.to_json()), request.to_data())
        with self.assertRaisesRegex(IrError, "exact frde1"):
            DisclosedEditRequest("frde1:short", "7")
        with self.assertRaisesRegex(IrError, "string CLI scalar"):
            DisclosedEditRequest("frde1:" + "a" * 64, 7)

    def test_disclosed_ir_edit_request_keeps_typed_ir_adjacent_to_the_wire_shape(self):
        identity = "frdi1:" + "b" * 64
        replacement = DisclosedIrEditRequest(identity, Expr.Int(7))
        self.assertEqual(replacement.to_data(), {
            "edit": identity,
            "value": {"kind": "int", "value": "7"},
        })
        deletion = DisclosedIrEditRequest(identity)
        self.assertEqual(deletion.to_data(), {"edit": identity})
        self.assertEqual(json.loads(replacement.to_json()), replacement.to_data())
        with self.assertRaisesRegex(IrError, "exact frdi1"):
            DisclosedIrEditRequest("frdi1:short", Expr.Int(7))
        with self.assertRaisesRegex(IrError, "typed IR node"):
            DisclosedIrEditRequest(identity, {"kind": "int", "value": "7"})


if __name__ == "__main__":
    unittest.main()
