# Counter placements as resolution payments

A counter placement offered by “you may … If you do” is the native counter
cost inside PayCost. The dependent branch tests that payment's actual
PaymentChosen receipt. An Optional instruction around an ordinary PutCounters
effect represents a different program and cannot replace that contract.

The shared payment grammar now uses the existing typed counter placement
phrase, including its recipients, counter kind and amount. The same cost can
be paid by another explicitly named player; payer and recipient remain separate
identities. Ordinary optional placements without a payment continuation retain
their existing Optional program. Required payments and declined-payment branches
reuse the same cost, preserving their own optionality and condition.

The three land-animation cases—Cyclone Sire, Noyan Dar, Roil Shaper, and Wall of
Resurgence—now compose through that payment and the existing retained-animation
grammar. The old learned instruction hardcoded the payment result to zero and
the creature type to Elemental. It is retired. The shared rules retain the
actual target and payment addresses after an unrelated earlier action, instead
of relying on the old fixed payment address. Edited amounts, recipient types,
base stats, creature types, granted keywords and durations remain operands.

This batch changes grammar coverage and removes a specialized instruction. It
adds no AST vocabulary, schema or owned canonical normal form. All 26 allowed
faces with native PutCounters costs were inventoried; no allowed reference used
a competing PlayerAction counter-cost form. Reference records remain unchanged.

## Validation

The eight compositional cases cover optional/required payments, an earlier
unrelated target, the source as recipient, edited animation characteristics,
a different payer and recipient, declined payments and ordinary optional
counter placement. Two frontend regressions verify the native identities and
print synthesized operands through complete semantic reparse.

The frozen 32,755-face audit retains reference equality, every previous native
reading and the same reading count: 33,825 valid, canonical readings, with no
missing, added or merged meanings. All 26 affected faces have verified/default
prints and 24 reproduce their original wording. The three land-animation faces
newly gain verified, exact and default printing; no affected face loses any of
those gates. Applying this focused comparison to the accepted print baseline
leaves 672 exact-wording gaps: 335 without a verified print and 337 with another
wording. A full printing rerun was not required for this unchanged-model family.

Compositional reference coverage rises by five faces to 20,007. Retiring the
specialized seed leaves 11,980 seeds (226 retired cumulatively), with 74 unused,
zero at-risk faces and the same four attribution limits. All eight repository
gates pass: 837 frontend tests, 108 owned-model tests, 75 Python tests and the
full compositional family suite, plus formatting, Clippy, pin and hygiene checks.
