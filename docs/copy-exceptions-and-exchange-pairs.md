# Copy exceptions and exchange target groups

Continuous copy actions now compose with the existing copy-exception grammar.
The copied object, affected population, duration and exceptions remain current
operands. An optional target remains optional. Keywords and mechanics use their
native Ability variants inside AddAbilities, each in its own ability scope:
Myriad stays a Mechanic, and a numbered mechanic retains its amount. These are
copiable additions, rather than later grants on top of the copy effect. Token
copy recipes share the same exception phrases.

A two-object exchange introduces one Exactly(2) target group. Its operands are
index 0 and index 1 of the same fresh slot, including after an unrelated target.
An activation's outside-combat restriction is a separate ActivationLimits
condition, Not(Window(Combat)), and composes with any supported activation body.
The specialized Djinn and Muddle declarations are retired; neither the creature
type nor a target slot is inferred from those card names.

## Equivalent pair representations

The generic exchange exposed duplicate native shapes on Shifting Borders,
Shifting Grift, Switcheroo and Visions of Duplicity. Six Spell frames used two
required singleton targets with the same eligibility and an explicit
Not(SameObject(first target)) on the second. One two-object target group chooses
the same distinct pair. Distinctness within one occurrence of “target” follows
[CR 115.3](https://media.wizards.com/2026/downloads/MagicCompRules%2020260619.pdf).
Two arbitrary singleton targets are not assumed distinct.

The owned canonicalizer proves this equivalence only for a plain Spell with
one ExchangeControl action, two ordinary permanent target domains, matching
required counts and empty target metadata. The actor must be AbilityController.
The second target's filter must be exactly the first eligibility plus the
explicit distinctness test. A bounded schema walk checks that shared eligibility
contains no TargetSlot uses, including indirect population/share uses. All input
fields must decode losslessly and the complete native ability must validate.

The first object remains index 0 of the retained slot. The second becomes index
1. Coalesced target declarations retain both origins, and the second operand's
origin travels with its rewritten slot. Independent Spree abilities retain their
separate target namespaces. Different filters, optional counts, other target
metadata, target-dependent eligibility, extra instructions and modal
announcements do not establish this proof.

U0036 records the corresponding 12 raw field edits on those four faces: one
announcement and one second-object operand for each of six Spell frames. This
is an owned AST normal-form change. The model vocabulary, wire schema 191 and
reference lock remain unchanged.

## Validation

The compositional family checks copied keywords, bare and numbered mechanics,
ward costs, token copies, optional targets, exchange after an unrelated target,
multiple exchange groups, granted activation scopes and non-pair refusals.
Frontend regressions edit the target filter, activation cost, copy duration and
mechanic amount, then verify a complete semantic reparse. Owned tests cover
normal-form equivalence, idempotence, validity, ineligible shapes, unknown fields,
invalid addresses, independent Spree frames and mapped origin preservation.

The frozen allowed-population audit retains reference agreement on all 32,755
faces and all 33,825 previous readings. Every current reading is native-valid
and canonical; all source/canonical gold records validate and are idempotent.
The four changed implicit reference singletons were materialized from the old
immutable gold before comparison with U0036. No old meaning is missing and no
new meaning remains after the owned normalization. The five duplicate additions
seen in the initial grammar-only experiment are not accepted as ambiguity.

The 366 affected faces retain 349 verified prints, 342 exact reproductions and
348 verified defaults, with no losses in those gates. Djinn of Infinite Deceits
and Muddle, the Ever-Changing newly gain verified, exact and default printing.
Against the accepted full-print baseline and subsequent focused overlays, the
tracked exact gap falls from 672 to 670: 333 have no verified print and 337 have
a different wording. This comparison is not a new full-population printing run.

Compositional reference coverage rises by seven faces to 20,014. Retiring the
two specialized declarations leaves 11,978 seeds (228 retired cumulatively).
The rule audit reports 80 unused seeds, zero at-risk faces and the same four
attribution limits. Other unused productions remain pending independent meaning
and printing checks. All eight repository gates pass: 840 frontend tests,
114 owned-model tests, 75 Python tests, the full compositional families, Clippy,
formatting, model/reference pin checks and hygiene.
