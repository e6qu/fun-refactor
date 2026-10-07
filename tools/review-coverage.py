#!/usr/bin/env python3
"""Replay a frozen collection and show which assigned questions lack submissions."""
import argparse
from pathlib import Path

from agent_eval import native_mcp, review_coverage, source_reviews, terminal_reviews
from agent_eval.study import encode

if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("collection", type=Path)
    args = parser.parse_args()
    frozen = native_mcp.decode(terminal_reviews.read(args.collection / "plan.json"))
    print(encode(review_coverage.report(frozen, source_reviews.read_inputs(args.collection),
                                       args.collection / "attempts")).decode())
