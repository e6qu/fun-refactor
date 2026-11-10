FROM ghcr.io/e6qu/fun-refactor/selected-native@sha256:54a7264cfac0082c59c32a5d5c5ad664e3520dbbe8a2284b661919ae09e83d69
COPY native-requirements.txt /opt/native-requirements.txt
RUN python3 -m venv /opt/venv && /opt/venv/bin/python3 -m pip install --no-index --no-deps --require-hashes \
    --requirement /opt/native-requirements.txt
COPY client /opt/client
COPY tool /opt/tool
COPY skill /opt/skill/fr
COPY agent_eval /probe/agent_eval
COPY selected-agent-probe.py /tools/selected-agent-probe.py
COPY clients.json profiles.json /tests/agent-eval/selected-agents/
ENV PATH="/opt/venv/bin:/opt/tool/bin:/opt/client/bin:/usr/local/bin:/usr/bin:/bin" \
    PYTHONPATH=/probe PYTHONDONTWRITEBYTECODE=1 RAYON_NUM_THREADS=1
