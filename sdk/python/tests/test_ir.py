import inspect
import json
import re
import unittest

from fr_ir import EXPRESSION_KINDS, STATEMENT_KINDS, TEMPLATE_KINDS, TYPE_KINDS, BinaryOp, Expr, IrError, SemanticBody, Stmt, TemplatePart, Type


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


if __name__ == "__main__":
    unittest.main()
