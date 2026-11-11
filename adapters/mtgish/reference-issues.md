# Reference issues

Confirmed disagreements where the pinned MTGish reference, not the grammar, is
wrong. Gold is never edited locally. Each `###` heading names one face; corpus
comparisons accept a lost match only for a face listed here that still parses.
After an upstream fix, advance `reference.json`, re-prepare the inputs and
recount. Paths are IR paths inside the face's ability list.

Revision for every entry unless noted: `1cd0b57efcf32b842557a9bb74b985df98da3daa`.

## Dropped "and only as a sorcery" activation limit

"Activate only if <condition> and only as a sorcery." sets both the condition and
the sorcery-speed limit (CR 602.5d). The reference keeps the condition but sets
`sorcery: false` for two faces, while Uchbenbak, the Great Mistake encodes the
same sentence with `sorcery: true`.

### Balustrade Wurm

- At `[3].args.ability.args.limits.sorcery`: reference `false`, expected `true`.
- Status: open.

### Resurrected Cultist

- At `[0].args.ability.args.limits.sorcery`: reference `false`, expected `true`.
- Status: open.

## Spell lines split into separate Spell abilities

A spell's instruction lines form one `Spell` ability, read in order. Control
populations for these three shapes keep one `Spell` in 254, 111 and 19 matched
faces; the reference splits the lines into separate `Spell` abilities only here.

### Barrage of Boulders

- At the ability list: reference two `Spell` abilities (damage, then the
  Ferocious restriction), expected one `Spell` holding both instructions.
- Status: open.

### Determined

- At the ability list: reference two `Spell` abilities (`EffectOn` can't be
  countered, then `Draw`), expected one `Spell` holding both instructions.
- Status: open.

### Draconic Intervention

- At the ability list after `[0]` (`Casting`): reference separate `Spell`
  abilities for the damage paragraph and "Exile Draconic Intervention.",
  expected one `Spell` holding the damage, its replacement and the exile.
- Status: open.

## Self references inside the source's own trigger

When an ability triggers on its source, "another", "other" and "it" in that
ability name `Source`. The reference does this in 92 faces with "another" in an
intervening if, 79 with a granted self-trigger's "it gets", 11 with an ETB
replacement count and 4 with a compound self/other trigger. It writes
`EventSubject` only in these faces.

### Waterspout Warden

- At `[0].args.intervening_if.args.left.args.event.ZoneChange.objects.args[2].args.args`:
  reference `{"kind":"EventSubject"}`, expected `{"kind":"Source"}`.
- Status: open.

### Veteran's Armaments

- At `[0].args.changes[0].args.args.body.instructions[0].args.operation.args.affected.Objects.args`:
  reference `{"kind":"EventSubject"}`, expected `{"kind":"Source"}`.
- Status: open.

### Storm Entity

- At `[1].args.body.instructions[0].args.operation.args.change.AddCounters.amount.args.event.Spell.spells.args.args`:
  reference `{"kind":"EventSubject"}`, expected `{"kind":"Source"}`.
- Status: open.

### Graven Dominator

- At `[2].args.body.instructions[0].args.operation.args.affected.Objects.args.filter.args[1].args.args`:
  reference `{"kind":"EventSubject"}`, expected `{"kind":"Source"}`.
- Status: open.

## Targeted objects named through another receipt

A later "that creature", "it" or "its" for a single target names the target.
The reference does this in 38 faces with "that creature" after a control change,
5 with "its mana value" and 2 with "their hand".

### In Thrall to the Pit

- At `[0].args.paid` and `[0].args.unpaid`: reference records the control
  change as result `0` and names the creature `Recorded{result:0}` for untap,
  haste and the delayed sacrifice, expected `Target{slot:0, index:0}` with no
  recorded result.
- Status: open.

### Karn's Touch

- At `[0].args.body.instructions[0].args.operation.args.changes[1].args.power.args.object`
  and `.toughness.args.object`: reference `{"kind":"Affected"}`, expected
  `{"kind":"Target","args":{"slot":0,"index":0}}`.
- Status: open.

### Liliana, Death Mage

- At `[2].args.body.instructions[0].args.operation.args.amount.args[1].args.args.scope.Players.players.args`:
  reference `{"kind":"Prior","args":"That"}`, expected
  `{"kind":"Target","args":{"slot":0,"index":0}}`, as in 22 of 24 controls.
- Status: open.

### Cryptic Coat

