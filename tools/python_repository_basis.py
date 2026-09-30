"""Bind Python repository evidence while ignoring only its release version literal."""
import hashlib
from pathlib import Path
import re

from evidence_basis import file_digest as ordinary_digest


def file_digest(path):
    path = Path(path)
    if path.parts[-5:] != ("sdk", "python", "src", "fr_ir", "_version.py"):
        return ordinary_digest(path)
    data = path.read_bytes()
    pattern = rb'(?m)^(VERSION = )"[0-9]+\.[0-9]+\.[0-9]+(?:[-+][0-9A-Za-z.-]+)?"(  # x-release-please-version)$'
    normalized = re.sub(pattern, rb'\1"<release>"\2', data)
    return hashlib.sha256(normalized).hexdigest()
