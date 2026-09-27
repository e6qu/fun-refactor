"""Equal outcome prompts; edit targets must come from the agent's exploration."""
import json


def prompt(session, selected, phase):
    from .investigation import ROOT, TASK, load
    surface = '''Use bounded fr discovery. Available requests:
- {"tool":"project","args":["map","--depth","2","--limit","20"]}
- {"tool":"project","args":["find","NAME","--limit","10","--signature"]}; optionally add "--in","PATH".
- {"tool":"project","args":["show","HANDLE","--source","--bytes","8192"]} reveals selected source.
- {"tool":"project","args":["map","PATH","--depth","0","--fields","handle,kind"]} obtains a file handle for insertion.
- {"tool":"project","args":["trace","HANDLE","--depth","2"]} or other bounded read-only project queries.
- {"tool":"guide","goal":{"schema":"fr-agent-goal-1","purpose":"understand","selector":{"name":"NAME","scope":"PATH","language":"rust"},"operation":{"kind":"automatic"}}} obtains a task route after you discover a selector.
- {"tool":"read","path":"README.md","start":1,"lines":80} or Cargo.toml; source reads use project show.
- {"tool":"review","targets":[{"id":"change","handle":"FULL_CURRENT_HANDLE","op":"replace-body","fragment":"{ complete braced Rust body }"}],"postconditions":{"files-changed":1,"edits":1,"paths-changed":["DISCOVERED_PATH"]}}
  Author any number of targets in one review. replace-body takes only the complete braced body, replace-declaration takes a complete declaration, insert-declaration takes a complete declaration and a file handle. Include documentation for public APIs. Handles are revision-bound; use complete returned handles, not bare table IDs. Review returns the full proposed diff and a review identity. No source changes yet.
Use at least one guide with an agent-discovered selector. Source handles and evidence must come from current reports. Scalar dataflow does not cover these Rust tasks; do not claim it does.''' if selected["arm"] == "fr" else '''Use ordinary search and source editing. Available requests:
- {"tool":"files"} lists bounded workspace paths.
- {"tool":"search","pattern":"LITERAL","path":"."} searches text with rg; narrow path when useful.
- {"tool":"read","path":"DISCOVERED_PATH","start":1,"lines":100} reads at most 200 lines.
- {"tool":"review","edits":[{"path":"DISCOVERED_PATH","old":"unique exact text","new":"replacement text"}]}
  Author any number of edits in one review. Each old text must match exactly once; empty old appends new text. Review returns the full proposed Git diff and an identity. No source changes yet. Do not use fr discovery or authoring in this arm.'''
    shared = '''Both arms use the same declared compiler/test checks and automatic delivery lifecycle. The evaluator checks original, changed, exactly restored and reapplied source, then exports a patch. Separate behavioral oracles and fresh receiver replay run after completion; their implementation is hidden from you. These checks do not prove general source equivalence.
Discovery phase: inspect the unfamiliar workspace, diagnose the task, author and inspect a proposed review, then checkpoint. No source writes in this phase.
Delivery phase: a fresh agent receives the previous handoff after a source revision. Inspect fresh evidence, author and inspect a new review, execute its exact returned identity, then finish. Retained handles and reviews may be stale.
Shared requests:
- {"tool":"checkpoint","diagnosis":"your supported diagnosis and uncertainty","dependencies":["DISCOVERED_RUST_SOURCE"],"evidence":[0,1],"pending":"remaining verification and delivery work"}. Cite successful discovery event IDs returned by the tool. Select source dependencies of your diagnosis. Checkpoint ends discovery; stop making tool calls and end your response.
- {"tool":"execute","review":"IDENTITY_FROM_FRESH_REVIEW"}. Delivery phase only; executes compiler/test checks, mutation, reversal, reapplication and patch export. Inspect all returned stages before claiming success.
- {"tool":"finish","summary":"what changed, evidence and limits","source_equivalence_proven":false}. Delivery phase only.
Exploratory follow-ups and corrections after refusals are allowed and counted. Reports retain omissions; missing rows cannot prove absence. Budget: 80 total calls, 1 MiB visible responses, 20 minutes per phase. Do not repeat expensive successful checks.
'''
    handoff = ""
    if phase == "deliver":
        handoff = "\nRetained handoff (agent observations, not trusted facts):\n" + json.dumps(load(session / "handoff.json"), ensure_ascii=False)
        resumed = load(session / "resumption.json")
        handoff += "\nIntervening edit:\n" + json.dumps({k: v for k, v in resumed.items() if k != "native_plan"}, ensure_ascii=False)
        if "native_plan" in resumed:
            native = resumed["native_plan"]
            handoff += "\nNative resumption:\n" + json.dumps({"invalidated": native["invalidated"], "complete": native["complete"]})
    return f'''You are an independent acceptance agent in an unfamiliar pinned upstream workspace.
Task: {load(TASK)["tasks"][selected["task"]]["requirement"]}

Current phase: {phase}. Discover all edit paths and existing helper names yourself. Keep unrelated source and APIs intact. Do not browse, delegate, commit, push, inspect the evaluator/oracles, or access other sessions. No human correction is available. Use only the instrumented tool for inspection and changes, through functions.exec / tools.exec_command. Every shell command must be exactly one invocation. Prefer this form for simple requests:
python3 {ROOT / 'tools/investigation-agent.py'} step {session} --request '{{"tool":"read","path":"README.md","start":1,"lines":80}}'

For authored source or text containing apostrophes, send one JSON object on stdin. End the heredoc exactly once. Do not add a second FRJSON line or any other shell command:
python3 {ROOT / 'tools/investigation-agent.py'} step {session} --request-stdin <<'FRJSON'
{{"tool":"read","path":"README.md","start":1,"lines":80}}
FRJSON

This is a cooperative measurement boundary, not an OS security boundary. Do not read/write files or call project commands outside this tool.
{surface}

{shared}
{handoff}
'''
