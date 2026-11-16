"""Restore a verified prior private candidate for a fresh diagnostic attempt."""
import hashlib
import json
import os
from pathlib import Path
import subprocess
import tarfile
import uuid

import retire as r

MAX_ARCHIVE_BYTES = 64 * 1024 * 1024
MAX_MEMBERS = 4096
HOOKS = '.cache/author-hooks.json'


def _relative(name):
    path = Path(name)
    r.require(not path.is_absolute() and path.parts and '..' not in path.parts and str(path) == name,
              'unsafe archive path')
    return path


def _git(root, *args, input_bytes=None):
    return subprocess.run(['git', '-C', str(root), *args], input=input_bytes,
                          stdout=subprocess.PIPE, stderr=subprocess.PIPE, check=True).stdout


def _read_archive(path):
    r.require(path.stat().st_size <= MAX_ARCHIVE_BYTES, 'candidate archive exceeds byte limit')
    members, total = {}, 0
    with tarfile.open(path, 'r:gz') as archive:
        for item in archive:
            r.require(item.isfile() and item.name not in members and len(members) < MAX_MEMBERS,
                      'invalid or repeated archive member')
            _relative(item.name)
            total += item.size
            r.require(total <= MAX_ARCHIVE_BYTES, 'candidate archive exceeds expanded byte limit')
            payload = archive.extractfile(item).read(MAX_ARCHIVE_BYTES + 1)
            r.require(len(payload) == item.size, 'truncated archive member')
            members[item.name] = (payload, item.mode)
    r.require('manifest.json' in members and 'candidate.patch' in members, 'archive metadata missing')
    return members


def restore_candidate(campaign, checkout, task_id, commit, allowed_files, reuse, *, rules=None):
    campaign, checkout = Path(campaign).resolve(), Path(checkout).resolve()
    attempt = reuse['attempt']
    try:
        parsed = uuid.UUID(attempt)
    except (ValueError, TypeError, AttributeError) as exc:
        raise ValueError('invalid prior attempt') from exc
    r.require(parsed.version == 4 and attempt in (str(parsed), parsed.hex), 'invalid prior attempt')
    archive_path = Path(reuse['path'])
    expected = campaign/'.cache/archives'/f'{attempt}.tar.gz'
    r.require(archive_path.is_absolute() and archive_path == expected and not archive_path.is_symlink()
              and archive_path.is_file(), 'archive is not the prior attempt archive')
    digest = r.digest(archive_path)
    r.require(digest == reuse['sha256'], 'candidate archive hash differs')
    members = _read_archive(archive_path)
    manifest = json.loads(members['manifest.json'][0])
    r.require(manifest['format'] == 1 and manifest['task'] == task_id
              and manifest['attempt'] == attempt and manifest['base_commit'] == commit,
              'archive does not match task and checkpoint')
    previous = campaign/'.cache/attempts'/attempt/'tasks'/f'{task_id}.json'
    r.require(r.digest(previous) == manifest['descriptor_sha256'], 'prior descriptor hash differs')
    previous_task = r.read(previous)
    if rules is not None:
        r.require(previous_task['rules'] == rules, 'prior assigned rules differ')
    allowed = set(allowed_files)
    r.require(set(previous_task['write_files']) <= allowed, 'prior task scope exceeds current scope')
    patch = members['candidate.patch'][0]
    r.require(hashlib.sha256(patch).hexdigest() == manifest['patch_sha256'], 'candidate patch hash differs')
    files = manifest['files']
    r.require(set(members) == {'manifest.json', 'candidate.patch'} | {'files/'+n for n in files},
              'archive member inventory differs')
    for name, metadata in files.items():
        _relative(name)
        data, mode = members['files/'+name]
        r.require(hashlib.sha256(data).hexdigest() == metadata['sha256'] and mode == metadata['mode'],
                  'archived file content or mode differs')
    deleted = set(manifest['deleted'])
    r.require(deleted <= allowed, 'archived deletions exceed current scope')
    for name in deleted:
        _relative(name)
    if patch:
        _git(checkout, 'apply', '--check', '--binary', '-', input_bytes=patch)
        _git(checkout, 'apply', '--binary', '-', input_bytes=patch)
    tracked = {os.fsdecode(x) for x in _git(checkout, 'ls-files', '-z').split(b'\0') if x}
    for name, metadata in files.items():
        if name not in allowed and name != HOOKS:
            continue  # diagnostic artifacts are evidence, not current source
        path = checkout/_relative(name)
        r.require(not path.is_symlink() and path.resolve().is_relative_to(checkout), 'restore path escapes checkout')
        payload = members['files/'+name][0]
        if name in tracked:
            r.require(path.is_file() and r.digest(path) == metadata['sha256']
                      and path.stat().st_mode & 0o777 == metadata['mode'],
                      'patch does not reproduce tracked file and mode')
        else:
            path.parent.mkdir(parents=True, exist_ok=True)
            r.require(not path.exists(), 'restore would overwrite an untracked file')
            path.write_bytes(payload)
            path.chmod(metadata['mode'])
    for name in deleted:
        r.require(not (checkout/_relative(name)).exists(), 'patch did not delete expected file')
    changed = {os.fsdecode(x) for x in _git(checkout, 'diff', '--name-only', '-z').split(b'\0') if x}
    changed |= {os.fsdecode(x) for x in _git(checkout, 'ls-files', '--others', '--exclude-standard', '-z').split(b'\0') if x}
    r.require(changed <= allowed, 'restored candidate exceeded current write scope')
    return {'archive': str(archive_path), 'sha256': digest, 'prior_attempt': attempt,
            'notice': 'Infrastructure-tainted candidate requires fresh independent diagnostics and adjudication.'}
