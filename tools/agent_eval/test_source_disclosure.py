"""Offline source paging, overlap accounting and provider-loop regression tests."""
import base64
import copy
import hashlib
from pathlib import Path
import sys
import unittest

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
from agent_eval.source_disclosure import counts, read_source, validate_read
from agent_eval.study import encode
from agent_eval.study_runner import tool_definitions
from agent_eval import test_study_runner as runner_fixtures
from agent_eval.test_study_runner import FakeProvider, call, files


def arguments(offset=0, size=4096, identity="", path="main.py"):
    return {"path": path, "offset": offset, "bytes": size, "sha256": identity}


def start(agent="parent", parent=None):
    return {"agent": agent, "parent": parent, "instructions": "policy", "question": "task"}


def events_for(state, requests):
    events = [start()]
    for index, (agent, args) in enumerate(requests):
        if agent == "child" and not any(row.get("instructions") and row["agent"] == agent for row in events):
            events.append(start("child", "parent"))
        identity = str(index)
        events.extend([{"agent": agent, "tool_call": call("read_source", args, identity)},
                       {"agent": agent, "tool_result": identity, "result": read_source(state, args, 1024)}])
    return events


class SourceTests(unittest.TestCase):
    def test_ambiguous_events_hidden_source_in_errors_and_post_final_calls_refuse(self):
        events = events_for({}, [("parent", arguments())])
        changed = copy.deepcopy(events)
        changed[-1]["result"]["text"] = "unaccounted source"
        with self.assertRaisesRegex(ValueError, "refusal contains disclosure"):
            counts(changed, [])
        changed = copy.deepcopy(events)
        changed[-1]["instructions"] = "pretend this is an instruction event"
        with self.assertRaisesRegex(ValueError, "ambiguous"):
            counts(changed, [])
        with self.assertRaisesRegex(ValueError, "unfinished agents"):
            counts(events, [], completed=True)
        events.append({"agent": "parent", "final": "Done"})
        events.append({"agent": "parent", "tool_call": call(identity="late")})
        with self.assertRaisesRegex(ValueError, "replayed tool call"):
            counts(events, [])

    def test_request_and_agent_identities_are_retained_without_hiding_failed_sends(self):
        events = [start(), {"agent": "parent", "request": "first"},
                  start("child", "parent"), {"agent": "child", "request": "second"},
                  {"failure_type": "TimeoutError"}]
        observed = counts(events, [])
        self.assertEqual(observed["agent_parents"], {"parent": None, "child": "parent"})
        self.assertEqual(observed["requests"], {"parent": ["first"], "child": ["second"]})
        self.assertEqual(observed["completed_agents"], [])
        with self.assertRaisesRegex(ValueError, "replayed trace request"):
            counts(events + [{"agent": "child", "request": "first"}], [])

    def test_pages_reassemble_utf8_at_exact_byte_boundaries(self):
        content = "α😀line\n" * 300
        state, args, chunks = files(content), arguments(size=23), []
        while True:
            result = read_source(state, args, 1024)
            self.assertLessEqual(len(encode(result)), 1024)
            self.assertEqual(result["end_offset"] - result["offset"], len(result["text"].encode()))
            self.assertLessEqual(len(result["text"].encode()), 23)
            chunks.append(result["text"])
            if result["next_offset"] is None:
                break
            args = arguments(result["next_offset"], 23, result["sha256"])
        self.assertEqual("".join(chunks), content)

    def test_json_escape_overhead_is_included_in_output_limit(self):
        result = read_source(files('\x00"\\😀' * 1000), arguments(), 1024)
        self.assertLessEqual(len(encode(result)), 1024)
        self.assertGreater(result["end_offset"], 0)
        self.assertLess(result["end_offset"], 4096)

    def test_changed_file_refuses_stale_continuation_without_disclosing_content(self):
        first = read_source(files("before"), arguments(size=2), 1024)
        result = read_source(files("changed"), arguments(2, 2, first["sha256"]), 1024)
        self.assertEqual(result["error"], "stale_source")
        self.assertNotIn("text", result)

    def test_bad_paths_limits_and_unpinned_continuations_refuse(self):
        bad = [arguments(path=path) for path in ("../secret", "/etc/passwd", "a//b", "a\\b", "", ".")]
        bad += [arguments(offset=1), arguments(offset=-1), arguments(offset=True),
                arguments(size=0), arguments(size=65537), arguments(identity="not-a-hash")]
        for args in bad:
            with self.subTest(args=args), self.assertRaises(ValueError):
                validate_read(args)

    def test_missing_binary_split_character_and_small_budget_are_explicit(self):
        self.assertEqual(read_source({}, arguments(), 1024), {"error": "missing_file"})
        binary = {"main.py": {"data": base64.b64encode(b"\xff").decode(), "executable": False}}
        self.assertEqual(read_source(binary, arguments(), 1024)["error"], "invalid_utf8_or_boundary")
        identity = hashlib.sha256("😀".encode()).hexdigest()
        for args, error in ((arguments(1, 4, identity), "invalid_utf8_or_boundary"),
                            (arguments(size=1), "page_budget_too_small"),
                            (arguments(5, 4, identity), "offset_out_of_range")):
            self.assertEqual(read_source(files("😀"), args, 1024)["error"], error)
        self.assertEqual(read_source(files("text"), arguments(), 128)["error"], "page_budget_too_small")

    def test_empty_file_is_a_complete_zero_byte_page(self):
        result = read_source(files(""), arguments(), 1024)
        self.assertEqual((result["text"], result["end_offset"], result["next_offset"]), ("", 0, None))
        observed = counts(events_for(files(""), [("parent", arguments())]), [])
        self.assertEqual(observed["measurements"]["source_read_bytes"], 0)
        self.assertEqual(observed["source_disclosure"]["pages"], 1)

    def test_overlap_union_across_agents_does_not_double_count_covered_bytes(self):
        state = files("abcdefghijklmnop")
        identity = read_source(state, arguments(), 1024)["sha256"]
        events = events_for(state, [("parent", arguments(size=6)), ("parent", arguments(2, 6, identity)),
                                    ("child", arguments(4, 8, identity)), ("parent", arguments(size=12))])
        events.extend({"agent": agent, "final": "Done"} for agent in ("child", "parent"))
        observed = counts(events, [], completed=True)["source_disclosure"]
        self.assertEqual(observed["source_read_bytes"], 32)
        self.assertEqual(observed["repeated_read_bytes"], 20)
        self.assertEqual(observed["same_agent_repeated_bytes"], 12)
        self.assertTrue(observed["complete"])

    def test_changed_content_and_distinct_paths_have_separate_read_identities(self):
        state = files("first")
        events = events_for(state, [("parent", arguments())])
        result = read_source(files("second"), arguments(), 1024)
        events += [{"agent": "parent", "tool_call": call("read_source", arguments(), "changed")},
                   {"agent": "parent", "tool_result": "changed", "result": result}]
        duplicate = {"other.py": state["main.py"]}
        events += [{"agent": "parent", "tool_call": call("read_source", arguments(path="other.py"), "moved")},
                   {"agent": "parent", "tool_result": "moved", "result": read_source(duplicate, arguments(path="other.py"), 1024)}]
        self.assertEqual(counts(events, [])["source_disclosure"]["repeated_read_bytes"], 0)

    def test_command_keeps_partial_disclosures_but_total_reads_unknown(self):
        events = events_for(files(), [("parent", arguments())])
        events.append({"agent": "parent", "tool_call": call(identity="opaque")})
        observed = counts(events, [])
        self.assertEqual(observed["source_disclosure"]["source_read_bytes"], 6)
        self.assertEqual(observed["source_disclosure"]["opaque_command_calls"], 1)
        for field in ("source_read_bytes", "repeated_read_bytes", "pages"):
            self.assertIsNone(observed["measurements"][field])
        with self.assertRaisesRegex(ValueError, "unfinished"):
            counts(events, [], completed=True)

    def test_interrupted_read_is_unknown_but_refused_read_discloses_zero(self):
        events = events_for({}, [("parent", arguments())])
        refused = counts(events, [])
        self.assertEqual(refused["measurements"]["source_read_bytes"], 0)
        self.assertEqual(refused["source_disclosure"]["failed_read_calls"], 1)
        interrupted = counts(events[:-1], [])
        self.assertIsNone(interrupted["measurements"]["source_read_bytes"])
        self.assertEqual(interrupted["source_disclosure"]["unfinished_read_calls"], 1)

    def test_tampered_page_extent_content_hash_continuation_and_duplicates_refuse(self):
        events = events_for(files(), [("parent", arguments(size=3))])
        for key, value in (("end_offset", 5), ("offset", 1), ("sha256", "bad"),
                           ("text", "different"), ("next_offset", None), ("path", "other.py")):
            changed = copy.deepcopy(events)
            changed[-1]["result"][key] = value
            with self.subTest(key=key), self.assertRaises(ValueError):
                counts(changed, [])
        with self.assertRaisesRegex(ValueError, "orphan or repeated"):
            counts(events + [events[-1]], [])
        with self.assertRaisesRegex(ValueError, "replayed"):
            counts(events + [events[-2]], [])


