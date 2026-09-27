"""Adversarial protocol checks; never contacts an agent service."""
import copy
import importlib.util
import json
import shlex
import subprocess
from pathlib import Path
import sys
import tempfile
import time
import unittest
from unittest.mock import patch

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
from agent_eval import investigation as trial
from agent_eval import investigation_run as live
from agent_eval.investigation_prompt import prompt

COHORT_SPEC = importlib.util.spec_from_file_location("investigation_cohort", trial.ROOT/"tools/investigation-cohort.py")
assert COHORT_SPEC is not None and COHORT_SPEC.loader is not None
cohort = importlib.util.module_from_spec(COHORT_SPEC)
COHORT_SPEC.loader.exec_module(cohort)

FR = None
if "--fr" in sys.argv:
    index = sys.argv.index("--fr")
    FR = str(Path(sys.argv[index+1]).resolve())
    del sys.argv[index:index+2]


class InvestigationProtocol(unittest.TestCase):
    def test_completed_behavioral_failure_is_retained_without_laundering_its_score(self):
        retained = trial.ROOT/"tests/agent-eval/results/2026-09-27-unknown-target-feature-files-failure"
        cohort.audit_comparison(retained, False)
        original_load = trial.load
        for field,value,message in (("passed",True,"outcome differs"),
                                    ("visible_bytes",0,"metrics differ"),
                                    ("resumption_passed",False,"complete its protocol")):
            def changed(path):
                result = original_load(path)
                if path == retained/"result.json":
                    result[field] = value
                return result
            with patch.object(trial,"load",side_effect=changed), self.assertRaisesRegex(ValueError,message):
                cohort.audit_comparison(retained, False)

    def test_concurrent_requests_cannot_observe_an_inflight_mutation(self):
        with tempfile.TemporaryDirectory() as directory:
            session=Path(directory)
            (session/"project").mkdir()
            (session/"project/lib.rs").write_text("before\n")
            trial.BASE.initialize(session/"project")
            trial.save(session/"state.json", {"phase":"deliver"})
            program = '''import sys,time,json
from pathlib import Path
sys.path.insert(0,sys.argv[1])
from agent_eval import investigation as t
session=Path(sys.argv[2]); kind=sys.argv[3]
t.config=lambda session: {}
def act(session, selected, request):
    source=session/'project/lib.rs'
    if request['tool']=='execute':
        source.write_text('inflight\\n')
        (session/'started').write_text('yes')
        time.sleep(0.4)
        source.write_text('after\\n')
    return {'source':source.read_text()}
t.act=act
print(json.dumps(t.step(session,{'tool':kind})))
'''
            argv=[sys.executable,"-c",program,str(trial.ROOT/"tools"),str(session)]
            writer=subprocess.Popen([*argv,"execute"],stdout=subprocess.PIPE,stderr=subprocess.PIPE)
            try:
                deadline=time.monotonic()+5
                while not (session/"started").exists() and time.monotonic()<deadline:
                    time.sleep(0.01)
                self.assertTrue((session/"started").exists())
                reader=subprocess.run([*argv,"read"],capture_output=True,text=True,timeout=5,check=True)
                output,error=writer.communicate(timeout=5)
                self.assertEqual(writer.returncode,0,error)
                self.assertEqual(json.loads(output)["event"],0)
                self.assertEqual(json.loads(reader.stdout),{"event":1,"result":{"source":"after\n"}})
                rows=trial.events(session)
                self.assertEqual([e["id"] for e in rows],[0,1])
                self.assertEqual(rows[0]["after"],rows[1]["before"])
            finally:
                if writer.poll() is None:
                    writer.kill(); writer.wait()

    def test_cohort_requires_matched_inputs_and_distinct_agent_processes(self):
        entries = [{"task":task,"arm":arm,"thread_ids":[f"{task}-{arm}-1",f"{task}-{arm}-2"],
            "profile":{"archive":"pinned"},"binary_sha256":"same", "bindings":{"runner":"same"},
            "requirement":"same", "settings":{"model":"same"}, "perturbation":{"path":"same"}}
            for task in ("unicode-dice","regex-escape-len") for arm in ("fr","files")]
        self.assertEqual(len(cohort.validate_pairs(entries)),2)
        with self.assertRaisesRegex(ValueError,"both arms"):
            cohort.validate_pairs(entries[:-1])
        for field in ("profile","binary_sha256","bindings","requirement","settings","perturbation"):
            bad=copy.deepcopy(entries); bad[1][field]="different"
            with self.assertRaisesRegex(ValueError,f"unmatched {field}"):
                cohort.validate_pairs(bad)
        bad=copy.deepcopy(entries); bad[1]["thread_ids"][0]=bad[0]["thread_ids"][0]
        with self.assertRaisesRegex(ValueError,"reused"):
            cohort.validate_pairs(bad)

    @unittest.skipUnless(FR, "native binary supplied by Cargo integration test")
    def test_native_review_retains_exact_manifest_and_refuses_stale_source(self):
        with tempfile.TemporaryDirectory() as directory:
            session = Path(directory)
            project = session/"project"
            project.mkdir()
            (project/".fr").mkdir()
            (project/"artifacts").mkdir()
            source = project/"lib.rs"
            source.write_text("pub fn value() -> u8 { 1 }\n")
            checks = [{"name":"syntax", "argv":[sys.executable,"-c",
                "import subprocess,tempfile; t=tempfile.TemporaryDirectory(); subprocess.run(['rustc','--crate-type','lib','lib.rs','-o',t.name+'/lib.rlib'],check=True)"],
                "cwd":".","timeout_seconds":20,"covers":["Rust compilation"]}]
            trial.save(project/".fr/checks.json", {"schema":1,"checks":checks})
            trial.BASE.initialize(project)
            selected = {"arm":"fr","binary":FR}
            client = trial.client(session, selected)
            def reviewed():
                target = client.project("find","value").definition_target()
                change = trial.make_change({"tool":"review","targets":[{"id":"change","handle":target.handle,
                    "op":"replace-body","fragment":"{ 2 }"}],"postconditions":{"files-changed":1,"edits":1}}, checks)
                native = client.review(change)
                value = {"kind":"fr","manifest_json":native.manifest.decode(),"report":native.to_data(),"before":trial.snapshot(project)}
                trial.save(session/"review.json",value)
                return trial.sha(trial.encode(value))
            identity = reviewed()
            source.write_text(source.read_text()+trial.PERTURBATION)
            before = source.read_bytes()
            with self.assertRaises(trial.FrRuntimeError):
                trial.execute_review(session, selected, identity)
            self.assertEqual(before,source.read_bytes())
            result = trial.execute_review(session, selected, reviewed())
            self.assertTrue(live.delivery_passed(result))
            self.assertIn("{ 2 }", source.read_text())
            self.assertTrue(source.read_text().endswith(trial.PERTURBATION))

    def test_prompts_do_not_supply_edit_paths_or_private_helpers(self):
        for task in trial.load(trial.TASK)["tasks"]:
            for arm in ("fr", "files"):
                text = prompt(Path("/tmp/trial"), {"task": task, "arm": arm}, "discover")
                for target in ("src/lib.rs", "sorensen_dice", "is_meta_character"):
                    self.assertNotIn(target, text)
                self.assertIn("Discover all edit paths", text)
                self.assertIn("source_equivalence_proven", text)

    def test_path_traversal_and_symlink_refuse(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            (root/"link").symlink_to("/tmp")
            for name in ("../file", "/etc/passwd", ".git/config", "target/file", "link/file"):
                with self.assertRaises(ValueError):
                    trial.checked_path(root, name)

    def test_file_review_restores_source_even_after_later_edit_refuses(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            source = root/"lib.rs"
            source.write_text("pub fn value() -> u8 { 1 }\n")
            before = source.read_bytes()
            with self.assertRaisesRegex(ValueError, "one exact match"):
                trial.baseline_patch(root, [{"path":"lib.rs", "old":"{ 1 }", "new":"{ 2 }"},
                                           {"path":"lib.rs", "old":"missing", "new":"bad"}])
            self.assertEqual(source.read_bytes(), before)

    def test_patch_is_relative_to_intervening_worktree_change(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            source = root/"lib.rs"
            source.write_text("pub fn value() -> u8 { 1 }\n")
            trial.BASE.initialize(root)
            source.write_text(source.read_text()+trial.PERTURBATION)
            before = source.read_bytes()
            change = trial.baseline_patch(root, [{"path":"lib.rs", "old":"{ 1 }", "new":"{ 2 }"}]).encode()
            self.assertNotIn(b"+// Independent", change)
            trial.BASE.git(root, "apply", "--check", data=change)
            trial.BASE.git(root, "apply", data=change)
            self.assertIn("{ 2 }", source.read_text())
            self.assertEqual(source.read_text().count(trial.PERTURBATION), 1)
            trial.BASE.git(root, "apply", "--reverse", data=change)
            self.assertEqual(source.read_bytes(), before)

    def test_stale_file_review_cannot_write(self):
        with tempfile.TemporaryDirectory() as directory:
            session = Path(directory)
            project = session/"project"
            project.mkdir()
            (project/"lib.rs").write_text("fn f() {}\n")
            trial.BASE.initialize(project)
            value = {"kind":"files", "before":trial.snapshot(project), "patch":"not reached"}
            trial.save(session/"review.json", value)
            (project/"lib.rs").write_text("fn f() { /* changed */ }\n")
            before = trial.snapshot(project)
            with self.assertRaisesRegex(ValueError, "stale source review"):
                trial.execute_review(session, {"arm":"files"}, trial.sha(trial.encode(value)))
            self.assertEqual(before, trial.snapshot(project))

    def test_all_native_delivery_stages_are_required(self):
        good = {"kind":"fr", "report":{"passed":True, "workflow":{"stages":[
            {"stage":name, "status":"passed"} for name in live.STAGES]}}}
        self.assertTrue(live.delivery_passed(good))
        for i in range(len(live.STAGES)):
            bad = copy.deepcopy(good)
            del bad["report"]["workflow"]["stages"][i]
            self.assertFalse(live.delivery_passed(bad))
            bad = copy.deepcopy(good)
            bad["report"]["workflow"]["stages"][i]["status"] = "failed"
            self.assertFalse(live.delivery_passed(bad))

    def test_file_delivery_requires_exact_restoration(self):
        value = {"kind":"files", "passed":True, "stages":[
            {"name":name, "passed":True, "snapshot":state} for name,state in
            (("original", "a"), ("changed", "b"), ("restored", "a"), ("reapplied", "b"))]}
        self.assertTrue(live.delivery_passed(value))
        value["stages"][2]["snapshot"] = "lost unrelated change"
        self.assertFalse(live.delivery_passed(value))

    def transcript(self, session, command, request):
        phase = "discover"
        (session/"prompt-discover.txt").write_text("task")
        rows = [{"type":"item.completed", "item":{"type":"command_execution", "command":command, "exit_code":0}},
                {"type":"turn.completed", "usage":{"input_tokens":10,"output_tokens":2}}]
        stream = session/"codex-discover-events.jsonl"
        stream.write_text("\n".join(json.dumps(r) for r in rows)+"\n")
        trial.save(session/"codex-discover-run.json", {"files":{stream.name:trial.sha(stream.read_bytes())},
            "prompt_sha256":trial.sha(b"task"), "exit_code":0, "stopped_reason":None, "seconds":1,
            "sampled_group_peak_rss_bytes":100})
        return live.transcript(session, phase, [{"phase":phase,"request":request}])

    def test_transcript_rejects_commands_that_only_contain_allowed_prefix(self):
        with tempfile.TemporaryDirectory() as directory:
            session = Path(directory)
            request = {"tool":"files"}
            allowed = f"python3 {trial.ROOT/'tools/investigation-agent.py'} step {session} --request-stdin <<'FRJSON'\n"+json.dumps(request)+"\nFRJSON"
            self.assertTrue(self.transcript(session, allowed, request)["passed"])
            wrapped = shlex.join(["/bin/zsh", "-lc", allowed])
            self.assertTrue(self.transcript(session, wrapped, request)["passed"])
            positional = shlex.join(["python3",str(trial.ROOT/'tools/investigation-agent.py'),"step",str(session),"--request",json.dumps(request)])
            self.assertTrue(self.transcript(session, positional, request)["passed"])
            self.assertFalse(self.transcript(session, positional+"; cat secret", request)["passed"])
            for command in ("cat secret; "+allowed, allowed+"\ncat secret", allowed.replace('"files"', '"read"')):
                self.assertFalse(self.transcript(session, command, request)["passed"])
                self.assertFalse(self.transcript(session, shlex.join(["/bin/zsh","-lc",command]), request)["passed"])

    def test_plan_checkpoint_rejects_uncited_or_failed_discovery(self):
        with tempfile.TemporaryDirectory() as directory:
            session = Path(directory)
            (session/"project").mkdir()
            (session/"project/lib.rs").write_text("fn f() {}\n")
            trial.save(session/"review.json", {"paths":["lib.rs"]})
            request = {"tool":"checkpoint", "dependencies":["lib.rs"], "evidence":[0], "diagnosis":"cause", "pending":"check"}
            for prior in ([], [{"request":{"tool":"read"}, "visible":{"error":"not found"}}],
                          [{"request":{"tool":"review"}, "visible":{}}]):
                with patch.object(trial,"events",return_value=prior), self.assertRaisesRegex(ValueError,"cite"):
                    trial.checkpoint(session, {"arm":"files"}, request)

    def test_source_event_chain_rejects_hidden_mutations_and_missing_events(self):
        original, resumed, final = {"lib.rs":"old"}, {"lib.rs":"comment"}, {"lib.rs":"new"}
        a,b,c = [trial.sha(trial.encode(v)) for v in (original,resumed,final)]
        rows = [{"id":0,"phase":"discover","request":{"tool":"read"},"before":a,"after":a},
                {"id":1,"phase":"deliver","request":{"tool":"execute"},"before":b,"after":c}]
        self.assertTrue(live.event_chain(rows,{"original":original},{"resume_snapshot":resumed},final))
        bad = copy.deepcopy(rows); bad[0]["after"] = c
        self.assertFalse(live.event_chain(bad,{"original":original},{"resume_snapshot":resumed},final))
        bad = copy.deepcopy(rows); bad[1]["before"] = a
        self.assertFalse(live.event_chain(bad,{"original":original},{"resume_snapshot":resumed},final))
        self.assertFalse(live.event_chain(rows[1:],{"original":original},{"resume_snapshot":resumed},final))


if __name__ == "__main__":
    unittest.main()
