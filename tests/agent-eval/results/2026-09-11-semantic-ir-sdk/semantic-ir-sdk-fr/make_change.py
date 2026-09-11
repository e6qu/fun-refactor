from fr_ir import Expr, SemanticBody, Stmt, Type


body = SemanticBody(
    [
        Stmt.Let("total", ty=Type.Int(), value=Expr.Int(0), mutable=True),
        Stmt.ForEach(
            "item",
            Expr.Name("values"),
            [
                Stmt.If(
                    Expr.Binary("gt", Expr.Name("item"), Expr.Int(0)),
                    [
                        Stmt.Assign(
                            Expr.Name("total"),
                            Expr.Binary("add", Expr.Name("total"), Expr.Name("item")),
                        )
                    ],
                )
            ],
        ),
        Stmt.Return(Expr.Name("total")),
    ]
)

body.write("change.json")
