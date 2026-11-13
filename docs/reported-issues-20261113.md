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
| [#8](https://github.com/vishvananda/oracle-grammar/issues/8) | Fixed locally: U0018 corrects 17 named-player step/expiration pairs; U0019 normalizes five equivalent live-population forms. Both controller-following controls are preserved. |
| [#9](https://github.com/vishvananda/oracle-grammar/issues/9) | Pending target identity versus successful-destroy receipt review. |
| [#10](https://github.com/vishvananda/oracle-grammar/issues/10) | Pending actual-reveal existence guard review. |
| [#11](https://github.com/vishvananda/oracle-grammar/issues/11) | Confirmed; awaiting model publication. Open PR #3 adds the missing named-player history window in schema 191 / version 0.2.0. Current main and both registry releases remain schema 190 / version 0.1.0; no reference amendment is claimed. |
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

## Named next-untap player: #8

U0018 corrects the 17 reported controller-following durations. The correction
changes both operands: `DuringNextUntapStep(AbilityController)` fixes expiration,
and `UntapStep.players = One(AbilityController)` fixes whose step the rule tests.
Changing only expiration would still let the step predicate follow a different
permanent controller, including when an object untaps during another player's
step. Affected identities, live evaluation, source, other effects and activation
or trigger contexts remain intact. Barl's Cage's explicit controller-following
wording and Orcish Farmer's distinct controller reference remain controls.

The explicit allowed wording cohort also includes Bontu's Last Reckoning,
Hazoret's Undying Fury, Kefnet's Last Word, Oketra's Last Mercy and Rhonas's Last
Stand. Their durations already name AbilityController, and their live battlefield
population is explicitly filtered to lands controlled by that player. U0019
normalizes their equivalent step-player predicate to the same named-player shape.
The owned canonicalizer proves this implication only for that literal stable
player role, a current battlefield Zone, a direct ControlledBy conjunct (bounded
to 64 conjuncts), one Skip rule, LiveObjects and the matching fixed-player duration.
It does not alias controller and player for snapshots, fixed/unfiltered pools,
other players or controller-following windows. Wrapper fields remain exact and
invalid extra fields remain invalid; replacing these ID-free roles neither drops
nor synthesizes a typed identifier. A schema-directed scan of the complete allowed
pre-batch reference checks that this is the full matching population.

Twenty-four original-wording cases cover those 22 faces and the two controls.
The frozen cff1b60 executable fails all 22 corrected/normalized expectations and
passes both controls. Regressions require the amended expected reading, retain
existing reading counts, and check the untap operands on every reading. Deep
Spawn retains its two preexisting cost readings (CostPart::Mill versus a generic
PlayerAction containing Mill); that separate cost-equivalence question is not
silently resolved by this untap change. Current-operand printing, exact trails,
normalization idempotence and the proof's excluded contexts are checked directly.
No schema, model pin or reference-lock change is introduced.

All eight repository gates pass for this batch; 768 frontend tests pass and one
remains ignored. The 24-case regression has 25 supported readings, all retained
with the correct untap operands. Full coverage remains 32,755 equal, 31,681
single-reading faces and 33,920 readings. One redundant plural untap seed is
retired: cumulative retirements reach 160, with 12,047 remaining (12,045 needed
plus the two protected edited-print rules). Compositional coverage is 19,866.

The strict native audit from cff1b60 intentionally exits with failure for exactly
18 old incorrect readings on the 17 corrected faces, with exactly 18 corrected
replacements on those faces. All five normalized faces preserve their native
meanings; no other reading is missing or added. All raw/canonical gold and all
33,920 current readings validate, canonical gold is idempotent, and native failure
output is empty. Source, binary, data and runtime-grammar hashes were rechecked.
This is a reviewed semantic correction with scoped normalization, not an
unconditional strict-preservation pass.

Against the actual immutable cff1b60 executable and its old reference, all 24
faces keep exact keyed and verified default prints; exact defaults improve from
2 to 16. Seventeen default texts change. The new search generates 308 candidates
and verifies 240, versus 312/244 previously; no assertion is made that every
candidate verifies or that these quick helper runs measure a speed improvement.
