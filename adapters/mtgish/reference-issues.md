# Reference issues

Disagreements between the pinned MTGish reference and the grammar. Gold is
never edited locally. Each `###` heading names one face whose reference encoding
is still disputed; corpus comparisons accept a lost match only for a face listed
under a `###` heading that still parses. Resolved and reference-retained faces
have no `###` heading, so they never excuse a lost match. After an upstream fix,
advance `reference.json`, re-prepare the inputs and recount. Paths are IR paths
inside the face's ability list.

Entries were reported against `1cd0b57efcf32b842557a9bb74b985df98da3daa`.
Upstream reviewed them in `docs/reference-decisions-20261111.md`; the current
reference is `7228b84b2b9398976246b595098f7f5e5fb4fc88` (schema 190). Paths
and statuses refer to that revision unless noted.

## Resolved upstream

Each named face matches at `7228b84`.

- Balustrade Wurm, Resurrected Cultist: "and only as a sorcery" keeps
  `sorcery: true`.
- Barrage of Boulders, Determined, Draconic Intervention: plain spell paragraphs
  form one `Spell` body. Suffocation and Temporal Firestorm (which the grammar
  refuses) moved to the same form.
- Demonlord of Ashmouth: the sacrifice cost chooses exactly one creature.
- Maskwood Nexus: every creature type is `Set(All)`.
- Phyrexian Totem: adjacent type additions put Artifact before Creature.
- Foreboding Steamboat: "put a card exiled with it into its owner's graveyard"
  is the typed `ExiledToGraveyard` cost part. The Eldrazi processors (Mind
  Raker, Murk Strider, Ruin Processor, Ulamog's Reclaimer, Wasteland Strangler)
  share that cost.
- Vampiric Embrace: `WithOtherObject` captures `AttachedTo(Source)` when the
  trigger matches, and the counter goes on `EventOther`.
- Alora, Cheerful Scout: the perpetual effect is applied to
  `CostObjects{payment:Resolution(0), component:[0], view:Performed, phase:After}`
  under the payment guard.
- Veteran's Armaments: the grammar withdrew its dispute and reads `it` in a
  granted holder's own trigger as `EventSubject`, as upstream kept.

## Perpetual effect after an ordinary return to hand

The producer's normal form puts `CreatePerpetualEffect` on the whole
`Transition{result, phase:After}` of the return (Giant Regrowth, Nightclub
Bouncer, Skyline Savior, Talion's Throneguard). Three faces whose Source names
the card `TheCardPutInHandThisWay` instead of `TheCardReturnedToHandThisWay`
keep the scalar `One(Result{..., index:0})`. The producer's put-in-hand branch
needs a recorded return, and these Source actions do not record one. Candela,
Aegis of Adagia has the same encoding and still matches through the grammar's
general scalar reading.

### Paths of Tuinvale

- At `[0].args.announcement.modes.options[0].body.instructions[1].args.operation.args.objects`
  and `options[1]`: reference `One(Result{result:0, phase:After, index:0})`
  and `result:1`, expected `Transition{result:0, phase:After}` and `result:1`.
- Status: open.

### Veko, Death's Doorkeeper

- At `[1].args.body.instructions[1].args.operation.args.objects`: reference
  `One(Result{result:0, phase:After, index:0})`, expected
  `Transition{result:0, phase:After}`.
- Status: open.

## "The total amount of mana paid this way" read per player

`Expr::PerformedTotal` adds up every run of a `ForEachPlayer`; `Performed` read
by a later instruction over the players is each player's own amount. Upstream
retained Alliance of Arms's `PerformedTotal` on that contract, and the grammar
now reads the sentence that way, so Alliance of Arms matches. Three faces with
the same join-forces sentence read `Performed(0)` from a per-player
instruction, so each player gets only their own payment instead of the total.

### Collective Voyage

- At `[0].args.body.instructions[1].args.body.instructions[0].args.operation.args.choices[0].count.UpTo`:
  reference `Performed(0)` inside the per-player search, expected
  `PerformedTotal(0)`.
- Status: open.

### Minds Aglow

- At `[0].args.body.instructions[1].args.operation.args.amount`: reference
  `Performed(0)` in `Draw{players:All}`, expected `PerformedTotal(0)`.
- Status: open.

### Shared Trauma

- At `[0].args.body.instructions[1].args.operation.args.amount`: reference
  `Performed(0)` in `Mill{players:All}`, expected `PerformedTotal(0)`.
- Status: open.

## A draw "except the first one they draw in each of their draw steps" read as a draw-step window

The trigger fires on every draw except a player's first draw in their own draw
step, so it fires on draws in any other step or turn. `Filtered{NthEachTurn{…,
NotEqual 1}, Window(DrawStep)}` fires only during a draw step, and only after
that turn's first draw. Orcish Bowmasters reads the trigger as
`Filtered{Player{DrawsCard, …}, Not(FirstDrawInOwnDrawStep(EventSubject))}`,
and the replacement wording uses `DrawExceptFirstInDrawStep`. The grammar now
reads the trigger as Orcish Bowmasters does, so Orcish Bowmasters matches.

### Leela, Sevateem Warrior

- At `[0].args.event.Filtered`: reference `NthEachTurn` over
  `Player{DrawsCard, OpponentsOf(AbilityController)}` with `NotEqual 1` and
  `Window(DrawStep)`, expected `Player{DrawsCard, OpponentsOf(AbilityController)}`
  with `Not(FirstDrawInOwnDrawStep(EventSubject))`.
- Status: open.

### Xyris, the Writhing Storm

- At `[1].args.event.Filtered`: same reference and expected forms as Leela,
  Sevateem Warrior.
- Status: open.

## "Sacrifice another creature or a token" split into two sacrifices

"Sacrifice <A> or <B>" sacrifices one permanent that is either. Gut, True Soul
Zealot and the Wyll of the Blade, Elder and Fey Pact faces read "sacrifice
another creature or an artifact" as one `Sacrifice` over
`AnyOf([AllOf([Creature, Not(SameObject(Source))]), CardType(Artifact)])`.
Old Man Willow alone reads the same disjunction as a choice among separate
sacrifices.

### Old Man Willow

- At `[1].args.body.instructions[0]`: reference `ChooseOne` whose options are a
  `Sacrifice` of another creature, a `Sacrifice` of a token and an empty
  option, with the reflexive trigger repeated in both sacrifices; expected
  `Optional` over one `Sacrifice` of
  `AnyOf([AllOf([Creature, Not(SameObject(Source))]), Token])`, followed by the
  reflexive trigger.
- Status: open.

## "It" in a granted or token holder's own trigger read as `Source`

In a holder's own trigger, `it` names the event's object, as in a printed
self-trigger. The reference reads it as `EventSubject` in granted and token
abilities such as Dismiss into Dream and Makeshift Mannequin ("When this
creature becomes the target of a spell or ability, sacrifice it."), Vrondiss,
Rage of Ancients ("When this token deals damage, sacrifice it."), Dionus,
Elvish Archdruid ("Whenever this creature becomes tapped during your turn,
untap it and put a +1/+1 counter on it."), Fungus Sliver and Veteran's
Armaments. Two faces read the same `it` as `Source`.

### Jace, Cunning Castaway

- At `[1].args.body.instructions[0].args.operation.args.tokens[0].recipe.Defined.abilities[0].args.body.instructions[0].args.operation.args.objects.args`:
  reference `{"kind":"Source"}`, expected `{"kind":"EventSubject"}`.
- Status: open.

### Tale of Katara and Toph

- At `[0].args.changes[0].args.args.body.instructions[0].args.operation.args.placements[0].recipients.args.args`:
  reference `{"kind":"Source"}`, expected `{"kind":"EventSubject"}`.
- Status: open.

## Retained upstream pending a witness

Upstream kept these encodings and named the test that would justify a change.
Each entry stays open until that witness is supplied or the dispute is dropped.

### Waterspout Warden

- At `[0].args.intervening_if.args.left.args.event.ZoneChange.objects.args[2].args.args`:
  reference `{"kind":"EventSubject"}`, expected `{"kind":"Source"}`.
- Witness: the history reader keeps the outer event frame; show that
  `EventSubject` rebinds to the queried history event.
- Status: open.

### Storm Entity

- At `[1].args.body.instructions[0].args.operation.args.change.AddCounters.amount.args.event.Spell.spells.args.args`:
  reference `{"kind":"EventSubject"}`, expected `{"kind":"Source"}`.
- Witness: the entry replacement's subject is the entering source; show where
  it differs from `Source`.
- Status: open.

### Graven Dominator

- At `[2].args.body.instructions[0].args.operation.args.affected.Objects.args.filter.args[1].args.args`:
  reference `{"kind":"EventSubject"}`, expected `{"kind":"Source"}`.
- Witness: review each `AnyOf` branch (source entry, haunted creature death)
  and its source incarnation separately.
- Status: open.

### In Thrall to the Pit

- At `[0].args.paid` and `[0].args.unpaid`: reference records the control
  change as result `0` and names the creature `Recorded{result:0}` for untap,
  haste and the delayed sacrifice, expected `Target{slot:0, index:0}` with no
  recorded result.
- Witness: a failed control change or moved target showing that `Recorded`
  names something other than the target.
- Status: open.

### Liliana, Death Mage

- At `[2].args.body.instructions[0].args.operation.args.amount.args[1].args.args.scope.Players.players.args`:
  reference `{"kind":"Prior","args":"That"}`, expected
  `{"kind":"Target","args":{"slot":0,"index":0}}`, as in 22 of 24 controls.
- Witness: verify the per-recipient reading of `Prior(That)`.
- Status: open.

### Cryptic Coat

- At `[0].args.body.instructions[1].args.operation.args.host`: reference
  `{"kind":"Recorded","args":{"result":0,"by":null}}`, expected
  `{"kind":"Result","args":{"result":0,"phase":"After","index":0}}`, as in 12
  cloak and manifest controls.
- Witness: a before/after mismatch between `Recorded` and the cloaked result.
- Status: open.

### Know Naught but Fire

- At `[0].args.body.instructions[0]`: reference one `DealDamage` whose amount
  reads the `Acting` player's hand, expected `ForEachPlayer` over
  `OpponentsOf(AbilityController)` with `Bound(0)` as recipient and hand owner.
- Witness: show that the implicit per-recipient row misreads the hand count.
- Status: open.

### Skybreen

- At `[2].args.body.instructions[0].args.operation.args.amount.args.args.scope.Players.players.args`:
  reference `{"kind":"Acting"}`, expected
  `{"kind":"Target","args":{"slot":0,"index":0}}`, as in Balor and
  Laquatus's Creativity.
- Witness: verify the per-recipient reading of `Acting`.
- Status: open.

### Bloodroot Apothecary

- At `[2].args.event.Player.objects.args`: reference adds
  `Not(SameObject(Source))`, expected `[Not(CardType(Creature)), Token]` with
  no source exclusion, as in 6 controls.
- Witness: a legal state where an opponent's sacrifice includes the source.
- Status: open.

### Ward of Piety

- At `[1].args.body.instructions[0].args.operation`: reference
  `RedirectSomeDamage([Constant(1), Targets(0)])` with limit `NextOccurrence`,
  expected `RedirectDamage(Targets(0))` with limit `TotalDamage(Constant(1))`,
  as in 16 "the next N damage" controls.
- Witness: a counterexample for N = 1, where the two forms may coincide.
- Status: open.

### Dream Thief

- At `[1].args.body.instructions[0].args.condition`: reference
  `ObjectMatches{object:Source, filter:Condition(PlayerDid ...)}`, expected
  `Compare` over `EventHistory` of blue spells you cast this turn other than
  this one, as in 12 controls.
- Witness: show the history predicate or own-spell exclusion is wrong, not
  only the wrapper.
- Status: open.

### Hit

- At `[0].args.body.instructions[0]` and `[1]`: reference `PayCost` with
  `If PaymentChosen` and `CostObjects`, expected a `Sacrifice` effect followed by
  damage equal to the sacrificed permanent's mana value through
  `Transition{result:0, phase:Before}`, as in 5 controls.
- Witness: a failed-payment or replacement case where the mandatory payment
  differs from an explicit sacrifice.
- Status: open.

### Deathbringer Regent

- At `[1].args.intervening_if`: reference
  `ObjectMatches{object:Source, filter:AllOf([Condition(Compare(CountObjects(Difference(creatures, Candidate)) >= 5)), Cast ...])}`,
  expected `AllOf([ObjectMatches(Source, cast from hand), Compare(...)])` whose
  count excludes the source with `Not(SameObject(Source))`, as in 6 "other
  creatures" count controls and the split "if you cast it and ..." of
  Inquisitor Captain.
- Witness: a binding counterexample for `Candidate` inside the conjunction.
- Status: open.

### Goblin Snowman

- At `[0].args.body.instructions`: reference two `CreateReplacement` effects for
  "dealt to" and "dealt by", expected one `CreateReplacement` whose event is
  `Either` of the two damage branches, as in 23 "dealt to and dealt by"
  controls such as Elvish Scout and Maze of Ith.
- The grammar also returns both `Either` branch orders here; that is a grammar
  issue.
- Witness: a damage or replacement interaction where the two effects differ
  from one.
- Status: open.

## Retained by the reference

Upstream judged these encodings correct or equivalent and kept them. Each face
is the only one with its encoding: matched faces with the same wording use the
grammar's encoding, so the grammar keeps that encoding. These faces stay
different until upstream unifies the wording. They are not reference errors,
and the grammar adds no alias for them.

#### Karn's Touch

- `Affected` names the object whose characteristics are evaluated. Karn,
  Silver Golem, Sydri, Galvanic Genius and Toymaker have the same sentence with
  `Target`.

#### Draining Whelk

- `Earlier{what:Countered, measure:Of(ManaValue)}` reads the saved target even
  if countering fails, which matches the Time Spiral Remastered release notes.
  12 countered-spell controls read `ObjectProperty` of the target.

#### Ebonblade Reaper

- `Acting` resolves to the controller in the first life loss and to the damaged
  player in the second. 4 controls name `AbilityController`.

#### Vulshok Factory

- `Query{objects:CostObjects{payment:Activation, ...}}` with
  `CurrentOrLastKnown` keeps the sacrificed source's counters. 37 faces that
  sacrifice themselves and count their own counters read `Source`.

#### Veteran Ghoulcaller

- `DuplicateLastKnown(EventSubject)` keeps the departed card's definition and
  perpetual changes. Kithkin Brinefarer has the same sentence with
  `Duplicate(One(EventSubject))`. The event's `OwnedBy(One(AbilityController))`
  follows the reference; the grammar's `InZone` there is a grammar issue.

#### Worzel, the Protector, Nemesis Mask, Molly Hayes, Runaway, Forbidden Crypt

- Redundant or equivalent forms: Worzel's `CardType(Land)` beside a basic
  Plains, Nemesis Mask's dual combat requirements, Molly Hayes's
  `Transition{result:0, phase:After}` for a one-card exile, and Forbidden
  Crypt's repeated graveyard scope.

## Same wording encoded more than one way

These are not single errors. The reference encodes the same wording in more
than one way, and one encoding should be chosen everywhere. Groups use `####`
headings and faces appear only in bullets, so these entries never excuse a lost
match. Faces the grammar currently scores as different are marked
*(different)*. Upstream kept every group at `7228b84`; each one's review
requirement comes before any normal form is chosen.

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
- Review: keep creator, source, granted holder and reflexive subject distinct;
  use `Source` only where the event frame proves it is the same holder.

#### "You may <action>." followed by sentences that depend on it

- `Optional` around the action and its follow-ups: 8 matched faces, such as
  Hellkite Courser, Pawn of Ulamog and Yedora, Grave Gardener; also Planebound
  Accomplice and Surprise Deployment.
- `PayCost{optional:true}` with the follow-ups under `If PaymentChosen`:
  Through the Breach, Swift Warkite, Vizier of Deferment, Purphoros,
  Bronze-Blooded, Dawn of the Dead *(different)*, Mirror Mockery *(different)*,
  Wandering Graybeard *(different)* and Ilharg, the Raze-Boar *(different)*.
- Review: the guarded payment form for Dawn, Mirror and Ilharg was kept on
  purpose; frequency alone does not reopen it.

#### Random return from a graveyard to a hand

- "Return <a card> at random from your graveyard to your hand" uses
  `WithSelections{chooser:Chance}` in triggered abilities (Ghoulraiser,
  Woodland Sleuth, Viconia, Drow Apostate, Nephalia) and
  `ChooseObjects{constraints:[AtRandom]}` in spells and activated abilities
  (Fossil Find, Make a Wish, The Deck of Many Things, Haunted Fengraf
  *(different)*).
- Review: compare cardinality, batch and population before choosing one form.

#### A targeted object's pronoun inside a delayed trigger

- `EventSubject`: Delif's Cone, Niko Aris, Barreling Attack and 8 more.
- `Target`: Delif's Cube *(different)*, Long River Lurker *(different)*,
  Okoye, Mighty and Adored, Arcbond and 1 more. Delif's Cone and Delif's Cube
  have near-identical wording.
- Review: a delayed turn event has no object subject; show whether
  `EventSubject` captures the outer trigger, and keep the target's identity.

#### "Another" in a compound "enters or attacks" self-trigger

- `EventSubject`: Sephiroth, Fabled SOLDIER, Picard, Steadfast Captain,
  Shredder, Unrelenting, Korvold, Fae-Cursed King *(different)* and 2 more.
- `Source`: Hei Bai, Spirit of Balance, with the same wording as Sephiroth.
- Review: check both event branches and any granted holder before unifying.

#### "If it's a <type> card" after a one-card exile

- `Compare(CountObjects(Matching(Transition{result:0, phase:Before}, type)) > 0)`:
  Audacious Swap, Kianne, Dean of Substance, Omen Machine *(different)*.
- `ObjectMatches(Result{result:0, phase:After, index:0}, type)`: Bruse Tarl,
  Roving Rancher, Deadbridge Chant, Search for Survivors.
- Review: Before and After views can differ under replacements; the printed
  sentence and actual receipt decide the view.

#### "Untap those creatures" after a per-opponent control change

- `Targets(0)`: Hideous Taskmaster. `Affected(0)`: In the Darkness Bind Them,
  with the same two sentences.
- Review: a failed control change must show whether the receipt names the
  attempted or the successful objects.

#### A type added to a returned creature

- Arrival-state `Changes` on the `Move`: Path of the Schemer, Spawning Pod.
- A `CreateContinuousEffect` on the moved result after the `Move`: Lorcan,
  Warlock Collector, Ashiok, Nightmare Weaver, Portal to Phyrexia *(different)*.
- Review: arrival changes set entry characteristics and later effects do not;
  review wording and entry replacements per card.

#### A property of a single result

- `ObjectProperty(Result{..., index:0})` in 32 controls such as Seek, and
  `Aggregate` sum over the result in 28, such as Daredevil, Fearless Fighter
  and Satya, Aetherflux Genius *(different)*.
- Review: unify only for a proven zero-or-one set with the same empty case.

#### The last-known state of a sacrificed fixed object

- `EventSubject` or `Source`: Foot Chopper, Riders of the Mark. `Recorded`:
  Ravenous Slime. `Transition{phase:Before}`: Myriad Construct *(different)*.
- Review: these coincide only with the exact original incarnation and an
  explicit last-known view.

#### "This card" after the card exiled itself

- `InZone(Source, Exile)`, `InZone(Transition, Exile)` and
  `InZone(CostObjects, Exile)` each appear (Lucius the Eternal, Liliana,
  Heretical Healer, Reborn Hero, Lamplight Phoenix *(different)*).
- Review: keep the post-cost or post-exile incarnation and its expected zone;
  Kogla shows a generic `Source` can be wrong.

#### The subject of a block-pair event on one card

- No Quarter *(different)* names the event subject `BlockPair(Attacker)` in
  "becomes blocked by a creature with lesser power" and `EventSubject` in the
  mirrored "blocks a creature with lesser power".
- Review: attacker and blocker roles bind per event; mirrored clauses may need
  different references.

#### Operand order of "N life more than your starting life total"

- `Add([N, StartingLife])` in conditions (Chalice of Life, Doctor Strange,
  Surgeon, Leyline of Hope) and `Add([StartingLife, N])` in activation
  restrictions (Speaker of the Heavens, Ayli, Eternal Pilgrim).
- Review: a normal-form choice under identical arithmetic, not a rules defect.

#### A perpetual effect on a creature that just died

- `EventSubject`: Forgeborn Phoenix ("whenever ... creature dies, it
  perpetually gains").
- `Query(InZone(One(EventDestination), Graveyard))`: Consumed by History,
  with the same wording.
- `Source`: Hook Horror (its own "it perpetually gets"), whose later "that
  card's toughness" and "return it" use the `Query`.

#### "He" or "she" naming a holder in its own trigger

- `Source`: Ultron, Unlimited ("Whenever Ultron attacks, he connives") and
  Tiger Shark, Abyssal Hunter ("enters or attacks, he connives").
- `EventSubject`: Norman Osborn *(different)* ("Whenever Norman Osborn deals
  combat damage to a player, he connives") and Jack of Hearts, Volatile Hero
  ("When Jack of Hearts dies, he deals damage").
- "It" in the same places is `EventSubject` in attack triggers (Atlantean
  Skirmisher, Copycrook) and `Damage(Source)` in combat-damage triggers
  (Cyclonus, the Saboteur, Security Bypass), so the gendered pronoun differs
  from "it" in each context, in opposite directions.
- Review: decide whether a gendered pronoun reads like "it" in each event
  frame; a dies trigger needs the departed object either way.

#### "Their life" after a named losing player

- `Acting`: Personal Incarnation *(different)* ("its owner loses half their
  life"), Scourge of the Skyclaves ("each player loses half their life") and
  Ebonblade Reaper (retained above).
- The named player itself: Blood Tribute, Revenge, Rush of Dread and Shard of
  the Nightbringer (`Target`); Havoc Festival, Quietus Spike and Grievous Wound
  (`EventSubject`). `Bound` under `ForEachPlayer`: Betor, Kin to All, Fraying
  Omnipotence, Skull Storm and Shahrazad.
- The grammar's `ControllerOf(Source)` for Personal Incarnation is a grammar
  issue: "their" must name the owner who loses the life.
- Review: choose `Acting` or the named player for a single loser; keep a loop
  binding only for a player group.
