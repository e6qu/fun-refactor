"""Scripted native tool exchanges. This module never forwards a request upstream."""
from contextlib import contextmanager
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
import json
import threading

from .selected_plan import require

CAP = 1024**2
COMPLETE = 'M1 scripted tool exchange completed.'


def responses(model, command, first):
    """Emit the Responses stream consumed by the pinned native Codex client."""
    item = ({'type': 'function_call', 'id': 'fc_m1', 'call_id': 'call_m1', 'name': 'exec_command',
             'arguments': json.dumps({'cmd': command, 'max_output_tokens': 2048}), 'status': 'completed'} if first else
            {'type': 'message', 'id': 'msg_m1', 'role': 'assistant', 'status': 'completed', 'phase': 'final_answer',
             'content': [{'type': 'output_text', 'text': COMPLETE, 'annotations': []}]})
    base = {'id': 'resp_m1_' + str(int(first)), 'object': 'response', 'created_at': 1,
            'model': model, 'status': 'in_progress', 'output': []}
    events = [('response.created', {'response': base}),
              ('response.output_item.added', {'output_index': 0, 'item': item})]
    if not first:
        events += [('response.content_part.added', {'item_id': item['id'], 'output_index': 0, 'content_index': 0,
                    'part': {'type': 'output_text', 'text': '', 'annotations': []}}),
                   ('response.output_text.delta', {'item_id': item['id'], 'output_index': 0, 'content_index': 0,
                    'delta': COMPLETE})]
    events += [('response.output_item.done', {'output_index': 0, 'item': item}),
               ('response.completed', {'response': {**base, 'status': 'completed', 'output': [item],
                'usage': {'input_tokens': 1, 'output_tokens': 1, 'total_tokens': 2,
                          'input_tokens_details': {'cached_tokens': 0}, 'output_tokens_details': {'reasoning_tokens': 0}}}})]
    return ''.join('event: ' + kind + '\ndata: ' + json.dumps({'type': kind, 'sequence_number': index, **body}) + '\n\n'
                   for index, (kind, body) in enumerate(events)).encode()


def chat(model, command, first):
    message = {'role': 'assistant', 'content': None if first else COMPLETE}
    if first:
        message['tool_calls'] = [{'index': 0, 'id': 'call_m1', 'type': 'function',
                                 'function': {'name': 'bash', 'arguments': json.dumps({
                                     'command': command, 'description': 'Inspect the fixture', 'timeout': 10000})}}]
    common = {'id': 'chatcmpl_m1', 'object': 'chat.completion.chunk', 'created': 1, 'model': model}
    chunks = [{**common, 'choices': [{'index': 0, 'delta': message, 'finish_reason': None}]},
              {**common, 'choices': [{'index': 0, 'delta': {}, 'finish_reason': 'tool_calls' if first else 'stop'}],
               'usage': {'prompt_tokens': 1, 'completion_tokens': 1, 'total_tokens': 2}}]
    return b''.join(b'data: ' + json.dumps(chunk).encode() + b'\n\n' for chunk in chunks) + b'data: [DONE]\n\n'


def validate(profile, path, value, index, expected):
    require(index in (0, 1), 'scripted exchange admits exactly two requests; no hidden helper or retry')
    codex = profile['client'] == 'codex'
    model = profile['model'] if codex else profile['model']['id']
    require(path == ('/v1/responses' if codex else '/v1/chat/completions'), 'unexpected scripted provider route')
    require(value['model'] == model and value['stream'] is True, 'client changed model or streaming mode')
    if codex:
        require(value['reasoning']['effort'] == profile['reasoning_effort'], 'Codex reasoning differs from its profile')
        require(value['service_tier'] == profile['request_service_tier'], 'Codex Fast did not reach the provider request')
        names = {tool['name'] for tool in value['tools'] if tool['type'] == 'function'}
        require('exec_command' in names, 'native Codex did not expose its ordinary command tool')
        outputs = [item['output'] for item in value['input'] if item.get('type') == 'function_call_output']
    else:
        require(value['reasoning_effort'] == profile['model']['variant'], 'OpenCode2 reasoning differs from its profile')
        names = {tool['function']['name'] for tool in value['tools'] if tool['type'] == 'function'}
        require('bash' in names, 'native OpenCode2 did not expose its ordinary command tool')
        outputs = [item['content'] for item in value['messages'] if item['role'] == 'tool']
    if index:
        require(len(outputs) == 1, 'native client did not return exactly one tool result')
        text = json.dumps(outputs[0], ensure_ascii=False)
        require(all(word in text for word in expected), 'native result omitted the inspected fixture or fr skill')


@contextmanager
def provider(profile, command, expected, output):
    requests, errors = [], []
    gate = threading.Lock()

    class Handler(BaseHTTPRequestHandler):
        def log_message(self, *_):
            pass

        def do_POST(self):
            try:
                self.connection.settimeout(10)
                length = int(self.headers['Content-Length'])
                require(0 < length <= CAP, 'scripted request exceeds its byte limit')
                value = json.loads(self.rfile.read(length))
                with gate:
                    index = len(requests)
                    require(sum(len(json.dumps(row).encode()) for row in requests) + length <= 4 * CAP,
                            'scripted transcript exceeds its byte limit')
                    requests.append({'path': self.path, 'body': value})
                    validate(profile, self.path, value, index, expected)
                    model = profile['model'] if profile['client'] == 'codex' else profile['model']['id']
                    payload = (responses if profile['client'] == 'codex' else chat)(model, command, index == 0)
                self.send_response(200)
                self.send_header('Content-Type', 'text/event-stream')
                self.send_header('Content-Length', str(len(payload)))
                self.end_headers()
                self.wfile.write(payload)
            except (KeyError, ValueError, TypeError) as error:
                errors.append(type(error).__name__ + ': ' + str(error))
                self.send_error(400, 'scripted exchange refused')

    server = ThreadingHTTPServer(('127.0.0.1', 0), Handler)
    server.timeout = 10
    thread = threading.Thread(target=server.serve_forever, daemon=True)
    thread.start()
    try:
        yield 'http://127.0.0.1:' + str(server.server_port), requests, errors
    finally:
        server.shutdown()
        server.server_close()
        thread.join(timeout=2)
        (output / 'provider.json').write_text(json.dumps({'requests': requests, 'errors': errors}, indent=2) + '\n')
