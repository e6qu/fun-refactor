FROM ubuntu:24.04
LABEL org.opencontainers.image.source=https://github.com/e6qu/fun-refactor
RUN apt-get update && apt-get install --yes --no-install-recommends \
    python3 python3-venv git gcc libc6-dev ca-certificates \
    && rm -rf /var/lib/apt/lists/*
