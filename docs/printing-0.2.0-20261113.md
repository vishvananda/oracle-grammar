# Grammar 0.2.0 printing coverage

The complete explicitly allowed population of 32,755 faces was measured with
immutable grammar-0.2.0 helpers, schema 191, and the release's amended reference.
The grammar includes its seeded productions. The original rules text has reminder
text and decorative presentation removed; this is not a measurement of the entire
released Oracle catalogue or of full reminder-text reproduction.

| Gate | Faces | Percentage |
| --- | ---: | ---: |
| At least one verified generated text, across default, keyed and enumerated paths | 32,313 | 98.65% |
| A verified default print without a wording key | 32,233 | 98.41% |
| Exact original rules wording with a key or ordered trail when needed | 31,905 | 97.40% |
| Exact original rules wording by default | 22,261 | 67.96% |

A verified output reparses to the same native IR. Exact wording is a separate
comparison. Keys select production choices and name spellings; output is built
from current operands and checked by reparse. The first row is a per-face union,
not merely the CLI's default-print `round_trip` total. Thus 80 faces have a
verified keyed or enumerated output despite the default search failing.
There are 408 faces with another verified wording but no recovered exact original,
and 442 with no verified output found by these bounded searches. These are
successful-search counts, not proofs that other faces are unprintable.

All 32,755 faces remain recognized and reference-equal in this frozen release
run; 31,687 have one supported reading. Rechecking the first enumerated candidate
beam separately keeps 308,506 of 430,521 candidate/IR pairs. Candidate-pair success
is separate from per-face printing success.

The audit uses `score --render --round-trip` against immutable helpers and grammar
at the published release, with the allowed population and SHA-verified Oracle and
reference inputs. Frozen producer main commit 42b3ab5 has the same Rust, model and
grammar inputs as grammar-v0.2.0 at 6c04c72; its subsequent publisher change is
tooling only. The input receipt and per-face results stay under ignored cache.
No held-out authoring inputs or corpus data are committed here. Quick helpers
were used for coverage, so this audit makes no performance claim.
