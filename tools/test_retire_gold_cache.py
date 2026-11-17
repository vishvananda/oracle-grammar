import json
from pathlib import Path
import tempfile
import unittest
from unittest.mock import patch

import retire_batch as batch


def fixture(root, name, records):
    run = root/name
    frozen = run/'inputs/frozen'
    frozen.mkdir(parents=True)
    (frozen/'gold.jsonl').write_text(''.join(json.dumps(card)+'\n' for card in records))
    (run/'inputs/manifest.json').write_text(json.dumps({'files': {'gold.jsonl': batch.r.digest(frozen/'gold.jsonl')}}))
    return run


class GoldCacheTests(unittest.TestCase):
    def test_cold_warm_family_bytes_and_deepcopy(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            original = {'faces': [{'name':'Example','characteristics':{'types':['Instant']},'abilities':[{'amount':1}]}]}
            run = fixture(root,'run',[original])
            evaluation = root/'evaluation'
            (evaluation/'edited-0').mkdir(parents=True)
            (evaluation/'family.json').write_text(json.dumps({'family':'test','cases':[
                {'name':'positive','expected':[{'kind':'Original'}]}, {'name':'reject','reject':True}, {'name':'other'}, {'name':'extra'}]}))
            edited = {'faces':[{'name':'Example','characteristics':{'types':['Instant']},'abilities':[{'amount':2}]}]}
            (evaluation/'edited-0/gold.jsonl').write_text(json.dumps(edited)+'\n')
            (evaluation/'edited-0/rows.jsonl').write_text(json.dumps({'name':'Example','text':'two','status':'equal'})+'\n')
            report = {'edited':[{'face':'Example','mutations':[{'path':[0,'amount'],'value':2}],
                                 'flags':[True,False,True,False]}]}
            cache=batch._GoldIndexCache()
            for modes in (False,True):
                expected=batch._family(evaluation,report,run,modes=modes)
                for _ in range(2):
                    actual=batch._family(evaluation,report,run,modes=modes,gold_cache=cache)
                    self.assertEqual(json.dumps(actual,ensure_ascii=False),json.dumps(expected,ensure_ascii=False))
                    actual['cases'][-1]['expected'][0]['amount']=999
                    self.assertEqual(cache.get(run)['Example'][0]['faces'][0]['abilities'][0]['amount'],1)

    def test_cache_lru_two_blobs_and_rebuild(self):
        with tempfile.TemporaryDirectory() as temporary:
            root=Path(temporary)
            runs=[fixture(root,str(i),[{'faces':[{'name':f'N{i}'}]}]) for i in range(3)]
            cache=batch._GoldIndexCache()
            with patch.object(batch.r,'rows',wraps=batch.r.rows) as rows:
                cache.get(runs[0]);cache.get(runs[1]);cache.get(runs[0]);cache.get(runs[2])
                self.assertEqual(rows.call_count,3)
                self.assertEqual(len(cache._indices),2)
                cache.get(runs[1]);self.assertEqual(rows.call_count,4)
                self.assertEqual(len(cache._indices),2)

    def test_missing_bad_manifest_and_changed_gold_fail_even_on_hit(self):
        with tempfile.TemporaryDirectory() as temporary:
            run=fixture(Path(temporary),'run',[{'faces':[{'name':'A'}]}]);cache=batch._GoldIndexCache()
            cache.get(run)
            path=run/'inputs/frozen/gold.jsonl';path.write_text(path.read_text()+'\n')
            with self.assertRaisesRegex(ValueError,'hash differs'):cache.get(run)
            manifest=run/'inputs/manifest.json';manifest.write_text(json.dumps({'files':{'gold.jsonl':'bad'}}))
            with self.assertRaisesRegex(ValueError,'hash is missing or invalid'):cache.get(run)
            manifest.unlink()
            with self.assertRaises(FileNotFoundError):cache.get(run)

    def test_duplicate_face_within_card_and_across_cards(self):
        with tempfile.TemporaryDirectory() as temporary:
            cards=[{'id':1,'faces':[{'name':'A'},{'name':'A'}]},
                   {'id':2,'faces':[{'name':'A'},{'name':'B'}]}, {'id':3,'faces':[{'name':'B'}]}]
            run=fixture(Path(temporary),'run',cards);index=batch._GoldIndexCache().get(run)
            for name in ('A','B','C'):
                expected=[card for card in cards if any(face['name']==name for face in card['faces'])]
                self.assertEqual(index.get(name,[]),expected)
            self.assertEqual([card['id'] for card in index['A']],[1,2])


if __name__=='__main__':unittest.main()
