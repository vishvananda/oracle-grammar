# Reference issues

Disagreements between the pinned MTGish reference and the grammar. Gold is
never edited locally. Each `###` heading names one face whose reference encoding
is still disputed; corpus comparisons accept a lost match only for a face listed
under a `###` heading that still parses. Resolved and reference-retained faces
have no `###` heading, so they never excuse a lost match. After an upstream fix,
advance `reference.json`, re-prepare the inputs and recount. Paths are IR paths
inside the face's ability list.

The current reference is `5408607bc4342aa3cdde1e764e2f405e71dccd39` (schema
190). Upstream reviewed the grammar's 53 differing faces at `1fe5782` in
`docs/reference-decisions-20261112.md`, with a per-face ledger in
`reference-decisions-20261112.json`, and closed the nine held for a witness in
`docs/reference-nine-resolutions-20261112.md`. It reopened 36 retained forms for
one producer form per wording in `docs/reference-consistency-20261112.md` and
closed all 36 by `5c1abe1` in `docs/reference-self-frames-20261112.md` and
`docs/reference-remaining30-20261112.md`. The faces below with a `###` heading
are disputed now.

## Disputed

### Cauldron Dance

- "Return target creature card from your graveyard to the battlefield. ... You
  may put a creature card from your hand onto the battlefield. That creature
  gains haste. Its controller sacrifices it at the beginning of the next end
  step." The second paragraph's haste and sacrifice under `If PaymentChosen(2)`
  name the first paragraph's returned creature, `Result{result:0, phase:After,
  index:0}`, at
  `1/args/body/instructions/4/args/then/instructions/0/args/operation/args/affected/Objects/args`
  and
  `1/args/body/instructions/4/args/then/instructions/1/args/operation/args/body/instructions/0/args/operation/args/objects/args`.
  "That creature" is the card put from the hand, the payment's
  `CostObjects{payment:Resolution(2), component:[0], view:Performed,
  phase:After}`, as in Meek Attack.

### Lieutenants of the Guard

- "Starting with you, each player votes for strength or numbers. Put a +1/+1
  counter on this creature for each strength vote and create a 1/1 white
  Soldier creature token for each numbers vote." The counter amount counts
  `Votes(Word("Feather"))`, a word not on this ballot (`Words(["Strength",
  "Numbers"])`), at
  `0/args/body/instructions/1/args/operation/args/placements/0/amount/args/Word`;
  it reads `Votes(Word("Strength"))`. Messenger Jays' "for each feather vote"
  has the same shape.

### Phage the Untouchable Avatar

- "Until end of turn, whenever target creature you control deals combat damage
  to a creature, destroy the damaged creature." The created trigger captures the
  target as the damage source, so the captured-target rule (Delif's Cube) names
  `Target{slot:0, index:0}` at
  `1/args/body/instructions/0/args/operation/args/body/instructions/0/args/operation/args/objects/args`
  and destroys the attacker. The damaged creature is the event's recipient,
  `Damage(Recipient)`, which the grammar reads.

### Seizures

- "Whenever enchanted creature becomes tapped, this Aura deals 3 damage to that
  creature's controller unless that player pays {3}." The reference damages
  `AbilityController` at
  `1/args/body/instructions/1/args/then/instructions/0/args/operation/args/recipients/args/args`;
  "that creature's controller" is `ControllerOf(EventSubject)`, as the payment's
  player already is.

### Woodland Bellower

- "Search your library for a nonlegendary green creature card with mana value 3
  or less": the reference's `Search` choice filter is
  `AllOf([Not(Supertype(Legendary)), Color(Green), Property(ManaValue <= 3)])`,
  without `CardType(Creature)`, so it also finds green noncreature cards. The
  face matches today only through a seeded rule that copies this filter; a
  compositional "nonlegendary" adjective reads the creature card.

## Resolved upstream

At `7228b84` (`docs/reference-decisions-20261111.md`):

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

At `cb24df1` (`docs/reference-decisions-20261112.md`):

- Collective Voyage, Minds Aglow, Shared Trauma: every player reads
  `PerformedTotal`, the total paid by all players, as Alliance of Arms does
  (MTGish #139).
- Leela, Sevateem Warrior and Xyris, the Writhing Storm: the trigger filters
  `DrawsCard` by `Not(FirstDrawInOwnDrawStep(EventSubject))`, as Orcish
  Bowmasters does (#270).
- Paths of Tuinvale and Veko, Death's Doorkeeper: an ordinary return records
  the full `Transition{result, phase:After}` for a perpetual change, and
  "perpetually becomes a Spirit" sets creature types (#271).
- Portal to Phyrexia: "is" or "has" after a move onto the battlefield describes
  the object as it enters (CR 611.2e, 614.12), so the characteristics join the
  arrival's `Changes`; a later "becomes" stays a continuous effect. Upstream
  applied the same correction to Ascent of the Worthy; Ashiok, Nightmare
  Weaver; Chainer, Dementia Master; Grave Betrayal; Liliana, Death's Majesty;
  Lorcan, Warlock Collector and Sorin, Vengeful Bloodlord (#272). The grammar
  reads all of them through `entry_characteristics_move`.
- A card's characteristics after an exile come from `Transition{phase:After}`;
  `Before` stays for a permanent exiled this way. Nyla and Negative Zone Portal
  changed with it, and Omen Machine now matches (see the one-card exile group).
- Lamplight Phoenix: "this card" after its own exile cost is
  `CostObjects{payment:Resolution(0), component:[0], view:Performed, phase:After}`,
  as in Estrid's Invocation, Hikari, Twilight Guardian and Obzedat, Ghost
  Council.
- Know Naught but Fire: `ForEachPlayer` binds each opponent to their own hand
  count.
- Old Man Willow: one optional `Sacrifice` of another creature or a token, then
  one reflexive trigger, as in Gut, True Soul Zealot.
- Jace, Cunning Castaway and Tale of Katara and Toph: "it" in the holder's own
  trigger is `EventSubject`.
- Goblin Snowman: one prevention whose event is `Either` of the two damage
  branches.
- Ward of Piety: "the next N damage" is `TotalDamage(N)` with `RedirectDamage`
  (#273).

At `0a8d67d` (`docs/reference-reflecting-mirror-20261112.md`):

- Reflecting Mirror: the target is any spell with a single target. The
  reference had also required its mana value to equal X, which excludes every
  nonzero target because X is twice that mana value (#274).

At `e1b51e1` (`docs/reference-consistency-20261112.md`):

- Bloodroot Apothecary: the opponent-sacrifice event no longer adds an
  unprinted `Not(SameObject(Source))`.
- Graven Dominator: "each other creature" excludes the source.
- Karn's Touch: the animation reads mana value from `Target`, as Karn, Silver
  Golem, Sydri, Galvanic Genius and Toymaker do.
- Korvold, Fae-Cursed King and 26 other "enters or attacks" self-triggers name
  the source, not the event subject, in their bodies. The grammar binds `it` and
  "another" after alternatives that include this object's entry to `Source`;
  "attacks or blocks" still names the subject in the reference.

At `7d5544b` (`docs/reference-codsworth-20261112.md`):

- Codsworth, Handy Helper, Danitha Capashen, Paragon and Golden-Tail Trainer:
  "Aura and Equipment spells" and "Aura and/or Equipment spells" are
  `AnyOf([Subtype(Aura), Subtype(Equipment)])`, either kind (#276).

At `724d110` (`docs/reference-self-frames-20261112.md`):

- Waterspout Warden: the self-attack trigger's intervening condition names
  `Source`, as do the eight equivalent self-attack templates.
- Storm Entity: its own entry count excludes `Source` from the spell history, as
  Thrasta, Tempest's Roar, Thunder Salvo and Call Forth the Tempest do.

At `e8b29bb` and `5c1abe1` (`docs/reference-remaining30-20261112.md`), one
producer form per wording for the remaining 30 reopened faces. These now match:

- Cryptic Coat: a singular cloak result is `Result{After, index:0}`.
- Ebonblade Reaper, Personal Incarnation and Skybreen: singular life loss names
  its recipient in quantities as well as the operation; Scourge of the Skyclaves
  keeps `Acting` for its plural loss.
- Liliana, Death Mage: up-to-one target populations are `Targets(slot)`.
- Delif's Cube and Long River Lurker: a created event that captures exactly one
  target names it in its body, as Delif's Cone, Niko Aris and Barreling Attack
  do. Zone changes keep their event subject.
- Electro, Assaulting Battery and Passenger Ferry: reflexive closures keep a
  proven self object as `Source`, as Sparktongue Dragon, Cyclops
  Superconductor, Ambergris, Agent of Balance and Leyline Tyrant now do.
- Draining Whelk: the counter follow-up reads the targeted spell's mana value
  with current or last-known information.
- Forbidden Crypt: an own-graveyard payment drops only the redundant `InZone`.
- In Thrall to the Pit: control, untap, haste and the delayed sacrifice name the
  original target.
- Wandering Graybeard: kinship templates use two explicit guards, an optional
  look and an optional reveal.
- Molly Hayes, Runaway: a singular top-card exile's permission names
  `One(Result{After, index:0})`.
- Myriad Construct: "sacrifice it ... equal to its power" names the original
  object with current or last-known information.
- Nemesis Mask: all-able-blockers requirements attach to the creature to be
  blocked, as Lure does.
- No Quarter: mirrored blocking-pair events name explicit `Attacker` and
  `Blocker` roles.
- Norman Osborn: connive names its object, and that object's controller acts.
- Skeletal Swarming: one static keeps keyword, attack requirement and pump in
  printed order.
- Veteran Ghoulcaller: to-hand duplication is `DuplicateLastKnown(EventSubject)`,
  as Kithkin Brinefarer's is, and the graveyard-origin filter keeps `InZone`.
- Vulshok Factory: a sacrificed source named by the activation cost is `Source`.
- Worzel, the Protector: a basic Plains has no extra `CardType(Land)`.

Upstream also settled these wordings, and the grammar now reads them:

- An optional put from a hand with dependent follow-ups is an optional `PayCost`
  of a typed `Move` part, and the follow-ups under `If PaymentChosen` read
  `CostObjects{payment:Resolution(r), component:[0], view:Performed, phase:After}`:
  Arms Race, Incandescent Soulstoke, Kavaron Consumed, Meek Attack, Oviya,
  Automech Artisan, Planebound Accomplice, Terminal Velocity and Ilharg, the
  Raze-Boar. Cauldron Dance is disputed above and Surprise Deployment is a
  same-wording split below.
- Any other optional entry or creation whose later instructions all change, move
  or sacrifice what entered is performed as an optional `PayCost` of a
  `PlayerAction`, and those instructions keep naming its result (upstream
  `producer_forms.rs`): Dawn of the Dead, Mirror Mockery, Admiral Brass,
  Unsinkable, Break Out, Hellkite Courser and Kharasha Foothills. Leaf-Crowned
  Elder has the kinship form.
- Resplendent Marshal: "another" and "other than this creature" under "enters or
  dies" name `Source`, inside the reflexive closure too, and "the exiled card"
  is the reflexive exile's receipt.
- Deathbringer Regent: the free condition is lifted out of the object filter;
  "there are five or more other creatures on the battlefield" counts
  `Difference{objects, excluding:One(Source)}`, as Portcullis's count does.

At `5408607` (`docs/reference-other-targets-20261112.md`):

- Jaheira, Heroic Harper, Rhino, Terrible Trampler and Amy Rose: "other target
  creatures" after an artifact, enchantment or Equipment target excludes the
  source, as Kinetic Ooze does. The grammar contrasts "other" with an earlier
  target only when that target's type matches.
- A statically `UpTo(1)` target population is `Targets(slot)` wherever it is
  used as objects, players or damage recipients; scalar fields, mandatory single
  targets and required payments keep their forms. The grammar lowers the scalar
  spelling at binding and restores it for the printer.

## Same wording encoded more than one way

The reference encodes the same wording in more than one way, and upstream kept
every encoding below. Groups use `####` headings and faces appear only in
bullets, so these entries never excuse a lost match. Faces the grammar scores as
different are marked *(different)*.

