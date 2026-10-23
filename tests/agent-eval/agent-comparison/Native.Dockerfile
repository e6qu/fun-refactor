FROM python:3.14-slim-bookworm
RUN apt-get update && apt-get install --yes --no-install-recommends git ca-certificates ripgrep bubblewrap \
    && rm -rf /var/lib/apt/lists/*
COPY tool /opt/tool
COPY client /opt/client
COPY skill /opt/skill/fr
COPY comparison_clients.py /probe/agent_eval/comparison_clients.py
COPY comparison_native.py /probe/comparison_native.py
ENV PATH="/opt/client/bin:/opt/tool/bin:/usr/local/bin:/usr/bin:/bin" \
    PYTHONDONTWRITEBYTECODE=1 \
    CODEGRAPH_TELEMETRY=0 \
    CODEGRAPH_PARSE_WORKERS=1 \
    CODEGRAPH_QUERY_POOL_SIZE=1 \
    CODEGRAPH_RESOLVE_WORKERS=1 \
    RAYON_NUM_THREADS=1
