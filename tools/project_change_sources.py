"""Admit and retain bounded original archives for the recorded project tasks."""
import hashlib
import json
from pathlib import Path, PurePosixPath
import tarfile
import urllib.request
from evidence_basis import native_source_bindings, file_digest


def bindings(root):
    names = ['tools/project-changes.py', 'tools/project_change_sources.py',
        'tools/project_change_delivery.py', 'tools/project_change_findings.py',
        'tools/project_change_checkers.py', 'tools/evidence_basis.py',
        'tools/project_change_records.py',
        'kernels/FrKernels/Project.lean',
        'tools/agent_eval/bounded_host.py', 'tools/agent_eval/study.py',
        'tests/agent-eval/project-changes/tasks.json', 'tests/agent-eval/project-changes/oracle.py',
        '.github/workflows/refinement-evidence.yml']
    names.extend(str(path.relative_to(root)) for path in (root / 'sdk/python/src').rglob('*.py'))
    return {**native_source_bindings(root), **{name: file_digest(root / name) for name in names}}


def digest(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


def save(path, value):
    path.parent.mkdir(parents=True, exist_ok=True)
    encoded = (json.dumps(value, indent=2, sort_keys=True) + '\n').encode()
    assert len(encoded) <= 2097152, 'project change report exceeds 2 MiB'
    path.write_bytes(encoded)


def download(task, output, limits):
    output.mkdir(parents=True, exist_ok=False)
    archive = output / 'source.tar.gz'
    url = f'https://codeload.github.com/{task["repository"]}/tar.gz/{task["revision"]}'
    with urllib.request.urlopen(url, timeout=30) as response:
        data = response.read(limits['archive_bytes'] + 1)
    assert len(data) <= limits['archive_bytes'], 'archive budget exceeded'
    archive.write_bytes(data)
    unpack(archive, output / 'original', limits)
    files = {str(p.relative_to(output / 'original')): digest(p)
             for p in sorted((output / 'original').rglob('*')) if p.is_file()}
    licenses = [name for name in files if Path(name).name.lower().startswith(('license', 'licence', 'copying'))]
    assert licenses, 'original project has no retained license'
    save(output / 'source.json', {'repository': task['repository'], 'revision': task['revision'],
        'url': url, 'archive_sha256': digest(archive), 'files': files, 'licenses': licenses})
    return output / 'original'


def unpack(archive, root, limits):
    assert archive.stat().st_size <= limits['archive_bytes']
    root.mkdir(parents=True, exist_ok=False)
    with tarfile.open(archive) as stream:
        members = stream.getmembers()
        assert 0 < len(members) <= limits['archive_members']
        assert sum(m.size for m in members) <= limits['expanded_bytes']
        prefix = PurePosixPath(members[0].name).parts[0]
        seen = set()
        for member in members:
            path = PurePosixPath(member.name)
            assert not path.is_absolute() and '..' not in path.parts
            assert path.parts and path.parts[0] == prefix
            assert member.isfile() or member.isdir(), 'links and special files are refused'
            relative = Path(*path.parts[1:])
            if relative == Path('.'):
                assert member.isdir()
                continue
            assert relative not in seen, 'duplicate archive member'
            seen.add(relative)
            destination = root / relative
            if member.isdir():
                destination.mkdir(parents=True, exist_ok=True)
            else:
                destination.parent.mkdir(parents=True, exist_ok=True)
                with stream.extractfile(member) as source:
                    destination.write_bytes(source.read())
                destination.chmod(0o755 if member.mode & 0o111 else 0o644)
