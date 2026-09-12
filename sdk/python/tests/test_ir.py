import inspect
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
)


def kinds(namespace):
    names = (name for name, value in inspect.getmembers(namespace, inspect.isfunction) if not name.startswith("_"))
    return tuple(re.sub(r"(?<!^)(?=[A-Z])", "-", name).lower() for name in names)


class IrTests(unittest.TestCase):
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


if __name__ == "__main__":
    unittest.main()
