"""Prescribed positive and negative controls; these are never live-agent evidence."""
from pathlib import Path

from . import investigation as trial
from . import investigation_run as live


def call(session, request):
    response = trial.step(session, request)
    trial.require("error" not in response["result"], str(response))
    return response


def replacement(task, project):
    if task == "unicode-dice":
        path, name = "src/lib.rs", "sorensen_dice"
        source = (project/path).read_text()
        start = source.index(f"pub fn {name}(")
        end = source.index("\n}\n", start)+2
        old = source[start:end]
        new = old.replace("a.len()", "a.chars().count()").replace("b.len()", "b.chars().count()")
        return [(path, name, old, new)]
    source = (project/"regex-syntax/src/lib.rs").read_text()
    start = source.index("pub fn escape(")
    end = source.index("\n}\n", start)+2
    old = source[start:end]
    return [("regex-syntax/src/lib.rs", "escape", old, old.replace("String::new()", "String::with_capacity(escape_len(text))")),
        ("regex-syntax/src/lib.rs", None, "", "\n/// Returns the escaped UTF-8 byte length without allocating.\npub fn escape_len(pattern: &str) -> usize { pattern.len() + pattern.chars().filter(|&c| is_meta_character(c)).count() }\n"),
        ("src/lib.rs", None, "", "\n/// Returns the escaped UTF-8 byte length without allocating.\npub fn escape_len(pattern: &str) -> usize { regex_syntax::escape_len(pattern) }\n")]


def proposal(session, task, arm):
    edits = replacement(task, session/"project")
    citations, targets = [], []
    for i, (path, name, old, new) in enumerate(edits):
        if arm == "files":
            citations.append(call(session, {"tool":"read", "path":path, "start":1, "lines":40})["event"])
        else:
            args = ["find", name, "--in", path] if name else ["map", path, "--depth", "0", "--fields", "handle,kind"]
            found = call(session, {"tool":"project", "args":args})
            citations.append(found["event"])
            handle = found["result"]["rows"][0][0]
            if name:
                call(session, {"tool":"project", "args":["show", handle, "--source", "--bytes", "8192"]})
            targets.append({"id":f"edit{i}", "handle":handle, "op":"replace-declaration" if name else "insert-declaration", "fragment":new})
    if arm == "fr":
        request = {"tool":"review", "targets":targets, "postconditions":{
            "files-changed":len({e[0] for e in edits}), "edits":len(edits), "paths-changed":sorted({e[0] for e in edits})}}
    else:
        request = {"tool":"review", "edits":[{"path":path, "old":old, "new":new} for path, _, old, new in edits]}
    reviewed = call(session, request)
    return reviewed["result"]["review"], citations


def run(session, binary, task, arm):
    trial.prepare(session, binary, task, arm)
    if arm == "fr":
        call(session, {"tool":"guide", "goal":{"schema":"fr-agent-goal-1", "purpose":"understand",
            "selector":{"name":"sorensen_dice" if task == "unicode-dice" else "escape", "scope":"src/lib.rs", "language":"rust"},
            "operation":{"kind":"automatic"}}})
    _, citations = proposal(session, task, arm)
    call(session, {"tool":"checkpoint", "diagnosis":"Prescribed rehearsal; targets supplied by controller, not discovered by an agent.",
        "dependencies":list(trial.BASE.edit_paths(task)), "evidence":citations, "pending":"Refresh current targets and deliver the prescribed implementation."})
    resumed = trial.resume(session)
    identity, _ = proposal(session, task, arm)
    delivered = call(session, {"tool":"execute", "review":identity})["result"]
    trial.require(live.delivery_passed(delivered), "prescribed lifecycle failed")
    call(session, {"tool":"finish", "summary":"Prescribed rehearsal completed", "source_equivalence_proven":False})
    final = trial.snapshot(session/"project")
    receiver = live.behavior(task, (session/"project/artifacts/change.patch").read_bytes(), resumed, final)
    trial.require(receiver["passed"], "prescribed receiver behavior failed")
    result = {"schema":"fr-unknown-target-rehearsal-1", "task":task, "arm":arm, "live_agent":False,
              "passed":True, "resumption":resumed, "receiver":receiver, "bindings":trial.bindings()}
    trial.save(session/"rehearsal.json", result)
    return {"passed":True, "task":task, "arm":arm, "live_agent":False, "oracle":receiver["oracle"]}
