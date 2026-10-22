# The resulting immutable image ID is retained with each rehearsal.
# Image setup is excluded from this integration control, never from live costs.
FROM python:3.14-slim-bookworm
RUN apt-get update && apt-get install --yes --no-install-recommends git ca-certificates \
    && rm -rf /var/lib/apt/lists/*
COPY tool /opt/tool
COPY comparison_probe.py comparison_plan.py /probe/
ENV PATH="/opt/tool/bin:/usr/local/bin:/usr/bin:/bin" \
    PYTHONDONTWRITEBYTECODE=1 \
    CODEGRAPH_TELEMETRY=0 \
    CODEGRAPH_PARSE_WORKERS=1 \
    CODEGRAPH_QUERY_POOL_SIZE=1 \
    CODEGRAPH_RESOLVE_WORKERS=1 \
    RAYON_NUM_THREADS=1
