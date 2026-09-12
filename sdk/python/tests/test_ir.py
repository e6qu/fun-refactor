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
    IrError,
    SemanticBody,
    SemanticChange,
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


if __name__ == "__main__":
    unittest.main()
