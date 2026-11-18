"""Restore a verified prior private candidate for a fresh diagnostic attempt."""
import hashlib
import json
import os
from pathlib import Path
import re
import shutil
import subprocess
import tarfile
import tempfile
import uuid

import mined
import retire as r
import retire_merge as merge

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


def _rebased_files(frozen, checkout, archived, prepared, task, allowed_files):
    allowed = set(allowed_files)
    hooks = r.read(archived/HOOKS) if (archived/HOOKS).exists() else []
    changes = merge.merge_grammar(frozen, checkout, archived, task['rules'], hooks)
    shutil.copytree(checkout/'grammar', prepared/'grammar')
    for name, text in changes.items():
        target = prepared/_relative(name)
        target.parent.mkdir(parents=True, exist_ok=True)
        target.write_text(text)
    removed, retained = mined.remove(prepared, task['rules'])
    r.require(set(removed) == set(task['rules']) and not retained,
              'rebased seeded removals have retained users')
    current = r.tree(checkout/'grammar')
    files = {'grammar/'+name: prepared/'grammar'/name
             for name, digest in r.tree(prepared/'grammar').items()
             if current.get(name) != digest}
    r.require(files.keys() <= allowed, 'rebased grammar exceeds current write scope')
    for name in (task['fixture'], task['edits'], HOOKS):
        path = archived/_relative(name)
        if path.is_file():
            r.require(name in allowed or name == HOOKS, 'rebased artifact exceeds current write scope')
            target = checkout/name
            r.require(not target.exists() or r.digest(target) == r.digest(path),
                      'rebased artifact would overwrite current source')
            files[name] = path
    for name in files:
        target = checkout/name
        r.require(not target.is_symlink() and target.resolve().is_relative_to(checkout),
                  'rebased candidate path escapes checkout')
    return files


def _frozen_origin(campaign, checkout, origin, commit):
    r.require(isinstance(origin, str) and re.fullmatch(r'[0-9a-f]{40}|[0-9a-f]{64}', origin),
              'candidate archive has no immutable origin checkpoint')
    previous = r.read(campaign/'checkpoints'/f'{origin}.json')
    inputs = Path(previous['inputs'])
    manifest = r.read(inputs/'manifest.json')
    frozen = inputs/'frozen'
    expected = {name.removeprefix('grammar/'): digest for name, digest in manifest['files'].items()
                if name.startswith('grammar/')}
    r.require(previous['commit'] == manifest['commit'] == origin and expected
              and r.tree(frozen/'grammar') == expected, 'candidate frozen origin differs')
    _git(checkout, 'merge-base', '--is-ancestor', origin, commit)
    return frozen


def _install_rebased(checkout, files):
    for name, path in files.items():
        target = checkout/name
        target.parent.mkdir(parents=True, exist_ok=True)
        shutil.copy2(path, target)


def rebase_candidate(campaign, checkout, task_id, commit, allowed_files, reuse, *, rules):
    """Prepare a reviewed archived draft from an accepted ancestor on the current checkout."""
    campaign, checkout = Path(campaign).resolve(), Path(checkout).resolve()
    origin = reuse.get('base_commit')
    frozen = _frozen_origin(campaign, checkout, origin, commit)
    with tempfile.TemporaryDirectory(prefix='.archive-rebase-', dir=checkout.parent) as scratch:
        scratch = Path(scratch)
        archived = scratch/'archived'
        _git(checkout, 'worktree', 'add', '--detach', str(archived), origin)
        try:
            try:
                result = restore_candidate(campaign, archived, task_id, origin, allowed_files, reuse, rules=rules)
                task = r.read(campaign/'.cache/attempts'/reuse['attempt']/'tasks'/f'{task_id}.json')
                files = _rebased_files(frozen, checkout, archived, scratch/'prepared', task, allowed_files)
            except (ValueError, FileNotFoundError, subprocess.CalledProcessError, tarfile.TarError) as error:
                raise merge.MergeConflict(str(error)) from error
            _install_rebased(checkout, files)
        finally:
            _git(checkout, 'worktree', 'remove', '--force', str(archived))
    return {**result, 'rebased_from': origin, 'rebased_onto': commit}


def _copy_reviewed_snapshot(archived, proof, report, task, allowed_files):
    from retire_queue import _proof_file

    path = Path(proof['report']['path'])
    work = path.parents[3]
    checkout = work.parents[2]/'workspaces'/work.name
    r.require(task['checkout'] == str(checkout) and checkout.resolve() == checkout,
              'reviewed draft checkout differs')
    shutil.copytree(path.parent/'grammar', archived/'grammar', symlinks=True)
    grammar = archived/'grammar'
    r.require(not any(item.is_symlink() for item in grammar.rglob('*'))
              and r.tree(grammar) == report['grammar'], 'reviewed draft grammar changed while copying')
    for key, original, digest in (('fixture', path.parent/'family.json', 'fixture_sha256'),
                                  ('edits', checkout/_relative(task['edits']), 'edits_sha256')):
        relative = _relative(task[key])
        r.require(str(relative) in allowed_files and original.resolve() == original,
                  'reviewed draft fixture or operand edits exceed the write scope')
        _, data = _proof_file({'path': str(original), 'sha256': report[digest]})
        target = archived/relative
        target.parent.mkdir(parents=True, exist_ok=True)
        target.write_bytes(data)


def restore_reviewed_draft(campaign, checkout, commit, job, allowed_files, proof):
    """Rebase the exact no-hook repair protected by an independent evaluation."""
    from retire_queue import _verify_reviewed_repair

    origin = proof['source_checkpoint']
    scope = set(allowed_files)
    with tempfile.TemporaryDirectory(prefix='.reviewed-rebase-', dir=checkout.parent) as scratch:
        scratch = Path(scratch)
        archived = scratch/'archived'
        try:
            frozen = _frozen_origin(campaign, checkout, origin, commit)
            report, task = _verify_reviewed_repair(campaign, origin, job, proof)
            r.require(report['hooks'] == [] and report['source_grammar'] == report['grammar'],
                      'reviewed draft uses coordinator hooks')
            _copy_reviewed_snapshot(archived, proof, report, task, scope)
            files = _rebased_files(frozen, checkout, archived, scratch/'prepared', task, scope)
        except (ValueError, OSError, KeyError, subprocess.CalledProcessError) as error:
            raise merge.MergeConflict(str(error)) from error
        _install_rebased(checkout, files)
    return {'kind': 'independently-reviewed-draft', 'report_sha256': proof['report']['sha256'],
            'rebased_from': origin, 'rebased_onto': commit}
