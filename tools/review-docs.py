#!/usr/bin/env python3
"""Check current documentation links and refresh generated prose on a runner."""
import argparse
import importlib.util
import json
import os
from pathlib import Path
import re
import subprocess
from urllib.parse import unquote, urlsplit

ROOT = Path(__file__).resolve().parents[1]


def guides():
    return (sorted(ROOT.glob('*.md')) + sorted((ROOT / 'docs').glob('*.md'))
            + sorted((ROOT / 'skills/fr').rglob('*.md'))
            + [ROOT / name for name in ('sdk/python/README.md', 'queries/README.md',
                                       'vendor/README.md', 'wasm-shim/README.md')])


def prose(text):
    lines, fence = [], None
    for line in text.splitlines():
        marker = re.match(r'^\s*(`{3,}|~{3,})(.*)$', line)
        if marker:
            run, rest = marker.groups()
            if fence is None:
                fence = run
            elif run[0] == fence[0] and len(run) >= len(fence) and not rest.strip():
                fence = None
            continue
        if fence is None:
            lines.append(line)
    if fence is not None:
        raise ValueError('unclosed code fence')
    return '\n'.join(lines)


def anchors(text):
    found, counts = set(), {}
    for line in prose(text).splitlines():
        heading = re.match(r'^\s{0,3}#{1,6}\s+(.+?)\s*#*\s*$', line)
        if heading:
            label = re.sub(r'\[([^\]]+)\]\([^)]*\)', r'\1', heading[1])
            value = re.sub(r'[^\w\s-]', '', label.lower()).replace(' ', '-')
            number = counts.get(value, 0)
            counts[value] = number + 1
            found.add(value if number == 0 else f'{value}-{number}')
    found.update(re.findall(r'<a\s+[^>]*?(?:id|name)=[\"\']([^\"\']+)', text))
    return found


def links(path):
    text = prose(path.read_text())
    text = re.sub(r'(`+).*?\1', '', text)
    return [match[1] for match in re.finditer(r'\[[^\]\n]*\]\(<?([^\s)>]+)>?(?:\s+[^)]*)?\)', text)] + [
        match[1] for match in re.finditer(r'^\s{0,3}\[[^\]]+\]:\s*<?([^\s>]+)>?', text, re.M)]


def check_links():
    failures, seen = [], {}
    pages = guides()
    for page in pages:
        try:
            targets = links(page)
        except ValueError as error:
            failures.append(f'{page.relative_to(ROOT)}: {error}')
            continue
        for target in targets:
            url = urlsplit(target)
            if url.scheme or url.netloc:
                continue
            child = (page.parent / unquote(url.path)).resolve() if url.path else page
            if not child.is_relative_to(ROOT) or not child.exists():
                failures.append(f'{page.relative_to(ROOT)}: missing {target}')
            elif url.fragment and child.suffix == '.md':
                if child not in seen:
                    seen[child] = anchors(child.read_text())
                if unquote(url.fragment) not in seen[child]:
                    failures.append(f'{page.relative_to(ROOT)}: missing heading {target}')
    if failures:
        raise ValueError('\n'.join(failures))
    print(f'Checked local links and headings in {len(pages)} current guides.')


def load(name, path):
    spec = importlib.util.spec_from_file_location(name, ROOT / path)
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


def refresh():
    if os.environ.get('GITHUB_ACTIONS') != 'true':
        raise ValueError('Refresh documentation on GitHub runners.')
    from agent_eval import native_outcomes, native_read_outcomes
    for stem, module in [('native-change', native_outcomes), ('native-read', native_read_outcomes)]:
        report = json.loads((ROOT / f'tests/agent-eval/opencode/{stem}-outcomes.json').read_text())
        (ROOT / f'docs/{stem}-outcomes.md').write_text(module.markdown(report['cohorts']))
    subprocess.run(['python3', '-B', 'tools/roadmap-status.py', '--write'], cwd=ROOT, check=True, timeout=60)
    check = load('doc_prose', 'tools/check-prose.py')
    counts, budget = check.gather(), check.read_budget()
    rising = {key: value for key, value in counts.items() if value > budget[key]}
    if rising:
        raise ValueError(f'Improve the prose; do not raise its budgets: {rising}')
    text = check.BUDGET.read_text()
    for key, value in counts.items():
        text = re.sub(rf'^{re.escape(key)} \d+$', f'{key} {value}', text, flags=re.M)
    check.BUDGET.write_text(text)
    print('Updated generated prose and lowered only improved writing counts.')


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--write', action='store_true')
    args = parser.parse_args()
    if args.write:
        refresh()
    check_links()


if __name__ == '__main__':
    try:
        main()
    except (OSError, ValueError) as error:
        raise SystemExit(str(error))
