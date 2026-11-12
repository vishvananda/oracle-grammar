# Reference issues

Disagreements between the pinned MTGish reference and the grammar. Gold is
never edited locally. Each `###` heading names one face whose reference encoding
is still disputed; corpus comparisons accept a lost match only for a face listed
under a `###` heading that still parses. Resolved and reference-retained faces
have no `###` heading, so they never excuse a lost match. After an upstream fix,
advance `reference.json`, re-prepare the inputs and recount. Paths are IR paths
inside the face's ability list.

The current reference is `0a8d67d968793f2aedade48349986b180d61d5a0` (schema
190). Upstream reviewed the grammar's 53 differing faces at `1fe5782` in
`docs/reference-decisions-20261112.md`, with a per-face ledger in
`reference-decisions-20261112.json`, and closed the nine held for a witness in
`docs/reference-nine-resolutions-20261112.md`. No face is disputed now, so no
`###` heading remains.

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

## Retained by the reference

Upstream judged these encodings valid contracts or equivalent forms and kept
them. In each, matched faces with the same wording use the grammar's encoding,
or the reference splits the wording with no majority, so the grammar keeps its
encoding and the face stays different. They are not reference errors, and the
grammar adds no alias for them. Faces in a group below are listed there.

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
  `Duplicate(One(EventSubject))`. Its event's `OwnedBy` is a separate
  encoding split (see "A card in your graveyard" below).

#### Bloodroot Apothecary

- `Not(SameObject(Source))` beside `[Not(CardType(Creature)), Token]` is
  redundant. A token copy would be sacrificed only by its controller (CR
  701.21a), who also controls the look-back trigger (CR 603.10a) and is not
  their own opponent. 6 controls with the same event omit the exclusion.

#### Worzel, the Protector, Nemesis Mask, Molly Hayes, Runaway, Forbidden Crypt

- Redundant or equivalent forms: Worzel's `CardType(Land)` beside a basic
  Plains; Nemesis Mask's `Block` requirement on every able creature, where
  Lure and 20 other statics require the creature to be `BeBlocked` by each;
  Molly Hayes's `Transition{result:0, phase:After}` for a one-card exile, where
  71 impulse faces read `One(Result{..., index:0})`; and Forbidden Crypt's
  repeated graveyard scope.

#### Hit

- A mandatory resolving `PayCost` of the sacrifice, with the damage under
  `If PaymentChosen` and the mana value read from
  `CostObjects{payment:Resolution(0), view:Inputs, phase:Before}`. Five
  controls read a `Sacrifice` effect and `Transition{result:0, phase:Before}`.
  Upstream showed the forms agree for this sole-target, one-sacrifice damage
  follow-up and kept the explicit cost receipt.

#### Deathbringer Regent

- `ObjectMatches{object:Source, filter:AllOf([Condition(Compare(...)), Cast ...])}`,
  whose count excludes `Candidate`. The grammar splits the conjunction and
  excludes `Source`, as 6 "other creatures" count controls and Inquisitor
  Captain do. Upstream found no binding counterexample and kept its form.

#### Dream Thief

- `ObjectMatches{object:Source, filter:Condition(PlayerDid ...)}`; 12 controls
  read `Compare` over `EventHistory`. The history predicate and own-spell
  exclusion are the same, and `ObjectMatches` reads the source's last-known
  identity after it leaves.

#### Cryptic Coat

- `Recorded{result:0}` names the cloaked permanent; 12 cloak and manifest
  controls read `Result{result:0, phase:After, index:0}`. Cloak makes at most
  one permanent, so the forms agree.

#### In Thrall to the Pit

- The control change is recorded as result `0`, and `Recorded{result:0}` names
  the creature for untap, haste and the delayed sacrifice; the grammar reads
  `Target{slot:0, index:0}`. With one target the two name the same object.

#### Liliana, Death Mage

- `One(Target)` for "up to one target creature card", and `Prior(That)` for
  the player in the last ability, where 22 of 24 controls read `Target`.

#### Skybreen

- `Acting` for "their hand"; Balor and Laquatus's Creativity read
  `Target{slot:0, index:0}`. With one target player the forms agree.

#### Waterspout Warden, Storm Entity, Graven Dominator

- `EventSubject` where the grammar reads `Source` for an excluded self. Nine
  "another ... entered the battlefield this turn" faces read `Source`, as do
  the spell-history counts of Thrasta, Tempest's Roar, Call Forth the Tempest
  and Thunder Salvo. Upstream showed each event subject is the source here and
  kept separate identities.

#### Skeletal Swarming

- The reference lists "attacks each combat if able" after the later pump; the
  grammar keeps the written order. Changes in one static apply by layer, and
  this is the only face whose requirement is written between two changes.

## Same wording encoded more than one way

The reference encodes the same wording in more than one way, and upstream kept
every encoding below. Groups use `####` headings and faces appear only in
bullets, so these entries never excuse a lost match. Faces the grammar scores as
different are marked *(different)*.

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
- Upstream keeps `Source` in these three: a reflexive payment is not an object
  event that rebinds `EventSubject`.

#### "You may <action>." followed by sentences that depend on it

- `Optional` around the action and its follow-ups: 8 matched faces, such as
  Hellkite Courser, Pawn of Ulamog and Yedora, Grave Gardener; also Planebound
  Accomplice and Surprise Deployment.
- `PayCost{optional:true}` with the follow-ups under `If PaymentChosen`:
  Through the Breach, Swift Warkite, Vizier of Deferment, Purphoros,
  Bronze-Blooded, Dawn of the Dead *(different)*, Mirror Mockery *(different)*,
  Wandering Graybeard *(different)* and Ilharg, the Raze-Boar *(different)*.
