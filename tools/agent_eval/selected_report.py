"""Explain the completed M1 comparison using only the approved public summary."""
import argparse
import json
from pathlib import Path

from .selected_plan import require


AGENTS = {'codex-luna-fast': 'Codex Luna Fast', 'opencode2-glm-flash': 'OpenCode2 GLM Flash',
          'opencode2-kimi-k3': 'OpenCode2 Kimi K3'}
TASKS = {'boltons-rename': 'Python rename', 'unicode-dice': 'Unicode repair', 'access-proof': 'Access-rule proof'}


def verify_checks(summary, checked, count):
    require(checked['passed'] is True and checked[f'all_{count}_checked'] is True and checked['checked_attempts'] == count,
            f'need successful independent checking of all {count} attempts')
    require(checked['core_checker_sha256'] == summary['core_checker_sha256'], 'independent checker identity differs')
    require(set(checked['attempts']) == {row['id'] for row in summary['attempts']}, 'independent attempt inventory differs')
    for row in summary['attempts']:
        verdict = checked['attempts'][row['id']]
        require(verdict['status'] == 'checked' and verdict['native_status'] == row['native_status'] and
                verdict['outcome'] == row['independent_outcome'], 'published outcome differs from fresh checking: ' + row['id'])
        require(row['independent_check'] == {key: verdict[key] for key in
                ('patch_accepted', 'check_stage', 'undo_exact', 'redo_exact')},
                'published patch checks differ from fresh checking: ' + row['id'])


def comparisons(summary, tasks):
    rows = summary['attempts']
    count = len(tasks) * len(AGENTS) * 4
    per_setup = len(tasks) * 2
    require(len(rows) == count and len({row['id'] for row in rows}) == count, f'retain all {count} distinct attempts')
    require(all(row['native_status'] in {'completed', 'failed'} and
                row['independent_outcome'] in {'passed', 'failed'} for row in rows),
            'finish and independently check every attempt before reporting completion')
    groups, matched = [], []
    for profile in AGENTS:
        for setup in ('files', 'fr'):
            selected = [row for row in rows if row['profile'] == profile and row['setup'] == setup]
            require({(row['task'], row['repetition']) for row in selected} ==
                    {(task, repeat) for task in tasks for repeat in (1, 2)} and len(selected) == per_setup,
                    'each profile/setup needs exactly two attempts per task')
            accepted = [row for row in selected if row['independent_outcome'] == 'passed']
            require(all(row['native_status'] == 'completed' for row in accepted), 'a timed-out CLI cannot become a success')
            total = sum(row['total_seconds'] for row in selected)
            groups.append({'profile': profile, 'setup': setup, 'accepted': len(accepted), 'attempted': per_setup,
                'successful_mean_seconds': sum(row['total_seconds'] for row in accepted) / len(accepted) if accepted else None,
                'total_seconds': total, 'seconds_per_success': total / len(accepted) if accepted else None})
        pairs = []
        for task in tasks:
            for repeat in (1, 2):
                pair = {row['setup']: row for row in rows if row['profile'] == profile and
                        row['task'] == task and row['repetition'] == repeat}
                if all(row['independent_outcome'] == 'passed' for row in pair.values()):
                    pairs.append(pair)
        ordinary = sum(pair['files']['total_seconds'] for pair in pairs)
        with_fr = sum(pair['fr']['total_seconds'] for pair in pairs)
        matched.append({'profile': profile, 'pairs': len(pairs),
            'ordinary_mean_seconds': ordinary / len(pairs) if pairs else None,
            'fr_mean_seconds': with_fr / len(pairs) if pairs else None,
            'fr_change_percent': (with_fr - ordinary) / ordinary * 100 if pairs else None})
    return groups, matched


def number(value):
    return 'undefined' if value is None else f'{value:.1f}'


def token_means(rows, profile, setup):
    fields = [('input_tokens',), ('cached_input_tokens',), ('output_tokens',), ('reasoning_output_tokens',)] \
        if profile == 'codex-luna-fast' else [('input',), ('cache', 'read'), ('output',), ('reasoning',)]
    pairs = {(row['task'], row['repetition']) for row in rows if row['profile'] == profile and
             row['setup'] != setup and row['independent_outcome'] == 'passed'}
    selected = [row for row in rows if row['profile'] == profile and row['setup'] == setup and
                row['independent_outcome'] == 'passed' and (row['task'], row['repetition']) in pairs]
    if not selected or not all(row['reported_usage_complete'] for row in selected):
        return [None] * len(fields)
    means = []
    for field in fields:
        total = 0
        for row in selected:
            value = row['reported_tokens']
            for key in field:
                if not isinstance(value, dict) or key not in value:
                    value = None
                    break
                value = value[key]
            if value is None:
                total = None
                break
            require(type(value) is int and value >= 0, 'reported token counters must be nonnegative integers or unknown')
            total += value
        means.append(total / len(selected) if total is not None else None)
    return means


