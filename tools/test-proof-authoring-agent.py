"""Exercise the current proof submission and grading without a model call."""
import argparse
import json
from pathlib import Path
import runpy
import tempfile


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--fr', type=Path, required=True)
    args = parser.parse_args()
    runner = Path(__file__).with_name('proof-authoring-agent.py').resolve()
    author = runpy.run_path(str(runner))
    requests = [
        {'tool': 'guide'}, {'tool': 'task'}, {'tool': 'check', 'tactics': 'rfl\n'},
        {'tool': 'review'}, {'tool': 'execute'},
        {'tool': 'finish', 'answer': author['EXPECTED_ANSWER']},
    ]
    assert tuple(request['tool'] for request in requests) == author['STEPS']
    with tempfile.TemporaryDirectory(prefix='fr-proof-author-control-') as temporary:
        session = Path(temporary) / 'session'
        author['prepare'](session, args.fr.resolve())
        project = session / 'project'
        original = author['source_files'](project)
        for request in requests:
            response = author['step'](session, request)
            if request['tool'] in ('guide', 'task', 'check', 'review'):
                assert author['source_files'](project) == original
            if request['tool'] in ('check', 'execute'):
                assert response['passed'] is True, response
        events = [
            {'type': 'item.completed', 'item': {
                'type': 'command_execution', 'exit_code': 0,
                'command': f'python3 {runner} step {session} --request-stdin',
            }} for _ in requests
        ]
        events.append({'type': 'turn.completed', 'usage': {'input_tokens': 1, 'output_tokens': 1}})
        (session / 'codex-events.jsonl').write_text(
            ''.join(json.dumps(event) + '\n' for event in events))
        run = {'exit_code': 0, 'timed_out': False}
        (session / 'codex-run.json').write_text(json.dumps(run))
        result = author['score'](session)
        assert result['passed'] is True, result
        run['timed_out'] = True
        (session / 'codex-run.json').write_text(json.dumps(run))
        assert author['score'](session)['passed'] is False
    print('Scripted proof delivery and timeout rejection passed; no model called.')


if __name__ == '__main__':
    main()
