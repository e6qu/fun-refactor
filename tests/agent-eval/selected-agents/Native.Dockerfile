FROM ubuntu:24.04
RUN apt-get update && apt-get install --yes --no-install-recommends python3 git ca-certificates ripgrep \
    && rm -rf /var/lib/apt/lists/*
COPY client /opt/client
COPY tool /opt/tool
COPY skill /opt/skill/fr
COPY agent_eval /probe/agent_eval
COPY selected-agent-probe.py /tools/selected-agent-probe.py
COPY clients.json profiles.json /tests/agent-eval/selected-agents/
ENV PATH="/opt/tool/bin:/opt/client/bin:/usr/local/bin:/usr/bin:/bin" \
    PYTHONPATH=/probe PYTHONDONTWRITEBYTECODE=1 RAYON_NUM_THREADS=1
