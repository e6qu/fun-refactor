"""Emit one valid source-free payload covering every SDK node constructor."""

from fr_ir import BinaryOp, Catch, Expr, Function, Param, SemanticBody, Stmt, TemplatePart, Type, UnaryOp, VariantArm


name = Expr.Name("x")
one = Expr.Int(1)
types = [
    Type.Unit(), Type.Bool(), Type.Int(), Type.Float(), Type.String(), Type.List(Type.Int()),
    Type.Set(Type.String()), Type.Map(Type.String(), Type.Int()), Type.Optional(Type.Int()),
    Type.Tuple([Type.Int(), Type.String()]), Type.Named("Result", [Type.Int()]),
    Type.Fn([Type.Int()], Type.String()),
]
expressions = [
    one, Expr.Float("1.5"), Expr.Str("s"), Expr.Bool(True), Expr.Null(), name,
    Expr.Field(name, "field"), Expr.Index(name, one), Expr.Call(name, [one]),
    Expr.Binary(BinaryOp.ADD, name, one), Expr.Unary(UnaryOp.NEG, one), Expr.Await(name),
    Expr.Propagate(name), Expr.Keyword("value", one), Expr.Cast(Expr.Name("T"), one),
    Expr.InstanceOf(name, Expr.Name("T")), Expr.New(Expr.Name("Thing"), [one]),
    Expr.RecordLit("Point", [("x", one)]), Expr.Coalesce(name, one),
    Expr.Ternary(Expr.Bool(True), one, Expr.Int(2)), Expr.Variant("Choice", "One", [("value", one)]),
    Expr.Tuple([one, name]), Expr.ListLit([one]), Expr.MapLit([(Expr.Str("k"), one)]),
    Expr.Template([TemplatePart.Text("x="), TemplatePart.Expr(name)]),
    Expr.Lambda([Param("n", Type.Int())], name, Type.Int()), Expr.SetLit([one]),
    Expr.Comprehension(name, "x", Expr.Name("xs"), Expr.Bool(True)),
]
statements = [
    Stmt.Return(one), Stmt.Let("x", Type.Int(), one, True), Stmt.Assign(name, one),
    Stmt.TupleAssign(["x", "y"], Expr.Tuple([one, one]), True),
    Stmt.If(Expr.Bool(True), [Stmt.Comment("then")], [Stmt.Comment("else")]),
    Stmt.IfPresent("x", name, [Stmt.Continue()]), Stmt.While(Expr.Bool(True), [Stmt.Break()]),
    Stmt.CountedFor([Stmt.Continue()], Stmt.Let("i", value=one), Expr.Bool(True), Stmt.Assign(name, one)),
    Stmt.ForEachIndexed("i", "x", Expr.Name("xs"), [Stmt.Continue()]),
    Stmt.Defer([Stmt.Comment("defer")]), Stmt.ErrDefer([Stmt.Comment("error")]),
    Stmt.Switch(name, [([one], [Stmt.Break()])], [Stmt.Continue()]),
    Stmt.MatchVariants(name, "Choice", [VariantArm("One", [("value", "x")], [Stmt.Return(name)])]),
    Stmt.WhilePresent("x", name, [Stmt.Break()]), Stmt.ForEach("x", Expr.Name("xs"), [Stmt.Continue()]),
    Stmt.Expr(one), Stmt.Assert(Expr.Bool(True), Expr.Str("ok")), Stmt.Comment("comment"),
    Stmt.LocalFunction(Function("inner", [Param("x", Type.Int())], Type.Int(), [Stmt.Return(name)])),
    Stmt.Block([Stmt.Comment("block")]), Stmt.Throw(name),
    Stmt.Try([Stmt.Throw(name)], [Catch("error", Type.Named("Error"), [Stmt.Return()])], [Stmt.Comment("finally")]),
    Stmt.Break(), Stmt.BreakWith("outer", one), Stmt.Continue(),
]
statements.extend(Stmt.Let(f"type_{index}", ty=ty) for index, ty in enumerate(types))
statements.extend(Stmt.Expr(expr) for expr in expressions)
print(SemanticBody(statements).to_json())
