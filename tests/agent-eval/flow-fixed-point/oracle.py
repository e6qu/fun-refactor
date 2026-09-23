from pathlib import Path

calls = []
marker = object()
namespace = {"source": lambda: marker, "sink": calls.append}
exec(compile(Path(__file__).with_name("subject.py").read_text(), "subject.py", "exec"), namespace)
namespace["delayed"](3)
assert calls == [0, 0, marker], calls
calls.clear()
namespace["overwritten"](3)
assert calls == [0, 0, 0], calls
calls.clear()
namespace["stopped"](3)
namespace["skipped"](3)
try:
    namespace["raised"](ValueError("expected"))
except ValueError:
    pass
else:
    raise AssertionError("raise must leave the function")
assert calls == [], calls
print("five finite behavioral oracles passed")