- At `[0].args.body.instructions[1].args.operation.args.host`: reference
  `{"kind":"Recorded","args":{"result":0,"by":null}}`, expected
  `{"kind":"Result","args":{"result":0,"phase":"After","index":0}}`, as in 12
  cloak and manifest controls.
- Status: open.

### Draining Whelk

- At `[2].args.body.instructions[1].args.operation.args.placements[0].amount`:
  reference `Earlier{what:Countered, measure:Of(ManaValue)}`, expected
  `ObjectProperty{object:Target{slot:0, index:0}, property:ManaValue,
  information:CurrentOrLastKnown}`, as in 12 countered-spell controls.
- Status: open.

## Players named as Acting outside a player loop

"You" and "your" name `AbilityController` (4 controls), a per-opponent amount
loops over the opponents (9 controls), and "their hand" after "target player"
names the target (2 controls).

### Ebonblade Reaper

- At `[0].args.body.instructions[0].args.operation.args.amount.args.numerator.args.player`:
  reference `{"kind":"Acting"}`, expected `{"kind":"AbilityController"}`.
- Status: open.

### Know Naught but Fire

- At `[0].args.body.instructions[0]`: reference one `DealDamage` whose amount
  reads the `Acting` player's hand, expected `ForEachPlayer` over
  `OpponentsOf(AbilityController)` with `Bound(0)` as recipient and hand owner.
- Status: open.

### Skybreen

- At `[2].args.body.instructions[0].args.operation.args.amount.args.args.scope.Players.players.args`:
  reference `{"kind":"Acting"}`, expected
  `{"kind":"Target","args":{"slot":0,"index":0}}`, as in Balor and
  Laquatus's Creativity.
- Status: open.

## Other single-face convention outliers

### Demonlord of Ashmouth

- At `[1].args.body.instructions[0].args.operation.args.cost.parts[0].Sacrifice.choose`:
  reference `null`, expected `{"Exactly":{"kind":"Constant","args":1}}`, as in
  25 "unless you sacrifice" controls.
- Status: open.

### Maskwood Nexus

- At `[2].args.changes[0].args.mode`: reference `"Add"`, expected `"Set"`, as in
  99 every-creature-type controls.
- Status: open.

### Phyrexian Totem

- At `[1].args.body.instructions[0].args.operation.args.changes[3].args` and
  `[4].args`: reference `"Creature"` then `"Artifact"`, expected `"Artifact"`
  then `"Creature"` in text order, as in 188 controls.
- Status: open.

### Worzel, the Protector

- At `[1].args.body.instructions[1].args.operation.args.objects.args.filter.args[1].args`:
  reference adds `CardType(Land)` to `[Supertype(Basic), Subtype(Plains)]`,
  expected the two qualities only, as in 29 "basic <land type> card" controls.
- Status: open.

### Bloodroot Apothecary

- At `[2].args.event.Player.objects.args`: reference adds
  `Not(SameObject(Source))`, expected `[Not(CardType(Creature)), Token]` with
  no source exclusion, as in 6 controls.
- Status: open.

### Vulshok Factory

- At `[1].args.body.instructions[0].args.operation.args.tokens[0].recipe.Defined.power_toughness[0]`
  and `[1]` `.args.recipients.args.args`: reference
  `Query{objects:CostObjects{payment:Activation, ...}}`, expected
  `{"kind":"Source"}`, as in 32 "charge counters on <this>" controls.
- Status: open.

### Ward of Piety

- At `[1].args.body.instructions[0].args.operation`: reference
  `RedirectSomeDamage([Constant(1), Targets(0)])` with limit `NextOccurrence`,
  expected `RedirectDamage(Targets(0))` with limit `TotalDamage(Constant(1))`,
  as in 16 "the next N damage" controls.
- Status: open.

### Dream Thief

- At `[1].args.body.instructions[0].args.condition`: reference
  `ObjectMatches{object:Source, filter:Condition(PlayerDid ...)}`, expected
  `Compare` over `EventHistory` of blue spells you cast this turn other than
  this one, as in 12 controls.
- Status: open.

### Hit

- At `[0].args.body.instructions[0]` and `[1]`: reference `PayCost` with
  `If PaymentChosen` and `CostObjects`, expected a `Sacrifice` effect followed by
  damage equal to the sacrificed permanent's mana value through
  `Transition{result:0, phase:Before}`, as in 5 controls.
- Status: open.

### Alliance of Arms

- At `[0].args.body.instructions[1].args.operation.args.tokens[0].amount`:
  reference `PerformedTotal(0)`, expected `Performed(0)`, as in the other 3
  join-forces controls.
