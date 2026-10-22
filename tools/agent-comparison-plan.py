#!/usr/bin/env python3
"""Print or verify the proposed 18-attempt comparison. No model access."""
import argparse
import json
from pathlib import Path

from agent_eval.comparison_plan import ROOT, prepare, settings, verify
from agent_eval.study import load

parser = argparse.ArgumentParser(description=__doc__)
action = parser.add_mutually_exclusive_group(required=True)
action.add_argument("--prepare", action="store_true")
action.add_argument("--verify", type=Path)
action.add_argument("--review-settings", type=Path)
args = parser.parse_args()
if args.prepare:
    result = prepare(ROOT)
elif args.verify:
    verify(load(args.verify), ROOT)
    result = {"preparation_matches": True, "live_calls_enabled": False}
else:
    settings(load(args.review_settings))
    result = {"required_settings_present": True, "live_calls_enabled": False}
print(json.dumps(result, indent=2, allow_nan=False))
