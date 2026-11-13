# Reference issues

Disagreements between the pinned MTGish reference and the grammar. The pinned
export is never edited in place: [`updates.json`](updates.json) records the only
local changes to it, applied when corpus inputs are prepared (see
[README](README.md#export-updates)). Each `###` heading names one face whose reference encoding
is still disputed; corpus comparisons accept a lost match only for a face listed
under a `###` heading that still parses. Resolved and reference-retained faces
have no `###` heading, so they never excuse a lost match. After an upstream fix,
advance `reference.json`, re-prepare the inputs and recount. Paths are IR paths
inside the face's ability list.

The current reference is `d4b172343b06f8887f700b425ebf69a67acf8efa` (schema
190). Upstream reviewed the grammar's 53 differing faces at `1fe5782` in
`docs/reference-decisions-20261112.md`, with a per-face ledger in
`reference-decisions-20261112.json`, and closed the nine held for a witness in
`docs/reference-nine-resolutions-20261112.md`. It reopened 36 retained forms for
one producer form per wording in `docs/reference-consistency-20261112.md` and
closed all 36 by `5c1abe1` in `docs/reference-self-frames-20261112.md` and
`docs/reference-remaining30-20261112.md`. At `35d1f2b` it closed the disputes
and same-wording groups this report listed at `5408607`, in
`docs/reference-followups-20261112.md` with a ledger in
`reference-followups-20261112.json`. At `54abf45` it gave one positional form to
a single player's library or graveyard in `docs/ordered-zone-audit-20261113.md`.

## Disputed

No face is disputed at this reference.

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

At `d67e6d0` (`docs/reference-random-damage-20261112.md`) and `6f9f2ef`
(`docs/reference-defending-player-20261112.md`):

- Phage the Untouchable Avatar: the created trigger destroys
  `Damage(Recipient)`, not the captured attacker.
- A random return from a graveyard is one `Move` of a `WithSelections{chooser:
  Chance}` selection counting `AsManyAsCan(1)`, with no redundant same-zone
  filter: Charmbreaker Devils, Exalted Flamer of Tzeentch, Fossil Find,
  Ghoulraiser, Nephalia, The Deck of Many Things, Trash Bin, Viconia, Drow
  Apostate and Woodland Sleuth.
- "Defending player" in a spell's target filter is the `PossibleDefender` combat
  role: False Orders, Blaze of Glory and Yare, as Stand or Fall.

At `35d1f2b` (`docs/reference-followups-20261112.md`):

- Cauldron Dance: haste and the delayed sacrifice name the creature put from the
  hand, the payment's `CostObjects{view:Performed, phase:After}`.
- Lieutenants of the Guard: the counter counts `Strength` votes.
- Seizures: the damage goes to the enchanted creature's controller.
- Woodland Bellower: the search filter includes `CardType(Creature)`.
- Grab the Reins shares Hit and Rupture's required sacrifice payment. The
  grammar reads "Sacrifice X. <damage>" as that payment only when the damage
  measures the sacrificed permanent: an implicit actor ("It deals"), which is the
  payment's input, or a possessive that names it. A written actor's bare "its"
  reads that actor, so that sacrifice stays a plain action, as for constant
  damage (Mogg Bombers) or a count (Landslide).
- Surprise Deployment shares the optional hand-entry payment of Ilharg, the
  Raze-Boar, Meek Attack and Planebound Accomplice.
- "Becomes an artifact creature" adds Artifact, then Creature, in every
  template; Start Your Engines matches Armed and Armored.
- Every "starting with you, each player votes" ballot is a
  `ForEachPlayer{players:All, starting_with:AbilityController}` loop, including
  Coercive Portal, Messenger Jays and Orchard Elemental. The grammar reads word
  and color ballots compositionally.
- Spirit Flare lost its invented target exclusion; Uchuulon keeps its exact
  payment selection.
- A cast history is `Compare(EventHistory(Spell Cast ...))`; see the threshold
  group below.
- A proven one-card receipt is tested with `ObjectMatches(Result{phase,
  index:0}, filter)` and measured with `ObjectProperty(Result{phase, index:0})`.
  The grammar lowers the counted spelling at binding and restores it for the
  printer. Satya keeps its batch; see below.
- In the Darkness Bind Them untaps the announced targets, as Hideous Taskmaster
  does.
- "This card" after its own exile keeps distinct action, cost and event
  namespaces; no change.
- A life margin names `StartingLife` before the constant it adds.
- A pure two-branch prevention names the recipient branch before the source
  branch.
- Forgeborn Phoenix and Hook Horror apply `CreatePerpetualEffect` to the guarded
  `EventDestination` in the graveyard, as Consumed by History does.

At `54abf45` (`docs/ordered-zone-audit-20261113.md`):

- A set read from one end of one player's library or graveyard is
  `Take{objects:Zone, count, from:Top|Bottom}`. `LibraryTops(One(player))` and a
  positional `One(Query(Zone, 0))` become that form; the grammar lowers the
  second at binding. A scalar top card is `Query{objects:Zone, index:0}`.
- "The top creature card" filters the zone before taking (Corpse Dance); "the top
  card, if it's a creature card" takes and then tests (Guiding Spirit). Bottom
  keeps `Bottom` (Phyrexian Furnace).
- `LibraryTops(All)` and `LibraryTops(OpponentsOf(...))` stay one card per
  library.
- Wand of Denial tests and moves its `Observed` look receipt.

## Same wording encoded more than one way

The reference encodes the same wording in more than one way, and upstream kept
every encoding below. Groups use `####` headings and faces appear only in
bullets, so these entries never excuse a lost match. Faces the grammar scores as
different are marked *(different)*. No group is open at this reference.

## Updated locally

Export updates in [`updates.json`](updates.json), by id. Each names its faces,
the IR path, the value it replaces and why.

- `U0001` (normalization): a count of spells cast is tested `>= 1`, as every other
  event history count is. Dream Thief ("draw a card if you've cast another blue
  spell this turn"), Hotheaded Giant, I Bask in Your Silent Awe, Laboratory
  Drudge and Spider-Man 2099 tested `> 0`; Patrician's Scorn, Soul Reap and
  Talara's Battalion already read Dream Thief's wording with `>= 1`.
- `U0002` (normalization): Bone Devourer and Necropotence Avatar capture their
  explicitly defined counter count once, then draw and lose life using the same
  `BoundValue`. This matches Parting Thoughts and Savanti Romero, Time's Exile.
  Necropotence Avatar captures after placing its death counter. The update keeps
  the original counter kinds, object identities, last-known information and action
  result IDs; the AST schema stays unchanged. See the
  [grammar work notes](../../docs/grammar-simplification-20261113.md).

## Open in the grammar

These faces differ because the grammar reads them differently; upstream's form
stands. They have no `###` heading.

- Satya, Aetherflux Genius: "sacrifice that token unless you pay an amount of
  {E} equal to its mana value" sums over every token created, because a doubling
  replacement can create several. The grammar reads "its" as `Result{index:0}`;
  reading the whole batch needs a batch reference for "its" with its own printing
  inverse.