- Status: open.

### Nemesis Mask

- At `[0]`: reference affects all creatures with
  `RequireCombat{action:Block(SameObject(AttachedTo(Source))), amount:AtLeast(1)}`,
  expected `affected:One(AttachedTo(Source))` with
  `RequireCombat{action:BeBlocked(CardType(Creature)), amount:Each}`, as in 19
  "all creatures able to block <it> do so" controls.
- Status: open.

### Foreboding Steamboat

- At `[1].args.body.instructions[0].args.operation.args.cost.parts[0]`:
  reference expands "put a card exiled with it into its owner's graveyard" to a
  `PlayerAction` that chooses and moves the card (result `1`), expected the
  `ExiledToGraveyard` cost part over cards exiled with the source (result `0`),
  as in 10 controls for the same action.
- Status: open.

### Molly Hayes, Runaway

- At `[0].args.body.instructions[2].args.operation.args.changes[0].args.PlayPermission.cards`:
  reference `{"kind":"Transition","args":{"result":0,"phase":"After"}}`,
  expected `{"kind":"One","args":{"kind":"Result","args":{"result":0,"phase":"After","index":0}}}`,
  as in 16 of 18 "exile the top card ... you may play that card" controls.
- Status: open.

### Forbidden Crypt

- At `[0].args.body.instructions[1].args.operation.args.cost.parts[0].Move.selections[0].objects.args.filter`:
  reference `InZone(Players{zone:Graveyard, players:One(AbilityController)})`,
  which repeats the selection's own graveyard scope, expected `{"kind":"Any"}`,
  as in Recall and Travel Through Caradhras.
- Status: open.

### Veteran Ghoulcaller

- At `[1].args.body.instructions[0].args.operation.args.card`: reference
  `DuplicateLastKnown(EventSubject)`, expected `Duplicate(One(EventSubject))`,
  as in 8 "conjure a duplicate of that card" controls such as Kithkin Brinefarer.
- The event's `OwnedBy(One(AbilityController))` follows the reference's
  convention; the grammar's `InZone` there is a grammar issue.
- Status: open.

### Deathbringer Regent

- At `[1].args.intervening_if`: reference
  `ObjectMatches{object:Source, filter:AllOf([Condition(Compare(CountObjects(Difference(creatures, Candidate)) >= 5)), Cast ...])}`,
  expected `AllOf([ObjectMatches(Source, cast from hand), Compare(...)])` whose
  count excludes the source with `Not(SameObject(Source))`, as in 6 "other
  creatures" count controls and the split "if you cast it and ..." of
  Inquisitor Captain.
- Status: open.

### Goblin Snowman

- At `[0].args.body.instructions`: reference two `CreateReplacement` effects for
  "dealt to" and "dealt by", expected one `CreateReplacement` whose event is
  `Either` of the two damage branches, as in 23 "dealt to and dealt by"
  controls such as Elvish Scout and Maze of Ith.
- The grammar also returns both `Either` branch orders here; that is a grammar
  issue.
- Status: open.

## Both readings wrong

The reference and the grammar both differ from the card's meaning here.

### Vampiric Embrace

- At `[2].args.body.instructions[0].args.operation.args.placements[0].recipients.args.args`:
  reference `{"kind":"EventSubject"}` (the dying creature), expected
  `AttachedTo(Source)`, the enchanted creature, as in 12 "put a +1/+1 counter on
  <the damage dealer>" controls such as Sengir Vampire. The grammar currently
  names the dying card in its graveyard.
- Status: open.

### Alora, Cheerful Scout

- At `[0].args.body.instructions[1].args.operation.args.body.instructions[1].args.then.instructions[0].args.operation`:
  reference `EffectOn` with a `PerpetualEffect` term on `Recorded{result:0}`,
  expected `CreatePerpetualEffect` with `Pump(+1/+1)` on the card the delayed
  trigger returned, as in 78 controls such as Candela, Aegis of Adagia. The
  grammar currently names the stale target.
- Status: open.

## Same wording encoded more than one way

These are not single errors. The reference encodes the same wording in more
than one way, and one encoding should be chosen everywhere. Groups use `####`
headings and faces appear only in bullets, so these entries never excuse a lost
match. Faces the grammar currently scores as different are marked
*(different)*.

#### Self references inside a reflexive "When you do" body

- `Source`: Electro, Assaulting Battery *(different)* ("he deals X damage"),
  Passenger Ferry *(different)* ("another target attacking creature"),
  Resplendent Marshal *(different)*, and 30 matched faces.
