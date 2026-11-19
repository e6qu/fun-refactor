#!/usr/bin/env python3
"""Create or verify the manifest for one complete set of release downloads."""
import argparse
import json
from pathlib import Path
from product_bundle import qualify

if __name__ == '__main__':
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--dist', type=Path, required=True)
    parser.add_argument('--version', required=True)
    parser.add_argument('--source-commit', required=True)
    parser.add_argument('--checker-commit', required=True)
    parser.add_argument('--write', action='store_true', help='Create a new manifest; otherwise verify the existing manifest')
    args = parser.parse_args()
    print(json.dumps(qualify(args.dist, args.version, args.source_commit, args.checker_commit, write=args.write), sort_keys=True))
