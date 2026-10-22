"""Publish a completed measurement after authenticating its collection steps."""
import argparse
import importlib.util
import io
import json
import os
from pathlib import Path
import tempfile
import zipfile

ROOT = Path(__file__).resolve().parents[1]


def load(name, path):
    spec = importlib.util.spec_from_file_location(name, ROOT / path)
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


publisher = load('publish_sessions', 'tools/publish-project-session-cost.py')
cost = publisher.cost
transport = load('saved_cost_transport', 'tools/saved-work-cost.py')


def authenticate(run, jobs):
    cost.require(run['status'] == 'completed' and run['run_attempt'] == 1
                 and run['path'] == '.github/workflows/project-session-cost.yml'
                 and run['conclusion'] in ('success', 'failure'), 'not a completed session collection')
    controls, = [job for job in jobs if job['name'] == 'Reject missing session costs and stale answers']
    measured, = [job for job in jobs if job['name'] == 'measure']
    cost.require(controls['conclusion'] == 'success', 'collection controls failed')
    required = {'Run cargo build --locked --release --bin fr',
                'Inspect the Python client with fr',
                'Measure five approaches with matching complete answers'}
    passed = {step['name'] for step in measured['steps'] if step['conclusion'] == 'success'}
    cost.require(required <= passed, 'measurement or its independent audit did not finish')
    failures = {step['name'] for step in measured['steps'] if step['conclusion'] == 'failure'}
    cost.require(not failures or failures == {'Retain the audited collection and update current tracking'},
                 'failure outside publication')
    cost.require(measured['conclusion'] in ('success', 'failure'), 'measurement job incomplete')


def extract(data, destination):
    with zipfile.ZipFile(io.BytesIO(data)) as archive:
        entries = archive.infolist()
        cost.require(len(entries) <= 4096 and sum(entry.file_size for entry in entries) <= 67108864,
                     'artifact exceeds its expanded budget')
        names = set()
        for entry in entries:
            path = Path(entry.filename)
            cost.require(not path.is_absolute() and '..' not in path.parts and not entry.is_dir()
                         and path.as_posix() == entry.filename and entry.filename not in names
                         and (path.parts[0] == 'project-session-cost' or path.as_posix() == 'client-inspection.json'),
                         'unsafe collection artifact')
            names.add(entry.filename)
            target = destination / path
            target.parent.mkdir(parents=True, exist_ok=True)
            target.write_bytes(archive.read(entry))
    return destination / 'project-session-cost'


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--collection-run', type=int, required=True)
    args = parser.parse_args()
    cost.require(os.environ.get('GITHUB_ACTIONS') == 'true' and os.environ['GITHUB_REF'].startswith('refs/heads/')
                 and os.environ['GITHUB_REF'] != 'refs/heads/main' and args.collection_run > 0,
                 'retain on a work-branch runner with an explicit collection run')
    run = transport.api(f'actions/runs/{args.collection_run}')
    jobs = transport.api(f'actions/runs/{args.collection_run}/jobs?per_page=100')['jobs']
    authenticate(run, jobs)
    artifacts = transport.api(f'actions/runs/{args.collection_run}/artifacts?per_page=100')['artifacts']
    artifact, = [entry for entry in artifacts if entry['name'] == 'complete-project-session-costs']
    data = transport.artifact_bytes(artifact)
    with tempfile.TemporaryDirectory(prefix='fr-retain-sessions-') as temporary:
        collection = extract(data, Path(temporary))
        publisher.publish(collection, run)
    publication = ROOT / f'tests/agent-eval/project-session-cost/publication-{args.collection_run}.json'
    cost.require(not publication.exists(), 'preserve the existing publication receipt')
    cost.save(publication, {'producer_run': run, 'producer_jobs': jobs, 'artifact': artifact,
        'retention_run_id': int(os.environ['GITHUB_RUN_ID']),
        'note': 'Collection and its answer/cost audit passed. Any later publication failure remains recorded in producer_jobs.'})


if __name__ == '__main__':
    main()
