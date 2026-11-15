# Reported issue audit — 2026-11-14

Scope: the ten open issues #19–#28 in this repository. Every named face is in
the immutable allowed population, and each was `equal` against the uncorrected
reference before this round. That equality was not evidence of a fix. Source
text remains the pinned Oracle input. Reference amendments use only
`adapters/mtgish/updates.json`, and the published export and reference lock
remain unchanged.

Each correction has an update entry, a grammar change that produces the
corrected IR, and a regression case in
`tests/fixtures/reported-corrections-20261114.json` (`src/tests/reported_corrections.rs`).
A case requires one reading equal to the corrected IR, checks the corrected
operands directly and prints the written text back exactly.

| Issue | Finding and disposition |
| --- | --- |
| [#19](https://github.com/vishvananda/oracle-grammar/issues/19) | Open: Naked Singularity and Reality Twist need one replacement whose output is chosen among the colors their land's current types allow. The model has no choice restricted to applicable outputs. |
| [#20](https://github.com/vishvananda/oracle-grammar/issues/20) | Partly fixed: U0045 intersects Token with MadeBy on six faces. The link to the printed creating ability (CR 607.2c) needs a linked-creation relation, like `ExiledWith`. |
| [#21](https://github.com/vishvananda/oracle-grammar/issues/21) | Open: Krovikan Vampire needs a historical death selector that tests the departing object and projects its first graveyard incarnation. |
| [#22](https://github.com/vishvananda/oracle-grammar/issues/22) | Open: Wood Elemental and Dracoplasm need the permanents actually sacrificed as that incarnation entered, tested through the source's current words. |
| [#23](https://github.com/vishvananda/oracle-grammar/issues/23) | Fixed locally: U0043 pays as Heroism resolves and shields that same creature. `ForEachObject` now documents its choice order. |
| [#24](https://github.com/vishvananda/oracle-grammar/issues/24) | Fixed locally, partly superseded: U0044 sacrificed as many Swamps as it could before "if you can't"; it is withdrawn in [the 2026-11-15 audit](reported-issues-20261115.md). U0046 names the acting player `PlayerRef::Acting` inside `PlayersDo`, and `oracle-ir` documents that contract. |
| [#25](https://github.com/vishvananda/oracle-grammar/issues/25) | Fixed locally: U0038 lets a snow Island add one or two additional {U}. |
| [#26](https://github.com/vishvananda/oracle-grammar/issues/26) | Fixed locally: U0039 leaves Aladdin's Lamp's kept card in place. |
| [#27](https://github.com/vishvananda/oracle-grammar/issues/27) | Fixed locally: U0040 offers Bison Whistle's four choices for any looked-at card. |
| [#28](https://github.com/vishvananda/oracle-grammar/issues/28) | Fixed locally: U0041 reverses two land differences; U0042 counts Boreas Charger's lands, not hands. |

## Optional mana replacement: #25

Snowfall says "its controller may add an additional {U}. If that Island is snow,
its controller may add an additional {U}{U} instead." Declining an optional
"instead" keeps the first option (CR 614.1a, 616.1). A snow Island therefore
offers zero, one or two additional {U}. U0038 changes only the snow branch's
mana, from two {U} to `ChooseOne([{U}, {U}{U}])`. The decider, the recipient
and the cumulative-upkeep restriction stay as they were.

The grammar's `instead_mana` now has two forms. `instead_mana_replaced` keeps
the required replacement ("Add {G}. If …, add {G}{G} instead"), and its second
action can't be optional. `instead_mana_may` reads two optional productions and
offers both under the condition. The existing restriction tests check both
branches, and an edited reading still prints its current filters, amounts and
restrictions.

## Unprinted actions and choice menus: #26, #27

Aladdin's Lamp moves all but one looked-at card to the bottom and never moves
the kept one. U0039 drops the old `ReorderLibrary(Top)` of the selected card.
The random bottom order now excludes `Selected` directly instead of the dropped
action's input. LookAt, the one-card choice, the cancelled draw and the real
draw stay intact.

Bison Whistle follows the maintainer's ruling on
[i5jb/mtgish#169](https://github.com/i5jb/mtgish/issues/169#issuecomment-6884910096).
After looking, the controller may choose the battlefield for a Bison, the hand
for a revealed creature, the graveyard for any card, or nothing. U0040 removes
the old Bison-or-creature guard and gives one four-option `ChooseOne`. The
graveyard option's population is the unrestricted top card.

## Land differences: #28

MTGish accepted the reversed subtraction on
[i5jb/mtgish#172](https://github.com/i5jb/mtgish/issues/172#issuecomment-6885300255).
U0041 makes Scholarship Sponsor and Enigma Ridges subtract each searching
player's lands from the captured maximum. U0042 counts the chosen opponent's
lands on the battlefield minus the ability controller's, not hand sizes, on
Boreas Charger. Sandstone Oracle and Slithermuse compare hand sizes and are
unchanged.

Boreas Charger's and Enigma Ridges' seeded rules named their search results by
literal ids. Their canonical readings did not print. Both now name the search
by a relative result.

## Resolution-time payment and choice order: #23

Heroism pays on resolution (ruling 2004-10-04). U0043 replaces the old waiting
shield with a `ForEachObject` over the attacking red creatures. Each run offers
{2}{R} to `ControllerOf(Bound(0))` and, unless it was paid, creates a shield
for damage from `SameObject(Bound(0))`. A creature that becomes an attacking
red creature later gets no shield.

The issue's follow-up asks for an explicit choice-order contract instead of a
card-specific node. `ForEachObject` now documents it in `oracle-ir`. The objects
are fixed as the loop begins. The set has no order of its own (CR 101.4,
101.4c). Runs whose first choice or payment is a player's are grouped by that
player in APNAP order, and each player picks the order of their own runs. Runs
that ask nobody run in an order the ability's controller picks. Iterations
still run one after another, not as one batch. The next `oracle-ir` release
carries this text.

## Compulsory sacrifice and the acting opponent: #24

The sacrifice half of this section is superseded: CR 118.12 makes the action a
cost, so the pinned record was right and U0044 is withdrawn. See
[the 2026-11-15 audit](reported-issues-20261115.md#resolution-time-costs-24).

Infernal Denizen's "sacrifice two Swamps. If you can't, …" is an instruction,
not a cost. A player with one Swamp sacrifices it and then takes the
consequence (CR 609.3). U0044 replaces the indivisible `PayCost` with
`Sacrifice{choose: AsManyAsCan(2)}` and tests
`Earlier{what: Sacrificed, measure: Count}` against fewer than two. The tail
rule now reads "sacrifice" directly, not any cost.

The second defect was the actor inside `PlayersDo`. The reference usually
named the acting player `AbilityController`. Ingenious Mastery's opponent
created Treasures for `One(AbilityController)`, and the shuffle on Natural
Selection, Portent and Visions did the same. Cloudhoof Kirin's mill and Stardew
Valley's control change also depended on that rebinding. Protection Racket read
the other way: "your library" inside its `PlayersDo` is the original
`AbilityController`. Thought Prison mixed both readings. Infernal Denizen needs
both identities at once: the opponent chooses and gains control, and the
eligible creatures stay controlled by the original controller.

`PlayersDo` now follows `EachPlayer`. Inside its body and `otherwise`,
`PlayerRef::Acting` is the acting player and `AbilityController` stays the
ability's controller. U0046 updates the eleven references that meant the actor:

- Cloudhoof Kirin and Ingenious Mastery: the players who mill, create Treasures
  and scry.
- Natural Selection, Portent and Visions: the player who may shuffle.
- Infernal Denizen and Stardew Valley: the chooser and the new controller.
- Thought Prison: the revealed hand and its owner filter.

The rest keep `AbilityController`. Infernal Denizen's eligible creatures stay
controlled by the original controller, and the controller still chooses from
Thought Prison's revealed hand. Ingenious Mastery's Treasure mana and Protection
Racket's "your library" are unchanged. The seeded rules for these faces now
produce `Acting`. The fixture checks Infernal Denizen, Ingenious Mastery,
Natural Selection, Stardew Valley, Thought Prison and the unchanged Protection
Racket.

## Linked created tokens: #20

`ObjectFilter::MadeBy` also includes nontokens put onto the battlefield by one
of the object's abilities. Arcane Artisan, Dance of Many, Dual Nature, Saproling
Burst, Tetravus and Tombstone Stairwell name tokens, so U0045 intersects
`Token`. Faerie Artisans' record already reads "other tokens created with this
creature" that way, and its two other readings now do too. This matches the lowering in
[vishvananda/mtgish@639c2ea9](https://github.com/vishvananda/mtgish/commit/639c2ea9).
It does not settle the linked-ability question. A token created by an unrelated
ability granted to Tetravus still matches, and CR 607.2c/607.5 needs a relation
bound to the printed creating ability and its acquisition.

## Model design still open: #19, #21, #22

- **#19.** Each clause is a separate `Replacement` of `ProduceMana` with a fixed
  `ReplaceMana` color. One replacement could take a `ChooseOne` of guarded
  outputs, but nothing restricts the choice to applicable colors. The player
  could then pick an inapplicable option and avoid the replacement.
- **#21.** `FromHistory` returns an event's subjects as they last existed. The
  return needs the first graveyard incarnation of each matched death, and only
  while it stays there (CR 400.7). It also needs the creature and damage tests
  applied to the departing battlefield object.
- **#22.** `Earlier{what: Sacrificed}` is scoped to preceding instructions. A
  characteristic-defining ability needs the sacrifices made as one exact
  incarnation entered, each with its historical characteristics.

None of these changes the corpus equality population, and none is claimed as
corrected.
