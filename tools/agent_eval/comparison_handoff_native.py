"""Two fresh native client processes, an intervening edit, and a delivered patch."""
import argparse
import hashlib
import json
import os
from pathlib import Path
import shutil
import signal
import time

import comparison_native as native
import comparison_handoff_actions as actions
from agent_eval.comparison_clients import HOSTS, SETUPS, require


def steps(host, setup, scenario, phase):
    result = []
    if setup == "codegraph":
        result.append({"kind": "mcp", "name": "mcp__codegraph__codegraph_explore" if host == "claude" else "codegraph_codegraph_explore",
                       "arguments": {"query": scenario["symbol"], "maxFiles": 2, "projectPath": "/workspace"},
                       "expected": [scenario["symbol"], "src/lib.rs"]})
    result.append({"kind": "command", "name": "Bash" if host == "claude" else "bash",
                   "arguments": {"command": f"python3 /probe/comparison_handoff_actions.py {phase} --setup {setup}",
                                 "description": f"Run the prescribed {phase} control on current task source"},
                   "expected": ["DISCOVER_OK" if phase == "discover" else "DELIVER_OK"]})
    return result


def probe(host, setup):
    require(not any(Path("/probe").glob("*grade*")) and not Path("/probe/oracle.py").exists(), "private grader leaked into the native image")
    scenario = json.loads(Path("/opt/handoff.json").read_text())
    # Copy children without replacing the explicitly configured mount-root metadata.
    for source in Path("/opt/project").iterdir():
        if source.is_dir():
            shutil.copytree(source, source.name)
        else:
            shutil.copy2(source, source.name)
    env, identity, version = native.prepare(host, setup)
    result = {"task": scenario["task"], "host": host, "setup": setup, "client_release": identity,
              "version": version, "live_model_calls": 0, "passed": False,
              "scope": "Prescribed complete-project repair and handoff; no autonomous agent, adoption, cost or unfamiliar-task claim."}
    try:
        result["discovery"] = native.exercise(host, setup, dict(env), steps(host, setup, scenario, "discover"), scenario["prompts"]["discover"])
        result["handoff"] = Path("HANDOFF.md").read_text()
        if setup == "codegraph":
            state = json.loads((actions.ARTIFACTS / "discovery.json").read_text())
            pid = state["codegraph_daemon_pid"]
            require(b"codegraph" in Path(f"/proc/{pid}/cmdline").read_bytes(), "daemon identity changed before shutdown")
            os.kill(pid, signal.SIGTERM)
            deadline = time.monotonic() + 5
            while True:
                try:
                    status = Path(f"/proc/{pid}/stat").read_text().split(") ", 1)[1].split()[0]
                except FileNotFoundError:
                    break
                if status == "Z":
                    break
                require(time.monotonic() < deadline, "CodeGraph daemon did not stop after graceful shutdown")
                time.sleep(0.05)
            result["codegraph_pause"] = {"daemon_pid": pid, "signal": "SIGTERM", "shutdown_before_edit": True}
        result["intervention"] = actions.intervene(scenario)
        if setup == "codegraph":
            result["codegraph_resume_sync"] = native.command(["codegraph", "sync", "/workspace", "--quiet"], env)
        result["delivery"] = native.exercise(host, setup, dict(env), steps(host, setup, scenario, "deliver"), scenario["prompts"]["deliver"])
        result["checks"] = json.loads((actions.ARTIFACTS / "delivery.json").read_text())
        result["stale_refusal"] = json.loads((actions.ARTIFACTS / "stale-refusal.json").read_text())
        if setup == "fr":
            result["reviews"] = {name: json.loads((actions.ARTIFACTS / (name + "-review.json")).read_text()) for name in ("old", "fresh")}
        result["patch"] = Path("change.patch").read_text()
        require(hashlib.sha256(result["patch"].encode()).hexdigest() == result["checks"]["patch_sha256"], "delivered patch changed")
        require(result["intervention"]["resumed"] == result["checks"]["before"], "delivery did not start from the intervening source")
        require(result["checks"]["stale_refused"] and result["checks"]["undo_exact"] and result["checks"]["redo_exact"], "handoff checks incomplete")
        result["passed"] = True
        return result
    except Exception as error:
        result["error"] = type(error).__name__ + ": " + str(error)[:4096]
        raise
    finally:
        (native.DIAGNOSTICS / "handoff-summary.json").write_text(json.dumps({key: result[key] for key in (
            "task", "host", "setup", "passed", "error", "checks", "intervention", "codegraph_pause") if key in result}) + "\n")
        (native.DIAGNOSTICS / "handoff-result.json").write_text(json.dumps(result, ensure_ascii=False) + "\n")


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--host", choices=HOSTS, required=True)
    parser.add_argument("--setup", choices=SETUPS, required=True)
    args = parser.parse_args()
    print(json.dumps(probe(args.host, args.setup), ensure_ascii=False))
