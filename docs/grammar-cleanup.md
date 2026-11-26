# Grammar cleanup and rule incidence

The first exact-duplicate cleanup shares 797 closed leaf declarations in 455
groups across 28 syntax modules. A group must have identical EBNF tokens and
all semantic and preference annotations. Quoted content is preserved. Callers'
nonterminal references and semantic capture names change together; groups with
capture-ordinal conflicts, reserved loader names or multiple names required by
Rust/tool callers remain separate. Existing ordinary comments remain, with a
pointer to the shared declaration where its local definition was removed.

The cleanup reduces loaded rule entries from 33,655 to 32,854, including four
anonymous helpers. Named declarations fall from 32,572 to 31,775. It preserves
the complete score output on the immutable 32,755-face retirement population;
the supplemental family check passes all 44,106 semantic, refusal and print
cases. The diagnostic allocation run also preserves all 26 sampled parses,
reading sets, printed texts and reparses. Structural cleanup does not establish
an overall speedup; timings and allocation measurements use separate builds.

A new incidence census unions production provenance across every accepted
syntactic derivation before semantic deduplication. On that allowed population:

| Measurement | Count |
| --- | ---: |
| Unique face names | 32,749 |
| Rules used by supported readings | 27,894 |
| Rules observed on one unique face name | 17,779 |
| Rules observed on one face occurrence | 17,778 |
| Face names with at least one singleton rule | 7,631 |
| Singleton rules with parameterized semantics | 13,327 |
| Singleton rules in reference-equal readings only | 17,758 |

These are observed dependencies, not proof of universal applicability or original
card authorship. Reusable filters, counts, references, vocabulary and scope
helpers belong with their semantic families even when one observed face uses
them. The singleton population is too large for one card-specific file. Only a
reviewed, manageable subset of specialized clauses should move, each naming
its observed card in a comment. Recount after further deduplication before
using these numbers for additional moves.
