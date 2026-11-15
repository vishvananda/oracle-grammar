# Print shapes that blow up search

The printer builds candidate texts from a beam over the grammar and accepts one only
after it reparses to the same IR. A few cards took minutes to print. Profiling the five
slowest cards in a 1-in-30 corpus sample showed one mechanism, and three grammar shapes
that feed it.

| Card | Failed short key | Full key |
| --- | --- | --- |
| Hidden Blade | 272 s | 0.27 s |
| Immersturm Predator | 55 s | 0.05 s |
| Dark Leo & Shredder | 37 s | 0.03 s |
| Infesting Radroach | 21 s | 0.04 s |

## The mechanism

When the default print differs from Oracle, the scorer derives the text's productions
(the full key) and first tries a shorter key: the productions the default print does not
already use. A key is a set with no positions. The short key mostly holds referring
productions such as `that_noun`, `it_pronoun` and `that_player`, and the printer prefers
them at every compatible site in every ability. Every candidate is then wrong the same
way:

- Hidden Blade: "That creature gets +1/+0 and has first strike." for the equipped creature.
- Infesting Radroach: "It can't block." for the card itself.
- Dark Leo & Shredder: "Attacking Ninjas that player controls have deathtouch."

No candidate verifies, so the whole fallback runs: the whole-card beam at two widths and
two views, then each stretch of abilities apart, then the canonical form, at beam 8 and
again at 32. Hidden Blade ran 56 such passes over up to 4,472 texts, binding each one
(about 20 ms) to test its meaning. The full key then printed in under 0.3 s.

The scorer now tries the short key once, with one narrow beam and no fallback, then the
full key, and gives the short key its whole search only when the full key fails too. On
the 1-in-30 sample this cut rendering from 315 s to 32 s. On the full corpus, rendering
all 32,755 faces fell from 28.7 to 18.8 minutes, with the same exact and default prints.

## Disallowed shapes

1. **Agreement alternation.** One production must not offer two inflections of a word,
   such as `("gets" | "get")`, `("has" | "have")`, `("becomes" | "become")` or
   `("counter" | "counters")`. The subject or a count elsewhere decides the form, so the
   printer builds both, and the wrong one ("That creature get +1/+0 and gain first
   strike") survives until style scoring or a semantic bind rejects it. Each group
   doubles a card's candidates. Write one production per number, as `clause_you` with
   `verb_operation_base` and `clause_third` with `verb_operation_third` do.
2. **Gendered pronoun alternation.** A production must not offer `"it" | "he" | "she"`
   for one referent. It cannot see the referent's gender, so the printer tries all of
   them ("Whenever he become tapped" for Immersturm Predator). Choose the pronoun from
   the card's context.
3. **Anaphor without an antecedent.** "It", "that creature", "that player" and similar
   phrases print only for something already mentioned earlier in the same ability. They
   are not a spelling of the card itself at an ability's start, of a static ability's
   subject, or of the controller.

`tools/shapes.py` finds shapes 1 and 2, and `tools/hygiene.py` runs it. The current
occurrences, all in hand-written rules, are listed in `tools/shape_baseline.json`. A new
occurrence fails the check, and a fixed one must leave the baseline. Shape 3 is a print
constraint: print-side reference symbols do not check for an antecedent yet, and only
the final reparse rejects a wrong one.
