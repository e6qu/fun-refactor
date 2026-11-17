"""Link independent oracles using Cargo's reported library locations."""
import json
from pathlib import Path
import subprocess
import sys
import unittest
from unittest.mock import patch

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
from agent_eval import regex_workspace


class CargoArtifacts(unittest.TestCase):
    def test_dependencies_can_live_outside_the_target_directory(self):
        messages = [
            {"reason": "compiler-artifact", "target": {"name": name}, "filenames": [path]}
            for name, path in (("regex", "/output/libregex.rlib"),
                               ("regex_syntax", "/cache/syntax/libregex_syntax-123.rlib"),
                               ("regex_automata", "/cache/automata/libregex_automata-456.rlib"))
        ]
        messages.append({"reason": "build-finished", "success": True})
        result = subprocess.CompletedProcess([], 0, b"\n".join(json.dumps(row).encode() for row in messages), b"")
        with patch.object(regex_workspace.subprocess, "run", return_value=result):
            _, arguments = regex_workspace.build_libraries(Path("/project"), ("regex", "regex-syntax"), {})
        self.assertIn("regex=/output/libregex.rlib", arguments)
        self.assertIn("regex_syntax=/cache/syntax/libregex_syntax-123.rlib", arguments)
        self.assertIn("dependency=/cache/automata", arguments)
        self.assertNotIn("dependency=/project/target/debug/deps", arguments)

    def test_missing_library_does_not_fall_back_to_old_build_outputs(self):
        result = subprocess.CompletedProcess([], 0, b'{"reason":"build-finished","success":true}\n', b"")
        with patch.object(regex_workspace.subprocess, "run", return_value=result):
            with self.assertRaisesRegex(ValueError, "every requested regex library"):
                regex_workspace.build_libraries(Path("/project"), ("regex",), {})


if __name__ == "__main__":
    unittest.main()