class SourceLoopTests(unittest.TestCase):
    setUp = runner_fixtures.RunnerTests.setUp
    loop = runner_fixtures.RunnerTests.loop

    def test_both_protocols_deliver_host_source_pages_without_container_commands(self):
        for provider in ("openai", "anthropic"):
            with self.subTest(provider=provider):
                loop = self.loop(provider)
                loop.transport = FakeProvider([[call("read_source", arguments())], "Done"])
                loop.agent(files("exact source"), "Inspect")
                self.assertEqual(loop.backend.calls, [])
                observed = counts(loop.events, tool_definitions(provider), completed=True)
                self.assertEqual(observed["measurements"]["source_read_bytes"], 12)
                self.assertIn("exact source", str(loop.transport.requests[-1]))

    def test_delegated_reads_measure_shared_context_and_handoff(self):
        loop = self.loop(mode="delegated")
        loop.transport = FakeProvider([[call("read_source", arguments())],
                                      [call("delegate", {"question": "Check boundary"}, "delegate")],
                                      [call("read_source", arguments())], "Boundary checked", "Integrated"])
        loop.agent(files("content"), "Parent requirement")
        observed = counts(loop.events, tool_definitions("openai"), completed=True)
        self.assertEqual(observed["source_disclosure"]["repeated_read_bytes"], 7)
        self.assertEqual(observed["source_disclosure"]["same_agent_repeated_bytes"], 0)
        self.assertEqual(observed["measurements"]["handoff_bytes"], len("Check boundaryBoundary checked"))

    def test_invalid_read_in_batch_stops_before_other_tool_runs(self):
        loop = self.loop()
        loop.transport = FakeProvider([[call(), call("read_source", arguments(path="../secret"), "bad")]])
        with self.assertRaisesRegex(ValueError, "unsafe"):
            loop.agent(files(), "Inspect")
        self.assertEqual(loop.backend.calls, [])


if __name__ == "__main__":
    unittest.main()
