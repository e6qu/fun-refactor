started = time.monotonic()
edges = []
source_bytes = 0
for name in FILES:
    source = (root/name).read_bytes()
    source_bytes += len(source)
    tree = ast.parse(source)
    for function in (node for node in tree.body if isinstance(node, ast.FunctionDef)):
        for call in (node for node in ast.walk(function) if isinstance(node, ast.Call)):
            assert isinstance(call.func, ast.Name)
            if call.func.id not in {"subtotal", "quote", "total"}:
                continue
            assert call.func.end_lineno is not None and call.func.end_col_offset is not None
        offsets = [0]
            for line in source.splitlines(keepends=True):
                offsets.append(offsets[-1]+len(line))
            edges.append({"consumer": function.name, "target": call.func.id, "path": name,
                "start": offsets[call.func.lineno-1]+call.func.col_offset,
                "end": offsets[call.func.end_lineno-1]+call.func.end_col_offset})
return {"edges": sorted(edges, key=lambda row: row["path"]), "source_bytes": source_bytes,
        "source_files": len(FILES), "seconds": time.monotonic()-started,
        "scope": "Independent AST inspection of this pinned direct-call fixture; not a general resolver."}