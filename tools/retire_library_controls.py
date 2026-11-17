"""Bounded checkpoint-specific Rust library regression controls for staging."""
import hashlib
import json
import os
from pathlib import Path
import re
import subprocess

import dev
import retire as r
import retire_cpu


def _file(path, limit):
    if (not path.is_absolute() or '..' in path.parts or any(part.is_symlink() for part in (path, *path.parents))
            or not path.is_file() or path.stat().st_size > limit):
        raise ValueError('library control file is missing, unsafe or too large')
    return path


def verify(control):
    if (r.digest(_file(control['profile'], 1024*1024)) != control['profile_sha256']
            or r.digest(_file(control['binary'], 1024*1024*1024)) != control['binary_sha256']
            or r.digest(control['build_path']) != control['build_sha256']
            or r.digest(control['manifest_path']) != control['manifest_sha256']):
        raise ValueError('library control identity changed')


def setup(checkpoint, source):
    source = Path(source)
    spec = checkpoint.get('library_regression_controls')
    if spec is None:
        return None
    if not isinstance(spec, dict) or set(spec) != {'path', 'sha256'} or any(
            not isinstance(spec[k], str) for k in spec) or not re.fullmatch(r'[0-9a-f]{64}', spec['sha256']):
        raise ValueError('library control profile path and SHA-256 required')
    profile = _file(Path(spec['path']), 1024*1024)
    data = profile.read_bytes()
    if hashlib.sha256(data).hexdigest() != spec['sha256']:
        raise ValueError('library control profile SHA-256 differs')
    document = json.loads(data)
    if not isinstance(document, dict) or set(document) != {'source_sha256', 'compiler', 'binary', 'tests'}:
        raise ValueError('invalid library control profile')
    tests, binary = document['tests'], document['binary']
    if (not isinstance(tests, list) or not 1 <= len(tests) <= 8 or
            any(not isinstance(t, str) or not re.fullmatch(r'[A-Za-z0-9_]+(?:::[A-Za-z0-9_]+)+', t) for t in tests)
            or len(set(tests)) != len(tests)):
        raise ValueError('library control requires 1–8 unique exact test names')
    if (not isinstance(binary, dict) or set(binary) != {'path', 'sha256'}
            or not isinstance(binary['path'], str) or not isinstance(binary['sha256'], str)
            or not re.fullmatch(r'[0-9a-f]{64}', binary['sha256'])):
        raise ValueError('invalid library control binary identity')
    binary_path = _file(Path(binary['path']), 1024*1024*1024)
    if binary_path.resolve().is_relative_to(Path(source).resolve()):
        raise ValueError('library control binary must be outside checkpoint source')
    if not checkpoint.get('cpu_pool') or type(checkpoint.get('cpu_capacity', 32)) is not int or not 2 <= checkpoint.get('cpu_capacity', 32) <= 32:
        raise ValueError('library control requires two CPU lease tokens')
    run = Path(checkpoint['inputs'])
    manifest_path, build_path = run/'manifest.json', run/'frozen/build.json'
    manifest, build = r.read(manifest_path), r.read(build_path)
    if (manifest.get('commit') != checkpoint['commit'] or manifest.get('files', {}).get('build.json') != r.digest(build_path)
            or document['source_sha256'] != build.get('source_sha256')
            or document['source_sha256'] != dev.source(source)
            or not isinstance(document['compiler'], dict) or document['compiler'] != build.get('compiler')):
        raise ValueError('library control source or compiler differs')
    control = {'profile': profile, 'profile_sha256': spec['sha256'], 'binary': binary_path,
               'binary_sha256': binary['sha256'], 'source_sha256': document['source_sha256'],
               'compiler': document['compiler'], 'tests': tests, 'manifest_path': manifest_path,
               'manifest_sha256': r.digest(manifest_path), 'build_path': build_path, 'build_sha256': r.digest(build_path)}
    verify(control)
    return control


def check(checkpoint, control, grammar, directory):
    verify(control)
    directory.mkdir()
    results = []
    with retire_cpu.reserve(Path(checkpoint['cpu_pool']), 2, checkpoint.get('cpu_capacity', 32)) as fds:
        r._LEASES.extend(fds)
        try:
            env = {k: v for k, v in os.environ.items() if not k.startswith('ORACLE_GRAMMAR_')}
            env.update(ORACLE_GRAMMAR_DIR=str(grammar), ORACLE_GRAMMAR_THREADS='2', RUST_TEST_THREADS='2', RAYON_NUM_THREADS='2')
            for index, test in enumerate(control['tests']):
                verify(control)
                log = directory/f'test-{index:03d}.log'
                command = [str(control['binary']), '--exact', test, '--nocapture']
                with log.open('w') as stream:
                    code = subprocess.run(command, env=env, stdout=stream, stderr=stream, check=False,
                                          pass_fds=tuple(r._LEASES)).returncode
                text = log.read_text()
                passed = (code == 0 and len(re.findall(r'^running 1 test\s*$', text, re.M)) == 1
                          and len(re.findall(r'^test '+re.escape(test)+r' \.\.\.', text, re.M)) == 1
                          and re.search(r'^test result: ok\. 1 passed; 0 failed;', text, re.M) is not None)
                results.append({'test': test, 'returncode': code, 'passed': bool(passed),
                                'log': str(log), 'log_sha256': r.digest(log), 'command': command})
                verify(control)
                if not passed:
                    break
        finally:
            for fd in fds:
                r._LEASES.remove(fd)
    report = {'source_sha256': control['source_sha256'], 'profile_sha256': control['profile_sha256'],
              'binary_sha256': control['binary_sha256'], 'grammar': str(grammar), 'threads': 2,
              'tests': results, 'passed': len(results) == len(control['tests']) and all(x['passed'] for x in results)}
    r.write(directory/'result.json', report)
    verify(control)
    if not report['passed']:
        raise ValueError('library regression control failed: '+str(directory/'result.json'))
    return report
