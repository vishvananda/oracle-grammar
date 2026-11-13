# Shared Gift branches

Gift definitions share one header and their closed rules-defined programs.
Card draws, Food, Treasure, tapped Fish, Octopus, extra turns and Rhystic Study
gifts all name GiftedPlayer. Food and Treasure reuse the existing intrinsic
token definitions. A gift's token count is independent of its statistics.

Spell specialization shares the existing kicker branch machinery. Bind the
whole spell first, then project its promise predicates into two independent
Spell branches. Negation, conjunction and disjunction preserve any remaining
predicate. Targets used only in the promised branch disappear from the other
announcement; dependencies between target specifications remain closed. Modal
announcements are refused by this specialization until their own contract is
implemented. Nested ability frames are not projected into the parent's choice.

A leading then-condition owns its conjunction or a closed instruction with
dependent followups, such as a reveal-until/put/remainder sequence or a payment
and success branch. It cannot absorb an arbitrary program across later
independent sentences. Direct and captured where-X readings remain supported
inside that condition; neither can be discarded to simplify its grammar. The inverse searches bounded common prefixes and suffixes
and both promise polarities. It can also reconstruct a conjunction with a remaining condition.
It prints current branch operands, rather than replaying a saved sentence, and
every accepted print reparses to the same native meaning. Casting additional
costs remain outside both resolution branches. Permanent gifts retain a bare
header and separate entry abilities. A header plus an independent root Spell
would bypass promise specialization, so that split proposal is rejected.

Reusable clauses cover graveyard target groups with per-card characteristic
restrictions, explicit battlefield-entry history, untapped owner-controlled
returns with counters and fixed plural seeks. A plural seek moves
its complete random selection; it cannot manufacture a first-card receipt.

U0032 corrects Octomancer's old 1/1 gift to one 8/8 blue Octopus. The allowed
original reminder and [CR702.174i](https://media.wizards.com/2026/downloads/MagicCompRules%2020261106.pdf)
agree on that definition. Preserve its blue color, Creature/Octopus types,
untapped entry, opponent recipient and separate end-step copy trigger. This
changes existing AST operands, not schema 191 or either owned model package.
The root grammar package advances to the next unpublished patch version;
published 0.2.0 artifacts and reference/model pins remain immutable.

The retirement pass keeps `m_return_up_to_two_target_creature_cards_3`: removing
it loses the verified default print of Smile at Death. It joins the two older
protected edited-print rules until the generic inverse covers their cases.

Do not equate sequential damage instructions with one DealDamageParts action.
Blooming Blast keeps its existing simultaneous-damage lowering in this batch.
Likewise, copying a returned creature requires its actual After incarnation;
the earlier graveyard target is not an alias for that receipt. Coiling Rebirth
retains its existing correct template pending a reusable movement-aware clause.

## Validation and retirement

The batch retires 40 seeds, bringing cumulative retirements since 0.1.0 to 205
and leaving 12,002 (11,999 needed and three protected printing rules).
Compositional coverage rises from 19,871 to 19,918 faces. All 32,755 allowed
faces still match the reference; 31,689 have one reading and there are 33,910
native-valid, canonical readings. The same four attribution limitations remain,
with no face at risk. All original and canonical reference records validate,
and canonical gold is idempotent.

The frozen native preservation audit reports five absent wire encodings:
Octomancer's wrong 1/1 gift, incorrect split Gift/Spell readings on Mind Spiral
and Sazacap's Brew, and two nested Betor, Kin to All encodings. The corrected
Octomancer reading is the only addition. No other native reading changes.
The audit deliberately does not report unconditional preservation success.

Betor's two absent encodings put the 40-toughness test inside the 20-toughness
branch. The new source reading keeps the sentence tests independent. These
encodings are equivalent for this card: if the 20 test fails, untapping does not
run, so the identical unchanged toughness query cannot satisfy 40. If it passes,
both encodings run the same untap before the same 40 test. The higher test and
life-loss body do not read a result produced by untapping. Removing this nesting
from each old encoding yields the corresponding retained native value exactly.
Both direct and per-player life-loss forms remain supported. This source-shape
reduction adds no global AST equality alias and changes no reference record.
Court of Embereth retains both its direct and captured where-X readings; the
captured definition remains inside its monarch branch. Closed reveal-until and
payment continuations retain their observations and completion references.

All eight gates pass: formatting, pin checks, hygiene, 75 Python tests, Clippy,
helper build, compositional families and library tests (798 frontend passes,
one ignored; 102 owned-model/contract passes, four ignored). Clippy used a
writable copy of the debug cache after the original cache became read-only;
its command and warning policy are unchanged. The 26 Gift fixtures retain all
exact keyed and verified default prints; 16 also pass with all seeds disabled.
Five additional conditional controls preserve receipts and alternative where-X
forms, and a synthetic sentence-boundary regression prevents a later draw from
being absorbed into a preceding promise condition.

Across the 64 affected/control faces, exact keyed prints increase from 63 to 64
and verified default prints from 63 to 64 against the immutable pre-batch
executable. Exact default wording decreases from 38 to 29; current operands and
semantic reparsing remain the required gate. The changed defaults were reviewed,
including Perch Protection's single conditional conjunction and Smile at Death's
restored return/counter wording. These are bounded search results from quick
helpers, not speed measurements or exhaustive claims about printable text.
