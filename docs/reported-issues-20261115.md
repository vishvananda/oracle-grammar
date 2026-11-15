# Reported issue audit — 2026-11-15

Scope: the #24 follow-up comments, #29 and #30. Every named face is in the
immutable allowed population. Reference amendments use only
`adapters/mtgish/updates.json`; the published export and the reference lock
stay unchanged. Each change has a grammar rule that produces the amended IR and
a regression case in `tests/fixtures/reported-corrections-20261115.json`
(`src/tests/reported_corrections.rs`). The superseded #24 cases in
`tests/fixtures/reported-corrections-20261114.json` now expect the amended IR.

| Issue | Finding and disposition |
| --- | --- |
| [#24](https://github.com/vishvananda/oracle-grammar/issues/24) | Fixed locally: U0044 is withdrawn because the pinned cost was right (CR 118.12). U0048 names Protection Racket's payer `Acting`. U0049 makes "you may have target player …" an `Optional` decided by the controller, and `oracle-ir` documents who declines `PlayersDo`. |
| [#29](https://github.com/vishvananda/oracle-grammar/issues/29) | Fixed locally: one producer normal form for each pair. The contract compares the two negated-condition filters as equal, U0050 normalizes two graveyard exiles to `ChooseObjects` with `Selected`, and Vexing Arcanix's reference already has the normal form. |
| [#30](https://github.com/vishvananda/oracle-grammar/issues/30) | Fixed locally: U0047 sums the power of every card exiled with Drach'Nyen. |

## Resolution-time costs: #24

The 2026-11-14 audit said Infernal Denizen's "sacrifice two Swamps. If you
can't, …" was an instruction, and U0044 sacrificed as many Swamps as it could.
That was wrong. CR 118.12 says that in "[do something]. If [a player] [does,
doesn't, or can't], [effect]" the action is a cost, paid when the ability
resolves, and the clause checks whether the player started to pay it. CR 118.3
forbids paying a cost without all of its resources. A player with one Swamp
therefore sacrifices nothing and takes the consequence, and the 2004-10-04
ruling adds that a player with two Swamps must sacrifice them. The pinned
`PayCost` of exactly two Swamps, tested with `Not(PaymentChosen)`, was right.
U0044 is removed from the update record, and the tail rule reads any cost
again. Frankenstein's Monster's "As this creature enters, exile X creature
cards from your graveyard. If you can't, …" stays an effect (U0025): it is a
replacement effect of a static ability, which CR 118.12 does not cover. Rules: <https://media.wizards.com/2026/downloads/MagicCompRules%2020261106.pdf>
(118.3, 118.12).

U0046's actor change stays. Two follow-ups make the contract explicit:

- Protection Racket's "that player may pay life" inside its `PlayersDo` over
  each opponent named the payer `Prior(That)`. U0048 names it `Acting`, the
  opponent whose turn of the process it is. "Your library" stays the
  controller's.
- `PlayersDo`'s `optional` said only that "the deciding player" may decline.
  In the reference that was the acting player for Infernal Denizen's "an
  opponent may" and Stardew Valley's "another player of your choice may", but
  the controller for "you may have target player …" on Cloudhoof Kirin, Natural
  Selection, Portent, Thought Prison and Visions. Twenty-one other records with
  that wording, such as Dire Undercurrents, already wrap the target's action
  in `Instruction::Optional` decided by `AbilityController`. U0049 rewrites the
  five the same way, with the target player in place of `Acting`. `oracle-ir`
  now says that `optional` lets the acting player decline, and that a choice
  someone else makes is an `Optional` around the action.

Cloudhoof Kirin had a seeded rule that produced the old form next to a general
rule that produced the new one. The seeded rule is retired, so the face now has
one reading.

## Producer normal forms: #29

The consumer's fresh lowerer and the reference spell three meanings
differently. Each pair now has one producer normal form. The contract compares
the two spellings as equal only where the proof does not depend on context.

- **Negated conditions (Total War).** `ObjectFilter::Not(Condition(c))` and
  `ObjectFilter::Condition(Not(c))` both evaluate `c` once, with the candidate
  bound to the same object at the same point, and negate the answer. The
  normal form puts the negation inside, as 22 of the 23 such filters in the
  reference do.
  `oracle-ir-contract` now compares the two as equal. Only comparison merges
  them, because each prints back from its own form.
- **Selection by the action (Frankenstein's Monster).** "Exile X creature cards
  from your graveyard" chooses with `ChooseObjects` and moves
  `Selected(choice)` in the next instruction. Later text reads the move's own
  `Transition` receipt for the cards actually exiled. Fifty-five reference
  records do this, among them the entering replacements on Living Lore and
  Sutured Ghoul. U0050 rewrites Frankenstein's Monster and Mimeoplasm, Revered
  One, the only entering replacements that used `WithSelections`. The contract
  does not merge the two encodings in general: that needs the choice and the
  move to be adjacent, with the receipt and the selection matched, and
  `WithSelections` also covers choices made inside one action, such as one per
  player.
- **A revealed card (Vexing Arcanix).** "If that card has the chosen name, that
  player puts it into their hand" reads the revealed card through
  `Observed(reveal)`, both in the test and in each move, as the reference
  already does. It is not re-queried from the top of the library, and the count
  needs no `Matching` with filter `Any`. A live query and the observed receipt
  agree only when nothing can move a card in between, so the contract does not
  merge them.

## Linked exiled cards: #30

Drach'Nyen's "X is the exiled card's power" read `Query` index zero of every
card exiled with it. A copied enter trigger can exile several cards, and
CR 607.3 then sums the answers, as U0008 does for Phyrexian Ingester. The card
says "the exiled card", not "creature card", so an exiled Vehicle keeps its
printed power (CR 208.3) and the population adds no `Creature` conjunct. U0047
sums `Power` over the cards exiled with the source. The grammar reads the linked
sum in `static_linked_card_power_keyword`, and the seeded twin is retired.
