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
| [#7](https://github.com/vishvananda/oracle-grammar/issues/7) | Fixed locally: U0020 adds the independent artifact exchange; shared grammar replaces three incorrect fragments. |
| [#8](https://github.com/vishvananda/oracle-grammar/issues/8) | Fixed locally: U0018 corrects 17 named-player step/expiration pairs; U0019 normalizes five equivalent live-population forms. Both controller-following controls are preserved. |
| [#9](https://github.com/vishvananda/oracle-grammar/issues/9) | Fixed locally: U0026 retains the original target in post-destroy characteristic tests, removing the competing successful-destroy receipt reading. |
| [#10](https://github.com/vishvananda/oracle-grammar/issues/10) | Fixed locally: U0021 guards both name branches by an actual observation and moves the observed card. |
| [#11](https://github.com/vishvananda/oracle-grammar/issues/11) | Confirmed; awaiting model publication. Open PR #3 adds the missing named-player history window in schema 191 / version 0.2.0. Current main and both registry releases remain schema 190 / version 0.1.0; no reference amendment is claimed. |
| [#12](https://github.com/vishvananda/oracle-grammar/issues/12) | Fixed locally: U0024 filters indestructibility after computing the complete least-power pool; Imperial Edict's independent choice is unchanged. |
| [#13](https://github.com/vishvananda/oracle-grammar/issues/13) | Fixed locally: U0025 performs a partial exile effect and tests actual recorded exile arrivals, preserving completion, creature counts and entry redirection. |
| [#14](https://github.com/vishvananda/oracle-grammar/issues/14) | Fixed locally: U0022 adopts historical damage participants; U0023 preserves both positive copy-type tests and relative payment printing. |

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

## Independent exchanges, observations and history: #7, #10, #14

U0020 appends Juxtapose's independent artifact choices and exchange after the
complete creature exchange. Each player chooses within their own greatest-mana-
value population, with AsManyAsCan(1) handling absent objects. The artifact
sequence remains unconditional even when the creature exchange cannot happen.
A shared grammar production handles two explicit permanent kinds, their tie
choices and both complete exchanges. It uses relative result addresses, so an
instruction prefix cannot redirect the exchange receipts. The three old mined
fragments incorrectly distributed a single creature exchange across all three
sentences; they are removed rather than retained as extra incorrect readings.

U0021 preserves Vexing Arcanix's naming/reveal instructions outside a new
actual-reveal existence guard. Both name-comparison outcomes sit inside that
guard; an empty library cannot enter the wrong-name damage branch. Both moves
read the Observed reveal receipt, preserving the named card rather than querying
the library's top again. A marked receipt anchors the condition and both moves
across branch-local result definitions. Costs, recipient, source, name domain and
damage amount remain intact.

U0022 replaces Maelstrom Archangel Avatar's fresh battlefield query with a
Union of FromHistory damage participants. Creature and controller restrictions
sit inside the historical event; recipients are all players, damage is Combat,
grouping/subject are Source, and the window remains ThisTurn. The union deduplicates
repeated events for the same object identity. Departed sources and subsequent
control changes cannot change that population. The loop still matches complete
ManaCosts, with the original random choice, rather than converting costs to
mana values.

Review of the same body found another incorrect lowering. U0023 preserves the
two printed positive chosen-copy type tests as independent If instructions,
each with an empty otherwise. Failure of the permanent test, including no chosen
copy, no longer offers the spell-casting branch. The latter requires the actual
Instant-or-Sorcery test. The repeated loop noun must agree with its introduction;
formerly unrepresented article/count words are limited to the source's one-copy
wording. The permanent-type test is independent of the loop's kind. Printing
also reads the actual preceding PayCost receipt instead of requiring literal
result ID 1: the old literal failed after ordinary canonical ID renumbering.
These corrections use existing schema-190 types; AST operands/program structure
change, while the model pin and reference lock remain unchanged.

Primary implementation/witness evidence for the reported defects is in the
published MTGish commits [4ba5c00f](https://github.com/vishvananda/mtgish/commit/4ba5c00f6e5a1b5f32abf8e017a10ccf74bd9fff),
[06de9c78](https://github.com/vishvananda/mtgish/commit/06de9c786d700414985cb9cf960402169039b830)
and [123fb912](https://github.com/vishvananda/mtgish/commit/123fb91240db27e8396a24bd10274ce9d1618183).
Local regressions independently check native operands, exact printing, edited
kinds/costs/amounts, relative receipts after a prefix, and refusal of unsupported
copy counts or disagreeing repeated nouns. Runtime backend witnesses are not
rerun or represented as local executions.

All eight repository gates pass; 772 frontend tests pass and one remains ignored.
All three original-wording fixtures have their sole corrected native reading,
exact keyed printing and a verified default, including canonical result IDs.
The four regressions also pass for edited operands and a prefixed exchange.
Juxtapose is now compositional. Three incorrect seeds are retired, bringing the
cumulative total to 163; 12,044 remain (12,042 needed and two protected unused).
Compositional coverage is 19,867, with the same four attribution limitations and
no at-risk face.

Full coverage remains 32,755 equal, 31,681 single-reading faces and 33,920
readings. The strict native audit intentionally rejects exactly the three old
incorrect meanings, replacing one reading on each reported face, with no other
missing or added meanings. Raw/canonical gold and all current readings validate;
canonical gold is idempotent and native failure output is empty. Audit hashes
were rechecked. Against the actual frozen 0fe30bd executable, the three-face
cohort retains all three exact keyed prints and improves exact defaults and
verified defaults from two to three. No backend runtime or speed measurement is
claimed.


## Original targets, direct choices and completion: #9, #12, #13

U0026 corrects Thermokarst and Extinguish the Light's characteristic-test
operand to the actual preceding Target. A performed Destroy Before receipt can
be empty when destruction fails; it does not represent attempted input. The
shared grammar repeats the captured target and removes that wrong alternative
on six allowed faces. Icequake, Raze to the Ground, Seedship Impact and Tainted
Treats already had the correct target in their references, so they need no
reference amendment. All six retain their already-supported correct reading.
This is an identity correction, not a Result/Target equivalence alias.

U0024 changes Drop of Honey and Porphyry Nodes' direct destruction eligibility.
The original full creature pool and Aggregate Minimum remain inside the choice
population. Matching(Not(HasKeyword(Indestructible))) wraps that complete pool;
it cannot make a higher-power creature eligible when only an indestructible
creature has the minimum. Chooser, AsManyAsCan(1), receipts and no-regeneration
destruction are retained. Imperial Edict's separately instructed choice remains
unrestricted by that direct-destruction filter.

U0025 replaces Frankenstein's Monster's all-or-nothing PayCost with an effect:
WithSelections chooses AsManyAsCan(X), then performs the exile Move. Completion
compares actual Exile After arrivals with X; failure retains the owner-graveyard
entry redirect. Counter amount counts creature-card Exile arrivals from the same
receipt. Matching(Transition{After}, InZone(Exile)) examines recorded arrival
incarnations, rather than current zone membership or requested inputs. A
replacement redirect does not count; later movement cannot erase an actual exile
arrival. The entering controller, CastX of the entering object, selection pool,
counter alternatives and destinations remain intact. A genuine activated exile
cost remains Exactly(2); no cost normalization is introduced.

The original-wording fixture now has ten cases, including Imperial Edict and the
four additional target controls. It uses the corpus's existing presentation mode
for reminder text, preserving Oracle rules words without MTGish source rewrites.
Four regressions check sole supported meanings, exact keyed and verified default
printing, native choice/count/identity operands, edited rewards and edited exile
counts. These checks do not execute a game backend. Primary published lowering
and native-witness evidence is in
[06de9c78](https://github.com/vishvananda/mtgish/commit/06de9c786d700414985cb9cf960402169039b830),
[2e6c5d60](https://github.com/vishvananda/mtgish/commit/2e6c5d6008fe90c72da86393e56f568a2a43f492)
and [9ffdb821](https://github.com/vishvananda/mtgish/commit/9ffdb82199daa30db62113eeb2ab42b0cef5ea85).
All changes use existing schema-190 operands; model pin and reference lock stay
unchanged. These are semantic corrections, not reference shape normalizations.

All eight gates pass; 776 frontend tests pass with one ignored. All ten cases
have one correct reading, an exact keyed print and a verified default. Full
coverage remains 32,755 equal, with 31,687 single-reading faces and 33,914
readings. The six removed alternatives were wrong successful-destruction tests;
no supported correct target reading is removed. The strict native audit
intentionally fails for exactly nine old wrong readings on nine faces, with
three corrected additions on Drop of Honey, Porphyry Nodes and Frankenstein's
Monster, and no other missing/additions. All native readings and gold validate;
canonical gold is idempotent and native failure output is empty.

Seed count remains 12,044 (12,042 needed and two protected unused), cumulative
retirements 163. Compositional coverage becomes 19,870, with the same four
attribution limitations and no at-risk face. Against the actual immutable
90ff55a executable and reference, the ten-face cohort retains ten exact keyed
and ten verified default prints. Exact defaults change from five to four;
verified current-operand printing and exact wording remain separate checks.
No speed improvement or backend execution is claimed.