def markdown(summary):
    groups, matched = comparisons(summary, TASKS)
    rows = summary['attempts']
    accepted = sum(row['independent_outcome'] == 'passed' for row in rows)
    lines = ['# M1: the same agents with and without fr', '',
        f'All 36 scheduled local CLI attempts have independently checked outcomes: {accepted} passed and {36 - accepted} failed.',
        'Each agent received the same three requests twice with ordinary tools and twice with ordinary tools plus fr:', '',
        '- Rename Boltons\' `clamp` to `clamp_value`, including its declared callers and documentation. Limiting 70 to 0 through 50 must still return 50.',
        '- Repair a Unicode text comparison. Comparing `éa` with `éab` must return `2/3`, counting adjacent character values rather than UTF-8 bytes.',
        '- Write a Lean proof that the supplied access model returns true exactly for staff or owners. Keep the definitions and claim unchanged.', '',
        'These are familiar development tasks. Two repetitions per case cannot establish a general success rate or general savings.',
        'The baseline is the same app/model using its ordinary tools. Ordinary tools remain available when fr is added.', '',
        '## Correct results and complete attempt time', '',
        'A pass requires the CLI to finish within its frozen limits and its submitted patch to pass independent GitHub checks.',
        'The checker applies the patch to the original project, checks the requested behavior or proof, checks delivery to another checkout,',
        'and verifies exact undo and redo. Known incorrect and incomplete changes must fail the same checks.', '',
        'GitHub independently checks patches. Timing, token and fr-use data are locally collected summaries; full transcripts stay local.', '',
        '| Agent | Tools | Passed / attempted | Mean successful seconds | All attempt seconds | Seconds spent per success |',
        '| --- | --- | ---: | ---: | ---: | ---: |']
    for group in groups:
        lines.append(f'| {AGENTS[group["profile"]]} | {"ordinary" if group["setup"] == "files" else "ordinary + fr"} | '
            f'{group["accepted"]} / 6 | {number(group["successful_mean_seconds"])} | {number(group["total_seconds"])} | '
            f'{number(group["seconds_per_success"])} |')
    lines += ['', 'Seconds spent per success divides time for **all six attempts**, including failures, by the number accepted.',
        'Different success counts can cover different tasks. Use the next table for matching successful requests and repetitions.', '',
        '| Agent | Pairs where both passed | Ordinary mean seconds | With fr mean seconds | Observed time change |',
        '| --- | ---: | ---: | ---: | ---: |']
    for pair in matched:
        change = 'undefined' if pair['fr_change_percent'] is None else f'{pair["fr_change_percent"]:+.1f}%'
        lines.append(f'| {AGENTS[pair["profile"]]} | {pair["pairs"]} | {number(pair["ordinary_mean_seconds"])} | '
                     f'{number(pair["fr_mean_seconds"])} | {change} |')
    lines += ['', 'Positive time change means the fr setup took longer on those matching successes.',
        'Recorded seconds start at controller attempt entry and include instructions, fresh task setup, indexing, CLI startup, model/tool/check work and export.',
        'Controller Python startup and outer guard admission/startup are outside that timer and remain unmeasured.',
        'They do not isolate fr execution time or prove why a model took longer. Shared environment preparation is separate in [summary.json](summary.json).', '',
        'A token is a unit of text counted by a model. The next table uses those same matching successful pairs.',
        'It shows means of each CLI\'s reported categories, rather than account charges. Their conventions differ;',
        'cached or reasoning counts can overlap other columns, so do not add columns or compare totals across clients.',
        'Usage from failed attempts remains in the summary and can be incomplete. No missing usage is replaced with zero.', '',
        '| Agent | Tools | Mean reported input | Mean cached input | Mean reported output | Mean reasoning |',
        '| --- | --- | ---: | ---: | ---: | ---: |']
    for profile in AGENTS:
        for setup in ('files', 'fr'):
            means = token_means(rows, profile, setup)
            lines.append(f'| {AGENTS[profile]} | {"ordinary" if setup == "files" else "ordinary + fr"} | ' +
                         ' | '.join('unknown' if value is None else number(value) for value in means) + ' |')
    lines += ['',
        '## Where the attempts succeeded', '',
        '| Agent | Request | Ordinary passes / 2 | With fr passes / 2 |', '| --- | --- | ---: | ---: |']
    for profile in AGENTS:
        for task in TASKS:
            counts = [sum(row['independent_outcome'] == 'passed' for row in rows if row['profile'] == profile and
                          row['task'] == task and row['setup'] == setup) for setup in ('files', 'fr')]
            lines.append(f'| {AGENTS[profile]} | {TASKS[task]} | {counts[0]} | {counts[1]} |')
    lines += ['', '## Limits, costs and what was actually observed', '',
        'Each attempt kept its original 150-second limit. The outer workstation guard stayed at 180 seconds. No failed slot was retried.',
        'The first GLM/fr rename timed out after short tool steps; model replies used most of its time.',
        'Its interrupted-session recorder incorrectly required a final outcome. That error remains in the original record and is fixed.',
        'A second immutable execution packet continued only the unstarted slots, with the same prompts, settings, task inputs and limits.',
        'Both packet hashes and original record hashes remain in the public summary.', '',
        'Across the nine timeouts, recorded completed tools occupied 0.5 to 26.6 seconds of each 150-second task budget.',
        'Overlapping tool intervals count once. Other elapsed time includes model replies, CLI overhead and any unrecorded unfinished call;',
        'it is not a measurement of fr execution time. Per-attempt timing details remain in the summary.', '',
        'A timed-out attempt can leave a patch. Its patch is checked and retained, but the attempt remains a failure.',
        'An fr command count records use, not a benefit. Reading help alone does not show that fr contributed to a delivered change.',
        'The two Codex Unicode attempts with fr available issued no observed fr command.', '',
        'Actual account charges are unknown. A model catalog listing a zero price does not establish free billing.',
        'The summary retains CLI-reported token categories separately by client, including cached tokens and incomplete usage after interruption.',
        'No shared hard token/request cap was exposed by these CLIs. OpenCode2 also had its fixed 32-step limit.',
        'Fresh conversations and task folders do not control operating-system or provider caches.',
        'The clients record selected models; these records do not independently establish the provider backend\'s model identity.', '',
        'Codex 0.164.0 selected Luna Fast with high reasoning. OpenCode2 2.0.32 selected GLM Flash or non-Pro Kimi K3 with high reasoning.',
        'Local attempts used released fr 0.84.2, Rust 1.98.1 and Lean 4.28.0. Independent GitHub checking used Rust 1.99.1 and Lean 4.28.0.',
        'The proof attempts started with the same generated model and unfinished claim. Proving that model does not establish the original source/model connection.', '',
        'Only submitted patches and summaries are public. Full CLI logs, prompts, session exports, source archives and execution packets stay local.', '',
        '## Every attempt', '',
        '| Agent | Task | Tools | Repeat | Outcome | Local seconds | Fr commands | Patch |',
        '| --- | --- | --- | ---: | --- | ---: | ---: | --- |']
    for row in rows:
        outcome = 'time limit' if row['stop_reason'] == 'wall_seconds' else row['independent_outcome']
        if row['native_status'] == 'failed' and row['independent_check']['patch_accepted']:
            outcome += '; patch passed'
        patch = f'[patch]({row["patch_file"]})' if row['patch_file'] else 'none'
        lines.append(f'| {AGENTS[row["profile"]]} | {TASKS[row["task"]]} | {"ordinary" if row["setup"] == "files" else "ordinary + fr"} | '
            f'{row["repetition"]} | {outcome} | {row["total_seconds"]:.3f} | {row["fr_use"]["commands_observed"]} | {patch} |')
    runs = sorted({row['independent_check_run'] for row in rows})
    lines += ['', 'Independent check runs: ' + ', '.join(f'[GitHub {run.rsplit("/", 1)[-1]}]({run})' for run in runs) + '.',
        'The full per-attempt settings, metrics, fr operations and original record hashes are in [summary.json](summary.json).',
        'See [PLAN.md](../../../../PLAN.md#next-work) for the next complete milestone.', '']
    return '\n'.join(lines)


if __name__ == '__main__':
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--data', type=Path, required=True)
    parser.add_argument('--checks', type=Path, required=True)
    parser.add_argument('--mode', choices=('write', 'check'), required=True)
    args = parser.parse_args()
    summary = json.loads((args.data / 'summary.json').read_text())
    verify_checks(summary, json.loads(args.checks.read_text()), 36)
    content = markdown(summary)
    path = args.data / 'README.md'
    if args.mode == 'write':
        path.write_text(content)
    else:
        require(path.read_text() == content, 'regenerate the M1 report from its checked public summary')
