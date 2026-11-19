"""Archive a finished private authoring checkout before removing its worktree."""
import hashlib
import io
import json
import os
from pathlib import Path
import subprocess
import tarfile
import uuid

import retire as r
from retire_queue import Queue

MAX_EDIT_BYTES = 1024 * 1024
MAX_EDIT_ARCHIVE_BYTES = 64 * 1024 * 1024


def _git(checkout, *args):
    return subprocess.check_output(['git', '-C', str(checkout), *args])


def _names(output):
    return [os.fsdecode(part) for part in output.split(b'\0') if part]


def _file_hash(data):
    return hashlib.sha256(data).hexdigest()


def _add(archive, name, data, mode=0o600):
    item = tarfile.TarInfo(name)
    item.size = len(data)
    item.mode = mode
    archive.addfile(item, io.BytesIO(data))


def _safe_name(value):
    path = Path(value)
    if path.is_absolute() or not path.parts or '..' in path.parts or str(path) != value:
        raise ValueError('invalid checkout-relative path')
    return path


def _terminal(run, taskid, receipt_path):
    campaign = run.parents[2]
    r.require(run.parent.name == 'attempts' and run.parent.parent.name == '.cache',
              'attempt is not under the campaign attempts directory')
    r.require(Path(taskid).name == taskid and taskid not in ('', '.', '..'), 'invalid task id')
    descriptor = run/'tasks'/f'{taskid}.json'
    task = r.read(descriptor)
    support = r.read(run/'support.json')
    r.require(support['tasks'][taskid] == r.digest(descriptor), 'task descriptor changed')
    checkout = Path(task['checkout'])
    expected = campaign/'workspaces'/run.name
    r.require(checkout.is_absolute() and checkout == expected and not checkout.is_symlink()
              and checkout.is_dir() and checkout.resolve().is_relative_to(campaign),
              'checkout is not the private attempt workspace')
    receipt_path = Path(receipt_path).resolve()
    r.require(receipt_path == (campaign/'.cache/receipts'/f'{run.name}.json').resolve(),
              'receipt path is not owned by this attempt')
    receipt = r.read(receipt_path)
    manifest = r.read(run/'inputs/manifest.json')
    commit = manifest['commit']
    r.require(receipt.get('run') == str(run) and receipt.get('task') == taskid
              and receipt.get('base_commit') == commit, 'terminal receipt identity differs')
    attempt = Queue(campaign/'queue.sqlite').get_attempt(run.name)
    r.require(attempt is not None and attempt['job_id'] == taskid and attempt['base_commit'] == commit
              and attempt['result'] is not None
              and attempt['result']['status'] in ('provisional-pass', 'needs-review', 'failed')
              and attempt['result']['evidence'].get('receipt') == str(receipt_path),
              'attempt lacks a matching terminal queue receipt')
    r.require(_git(checkout, 'rev-parse', 'HEAD').decode().strip() == commit, 'checkout base changed')
    r.require(not _git(checkout, 'diff', '--cached', '--name-only', '-z'), 'checkout has staged changes')
    return campaign, checkout, task, receipt, commit


