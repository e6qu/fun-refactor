"""Scripted native tool exchanges. This module never forwards a request upstream."""
from contextlib import contextmanager
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
import json
import threading

from .selected_plan import require

CAP = 1024**2
COMPLETE = 'M1 scripted tool exchange completed.'


def responses(model, command, first):
    """Emit native Responses WebSocket events, including the code-mode tool call."""
    script = 'const result = await tools.exec_command(' + json.dumps({'cmd': command, 'max_output_tokens': 2048}) + '); text(result);'
    item = ({'type': 'custom_tool_call', 'id': 'ctc_m1', 'call_id': 'call_m1', 'name': 'exec', 'namespace': 'functions',
             'input': script, 'status': 'completed'} if first else
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
    return [{'type': kind, 'sequence_number': index, **body} for index, (kind, body) in enumerate(events)]


def chat(model, command, first):
    message = {'role': 'assistant', 'content': None if first else COMPLETE}
    if first:
        message['tool_calls'] = [{'index': 0, 'id': 'call_m1', 'type': 'function',
                                 'function': {'name': 'shell', 'arguments': json.dumps({
                                     'command': command, 'workdir': '/workspace', 'timeout': 10000, 'background': False})}}]
    common = {'id': 'chatcmpl_m1', 'object': 'chat.completion.chunk', 'created': 1, 'model': model}
    chunks = [{**common, 'choices': [{'index': 0, 'delta': message, 'finish_reason': None}]},
              {**common, 'choices': [{'index': 0, 'delta': {}, 'finish_reason': 'tool_calls' if first else 'stop'}],
               'usage': {'prompt_tokens': 1, 'completion_tokens': 1, 'total_tokens': 2}}]
    return b''.join(b'data: ' + json.dumps(chunk).encode() + b'\n\n' for chunk in chunks) + b'data: [DONE]\n\n'


def codex_settings(profile, path, value):
    require(path == '/v1/responses' and value['type'] == 'response.create', 'expected a native Responses WebSocket request')
    require(value['model'] == profile['model'], 'Codex changed model')
    require(value['reasoning']['effort'] == profile['reasoning_effort'], 'Codex reasoning differs from its profile')
    require(value['service_tier'] == profile['request_service_tier'], 'Codex Fast did not reach the provider request')


def warmup(profile, path, value):
    codex_settings(profile, path, value)
    require(value['generate'] is False and value['input'] == [], 'warmup must not generate text or contain a prompt')
    response = {'id': 'resp_m1_warmup', 'object': 'response', 'created_at': 1, 'model': profile['model'],
                'output': [], 'status': 'completed', 'usage': {'input_tokens': 0, 'output_tokens': 0, 'total_tokens': 0}}
    return [{'type': 'response.created', 'sequence_number': 0, 'response': {**response, 'status': 'in_progress'}},
            {'type': 'response.completed', 'sequence_number': 1, 'response': response}]


def validate(profile, path, value, index, expected, initial):
    require(index in (0, 1), 'scripted exchange admits exactly two requests; no hidden helper or retry')
    codex = profile['client'] == 'codex'
    model = profile['model'] if codex else profile['model']['id']
    require(path == ('/v1/responses' if codex else '/v1/chat/completions'), 'unexpected scripted provider route')
    require(value['model'] == model, 'client changed model')
    if codex:
        codex_settings(profile, path, value)
        names = {tool['name'] for item in initial['input'] if item.get('type') == 'additional_tools'
                 for namespace in item['tools'] if namespace['type'] == 'namespace' and namespace['name'] == 'functions'
                 for tool in namespace['tools']}
        require('exec' in names, 'native Codex did not expose its code-mode execution tool')
        require(not names.intersection({'spawn_agent', 'send_input', 'close_agent'}), 'Codex exposed delegation in a single-agent control')
        outputs = [item for item in value['input'] if item.get('type') == 'custom_tool_call_output']
    else:
        require(value['stream'] is True, 'OpenCode2 stopped streaming')
        require(value['reasoning_effort'] == profile['model']['variant'], 'OpenCode2 reasoning differs from its profile')
        names = {tool['function']['name'] for tool in value['tools'] if tool['type'] == 'function'}
        require('shell' in names, 'native OpenCode2 did not expose its ordinary command tool')
        require('subagent' not in names, 'OpenCode2 exposed delegation in a single-agent control')
        outputs = [item for item in value['messages'] if item['role'] == 'tool']
    if index:
        require(len(outputs) == 1, 'native client did not return exactly one tool result')
        identity, payload = ('call_id', 'output') if codex else ('tool_call_id', 'content')
        require(outputs[0].get(identity) == 'call_m1', 'native tool result belongs to a different call')
        text = json.dumps(outputs[0][payload], ensure_ascii=False)
        require(all(word in text for word in expected), 'native result omitted the inspected fixture or fr skill')


@contextmanager
def websocket_provider(profile, command, expected, output):
    from websockets.sync.server import serve
    requests, errors, connections, warmups = [], [], [], []
    gate = threading.Lock()

    def handle(connection):
        connections.append(connection)
        try:
            for raw in connection:
                require(isinstance(raw, str) and len(raw.encode()) <= CAP, 'invalid native WebSocket message')
                value = json.loads(raw)
                complete = False
                with gate:
                    require(sum(len(json.dumps(row).encode()) for row in requests) + len(raw.encode()) <= 4 * CAP,
                            'WebSocket transcript exceeds its byte limit')
                    if value.get('generate') is False:
                        require(not requests and not warmups, 'unexpected repeated or late native warmup')
                        warmups.append({'path': connection.request.path, 'body': value})
                        events = warmup(profile, connection.request.path, value)
                    else:
                        index = len(requests)
                        requests.append({'path': connection.request.path, 'body': value})
                        validate(profile, connection.request.path, value, index, expected, requests[0]['body'])
                        events = responses(profile['model'], command, index == 0)
                        complete = index == 1
                for event in events:
                    connection.send(json.dumps(event))
                if complete:
                    return
        except Exception as error:
            errors.append(type(error).__name__ + ': ' + str(error))
            connection.close(code=1008, reason='scripted exchange refused')

    with serve(handle, '127.0.0.1', 0, compression=None, server_header=None,
               open_timeout=10, close_timeout=2, ping_interval=None, ping_timeout=None, max_size=CAP, max_queue=4) as server:
        worker = threading.Thread(target=server.serve_forever, daemon=True)
        worker.start()
        try:
            yield 'http://127.0.0.1:' + str(server.socket.getsockname()[1]), requests, errors, warmups
        finally:
            for connection in connections:
                connection.close()
            server.shutdown()
            worker.join(timeout=2)
            (output / 'provider.json').write_text(json.dumps({'transport': 'websocket', 'requests': requests,
                'warmups': warmups, 'errors': errors}, indent=2) + '\n')


@contextmanager
def chat_provider(profile, command, expected, output):
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
                    validate(profile, self.path, value, index, expected, requests[0]['body'])
                    payload = chat(profile['model']['id'], command, index == 0)
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
        yield 'http://127.0.0.1:' + str(server.server_port), requests, errors, []
    finally:
        server.shutdown()
        server.server_close()
        thread.join(timeout=2)
        (output / 'provider.json').write_text(json.dumps({'requests': requests, 'errors': errors}, indent=2) + '\n')
