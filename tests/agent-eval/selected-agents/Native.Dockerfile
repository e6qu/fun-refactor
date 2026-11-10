FROM ubuntu:24.04
RUN sed -i 's|http://archive.ubuntu.com|http://azure.archive.ubuntu.com|; s|http://security.ubuntu.com|http://azure.archive.ubuntu.com|' \
    /etc/apt/sources.list.d/ubuntu.sources && grep -q azure.archive.ubuntu.com /etc/apt/sources.list.d/ubuntu.sources \
    && apt-get update && apt-get install --yes --no-install-recommends python3 python3-venv git ca-certificates ripgrep \
    && rm -rf /var/lib/apt/lists/*
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
