"""Select an audited current session measurement while preserving older records."""
import argparse
import importlib.util
import json
import os
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
spec = importlib.util.spec_from_file_location('sessions', ROOT / 'tools/project-session-cost.py')
cost = importlib.util.module_from_spec(spec)
spec.loader.exec_module(cost)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--collection', type=Path, required=True)
    args = parser.parse_args()
    cost.require(os.environ.get('GITHUB_ACTIONS') == 'true' and os.environ['GITHUB_REF'].startswith('refs/heads/')
                 and os.environ['GITHUB_REF'] != 'refs/heads/main', 'publish from a work-branch runner')
    report = json.loads((args.collection / 'result.json').read_text())
    cost.require(report['run_id'] == int(os.environ['GITHUB_RUN_ID']) and report['source_commit'] == os.environ['GITHUB_SHA'],
                 'the collection belongs to another run')
    directory = ROOT / f'tests/agent-eval/results/2026-10-22-project-sessions-{report["run_id"]}'
    summary = cost.retain_collection(args.collection, directory)
    relative = str(directory.relative_to(ROOT))
    catalog_path = ROOT / 'tests/agent-eval/roadmap.json'
    catalog = json.loads(catalog_path.read_text())
    entry, = [entry for entry in catalog['evidence'] if entry['id'] == 'current-saved-work-cost']
    entry.update(path=relative + '/result.json', sha256=cost.file_digest(directory / 'result.json'),
        audit='python3 -B tools/project-session-cost.py --audit --output ' + relative,
        scope='Complete CLI, fresh Python, running Python and saved-answer sessions on three familiar projects. '
              'All 135 answers agree; includes startup, edits, validation, shutdown and cleanup. No live-agent savings claim.')
    obligation, = [row for row in catalog['obligations'] if row['id'] == 'C.performance']
    obligation['next'] = 'Use the complete CLI and Python measurements to choose improvements; measure live-agent work separately.'
    catalog_path.write_text(json.dumps(catalog, indent=2, sort_keys=True) + '\n')
    lines = ['# What does using Python and saving answers cost?', '',
        'Suppose you ask where `glob_with` is declared and what code a change might affect. '
        'You ask again without editing, then change its name in one source file and ask about the old name. '
        'The last question checks whether the saved answer expires. It does not deliver a complete rename.', '',
        'We repeat that session on glob (Rust file matching), Boltons (Python utilities) and JMESPath (Python JSON queries). '
        'Each of five approaches gets three sessions per project and must return the same complete answers. '
        'All 135 answers agree, including missing names and answers limited by the declared search bounds.', '',
        '## What we compare', '',
        '- **CLI:** a small Python script runs the compiled `fr` command and reads its JSON output. It imports no fr Python library.',
        '- **Python, restarted:** start the installed fr Python library for every question, then exit.',
        '- **Python, kept running:** keep that library in one Python process for all three questions.',
        '- **Saved, restarted:** restart Python for each question, save the answer and its dependencies, and validate them before reuse.',
        '- **Saved, kept running:** do the same saved-answer work in one Python process.', '',
        'The Python library is also called the SDK. A dependency is an input an answer relies on, such as a source file or import search. '
        'Validation checks whether those inputs still match. A kept-running Python process still starts the compiled `fr` command for each native request. '
        'This experiment adds no daemon or new user interface.', '',
        'All approaches start with separate empty fr caches and keep those caches across their three questions. '
        'A cache stores analysis so later requests can avoid repeating work. Only the saved approaches also retain answers and task plans. '
        'Order rotates by project and repetition; operating-system file caches remain uncontrolled.', '',
        '## Complete session time', '',
        'Seconds below include the coordinator, source copy, Python starts, questions, edit, validation, evidence recording, explicit shutdown and cleanup. '
        'Each value is the median: the middle of three measurements. Percentages compare that median with the CLI median in the same row.', '',
        '| Project | CLI seconds | Python restarted | Python kept running | Saved restarted | Saved kept running |',
        '|---|---:|---:|---:|---:|---:|']
    for project, conditions in summary.items():
        baseline = conditions['cli-fresh']['median_seconds']
        values = [f'{baseline:.3f}'] + [f'{conditions[condition]["median_seconds"]:.3f} ({(conditions[condition]["median_seconds"] / baseline - 1) * 100:+.1f}%)'
            for condition in cost.CONDITIONS[1:]]
        lines.append('| ' + project + ' | ' + ' | '.join(values) + ' |')
    lines += ['', 'The CLI baseline includes a Python coordinator. These percentages measure the added library, process and saved-answer work in this procedure; '
        'they do not compare Python with Rust as programming languages. All approaches use the same optimized native executable.', '',
        '## Processor work, memory and stored data', '',
        'CPU seconds sum user and system processor work for the session and its waited-for children, from GNU time, rounded to 0.01 seconds. Elapsed seconds also include waiting. '
        'The separate process monitor uses a coarse CPU clock that can report zero for short processes; those samples remain in raw diagnostics and do not supply this table. '
        'Memory is the largest sampled sum of resident memory, meaning memory present in RAM, for that group of processes. '
        'Sampling can miss short peaks; shared pages can count twice. One MiB is 1,048,576 bytes and one KiB is 1,024 bytes.', '',
        '| Project / approach | Median CPU seconds | Maximum sampled memory MiB | Median final fr cache KiB | Median final saved data KiB |',
        '|---|---:|---:|---:|---:|']
    import statistics
    for project in summary:
        for condition in cost.CONDITIONS:
            rows = [row for row in report['sessions'] if (row['project'], row['condition']) == (project, condition)]
            finals = [json.loads((args.collection / row['path'] / 'session.json').read_text())['stages'][-1] for row in rows]
            lines.append(f'| {project} / {condition} | {statistics.median(row["cpu_seconds"] for row in rows):.2f} | '
                f'{max(row["measurement"]["sampled_aggregate_rss_bytes"] for row in rows) / 1048576:.1f} | '
                f'{statistics.median(stage["native_cache_bytes"] for stage in finals) / 1024:.1f} | '
                f'{statistics.median(stage["saved_bytes"] for stage in finals) / 1024:.1f} |')
    lines += ['', 'Full answers remain local; clients reply with content fingerprints, which identify the answer bytes. '
        'Equal evidence objects share one recorded file. Evidence hashing, database snapshots and recording are instrumentation included in elapsed time. '
        'Evidence archives are separate from the production caches and saved data above. Directory and filesystem block overhead are unmeasured.', '',
        '## Setup and limits', '',
        'The installed SDK has precompiled modules checked against its exact source. These one-time setup costs are outside session totals:', '',
        '| Setup | Seconds |', '|---|---:|']
    lines += [f'| {name.replace("_", " ")} | {value:.3f} |' for name, value in report['setup'].items()]
    lines += ['', 'Native compilation, dependency provisioning and runner startup are workflow setup; inspect their durations in the linked run. '
        'Source archives, their licenses, exact versions, invocation arguments, individual timings and limits are retained in the raw record. '
        'Each session remains bounded by 20 elapsed seconds, 10 sampled CPU seconds, 512 MiB sampled memory and 16 MiB workspace growth. '
        'These trusted-runner process limits are sampled controls, not a security sandbox.', '',
        'All three projects were already familiar. The original first JMESPath evaluation remains unchanged and separately audited. '
        'Three repetitions cannot establish general savings. There is no live agent, model-token, billing or CodeGraph comparison here. '
        'Earlier measurements use different procedures and runners; compare approaches within this run.', '',
        f'[Raw record](../{relative}/result.json) | [Workflow run](https://github.com/e6qu/fun-refactor/actions/runs/{report["run_id"]}) | '
        '[Previous saved-answer comparison](distinct-answer-hashing.md)', '']
    (ROOT / 'docs/project-session-cost.md').write_text('\n'.join(lines))


if __name__ == '__main__':
    main()
