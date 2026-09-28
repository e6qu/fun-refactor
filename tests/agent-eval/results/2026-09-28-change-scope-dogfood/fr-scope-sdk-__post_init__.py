if not 0 <= len(self.requests) <= 16:
    raise IrError("task change accepts 0 through 16 project requests")
ids = [request.id for request in self.requests]
if len(ids) != len(set(ids)):
    raise IrError("task change request IDs must be unique")
seen: set[str] = set()
for request in self.requests:
    for argument in request.arguments:
        if isinstance(argument, ProjectReference) and argument.request not in seen:
            raise IrError("project request references must name an earlier request")
    seen.add(request.id)
if not 1 <= len(self.targets) <= 32:
    raise IrError("task change needs 1 through 32 targets")
target_ids = [target.id for target in self.targets]
if len(target_ids) != len(set(target_ids)):
    raise IrError("task change target IDs must be unique")
if any(isinstance(target.handle, ProjectReference)
       and target.handle.request not in seen for target in self.targets):
    raise IrError("task target references an unknown project request")
allowed = {"files-changed", "edits", "changed-operations", "paths-changed"}
if not self.postconditions or not set(self.postconditions) <= allowed:
    raise IrError("task change postconditions are empty or unknown")
for name in ("files-changed", "edits", "changed-operations"):
    value = self.postconditions.get(name)
    if value is not None and (not isinstance(value, int) or isinstance(value, bool) or value < 0):
        raise IrError("numeric task postconditions must be non-negative integers")
paths = self.postconditions.get("paths-changed")
if paths is not None and (not isinstance(paths, list) or any(
        not isinstance(item, str) or Path(item).is_absolute()
        or not Path(item).parts or any(part in (".", "..") for part in Path(item).parts)
        for item in paths)):
    raise IrError("paths-changed must contain normalized relative paths")
if (not self.checks or len(self.checks) != len(set(self.checks))
        or any(not isinstance(name, str) or not name for name in self.checks)):
    raise IrError("task change needs named checks")
if (len(self.acceptance_checks) > 32
        or any(not isinstance(name, str) or not name for name in self.acceptance_checks)
        or len(set(self.acceptance_checks)) != len(self.acceptance_checks)):
    raise IrError("acceptance checks need at most 32 distinct names")
if self.change_scope is not None:
    if (not isinstance(self.change_scope, Mapping) or set(self.change_scope) != {"key", "digest"}
            or any(not isinstance(v, str) for v in self.change_scope.values())
            or not 1 <= len(self.change_scope["key"].encode()) <= 16384
            or len(self.change_scope["digest"]) != 64
            or any(c not in "0123456789abcdef" for c in self.change_scope["digest"])):
        raise IrError("change scope needs a bounded dependency key and SHA-256 digest")
encoded = json.dumps(self.to_data(), ensure_ascii=False, separators=(",", ":")).encode()
if len(encoded) > 65536:
    raise IrError("task change manifest exceeds 64 KiB")