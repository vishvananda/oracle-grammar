"""Print-blowup shapes are found, and the baseline only shrinks."""
from pathlib import Path
import json
import tempfile
import unittest

import shapes


class ShapeTests(unittest.TestCase):
    def setUp(self):
        self.directory = tempfile.TemporaryDirectory()
        self.addCleanup(self.directory.cleanup)
        self.root = Path(self.directory.name)
        (self.root/'grammar').mkdir()
        (self.root/'tools').mkdir()

    def grammar(self, text, baseline):
        (self.root/'grammar'/'a.ebnf').write_text(text)
        (self.root/'tools'/'shape_baseline.json').write_text(json.dumps(baseline))

    def test_finds_agreement_and_pronoun_alternations(self):
        self.grammar('pump ::= affected ("gets" | "get") signed\n'
                     'size ::= player ("has" | "have") "no" "cards"\n'
                     'pronoun ::= "it" | "he" | "she"\n'
                     'owner ::= ("it" | "its") "owner"\n'
                     'plain ::= "gets" signed\n', {})
        self.assertEqual(shapes.found(self.root), {
            'pump': ['agreement get|gets'],
            'size': ['agreement has|have'],
            'pronoun': ['pronoun he|it|she'],
        })

    def test_new_shapes_fail_and_fixed_ones_leave_the_baseline(self):
        self.grammar('pump ::= affected ("gets" | "get") signed\n',
                     {'size': ['agreement has|have']})
        errors = shapes.check(self.root)
        self.assertEqual(len(errors), 2)
        self.assertIn('pump: new print-blowup shape agreement get|gets', errors[0])
        self.assertIn('size: shape agreement has|have is gone', errors[1])

    def test_repository_matches_its_baseline(self):
        self.assertEqual(shapes.check(), [])


if __name__ == '__main__':
    unittest.main()
