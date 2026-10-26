"""Verify an explicitly named hosted fr build before reusing it for proof controls."""
import argparse
import hashlib
import json
import os
from pathlib import Path
import subprocess

from .selected_plan import ROOT, require


def command(argv):
    return subprocess.run(argv, cwd=ROOT, check=True, capture_output=True, text=True, timeout=60).stdout.strip()


def verify(run, binary):
    require(os.environ.get('GITHUB_ACTIONS') == 'true' and run.isdigit(), 'choose an explicit hosted build run')
    require(binary.is_file() and not binary.is_symlink(), 'the downloaded fr executable is missing or linked')
    repository = os.environ['GITHUB_REPOSITORY']
    metadata = json.loads(command(['gh', 'api', f'repos/{repository}/actions/runs/{run}']))
    require(metadata['head_repository']['full_name'] == repository, 'reuse a build from this repository')
    pages = json.loads(command(['gh', 'api', '--paginate', '--slurp', f'repos/{repository}/actions/runs/{run}/jobs?per_page=100']))
    jobs = [job for page in pages for job in page['jobs']]
    builder = 'Check early returns and numeric proof models'
    builders = [row for row in jobs if row['name'] == builder or row['name'].endswith('/ ' + builder)]
    require(len(builders) == 1 and builders[0]['conclusion'] == 'success', 'the chosen fr build and model tests must pass')
    commit = metadata['head_sha']
    require(len(commit) == 40 and all(char in '0123456789abcdef' for char in commit), 'build source commit is malformed')
    command(['git', '-c', 'maintenance.auto=false', '-c', 'gc.auto=0', '-c', 'credential.helper=',
             '-c', 'credential.helper=!gh auth git-credential', 'fetch', '--no-tags', '--depth=1', 'origin', commit])
    names = [line.rstrip('/') for line in (ROOT / 'native-source-inputs.txt').read_text().splitlines()
             if line and not line.startswith('!')]
    current = {name: command(['git', 'rev-parse', 'HEAD:' + name]) for name in names}
    built = {name: command(['git', 'rev-parse', commit + ':' + name]) for name in names}
    require(current == built, 'native fr inputs changed; build and test the changed program before reusing an artifact')
    with binary.open('rb') as stream:
        executable_digest = hashlib.file_digest(stream, 'sha256').hexdigest()
    result = {'run': run, 'source_commit': commit, 'native_inputs': built,
              'executable_sha256': executable_digest, 'passed': True, 'live_model_calls': 0}
    return result


if __name__ == '__main__':
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--run', required=True)
    parser.add_argument('--fr', type=Path, required=True)
    parser.add_argument('--output', type=Path, required=True)
    arguments = parser.parse_args()
    result = verify(arguments.run, arguments.fr.absolute())
    arguments.output.write_text(json.dumps(result, indent=2) + '\n')