#### A sacrifice followed by damage equal to its property

- `PayCost` of the sacrifice, the damage under `If PaymentChosen` and the
  property read from `CostObjects{payment:Resolution(0), view:Inputs, phase:Before}`:
  Hit *(different)* and Rupture *(different)*.
- A `Sacrifice` effect read through `Transition{result:0, phase:Before}`: Grab
  the Reins ("Sacrifice a creature. Grab the Reins deals damage equal to that
  creature's power to any target").
- Rupture and Grab the Reins have the same two sentences up to the damage
  recipients. Upstream's Hit contract says Rupture now shares the payment form;
  Grab the Reins still reads the effect.

#### An optional put from a hand followed by a delayed return

- `PayCost` of the put from the hand, the delayed trigger under
  `If PaymentChosen` and the creature read from the payment's `CostObjects`:
  Ilharg, the Raze-Boar ("you may put a creature card from your hand onto the
  battlefield tapped and attacking. Return that creature to your hand at the
  beginning of the next end step"), Meek Attack and Planebound Accomplice.
- An `Optional` put followed by an unguarded delayed trigger reading
  `Result{result:1, phase:After}`: Surprise Deployment *(different)* ("You may
  put a nonwhite creature card from your hand onto the battlefield. At the
  beginning of the next end step, return that creature to your hand").
- The source spells Surprise Deployment with `MayAction` and the others with
  `MayCost` and `If(CostWasPaid)`. The grammar guards every such entry.

#### "Vehicles you control become artifact creatures until end of turn"

- `AddCardType(Artifact)` and `AddCardType(Creature)`, as for a crewed Vehicle
  that "becomes an artifact creature": Armed and Armored and The Thanos-Copter.
- `SetCardTypes([Artifact, Creature])`: Start Your Engines, with the same
  sentence followed by "Creatures you control get +2/+0 until end of turn".
- One-card seeded rules keep both until upstream chooses; a hand rule for the
  plural would give Start Your Engines both readings.

#### "Starting with you, each player votes for <word> or <word>"

- `ForEachPlayer{players:All, starting_with:AbilityController}` around a
  `Vote{actor:Bound(0)}`: 17 faces, such as Tyrant's Choice, Plea for Power,
  Magister of Worth and Emissary Green.
- One `Vote{actor:AbilityController}`: Coercive Portal, Lieutenants of the
  Guard, Messenger Jays and Orchard Elemental, all triggered abilities, as are
  Magister of Worth, Emissary Green, Galadriel, Elven-Queen and Tivit, Seller of
  Secrets.
- The grammar reads the loop; one-card seeded rules keep the single vote, so
  Coercive Portal and Orchard Elemental have both readings.

#### A target constraint one card adds to its wording

- Spirit Flare ("Tap target untapped creature you control. If you do, it deals
  damage equal to its power to target attacking or blocking creature an
  opponent controls") adds `Not(SameObject(Target{slot:0}))` to the second
  target, though "you control" and "an opponent controls" are already disjoint.
- Uchuulon ("exile up to one target creature card from an opponent's
  graveyard. If you do, ...") adds `CommonProperty(Owners)` and an unchosen
  count; the 25 faces exiling "target creature card from an opponent's
  graveyard" have neither.
- Both faces are equal through one-card seeded rules, and the compositional
  required payment ("<action>. If you do, ...") gives each a second reading
  without the constraint.

#### "If you've cast another <color> spell this turn"

- `PlayerDid{event:CastSpell(...), window:ThisTurn}`: Dream Thief *(different)*
  ("draw a card if you've cast another blue spell this turn") and Hotheaded
  Giant ("unless you've cast another red spell this turn").
- `Compare(EventHistory(Spell Cast ...) > 0)`: Soul Reap ("Its controller loses
  3 life if you've cast another black spell this turn"), Patrician's Scorn,
  Talara's Battalion and Mage Duel.
- Dream Thief and Soul Reap both put the condition on a body effect; the grammar
  reads the history count for both.

#### Random return from a graveyard to a hand

- Haunted Fengraf's contract is one `Move` of a `WithSelections{chooser:Chance}`
  selection whose count `AsManyAsCan(1)` handles an empty pool. Fossil Find,
  Make a Wish, Reap the Past, Surreal Memoir, The Deck of Many Things and Trash
  Bin match it.
- The same template has `Exactly(1)` in Charmbreaker Devils, Exalted Flamer of
  Tzeentch, Ghoulraiser, Nephalia, Viconia, Drow Apostate and Woodland Sleuth.
- A single untyped card keeps a redundant `InZone(your graveyard)` filter in
  Fossil Find, The Deck of Many Things and Trash Bin, but not in Nephalia.
- The grammar returns both counts for this template until one is chosen.

#### "If it's a <type> card" after a one-card move

- `Compare(CountObjects(Matching(Transition{result, phase:After}, type)) > 0)`:
  23 faces, such as Audacious Swap, Bioplasm, Dimensional Infiltrator, Kianne,
  Dean of Substance, Mind's Dilation and Omen Machine.
- `ObjectMatches(Result{result, phase:After, index:0}, type)`: Bruse Tarl,
  Roving Rancher, Rundvelt Hordemaster and Pep, Raucous Raider; Deadbridge
  Chant and Search for Survivors after a random choice.
- The grammar reads "<player> exiles the top card of their library. If it's a
  <type> card" with the count, as every face with that wording does. After
  "exile the top card of your library" both forms occur, so the general reading
  is the scalar and one-card seeded rules supply the count.

#### "Untap those creatures" after a per-opponent control change

- `Targets(0)`: Hideous Taskmaster. `Affected(0)`: In the Darkness Bind Them,
  with the same two sentences.
- Review: a failed control change must show whether the receipt names the
  attempted or the successful objects.

#### A property of a single result

- `ObjectProperty(Result{..., index:0})` in 32 controls such as Seek, and
  `Aggregate` sum over the result in 28, such as Daredevil, Fearless Fighter
  and Satya, Aetherflux Genius *(different)*.
- Satya is the only face that reads a property of created tokens. Upstream
  rejected the scalar: token doubling can make one requested token several,
  and the sacrifice already addresses all of them. The grammar reads "its" as
  `Result{index:0}`; reading the whole batch needs a batch reference for "its"
  with its own printing inverse.

#### "This card" after the card exiled itself

- `InZone(Source, Exile)`, `InZone(Transition, Exile)` and
  `InZone(CostObjects, Exile)` each appear (Lucius the Eternal, Liliana,
  Heretical Healer, Reborn Hero, Lamplight Phoenix).
- Review: keep the post-cost or post-exile incarnation and its expected zone;
  Kogla shows a generic `Source` can be wrong.

#### Operand order of "N life more than your starting life total"

- `Add([N, StartingLife])` in conditions (Chalice of Life, Doctor Strange,
  Surgeon, Leyline of Hope) and `Add([StartingLife, N])` in activation
  restrictions (Speaker of the Heavens, Ayli, Eternal Pilgrim).
- Review: a normal-form choice under identical arithmetic, not a rules defect.

#### Branch order of "dealt to and dealt by"

- Dealt-to branch first: 19 faces, such as Elvish Scout and Maze of Ith.
- Dealt-by branch first: Cephalid Illusionist, Energy Arc, Soratami Cloud
  Chariot and Statecraft.
- The grammar returns both orders for this wording until one is chosen.

#### A perpetual effect on a creature that just died

- `EventSubject`: Forgeborn Phoenix ("whenever ... creature dies, it
  perpetually gains").
- `Query(InZone(One(EventDestination), Graveyard))`: Consumed by History,
  with the same wording.
- `Source`: Hook Horror (its own "it perpetually gets"), whose later "that
  card's toughness" and "return it" use the `Query`.
