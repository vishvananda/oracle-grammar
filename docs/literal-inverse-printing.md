# Closed literal declarations in the inverse

The published 0.2.0 printing audit found no verified output for Six-Sided Die
or Craig Boone, Novac Guard. Both have supported literal ability declarations.
The printer could not consistently match those declarations after native
normalization had flattened instruction groups and renamed result IDs.

The grammar compiler now derives an additional inverse view from a closed
literal `Ability` declaration. Eligibility requires only literal words, marks,
self-name tokens and newlines in every alternative, and a pattern without
holes, splices or symbolic functions. Inspection is bounded to 8,192 pattern
nodes and depth 64. The filled declaration must validate before binding and
again afterward; its canonical native value supplies the additional inverse
pattern. Invalid declarations are not repaired into printable ones.

Enumeration, text matching and ordered-key replay use the same derived view.
The discrimination index includes it, and printing retains one additional
whole-card native view alongside the existing maximum of 64 lexical views.
The native view has a separate bounded search and verification allowance. It
is tried first, with the lexical search retained as an independent fallback.
Enumeration retains the lexical candidates before additional native candidates
and combines their derivation evidence.
This prevents additional native guesses from crowding out an existing verified
default, as the Smile at Death regression demonstrates.
The forward direction still fills the original `@sem` declaration. Verified
printing still reparses the complete output and requires the requested native
reading, including its current operands. No original card text is retained by
this mechanism.
The requested operands must validate before constructing the native view;
normalization cannot conceal invalid edited input.

The regression suite covers group flattening, result-ID normalization, exact
trail replay and changed amounts that must stop matching the old literal
wording. Six-Sided Die also has a fixture from the explicitly allowed audit
population, checked through parsing, default printing and exact trail replay.

## Allowed-population evidence

Immutable development helpers were measured on the 850 faces that missed exact
wording in the [published 0.2.0 audit](printing-0.2.0-20261113.md), using the
current amended reference and the same explicitly allowed population. The
categories below are disjoint:

| Printing outcome within that cohort | Before | After |
| --- | ---: | ---: |
| No verified output found | 442 | 391 |
| Verified other wording, without exact recovery | 408 | 336 |
| Exact original rules wording recovered | 0 | 123 |

Compared with published 0.2.0, the current tree recovers verified output for
51 previously unsuccessful faces.
Of the 123 exact recoveries, 48 previously had no verified output and 75 already
printed another wording. Three further recoveries print another verified
wording. Six-Sided Die and Craig Boone, Novac Guard now both have exact defaults.
All 64 controls retain verified defaults and exact keyed wording; their 29
exact defaults are unchanged. These are cohort measurements, not a new complete
32,755-face printing audit.

The comparison is cumulative since 0.2.0: Lilysplash Mentor was already exact
and printable after the preceding Gift batch. It is the one face shared by
this gap cohort and that batch's 64-face audit; the numbers above must not all
be attributed to this compiler change alone.

Full recognition remains 32,755 reference-equal faces, with 31,689 single
readings. Native validation preserves all 33,910 pre-batch readings with no
additions, removals or merged readings. Source gold remains valid; canonical
gold remains valid and idempotent. All eight repository gates pass, including
801 frontend tests, 102 owned-model tests and 75 Python tool tests. Three
regression tests were added. Frozen inputs, helpers, per-face results and validation receipts remain
under the ignored audit cache.

This changes inverse matching only. It introduces no AST schema, vocabulary,
reference amendment or face normalization. The owned model remains schema 191;
the reference lock and amendment ledger are unchanged. No performance claim
is made from the development-helper checks.