- `EventSubject`: Sparktongue Dragon ("it deals 3 damage"), Cyclops
  Superconductor, Ambergris, Agent of Balance ("another target creature") and
  Leyline Tyrant ("it deals that much damage").
- Direct counterparts: Passenger Ferry and Ambergris (attack trigger,
  "another target"); Electro and Leyline Tyrant (leave or die, "When you do,
  it/he deals").

#### "You may <action>." followed by sentences that depend on it

- `Optional` around the action and its follow-ups: 8 matched faces, such as
  Hellkite Courser, Pawn of Ulamog and Yedora, Grave Gardener; also Planebound
  Accomplice and Surprise Deployment.
- `PayCost{optional:true}` with the follow-ups under `If PaymentChosen`:
  Through the Breach, Swift Warkite, Vizier of Deferment, Purphoros,
  Bronze-Blooded, Dawn of the Dead *(different)*, Mirror Mockery *(different)*,
  Wandering Graybeard *(different)* and Ilharg, the Raze-Boar *(different)*.

#### Random return from a graveyard to a hand

- "Return <a card> at random from your graveyard to your hand" uses
  `WithSelections{chooser:Chance}` in triggered abilities (Ghoulraiser,
  Woodland Sleuth, Viconia, Drow Apostate, Nephalia) and
  `ChooseObjects{constraints:[AtRandom]}` in spells and activated abilities
  (Fossil Find, Make a Wish, The Deck of Many Things, Haunted Fengraf
  *(different)*).

#### A targeted object's pronoun inside a delayed trigger

- `EventSubject`: Delif's Cone, Niko Aris, Barreling Attack and 8 more.
- `Target`: Delif's Cube *(different)*, Long River Lurker *(different)*,
  Okoye, Mighty and Adored, Arcbond and 1 more. Delif's Cone and Delif's Cube
  have near-identical wording.

#### "Another" in a compound "enters or attacks" self-trigger

- `EventSubject`: Sephiroth, Fabled SOLDIER, Picard, Steadfast Captain,
  Shredder, Unrelenting, Korvold, Fae-Cursed King *(different)* and 2 more.
- `Source`: Hei Bai, Spirit of Balance, with the same wording as Sephiroth.

#### "If it's a <type> card" after a one-card exile

- `Compare(CountObjects(Matching(Transition{result:0, phase:Before}, type)) > 0)`:
  Audacious Swap, Kianne, Dean of Substance, Omen Machine *(different)*.
- `ObjectMatches(Result{result:0, phase:After, index:0}, type)`: Bruse Tarl,
  Roving Rancher, Deadbridge Chant, Search for Survivors.

#### "Untap those creatures" after a per-opponent control change

- `Targets(0)`: Hideous Taskmaster. `Affected(0)`: In the Darkness Bind Them,
  with the same two sentences.

#### A type added to a returned creature

- Arrival-state `Changes` on the `Move`: Path of the Schemer, Spawning Pod.
- A `CreateContinuousEffect` on the moved result after the `Move`: Lorcan,
  Warlock Collector, Ashiok, Nightmare Weaver, Portal to Phyrexia *(different)*.

#### A property of a single result

- `ObjectProperty(Result{..., index:0})` in 32 controls such as Seek, and
  `Aggregate` sum over the result in 28, such as Daredevil, Fearless Fighter
  and Satya, Aetherflux Genius *(different)*.

#### The last-known state of a sacrificed fixed object

- `EventSubject` or `Source`: Foot Chopper, Riders of the Mark. `Recorded`:
  Ravenous Slime. `Transition{phase:Before}`: Myriad Construct *(different)*.

#### "This card" after the card exiled itself

- `InZone(Source, Exile)`, `InZone(Transition, Exile)` and
  `InZone(CostObjects, Exile)` each appear (Lucius the Eternal, Liliana,
  Heretical Healer, Reborn Hero, Lamplight Phoenix *(different)*).

#### The subject of a block-pair event on one card

- No Quarter *(different)* names the event subject `BlockPair(Attacker)` in
  "becomes blocked by a creature with lesser power" and `EventSubject` in the
  mirrored "blocks a creature with lesser power".

#### Operand order of "N life more than your starting life total"

- `Add([N, StartingLife])` in conditions (Chalice of Life, Doctor Strange,
  Surgeon, Leyline of Hope) and `Add([StartingLife, N])` in activation
  restrictions (Speaker of the Heavens, Ayli, Eternal Pilgrim *(different)*).
