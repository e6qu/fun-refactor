#!/usr/bin/env python3
"""Retain or verify the exact Python runtime named by a terminal change plan."""
import argparse
from pathlib import Path
import shutil

from agent_eval import source_reviews, terminal_changes as changes
from agent_eval.study import load, require


def paths(runtime):
    require(runtime and all(Path(n).name == n and n.endswith('.py') for n in runtime), 'invalid runtime inventory')
    return {n: Path(n) if '-' in n else Path('agent_eval') / n for n in runtime}


def verify(frozen, destination):
    runtime = frozen['plan']['runtime']
    expected = paths(runtime)
    actual = {p.relative_to(destination) for p in destination.rglob('*') if p.is_file()}
    require(actual == set(expected.values()), 'runtime snapshot inventory differs')
    require(all(not (destination / p).is_symlink() and source_reviews.identity(destination / p) == runtime[n]
                for n, p in expected.items()), 'runtime snapshot bytes differ')


def retain(frozen, destination):
    require(frozen['plan']['runtime'] == changes.implementation(), 'runtime changed before retention')
    destination.mkdir(parents=True, exist_ok=False)
    for name, relative in paths(frozen['plan']['runtime']).items():
        target = destination / relative
        target.parent.mkdir(parents=True, exist_ok=True)
        shutil.copyfile(Path(__file__).parent / relative, target)
    verify(frozen, destination)


if __name__ == '__main__':
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('command', choices=('retain', 'verify'))
    parser.add_argument('inputs', type=Path)
    parser.add_argument('destination', type=Path)
    args = parser.parse_args()
    (retain if args.command == 'retain' else verify)(load(args.inputs / 'plan.json'), args.destination)