def _snapshot(checkout, task):
    tracked = set(_names(_git(checkout, 'diff', '--no-ext-diff', '--name-only', '-z')))
    untracked = set(_names(_git(checkout, 'ls-files', '--others', '--exclude-standard', '-z')))
    changed = tracked | untracked
    r.require(changed <= set(task['write_files']), 'checkout contains out-of-scope changes')
    files, modes, deleted = {}, {}, []
    for name in sorted(changed):
        relative = _safe_name(name)
        path = checkout/relative
        if path.is_symlink() or not path.resolve().is_relative_to(checkout):
            raise ValueError('changed symlinks are not supported by archival')
        if not path.exists():
            deleted.append(name)
        elif path.is_file():
            files[name] = path.read_bytes()
            modes[name] = path.stat().st_mode & 0o777
        else:
            raise ValueError('changed path is not a regular file')
    hooks = task.get('hooks')
    if hooks is not None:
        if not isinstance(hooks, str):
            raise ValueError('invalid hooks path')
        relative = _safe_name(hooks)
        if len(relative.parts) < 2 or relative.parts[0] != '.cache':
            raise ValueError('hooks path must be inside .cache')
        path = checkout/relative
        cache = checkout/'.cache'
        if (cache.is_symlink() or path.is_symlink()
                or not path.resolve().is_relative_to(cache.resolve())
                or not cache.resolve().is_relative_to(checkout)):
            raise ValueError('hooks path escapes checkout or is a symlink')
        if path.exists():
            if not path.is_file():
                raise ValueError('hooks path is not a regular file')
            files[hooks] = path.read_bytes()
            modes[hooks] = path.stat().st_mode & 0o777
    diagnostics = checkout/'.cache/author-checks'
    if diagnostics.exists():
        r.require(diagnostics.resolve().is_relative_to(checkout), 'diagnostics escape checkout')
        for path in sorted(diagnostics.rglob('*')):
            if path.is_symlink():
                raise ValueError('diagnostic symlinks are not supported by archival')
            relative = path.relative_to(checkout)
            if (path.is_file() and path.suffix in ('.json', '.jsonl', '.log', '.txt')
                    and not {'grammar', 'expected-removals', 'backup'}.intersection(relative.parts)):
                files[relative.as_posix()] = path.read_bytes()
                modes[relative.as_posix()] = path.stat().st_mode & 0o777
    patch = _git(checkout, 'diff', '--no-ext-diff', '--binary', '--full-index')
    return files, modes, deleted, patch


def _verify_archive(path, manifest, files, modes, patch):
    expected = {'manifest.json', 'candidate.patch'} | {'files/'+name for name in files}
    with tarfile.open(path, 'r:gz') as archive:
        members = archive.getmembers()
        r.require(len(members) == len(expected) and {m.name for m in members} == expected
                  and all(m.isfile() for m in members), 'archive member set differs')
        decoded = json.load(archive.extractfile('manifest.json'))
        r.require(decoded == manifest, 'archive manifest differs')
        r.require(_file_hash(archive.extractfile('candidate.patch').read()) == _file_hash(patch),
                  'candidate patch failed archive verification')
        for name, data in files.items():
            stored = archive.extractfile('files/'+name).read()
            r.require(_file_hash(stored) == _file_hash(data)
                      and archive.getmember('files/'+name).mode == modes[name],
                      'candidate archive content or mode differs: '+name)


def archived_edits(run, taskid, checked_sha256):
    """Read hash-bound edit fixtures from a removed checkout without restoring it."""
    run = Path(run)
    r.require(run.resolve() == run and run.parent.name == 'attempts' and run.parents[1].name == '.cache',
              'invalid archived attempt path')
    r.require(len(_safe_name(taskid).parts) == 1, 'invalid archived task identity')
    task_path = run/'tasks'/f'{taskid}.json'
    task = r.read(task_path)
    relative = str(_safe_name(task['edits']))
    r.require(relative in task['write_files'], 'the edit fixture is outside the archived assignment')
    receipt = r.read(run/'archive.json')
    archive_path = run.parents[1]/'archives'/f'{run.name}.tar.gz'
    r.require(archive_path.resolve() == archive_path and receipt['archive'] == str(archive_path)
              and receipt['checkout_removed'] is True, 'archived checkout identity differs')
    with archive_path.open('rb') as stream:
        content = stream.read(MAX_EDIT_ARCHIVE_BYTES+1)
    r.require(len(content) <= MAX_EDIT_ARCHIVE_BYTES and _file_hash(content) == receipt['sha256'],
              'archived checkout hash or size differs')
    members, total = {}, 0
    expected = {'manifest.json', 'files/'+relative}
    with tarfile.open(fileobj=io.BytesIO(content), mode='r|gz') as archive:
        for index, item in enumerate(archive):
            total += item.size
            r.require(index < 8192 and 0 <= total <= 4*MAX_EDIT_ARCHIVE_BYTES, 'checkout archive exceeds read bound')
            if item.name in expected:
                r.require(item.isfile() and 0 <= item.size <= MAX_EDIT_BYTES and item.name not in members,
                          'invalid edit archive member')
                members[item.name] = archive.extractfile(item).read()
    r.require(members.keys() == expected, 'archived edit or manifest is missing')
    manifest = json.loads(members['manifest.json'])
    r.require(manifest['format'] == 1 and manifest['task'] == taskid and manifest['attempt'] == run.name
              and manifest['descriptor_sha256'] == r.digest(task_path), 'archived descriptor identity differs')
    data = members['files/'+relative]
    r.require(manifest['files'][relative]['sha256'] == _file_hash(data) == checked_sha256,
              'archived edits differ from the independent evaluation')
    edits = json.loads(data)
    r.require(isinstance(edits, list), 'archived edit fixtures must be an array')
    return edits