- Upstream keeps the guarded payment for Dawn, Mirror and Ilharg (CR 118.12)
  and Wandering Graybeard's guarded look and reveal.

#### Random return from a graveyard to a hand

- `ChooseObjects{constraints:[AtRandom]}`: 19 faces of every ability kind, such
  as Fossil Find, Surreal Memoir, Make a Wish, The Deck of Many Things and
  Haunted Fengraf.
- `WithSelections{chooser:Chance}`: Charmbreaker Devils, Exalted Flamer of
  Tzeentch, Ghoulraiser, Nephalia, Viconia, Drow Apostate and Woodland Sleuth.
- The grammar reads a written card filter with `ChooseObjects` and returns the
  chance selection beside it, so the five faces among these that have a filter
  carry both readings until one form is chosen. Upstream found both choose one
  card uniformly at resolution.

#### A targeted object's pronoun inside a delayed trigger

- `EventSubject`: Delif's Cone, Niko Aris, Barreling Attack and 8 more.
- `Target`: Delif's Cube *(different)*, Long River Lurker *(different)*,
  Okoye, Mighty and Adored, Arcbond and 1 more. Delif's Cone and Delif's Cube
  have near-identical wording.
- Long River Lurker's "you may exile it" follows the pronoun: a `Move` cost of
  the target under `Target`, and a `PlayerAction` exile of `EventSubject` read
  back through `Transition` under `EventSubject`.
- Upstream keeps the captured target: the delayed event matches
  `SameObject(Target(0))`, so its subject is that original target.

#### "Another" in a compound "enters or attacks" self-trigger

- `EventSubject`: Arashin Foremost, Ellivere of the Wild Court, Picard,
  Steadfast Captain, Shredder, Unrelenting, The Rani and Korvold, Fae-Cursed
  King *(different)*.
- `Source`: Hei Bai, Spirit of Balance. Sephiroth, Fabled SOLDIER reads both.
- Among sacrifices the split is one each: Korvold `EventSubject`, Hei Bai
  `Source`, Sephiroth both; the grammar reads a sacrifice's "another" as
  `Source`.

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

#### The last-known state of a sacrificed fixed object

- `EventSubject` or `Source`: Foot Chopper, Riders of the Mark. `Recorded`:
  Ravenous Slime. `Transition{phase:Before}`: Myriad Construct *(different)*.
- Upstream keeps Myriad Construct's sacrifice receipt with `CurrentOrLastKnown`,
  which pins the exact original object and its power.

#### "This card" after the card exiled itself

- `InZone(Source, Exile)`, `InZone(Transition, Exile)` and
  `InZone(CostObjects, Exile)` each appear (Lucius the Eternal, Liliana,
  Heretical Healer, Reborn Hero, Lamplight Phoenix).
- Review: keep the post-cost or post-exile incarnation and its expected zone;
  Kogla shows a generic `Source` can be wrong.

#### The subject of a block-pair event on one card

- No Quarter *(different)* names the event subject `BlockPair(Attacker)` in
  "becomes blocked by a creature with lesser power" and `EventSubject` in the
  mirrored "blocks a creature with lesser power"; the grammar reads
  `BlockPair(Blocker)` there.
- Upstream showed both spellings generate byte-identical code.

#### Operand order of "N life more than your starting life total"

- `Add([N, StartingLife])` in conditions (Chalice of Life, Doctor Strange,
  Surgeon, Leyline of Hope) and `Add([StartingLife, N])` in activation
  restrictions (Speaker of the Heavens, Ayli, Eternal Pilgrim).
- Review: a normal-form choice under identical arithmetic, not a rules defect.

#### "A card in your graveyard" in a zone-change trigger

- `InZone(Players{zone:Graveyard, players:One(AbilityController)})`: Kaya,
  Spirits' Justice ("creature cards in your graveyard are put into exile").
- `OwnedBy(One(AbilityController))` beside `from: Graveyard`: Veteran
  Ghoulcaller *(different)* ("a card in your graveyard is put into your hand").
- The grammar keeps `InZone` for this wording.

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

#### "He" or "she" naming a holder in its own trigger

- `Source`: Ultron, Unlimited ("Whenever Ultron attacks, he connives") and
  Tiger Shark, Abyssal Hunter ("enters or attacks, he connives").
- `EventSubject`: Norman Osborn *(different)* ("Whenever Norman Osborn deals
  combat damage to a player, he connives") and Jack of Hearts, Volatile Hero
  ("When Jack of Hearts dies, he deals damage").
- Upstream keeps Norman Osborn's form: connive is object-directed (CR
  701.50a-b), and the object's controller performs it.

#### "Their life" after a named losing player

- `Acting`: Personal Incarnation *(different)* ("its owner loses half their
  life"), Scourge of the Skyclaves ("each player loses half their life") and
  Ebonblade Reaper (retained above).
- The named player itself: Blood Tribute, Revenge, Rush of Dread and Shard of
  the Nightbringer (`Target`); Havoc Festival, Quietus Spike and Grievous Wound
  (`EventSubject`). `Bound` under `ForEachPlayer`: Betor, Kin to All, Fraying
  Omnipotence, Skull Storm and Shahrazad.
- The grammar reads the named loser for Personal Incarnation,
  `OwnerOf(Source)`, and `Acting` for a player group. Upstream keeps `Acting`
  as the player receiving the life loss.
