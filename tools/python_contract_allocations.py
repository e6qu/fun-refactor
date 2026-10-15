"""Inspect recorded allocation declarations independently with Python's AST."""
import ast


def scalar(node):
    try:
        result = ast.literal_eval(node)
    except (ValueError, TypeError, SyntaxError):
        return False
    return type(result) in {type(None), bool, int, float, complex, str, bytes}


def literal_key(node):
    return scalar(node) or isinstance(node, ast.Tuple) and all(literal_key(part) for part in node.elts)


def dictionary(node, shadowed):
    if isinstance(node, ast.Dict):
        return all(key is not None and literal_key(key) and allocation(value, shadowed)
                   for key, value in zip(node.keys, node.values))
    return (isinstance(node, ast.Call) and isinstance(node.func, ast.Name) and node.func.id == 'dict'
            and not shadowed and len(node.args) <= 1
            and all(dictionary(argument, shadowed) for argument in node.args)
            and all(keyword.arg is not None and keyword.arg.isascii() and allocation(keyword.value, shadowed)
                    for keyword in node.keywords)
            and len({keyword.arg for keyword in node.keywords}) == len(node.keywords))


def allocation(node, shadowed=False):
    if scalar(node):
        return True
    if isinstance(node, (ast.List, ast.Tuple)):
        return all(allocation(part, shadowed) for part in node.elts)
    if isinstance(node, ast.Set):
        return all(literal_key(part) for part in node.elts)
    if isinstance(node, ast.Lambda):
        args = node.args
        parameters = [*args.posonlyargs, *args.args, *args.kwonlyargs,
                      *([args.vararg] if args.vararg else []), *([args.kwarg] if args.kwarg else [])]
        return (all(argument.arg.isascii() and argument.annotation is None for argument in parameters)
                and all(scalar(default) for default in [*args.defaults, *args.kw_defaults] if default is not None))
    return dictionary(node, shadowed)


def audit_declaration(node, statements):
    """Check spelling cannot turn a locally replaced dict into a builtin call."""
    shadowed = False
    for statement in statements:
        if statement is node:
            break
        if isinstance(statement, (ast.FunctionDef, ast.AsyncFunctionDef, ast.ClassDef)):
            shadowed |= statement.name == 'dict'
        elif isinstance(statement, (ast.Import, ast.ImportFrom)):
            shadowed |= any((alias.asname or alias.name.split('.')[0]) in {'dict', '__builtins__', '*'}
                            for alias in statement.names)
        elif isinstance(statement, (ast.Assign, ast.AnnAssign, ast.AugAssign)):
            targets = statement.targets if isinstance(statement, ast.Assign) else [statement.target]
            shadowed |= any(isinstance(target, ast.Name) and target.id in {'dict', '__builtins__'}
                            for root in targets for target in ast.walk(root))
    return allocation(node.value, shadowed)
