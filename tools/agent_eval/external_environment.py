"""Keep workstation compiler flags out of pinned third-party evaluations."""

import os
from pathlib import Path


def environment():
    # Leave warnings visible and preserve each source tree's own lint settings.
    # Host policy such as -D warnings belongs to fr, not historical upstream code.
    flags = {"RUSTFLAGS", "CARGO_ENCODED_RUSTFLAGS", "RUSTDOCFLAGS",
             "CARGO_ENCODED_RUSTDOCFLAGS"}
    env = {key: value for key, value in os.environ.items()
           if key not in flags and not (key.startswith("CARGO_TARGET_") and key.endswith("_RUSTFLAGS"))}
    env.update(CARGO_HOME=str(Path(__file__).resolve().parents[2] / "target/cargo-home"),
               CARGO_NET_OFFLINE="true")
    return env
