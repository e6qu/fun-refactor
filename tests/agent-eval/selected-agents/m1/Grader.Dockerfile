FROM ghcr.io/e6qu/fun-refactor/m1-grader@sha256:a16b84fc12902cdccce11f482bb6366b10bf08c0784a749ebd6caf1d80116fd4
COPY requirements.txt /opt/requirements.txt
RUN python3 -m venv /opt/python && /opt/python/bin/python3 -m pip install \
    --no-index --no-deps --require-hashes --requirement /opt/requirements.txt
ENV PATH="/opt/python/bin:/opt/rust/bin:/opt/lean/bin:/opt/tool/bin:/usr/bin:/bin" \
    PYTHONPATH=/review/tools PYTHONDONTWRITEBYTECODE=1 HOME=/tmp/home \
    CARGO_HOME=/review/target/cargo-home CARGO_NET_OFFLINE=true CARGO_BUILD_JOBS=1 \
    CARGO_INCREMENTAL=0 CARGO_PROFILE_DEV_DEBUG=0 CARGO_PROFILE_TEST_DEBUG=0 \
    RUST_TEST_THREADS=1 RAYON_NUM_THREADS=1