def archive_checkout(run, taskid, terminal_receipt):
    """Verify, archive, and remove a terminal attempt's private worktree.

    The queue must already contain its terminal outcome and matching receipt.
    Evaluation, support, task, packet, and frozen input artifacts remain in place.
    """
    run = Path(run).resolve()
    campaign, checkout, task, receipt, commit = _terminal(run, taskid, terminal_receipt)
    files, modes, deleted, patch = _snapshot(checkout, task)
    folder = campaign/'.cache/archives'
    folder.mkdir(parents=True, exist_ok=True)
    target = folder/(run.name+'.tar.gz')
    r.require(not target.is_symlink(), 'attempt archive is a symlink')
    temporary = folder/(run.name+'.'+uuid.uuid4().hex+'.tmp')
    manifest = {'format': 1, 'task': taskid, 'attempt': run.name, 'base_commit': commit,
                'descriptor_sha256': r.digest(run/'tasks'/f'{taskid}.json'),
                'receipt_sha256': r.digest(terminal_receipt), 'receipt': receipt,
                'patch_sha256': _file_hash(patch), 'deleted': deleted,
                'files': {name: {'sha256': _file_hash(data), 'mode': modes[name]}
                          for name, data in sorted(files.items())}}
    try:
        if target.exists():
            _verify_archive(target, manifest, files, modes, patch)
        else:
            with tarfile.open(temporary, 'w:gz') as archive:
                _add(archive, 'manifest.json', json.dumps(manifest, sort_keys=True).encode())
                _add(archive, 'candidate.patch', patch)
                for name, data in sorted(files.items()):
                    _add(archive, 'files/'+name, data, modes[name])
            _verify_archive(temporary, manifest, files, modes, patch)
        r.require(_git(checkout, 'diff', '--no-ext-diff', '--binary', '--full-index') == patch,
                  'checkout changed during archival')
        for name, data in files.items():
            current = checkout/_safe_name(name)
            r.require(current.is_file() and not current.is_symlink()
                      and current.resolve().is_relative_to(checkout)
                      and r.digest(current) == _file_hash(data)
                      and current.stat().st_mode & 0o777 == modes[name],
                      'checkout file changed during archival: '+name)
        for name in deleted:
            r.require(not (checkout/_safe_name(name)).exists(), 'deleted file reappeared during archival')
        hooks = task.get('hooks')
        if hooks is not None and hooks not in files:
            path = checkout/_safe_name(hooks)
            r.require(not path.exists() and not path.is_symlink(), 'hooks appeared during archival')
        current_names = (set(_names(_git(checkout, 'diff', '--no-ext-diff', '--name-only', '-z')))
                         | set(_names(_git(checkout, 'ls-files', '--others', '--exclude-standard', '-z'))))
        expected_names = (set(files) & set(task['write_files'])) | set(deleted)
        r.require(current_names == expected_names, 'checkout changed during archival')
        if temporary.exists():
            os.replace(temporary, target)
        subprocess.run(['git', '-C', str(checkout), 'worktree', 'remove', '--force', str(checkout)],
                       check=True, cwd=campaign, stdout=subprocess.DEVNULL, stderr=subprocess.PIPE)
        return {'archive': str(target), 'sha256': r.digest(target), 'files': len(files),
                'deleted': deleted, 'checkout_removed': True}
    finally:
        temporary.unlink(missing_ok=True)
