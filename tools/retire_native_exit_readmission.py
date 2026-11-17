"""Verify atomic revalidation of an old native preservation CLI exit-1 batch."""
import hashlib
import json
from itertools import zip_longest
from pathlib import Path
import subprocess

import retire as r
import retire_worker as worker
import retire_native_outcome
import retire_library_readmission as library
from retire_native_readmission import _allowed, _control, _grammar_tree


def verify(campaign, selected, checkpoint, rejections, evidence, read_proof, gates):
    if evidence.get('kind') != 'isolated-native-exit-one-revalidation':
        raise ValueError('native exit-one evidence is required')
    reads = []
    def checked(spec):
        path, data = read_proof(spec)
        reads.append((dict(spec), path.resolve(), hashlib.sha256(data).hexdigest()))
        return path, data
    paths, values = {}, {}
    for name in ('manifest', 'validation', 'repository_log', 'isolation', 'supplement'):
        path, data = checked(evidence.get(name))
        paths[name] = path.resolve()
        values[name] = data if name == 'repository_log' else json.loads(data)
    manifest, validation, isolation, supplement = (values[x] for x in ('manifest','validation','isolation','supplement'))
    batch = manifest.get('id')
    folder = campaign/'.cache/batches'/str(batch)
    if (not isinstance(batch,str) or not batch or Path(batch).name != batch
            or paths['manifest'] != (folder/'manifest.json').resolve()
            or paths['validation'] != (folder/'validation.json').resolve()
            or paths['repository_log'] != (folder/'repository-check.log').resolve()
            or not _allowed(campaign,paths['isolation']) or not _allowed(campaign,paths['supplement'])):
        raise ValueError('native exit-one artifact paths differ')
    included = [item for item in manifest.get('jobs', []) if item.get('status') == 'included']
    ids = [item.get('job_id') for item in included]
    excluded = isolation.get('excluded_jobs')
    if (not included or any(not isinstance(x,str) or not x for x in ids) or len(set(ids)) != len(ids)
            or not set(selected) <= set(ids)
            or any(item.get('attempt_id') != selected[item['job_id']] for item in included if item['job_id'] in selected)
            or not isinstance(excluded,list) or not excluded or any(not isinstance(x,str) for x in excluded)
            or len(set(excluded)) != len(excluded) or not set(excluded) <= set(ids)
            or set(selected) != set(ids)-set(excluded)
            or isolation.get('kind') != 'isolated-native-regressions-exit-one'
            or isolation.get('manifest') != batch or isolation.get('candidate') != manifest.get('commit')
            or isolation.get('parent') != checkpoint or not isinstance(isolation.get('reason'),str)
            or not isolation['reason'].strip() or isolation.get('historical_supplement') != evidence.get('supplement')):
        raise ValueError('native isolation selected jobs or excluded set differs')
    if (manifest.get('base_commit') != checkpoint or validation.get('scope') != 'combined-batch'
            or validation.get('status') != 'failed' or validation.get('repository_check_returncode') != 0
            or validation.get('candidate') != manifest.get('commit') or validation.get('source_checkpoint') != checkpoint
            or validation.get('included_jobs') != ids or 'full_checks' in validation or 'rows' in validation
            or any(v.get('evidence',{}).get('manifest') != batch or v.get('evidence',{}).get('validation') != validation
                   for v in rejections.values())
            or any(status not in ('passed','reused') for status in gates(values['repository_log']).values())):
        raise ValueError('original rejected batch or repository gates differ')
    cp_path = campaign/'checkpoints'/f'{checkpoint}.json'
    cp_bytes = cp_path.read_bytes(); cp = json.loads(cp_bytes); inputs = Path(cp['inputs']).resolve()
    if cp.get('commit') != checkpoint or not inputs.is_relative_to(campaign.resolve()):
        raise ValueError('checkpoint identity differs')
    base_manifest_path=inputs/'manifest.json';base_manifest_bytes=base_manifest_path.read_bytes();base_manifest=json.loads(base_manifest_bytes)
    base_tree={k.removeprefix('grammar/'):v for k,v in base_manifest.get('files',{}).items() if k.startswith('grammar/')}
    run=(folder/'validation-inputs').resolve(); frozen_manifest_path=run/'manifest.json'; frozen_bytes=frozen_manifest_path.read_bytes()
    frozen=json.loads(frozen_bytes);candidate_tree={k.removeprefix('grammar/'):v for k,v in frozen.get('files',{}).items() if k.startswith('grammar/')}
    if (Path(validation.get('inputs','')).resolve()!=run or base_manifest.get('commit')!=checkpoint
            or frozen.get('commit')!=manifest.get('commit') or frozen.get('source')!=base_manifest.get('source')
            or frozen.get('population')!=base_manifest.get('population')
            or not base_tree or candidate_tree!=manifest.get('grammar')
            or _grammar_tree(inputs/'frozen/grammar')!=base_tree or _grammar_tree(run/'frozen/grammar')!=candidate_tree):
        raise ValueError('native frozen input identity differs')
    for key in ('full_replay','original_reports','control_provenance'):
        proof_path, _ = checked(isolation.get(key))
        if not _allowed(campaign,proof_path):raise ValueError('native provenance escapes scratch')
    r.verify(inputs)
    r.verify(run)
    build = r.read(run/'frozen/build.json')
    parent_build = r.read(inputs/'frozen/build.json')
    if (not isinstance(build, dict) or not isinstance(parent_build, dict)
            or build.get('source_sha256') != frozen.get('source')
            or build.get('source_sha256') != parent_build.get('source_sha256')
            or not isinstance(build.get('compiler'), dict) or build['compiler'] != parent_build.get('compiler')
            or not isinstance(build.get('binaries'), dict)
            or any(build['binaries'].get(name) != frozen['files'].get('bin/'+name) for name in r.HELPERS)):
        raise ValueError('candidate frozen helper identity differs')
    baseline = Path(cp['baseline']).resolve()
    if not baseline.is_relative_to(campaign.resolve()) or r.digest(baseline) != validation.get('baseline_sha256'):
        raise ValueError('checkpoint baseline hash differs')
    for name in ('allowed.json','gold.jsonl','oracle.json'):
        if frozen['files'].get(name) != base_manifest['files'].get(name) or not frozen['files'].get(name):
            raise ValueError('candidate frozen population differs from checkpoint: '+name)
    data = worker.payload(run)
    with baseline.open() as original, (run/'frozen/parse.jsonl').open() as parsed:
        for old, saved, entry in zip_longest(original, parsed, data[0].values()):
            if old is None or saved is None or entry is None:
                raise ValueError('candidate baseline population is incomplete')
            row = json.loads(old)
            if row != json.loads(saved) or entry.get('parse') != row or entry.get('flags') != r.flags(row):
                raise ValueError('candidate baseline parse or print overlay differs')
    checks=folder/'full-checks'
    command=[str((run/'frozen/bin/validate-readings').resolve()),str(checks/'gold.jsonl'),str(checks/'rows.jsonl'),
             str(checks/'native.json'),str(checks/'native-failures.jsonl'),'--preserve-from',str(checks/'before.jsonl')]
    error=subprocess.CalledProcessError(1,command)
    if validation.get('reason') != str(error):
        raise ValueError('original native exit-one command differs')
    required=[folder/'manifest.json',folder/'validation.json',run/'manifest.json',
              *[checks/x for x in ('commands.jsonl','native.json','native-failures.jsonl','slice.json','rows.jsonl','before.jsonl','gold.jsonl','oracle.json')]]
    saved=supplement.get('input_sha256')
    if (supplement.get('kind')!='historical-native-exit-one-supplement' or supplement.get('batch')!=batch
            or supplement.get('candidate')!=manifest.get('commit') or supplement.get('checkpoint')!=checkpoint
            or supplement.get('recorded_reason')!=validation['reason'] or not isinstance(saved,dict)
            or set(saved)!={str(p) for p in required} or any(r.digest(p)!=saved[str(p)] for p in required)):
        raise ValueError('historical native supplement identity differs')
    full=retire_native_outcome.preservation_failure(error,run,checks,data)
    if full!=supplement.get('full_checks') or full.get('faces')!=frozen.get('population') or not full.get('failures'):
        raise ValueError('recomputed historical native result differs')
    named={v['face'] for v in full['failures'] if isinstance(v,dict) and isinstance(v.get('face'),str)}
    count=isolation.get('population_count');controls=isolation.get('controls');hashes=set();trees={}
    if type(count) is not int or count<=0 or count>full['faces'] or not named or not isinstance(controls,dict):
        raise ValueError('native isolation population differs')
    expected={'base':[], 'full':list(range(len(ids))), 'remaining':[i for i,j in enumerate(ids) if j not in set(excluded)]}
    for name in ('base','full','remaining'):
        decoded,population_hash=_control(campaign,controls.get(name),checked)
        identity,result,population=decoded['identity'],decoded['result'],decoded['population'];cards=population.get('cards') if isinstance(population,dict) else None
        if (identity.get('indices')!=expected[name] or type(result.get('faces')) is not int or result['faces']!=count
                or not isinstance(result.get('failures'),list) or (name=='full')!=bool(result['failures'])
                or not isinstance(cards,list) or any(not isinstance(c,str) for c in cards) or len(set(cards))!=len(cards)
                or len(cards)>count or not named<=set(cards)):
            raise ValueError('native isolation control differs: '+name)
        trees[name]=identity['tree'];hashes.add(population_hash)
    if len(hashes)!=1 or trees['base']!=base_tree or trees['full']!=candidate_tree:
        raise ValueError('native isolation control grammar or population differs')
    for spec in evidence['reports'].values():checked(spec)
    library._replay(campaign,checkpoint,included,expected['remaining'],trees['remaining'],paths['isolation'].parent,evidence['reports'])
    for spec,path,digest in reads:
        new_path,data=read_proof(spec)
        if new_path.resolve()!=path or hashlib.sha256(data).hexdigest()!=digest:
            raise ValueError('native proof changed during replay')
    if (cp_path.read_bytes()!=cp_bytes or base_manifest_path.read_bytes()!=base_manifest_bytes
            or frozen_manifest_path.read_bytes()!=frozen_bytes or any(r.digest(p)!=saved[str(p)] for p in required)
            or _grammar_tree(inputs/'frozen/grammar')!=base_tree or _grammar_tree(run/'frozen/grammar')!=candidate_tree
            or r.digest(baseline)!=validation['baseline_sha256']):
        raise ValueError('native historical inputs changed during replay')
    for name in ('base','full','remaining'):
        if _grammar_tree(Path(controls[name]['identity']['path']).resolve().parent/'grammar')!=trees[name]:
            raise ValueError('native control grammar changed during replay')
    r.verify(inputs)
    r.verify(run)
    return {'manifest':batch,'excluded_jobs':excluded,'focused_faces':count,'requires_fresh_full_validation':True}
