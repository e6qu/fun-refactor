#!/usr/bin/env python3
"""Offline scheduling, CPU precision and failure-accounting checks."""
import copy
import importlib.util
import io
import json
import os
from pathlib import Path
import tempfile
import unittest
from unittest.mock import patch

from agent_eval import client_memory_profile as profile, scripted_stream as stream


def load(name, filename):
    spec = importlib.util.spec_from_file_location(name, Path(__file__).with_name(filename))
    value = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(value)
    return value


delivery = load("delivery", "check-streaming-delivery.py")
original = load("streaming_tests", "test-change-streaming.py")


class DeliveryTests(unittest.TestCase):
    def test_all_schedules_preserve_payload_and_match_paced_deadline(self):
        reply = delivery.base.ORIGINAL_RESPONSE(1, "one-answer")
        reply["choices"][0]["message"]["content"] = '日本語 "quote"\n' * 50
        expected = original.assemble(stream.frames(reply, "whole"))
        durations = {}
        for mode in stream.DELIVERY_MODES:
            payload, deadlines = stream.schedule(reply, mode)
            self.assertEqual(original.assemble(payload), expected)
            self.assertEqual(len(payload), len(deadlines))
            self.assertEqual(deadlines, sorted(deadlines))
            durations[mode] = deadlines[-1]
        self.assertEqual(durations["whole-burst"], 0)
        self.assertEqual(durations["chunked-burst"], 0)
        self.assertGreater(durations["whole-paced"], 0)
        self.assertEqual(durations["whole-paced"], durations["chunked-paced"])

    def test_absolute_deadlines_do_not_accumulate_write_latency(self):
        reply = delivery.base.ORIGINAL_RESPONSE(1, "one-answer")
        reply["choices"][0]["message"]["content"] = delivery.base.TEXT
        now = [0.0]
        class Writer(io.BytesIO):
            def write(self, data):
                now[0] += 0.0005
                return super().write(data)
        class Handler:
            wfile = Writer()
            def send_response(self, status): pass
            def send_header(self, name, value): pass
            def end_headers(self): pass
        def sleep(seconds): now[0] += seconds
        result = stream.write_scheduled(Handler(), reply, "chunked-paced", clock=lambda: now[0], sleep=sleep)
        self.assertAlmostEqual(result["elapsed_seconds"], result["scheduled_seconds"] + 0.0005)
        self.assertEqual(result["max_lateness_seconds"], 0)

    def test_linux_ticks_exclude_children_and_preserve_fractional_cpu(self):
        fields = ["0"] * 22
        fields[0], fields[2], fields[11], fields[12] = "R", "10", "123", "45"
        fields[13], fields[14] = "99999", "99999"  # child CPU must not be double counted.
        stat = "11 (tool ) with spaces) " + " ".join(fields)
        self.assertEqual(profile.linux_cpu(stat, 10, 100), 1.68)
        with self.assertRaisesRegex(ValueError, "group changed"):
            profile.linux_cpu(stat, 12, 100)
        for malformed in ("", "11 () R", stat.replace("123", "-1")):
            with self.assertRaises(ValueError):
                profile.linux_cpu(malformed, 10, 100)
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            (root / "11").mkdir()
            (root / "11/stat").write_text(stat)
            rows, counter = profile.measured_processes("11 10 100 00:01 /tmp/opencode\n12 10 10 00:05 /tmp/exited",
                                                       10, platform="linux", proc=root, ticks=100)
            self.assertEqual(rows, [{"pid": 11, "rss_bytes": 102400, "cpu_seconds": 1.68, "executable": "opencode"}])
            self.assertEqual(counter, {"source": "linux-proc-stat", "quantum_seconds": 0.01})

    def test_no_local_capture_or_admission_and_no_resuming_failed_cell(self):
        with tempfile.TemporaryDirectory() as temporary, patch.dict(os.environ, {}, clear=True):
            root = Path(temporary) / "absent"
            with self.assertRaisesRegex(ValueError, "only on GitHub"):
                delivery.check(root, 0, "whole-burst", Path("fr"), Path("opencode"))
            self.assertFalse(root.exists())
        failed = {"status": "failed", "mode": "whole-burst", "case": delivery.base.control.CASES[0]}
        unstarted = {"status": "not_started", "mode": "whole-burst"}
        value = delivery.summarize(0, "whole-burst", [failed, unstarted])
        self.assertFalse(value["admitted"])
        self.assertFalse(value["resource_qualified"])
        self.assertIsNone(value["mean_cpu_seconds"])
        with self.assertRaisesRegex(ValueError, "resumed"):
            delivery.summarize(0, "whole-burst", [failed, failed])

    def test_matrix_differences_need_complete_equivalent_cells(self):
        def cell(path):
            mode = next(m for m in stream.DELIVERY_MODES if str(path).endswith(m))
            case = 0 if "-0-" in str(path) else 2
            cpu = {"whole-burst": 2, "whole-paced": 3, "chunked-burst": 5, "chunked-paced": 7}[mode]
            row = {"status": "completed", "mode": mode, "case": delivery.base.control.CASES[case],
                   "admitted": True, "process": {"sampled_cpu_seconds": cpu}, "text_delta_events": 20 if mode.startswith("chunked") else 2}
            row.update({key: "same" for key in ("submission_sha256", "reply_identities", "runtime", "binary_sha256", "opencode_sha256", "cpu_counter")})
            row["cpu_counter"] = {"source": "linux-proc-stat" if "ubuntu-latest" in str(path) else "darwin-ps-time",
                                  "quantum_seconds": 0.01}
            return delivery.summarize(case, mode, [row, copy.deepcopy(row)])
        with patch.object(delivery, "report", side_effect=cell):
            result = delivery.matrix(Path("unused"))
        self.assertFalse(result["admitted"])
        self.assertEqual(result["groups"][0]["mean_cpu_differences"],
                         {"fragmentation_burst": 3, "fragmentation_paced": 4, "pacing_whole": 1, "pacing_chunked": 2})
        def different(path):
            result = cell(path)
            if str(path).endswith("chunked-paced"):
                result["attempts"][0]["submission_sha256"] = "different"
            return result
        with patch.object(delivery, "report", side_effect=different), self.assertRaisesRegex(ValueError, "submission_sha256"):
            delivery.matrix(Path("unused"))


if __name__ == "__main__":
    unittest.main()
