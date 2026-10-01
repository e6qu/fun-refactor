#!/usr/bin/env python3
"""Run a Python evaluator without inheriting the host's Rust lint policy."""

import os
import sys

from agent_eval.external_environment import environment

if len(sys.argv) < 2:
    raise SystemExit("usage: external-eval.py SCRIPT [ARGS...]")
os.execve(sys.executable, [sys.executable, *sys.argv[1:]], environment())
