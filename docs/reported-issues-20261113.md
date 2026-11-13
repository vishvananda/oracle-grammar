# Reported issue audit — 2026-11-13

Scope: the eleven open issues #4–#14 in this repository. The named audit
population contains the 38 reported faces plus Barl's Cage and Orcish Farmer as
controller-following duration controls; all 40 are in the immutable allowed
population. Source text remains the pinned Oracle input. Reference amendments
use only `adapters/mtgish/updates.json`; the published export and reference lock
remain unchanged. An equal parse against an uncorrected reference is not evidence
that the report is fixed. Local structural IR checks and upstream runtime
witnesses are identified separately.

| Issue | Finding and disposition |
| --- | --- |
| [#4](https://github.com/vishvananda/oracle-grammar/issues/4) | Fixed locally: nine reversed damage events in both pinned IR and seeded grammar. U0015 corrects the written source/recipient direction. |
| [#5](https://github.com/vishvananda/oracle-grammar/issues/5) | Fixed locally: the missing Aura subtype on Gauntlets' attachment population. U0016 conjoins it with the existing two-target attachment disjunction. |
| [#6](https://github.com/vishvananda/oracle-grammar/issues/6) | Fixed locally: Mercenaries' omitted source in reference and grammar. U0017 preserves its same-creature source and activating-player recipient. |
| [#7](https://github.com/vishvananda/oracle-grammar/issues/7) | Pending the independent artifact-exchange correction and validation. |
| [#8](https://github.com/vishvananda/oracle-grammar/issues/8) | Pending named-player untap duration review, with controller-following controls. |
| [#9](https://github.com/vishvananda/oracle-grammar/issues/9) | Pending target identity versus successful-destroy receipt review. |
| [#10](https://github.com/vishvananda/oracle-grammar/issues/10) | Pending actual-reveal existence guard review. |
| [#11](https://github.com/vishvananda/oracle-grammar/issues/11) | Prepared model work is in open PR #3; review the release dependency before amending Arboria. |
| [#12](https://github.com/vishvananda/oracle-grammar/issues/12) | Pending direct destroy-choice legality review; retain Imperial Edict's independent choice as a control. |
| [#13](https://github.com/vishvananda/oracle-grammar/issues/13) | Pending effect completion and actual exile-arrival receipt review. |
| [#14](https://github.com/vishvananda/oracle-grammar/issues/14) | Pending adoption of the damage-time history population from upstream commit 123fb912, preserving mana cost rather than mana value. |

## Prevention direction and filters: #4–#6

U0015 corrects eight shields that say damage dealt **to** a creature to match
that creature as recipient and allow arbitrary damage sources. Barbed Wire says
damage dealt **by** the artifact: it instead keeps that same permanent as source
and allows arbitrary recipients. Amount, limit, duration, event grouping/subject,
activation cost and replacement body remain intact. This corrects actual AST
operands; it is not an equivalence or identity normalization.

U0016 retains both exchanged target identities, the success guard and the
existing attachment disjunction. It adds Subtype(Aura) as a conjunction outside
that disjunction. This excludes attached Equipment while preserving Auras on
either target. U0017 keeps Mercenaries' AbilityController recipient, Any-player
activation permission, NextOccurrence limit, cost and duration. It restricts the
damage source to the named source permanent, so unrelated damage cannot consume
the shield. Existing schema 190 types suffice for all three corrections.

The eleven regression cases isolate each affected activated ability, retaining
its original name context and types. All eleven fail against 8662bc2 because no
reading equals the corrected IR. After the grammar changes all eleven pass with
one corrected native reading, an exact keyed print and a verified default.
They check the grammar's meanings, not merely the amended gold values.

Primary implementation/witness evidence is available in the published MTGish
commits [91a2fe43](https://github.com/vishvananda/mtgish/commit/91a2fe43f26c2cd46e9120f58809c64182664921),
[855deccc](https://github.com/vishvananda/mtgish/commit/855deccca5ba5b32312b3e01835094af5d71b2ed)
and [4ba5c00f](https://github.com/vishvananda/mtgish/commit/4ba5c00f6e5a1b5f32abf8e017a10ccf74bd9fff).
Those commits include Source/parser regressions and Cantrip witnesses for the
reported direction, Aura-versus-Equipment and unrelated-source distinctions.
This repository does not contain that backend; it independently validates the
typed operands and both grammar directions, without claiming to rerun Cantrip.

All eight repository gates pass on the corrected tree, including the original
compositional families. The eleven seeded-meaning cases are data fixtures under
`tests/fixtures/` and run in `src/tests/prevention_reference.rs`; they are not
credited as compositional coverage. The frontend has 765 passing tests and one
ignored test; Python's 70 tests and the existing model-package checks pass.

All 32,755 allowed faces still match the corrected reference, with unchanged
31,681 single-reading faces and 33,920 readings. The strict preservation audit
**intentionally fails** for exactly eleven old incorrect readings, one on each
corrected face, and records exactly one corrected replacement on each. There
are no missing or added meanings on other faces. All 33,920 current readings,
raw/canonical gold, and the eleven explicit old operands validate; canonical gold
is idempotent. This is a reviewed semantic delta, not a blanket preservation
pass. The forty-face issue cohort remains recognized and equal; all forty have
exact keyed prints and 39 have a verified default. The eleven corrected isolated
abilities each have exact and verified default prints. No backend runtime test
or measured optimization claim is made here.
