# Numeric thresholds and closed conditional followups

The former `m_operation_if_number_is_value_or_more` production consumed an
arbitrary number before "is", but its meaning always queried the source
spell's X. It ignored the written left operand, and the inverse could not
reconstruct that operand. Shared numeric conditions now retain both numbers
and the comparison. Constants remain constants; an announced X remains the
choice belonging to its own casting or activation frame.

A conditional consequent ends with its conjunction or a complete dependent
unit. It cannot absorb an arbitrary program of later independent sentences.
Exile followed by permission to play the exiled card, observed-hand selection
followed by discard, and reveal-until followed by a free cast have explicit
shared units. Their result references are relative to the actual producers.
In particular, a destroyed object's controller comes from its Before receipt,
including when observable effects precede the destruction.

An optional offer followed by "If you don't" also has a fallback when its
enclosing condition is false. Nesting the entire negative followup inside an
otherwise-empty conditional loses that outcome. The complete conditional-offer
declaration supplies both fallback branches with independent result bindings.

Private derivation marks distinguish an explicit where-definition from an
unrelated numeric effect. A numeric X test must use its complete definition;
splitting a definition off an earlier sentence cannot introduce a new casting
choice into a trigger. Independent abilities and costs retain separate scopes.
A counter amount followed by a test of the same defined X samples the value
once immediately before its first use, including a turn-history definition.
The inverse uses the same bounded shape and definition checks as the forward
capture. The private marks are removed before native validation.

Plural "other" filters exclude the current compatible created token batch as
a set. A token batch cannot be replaced by the source spell or by one token
at index zero. With no compatible current batch, ordinary target and holder
contrasts retain their existing meanings. Explicit object exclusions keep
their own syntax. An older whole-tail rule that consumed a quoted blocking
restriction without adding it to the token's abilities has been removed.

## Reference normalization U0033

Within a spell's own frame, querying Source.SpellX with current-or-last-known
information and using AnnouncedX refer to the same chosen casting value.
The frontend now lowers that exact query to AnnouncedX, consistently with the
spell's other amounts and filters. This includes its target distribution.
[CR 107.3a and 707.10](https://media.wizards.com/2026/downloads/MagicCompRules%2020261106.pdf)
define the choice and its preservation when the spell is copied.

The amendment ledger records 28 exact operand replacements on 21 allowed
faces. It changes no text, threshold, comparison, branch, actor, target or
result identity. The frontend traverses typed abilities and rejects unknown
or missing fields before normalization. Independent abilities, costs,
deferred/reflexive/replacement programs, other objects, captured information
and historical X queries are excluded. This is an explicit frontend lowering
and reference shape normalization, rather than an equality exception.

Printing keeps a bounded inverse view for older declarations whose numeric
casting conditions still spell X as Source.SpellX. It projects only a casting
comparison's current left operand, leaving ordinary amounts, independent
abilities and deferred scopes alone. Accepted text must reparse to the preferred
AnnouncedX operands. The view retains the current threshold and comparison;
it does not replay original text or add a canonical equality exception.

The owned AST schema and vocabulary are unchanged. The model pin and reference
lock retain their existing versions and checksums; only `updates.json` amends
the prepared reference.

A postposed permission ("You may play that card this turn") shares the same
meaning declaration as the duration-first form. The old seeded form incorrectly
used its card pronoun as the continuous effect's source and ignored it as the
permitted card. Both surface orders now retain the creator as Source and the
actual preceding card receipt as the permission's operand. The seven dependent
allowed references already have that source, so this fixes lowering without a
further reference amendment.

Further permission corrections keep the card being played separate from the
creator of the permission and the object named by its expiration window.
Nivix and Yasmin now retain Source as creator while using the written card
pronoun as the permission's operand. The until-another-card form uses its
second object phrase for the creator named after "with"; the old rule instead
used the card being played. A known exile receipt cannot satisfy a permission
that only allows casting from the graveyard. Existing references for these
faces already express the correct roles.

The optional player-or-planeswalker recipient now shares the ordinary damage
pipeline. The former whole loyalty-ability seed confused the damage operand
with the later target-count operand and ignored its written type filter.
Chandra's original numbers were both one, concealing that lowering bug. A
regression separates loyalty two, damage three and up-to-one target, then
prints edited damage seven from the current operands. This corrects lowering
without changing the AST schema or the already-correct original reference.

Receipt annotations remain transparent to existing semantic guards. A choice
of a hand card put onto the battlefield still pays with that card's native
Move cost; wrapping its movement cannot introduce a second PlayerAction cost
reading. A single top-library-card exile also retains its Exile zone guard in
later movement, preserving its scalar receipt rather than selecting a card
from another zone. Permissions whose card pronoun has no supported exile
receipt refuse instead of substituting the spell or event subject.

The native comparison is documented separately in the
[reading review](threshold-reading-review.md), including the intentional
normalizations and removals of old alternatives. Strict preservation of every
old parser alternative is not asserted.

## Allowed-population validation

The explicitly allowed population still has 32,755 recognized, reference-equal
faces, including 31,712 with one reading. All 33,884 resulting native readings
validate and are canonical. Comparison with the explicitly materialized old
readings, after the documented casting-X normalization, finds exactly the 26
reviewed alternatives described above, with no new readings or merged readings.
This is a reviewed correction and normalization comparison, not a strict
preservation pass for every old alternative.

The 850 faces missing exact wording in the published 0.2.0 audit were measured
again with immutable helpers, original Oracle text and the amended reference:

| Outcome in the original gap cohort | Previous checkpoint | This batch |
| --- | ---: | ---: |
| No verified reverse print found | 391 | 381 |
| Verified other wording, without exact recovery | 336 | 338 |
| Exact original rules wording recovered | 123 | 131 |

This batch recovers ten additional verified outputs and eight additional exact
wordings, leaving 719 exact-wording gaps in that tracked cohort. The 73 affected
controls retain every previously successful verified, default and exact print;
all 64 Gift controls retain those three successes too. Their exact defaults
remain 29. These focused printing checks are not a new complete population
printing total; the separate full printing refresh must check for gaps elsewhere.

Eleven seeded declarations have been replaced or retired in this batch, leaving
11,991. A complete rule audit finds 19,951 faces reachable without seeded rules,
no at-risk faces and the same four unresolved attribution cases. Three protected
unused declarations remain because printing and semantic coverage are separate.
All eight repository gates pass, including 814 frontend tests, 102 owned-model
tests and 75 Python tests. Coverage uses development helpers; no performance
claim is made. Frozen inputs, helpers, population slices and per-face evidence
remain under the ignored audit cache.
