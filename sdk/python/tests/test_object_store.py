"""Check durable batch writes, failure recovery and verification of saved answers."""
from concurrent.futures import ThreadPoolExecutor
from contextlib import closing
import json
from pathlib import Path
import sqlite3
import subprocess
import sys

import pytest

from fr_ir.context import DirectoryObjectStore, restore_stored_value, store_merkle_value
from fr_ir.ir import IrError, merkle_object_pack
from fr_ir.runtime import FrRuntimeError


def test_a_pack_uses_one_transaction_and_survives_reopening(tmp_path, monkeypatch):
    store = DirectoryObjectStore(tmp_path / "objects")
    statements = []
    connect = sqlite3.connect

    def observed(*args, **kwargs):
        connection = connect(*args, **kwargs)
        connection.set_trace_callback(statements.append)
        return connection

    monkeypatch.setattr(sqlite3, "connect", observed)
    value = {str(index): [index, "shared"] for index in range(100)}
    saved = store_merkle_value(store, value)
    assert saved.objects > 200
    assert sum(sql.startswith("BEGIN IMMEDIATE") for sql in statements) == 1
    assert statements.count("BEGIN") == 1
    assert statements.count("COMMIT") == 2
    assert "PRAGMA synchronous=EXTRA" in statements
    statements.clear()
    assert restore_stored_value(store, saved.digest) == value
    assert statements.count("BEGIN") == 1
    assert statements.count("COMMIT") == 1
    assert restore_stored_value(DirectoryObjectStore(store.root), saved.digest) == value
    assert [path.name for path in store.root.iterdir()] == ["objects.sqlite3"]


def test_a_conflicting_record_rolls_back_the_entire_batch(tmp_path):
    store = DirectoryObjectStore(tmp_path / "objects")
    first, second = "1" * 64, "2" * 64
    original = {"kind": "scalar", "value": "original"}
    store.put_many({first: original})
    with pytest.raises(FrRuntimeError, match="conflicting immutable"):
        store.put_many({second: {"kind": "scalar", "value": "new"},
                        first: {"kind": "scalar", "value": "conflict"}})
    reopened = DirectoryObjectStore(store.root)
    assert reopened.get(first) == original
    assert reopened.get(second) is None


def test_a_killed_writer_leaves_no_partial_answer(tmp_path):
    store = DirectoryObjectStore(tmp_path / "objects")
    saved = store_merkle_value(store, {"existing": "answer"})
    script = r'''
from contextlib import contextmanager
from pathlib import Path
import sys, time
from fr_ir.context import DirectoryObjectStore
class PausedStore(DirectoryObjectStore):
    pause = False
    @contextmanager
    def _connection(self):
        with super()._connection() as connection:
            yield connection
            if self.pause:
                print("before commit", flush=True)
                time.sleep(10)
store = PausedStore(Path(sys.argv[1]))
store.pause = True
store.put_many({"9" * 64: {"kind": "scalar", "value": "unfinished"}})
'''
    child = subprocess.Popen([sys.executable, "-B", "-c", script, str(store.root)],
                             stdout=subprocess.PIPE, stderr=subprocess.PIPE)
    try:
        import selectors
        with selectors.DefaultSelector() as selector:
            selector.register(child.stdout, selectors.EVENT_READ)
            assert selector.select(timeout=5), "writer did not reach the uncommitted transaction"
        assert child.stdout.readline() == b"before commit\n"
        assert store.get("9" * 64) is None
        child.kill()
        child.communicate(timeout=5)
    finally:
        if child.poll() is None:
            child.kill()
        child.communicate(timeout=5)
    reopened = DirectoryObjectStore(store.root)
    assert reopened.get("9" * 64) is None
    assert restore_stored_value(reopened, saved.digest) == {"existing": "answer"}


def test_concurrent_writers_do_not_overwrite_immutable_bytes(tmp_path):
    store = DirectoryObjectStore(tmp_path / "objects")
    key = "3" * 64
    values = [{"kind": "scalar", "value": index} for index in range(4)]

    def write(value):
        try:
            store.put_many({key: value})
            return value
        except FrRuntimeError as error:
            assert "conflicting immutable" in str(error)
            return None

    with ThreadPoolExecutor(max_workers=4) as pool:
        successes = [value for value in pool.map(write, values) if value is not None]
    assert len(successes) == 1
    assert store.get(key) == successes[0]


@pytest.mark.parametrize("delete", [False, True])
def test_changed_or_missing_records_fail_content_verification(tmp_path, delete):
    store = DirectoryObjectStore(tmp_path / "objects")
    value = {"nested": ["answer", "answer"]}
    saved = store_merkle_value(store, value)
    scalar = next(key for key, record in merkle_object_pack(value)["objects"].items()
                  if record.get("kind") == "scalar")
    with closing(sqlite3.connect(store.database)) as connection, connection:
        if delete:
            connection.execute("DELETE FROM objects WHERE digest = ?", (scalar,))
        else:
            connection.execute("UPDATE objects SET record = ? WHERE digest = ?",
                               (json.dumps({"kind": "scalar", "value": "forged"}).encode(), scalar))
    with pytest.raises(IrError):
        restore_stored_value(store, saved.digest)


def test_oversized_or_non_json_records_are_not_admitted(tmp_path):
    store = DirectoryObjectStore(tmp_path / "objects")
    for value in ("x" * 1_048_576, float("nan")):
        with pytest.raises(FrRuntimeError):
            store.put_many({"4" * 64: {"kind": "scalar", "value": value}})
        assert store.get("4" * 64) is None
    with closing(sqlite3.connect(store.database)) as connection, connection:
        connection.execute("INSERT INTO objects VALUES (?, ?)", ("4" * 64, b"x" * 1_048_577))
    with pytest.raises(FrRuntimeError, match="oversized"):
        store.get("4" * 64)


@pytest.mark.parametrize("suffix", ["", "-journal", "-wal", "-shm"])
def test_database_and_journal_symlinks_are_rejected(tmp_path, suffix):
    store = DirectoryObjectStore(tmp_path / "objects")
    outside = tmp_path / "outside"
    outside.write_bytes(b"untouched")
    path = Path(str(store.database) + suffix)
    path.unlink(missing_ok=True)
    path.symlink_to(outside)
    with pytest.raises(FrRuntimeError, match="unsafe"):
        store.get("5" * 64)
    assert outside.read_bytes() == b"untouched"


def test_unrecognized_directory_contents_do_not_select_an_older_reader(tmp_path):
    (tmp_path / "old.json").write_text("{}")
    with pytest.raises(FrRuntimeError, match="empty directory"):
        DirectoryObjectStore(tmp_path)
