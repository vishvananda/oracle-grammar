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
  printer. Created-token properties keep their whole batch, including Satya.
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
- `U0003` (normalization): thirteen scalar Suspect operations become
  `KeywordEach{action:Suspect, objects:One(object)}`, matching the seven existing
  set-form operations. Up-to-one targets use their whole `Targets(slot)`
  population. The actor, source/holder references and moved-object receipts stay
  intact. The owned canonicalizer applies the same normalization to legacy IR,
  with typed ID provenance; the wire schema is unchanged. The update names every
  affected face and exact operation path.
  Case of the Stashed Skeleton already used `Created(result)` in the reference;
  the grammar removes an incorrect competing first-token reading without
  changing that reference. `One(Result{index:0})` is not a batch equality alias.


- `U0014` (normalization): The Ringhart Crest's `Either` of two `CastSpell`
  predicates becomes one `CastSpell` over `AnyOf` spell filters, matching the
  existing form on Volshe Tideturner. Preserve chosen creature type, legendary
  creature restriction and all object identities. The owned canonicalizer
  merges only nonempty alternatives consisting entirely of casting predicates;
  mixed payment actions and ability source domains remain distinct. No AST
  schema change or semantic correction is introduced.

- `U0015` (correction, issue #4): nine damage-prevention events retain their
  written “to” versus “by” direction; the matching seeded grammar meanings are
  corrected too. Eight creatures are recipients, while Barbed Wire is a source.
- `U0016` (correction, issue #5): Gauntlets of Chaos destroys only Auras attached
  to either exchanged permanent; the Aura restriction surrounds the full
  attachment disjunction.
- `U0017` (correction, issue #6): Mercenaries' shield matches its source creature
  and the activating player, preserving both identities and the next-time limit.

- `U0018` (correction, issue #8): seventeen “your next untap step” effects fix
  both their step predicate and expiration to AbilityController, preserving the
  affected object rather than following its later controller.
- `U0019` (normalization): five live land populations already constrained to
  AbilityController use that stable role explicitly in their fixed-player untap
  rule. A bounded typed proof preserves the equivalent legacy predicate; fixed,
  snapshot, unfiltered and controller-following contexts are excluded.

- `U0020` (correction, issue #7): Juxtapose performs its independent artifact
  exchange after its creature exchange, retaining each controller's tie choices.
- `U0021` (correction, issue #10): Vexing Arcanix guards both name branches by
  an actual reveal and moves the observed card; naming remains outside the guard.
- `U0022` (correction, issue #14): Maelstrom Archangel Avatar queries distinct
  damage-time creature sources with historical controller filtering; it retains
  mana-cost matching and the ThisTurn window.
- `U0023` (correction): that Avatar's two positive chosen-copy type checks remain
  independent; absence of a chosen copy cannot enter an unconditional cast else.
- `U0024` (correction, issue #12): Drop of Honey and Porphyry Nodes choose a
  destructible member of the full least-power pool; indestructibility does not
  change the minimum. Imperial Edict's independent choice remains unrestricted.
- `U0025` (correction, issue #13): Frankenstein's Monster performs a partial
  exile effect and checks actual recorded exile arrivals for completion and
  creature-card counter counts. Real costs retain their payment contracts.
- `U0026` (correction, issue #9): Thermokarst and Extinguish the Light test the
  original target even when destruction fails, independently of its receipt.
- `U0027` (correction, issue #15): Wand of Ith tests observed nonlands in its
  second positive reveal-count condition; neither branch applies to no reveal.
- `U0028` (correction, issue #16): three all-player shuffle effects bind each
  drawing player to their own Before movement receipt in a separate draw phase.
  Single-player and distinct mill-recipient controls keep their original counts.
- `U0029` (correction, issue #17): Total War tests the current combat's
  nonattackers while preserving the separate continuous-control turn exemption.
- `U0030` (correction, issue #18): Shapeshifter's intrinsic chosen-number
  definition is a CDA with SetBasePowerToughness and its symbolic subtraction.
  Ordinary, granted and conditional setters keep their context classification.

- `U0031` (correction, issue #11): Arboria tests the putting actor with the named
  player's most recent completed turn, using schema-191 LastTurnOf. The cast
  alternative and controller-based entry forms remain distinct. Preparation
  explicitly migrates the allowed schema-190 reference before amendments.

- `U0032` (correction): Octomancer's gift creates one 8/8 blue Octopus, as its
  original Oracle reminder and CR702.174i specify. The old lowering reused the
  article's count as both power and toughness, incorrectly making a 1/1. Only
  those stat operands change; the recipient and separate end-step ability stay
  intact. This is an AST operand correction, with no schema change or equality
  normalization. See [Gift branches](../../docs/gift-branches.md).

- `U0033` (normalization): 28 own-spell Source.SpellX operands on 21 allowed
  faces now use AnnouncedX, consistently with the same spells' other amounts
  and filters. Casting and copied-spell X denote that same choice (CR107.3a,
  CR707.10). Thresholds, branches, actors, targets and result identities remain
  intact. Typed frontend lowering excludes independent abilities, costs,
  deferred programs, other objects, captured information and historical X.
  The owned AST schema and reference lock are unchanged. See
  [numeric thresholds](../../docs/spell-x-thresholds.md) for the exact scope.

- `U0037` (correction): Death Charmer's "that creature's controller loses 2 life
  unless they pay {2}" makes the damaged creature's controller lose the life, the
  same player who may pay. The old lowering named the ability's controller. Only
  the LoseLife player changes.

- `U0038` (correction): Snowfall's snow branch lets the Island's controller add
  one or two additional {U}. Declining the optional "instead" keeps the first
  option (CR614.1a, 614.15, 616.1), so the snow body now chooses between {U} and
  {U}{U}. See #25.

- `U0039` (correction): Aladdin's Lamp leaves the card it keeps in place. The old
  lowering added an unprinted ReorderLibrary(Top) of that card. The random bottom
  order now excludes the selected card directly. See #26.

- `U0040` (correction): Bison Whistle offers one four-option choice for any
  looked-at card: Bison to the battlefield, creature revealed to hand, any card
  to the graveyard, or nothing. The old lowering offered the graveyard only when
  the card was neither a Bison nor a creature. See #27 and i5jb/mtgish#169.

- `U0041` (correction): Scholarship Sponsor and Enigma Ridges subtract each
  searching player's lands from the captured maximum. The old lowering
  subtracted in reverse. See #28.

- `U0042` (correction): Boreas Charger's difference counts the chosen
  opponent's lands minus yours, not hand sizes. See #28.

- `U0043` (correction): Heroism offers each attacking red creature's {2}{R}
  as the ability resolves (ruling 2004-10-04) and shields that same creature
  unless it was paid. The old lowering asked for the payment inside one shield
  on any attacking red creature, each time damage would be dealt. The payment
  order follows the ForEachObject choice-order contract. See #23.

- `U0044` (withdrawn): Infernal Denizen sacrificed as many Swamps as it could, up
  to two, before testing "if you can't". CR 118.12 makes "[do something]. If
  [a player] can't" a cost paid as the ability resolves, and CR 118.3 forbids
  paying part of it, so the pinned two-Swamp `PayCost` and its `PaymentChosen`
  test were right. The record is removed. See #24.

- `U0045` (correction): "tokens created with" a permanent, and "the token" it
  created, intersect Token with MadeBy on Arcane Artisan, Dance of Many, Dual
  Nature, Saproling Burst, Tetravus and Tombstone Stairwell. MadeBy alone also
  admits a nontoken put onto the battlefield by one of the permanent's
  abilities. The link to the printed creating ability (CR 607.2c) is still
  open. See #20.

- `U0046` (correction): inside `PlayersDo`, the acting player is
  `PlayerRef::Acting` and `AbilityController` stays the ability's controller, as
  in `EachPlayer`. The old records named the actor `AbilityController` on
  Cloudhoof Kirin, Infernal Denizen, Ingenious Mastery, Natural Selection,
  Portent, Stardew Valley, Thought Prison and Visions. Protection Racket already
  used the original controller for "your library". Eleven references change.
  Infernal Denizen's eligible creatures stay controlled by the original
  controller, and the controller still chooses from Thought Prison's revealed
  hand. See #24.

- `U0047` (correction): Drach'Nyen's X is the sum of the current power of every card
  exiled with it (CR 607.3), as U0008 reads Phyrexian Ingester. It says "the exiled
  card", not "creature card", so the population keeps an exiled Vehicle, whose printed
  power still counts (CR 208.3), and adds no Creature conjunct. The old record read
  Query index zero of an unordered exile population. See #30.

- `U0048` (normalization): Protection Racket's payer, "that player" inside its
  `PlayersDo` over each opponent, is `PlayerRef::Acting`, not `Prior(That)`. The
  revealed card still comes from the controller's library. See #24.

- `U0049` (normalization): "you may have target player [act]" is the
  controller's choice: `Instruction::Optional` with decider `AbilityController`
  around the target player's action, as on twenty-one other faces such as Dire
  Undercurrents. Cloudhoof Kirin, Natural Selection, Portent, Thought Prison and
  Visions used `PlayersDo{optional: true}` instead, which left the decider
  unstated. `PlayersDo`'s `optional` is now only the acting players' own choice,
  as in Infernal Denizen's "an opponent may". See #24.

- `U0050` (normalization): Frankenstein's Monster and Mimeoplasm, Revered One
  choose the creature cards to exile with `ChooseObjects` and move `Selected`,
  as fifty-five other graveyard exiles do, instead of `WithSelections`. The
  number actually exiled is still read from the move's receipt. See #29.

- `U0051` (normalization): Ignite Memories' "Target player reveals a card at
  random from their hand" is `RevealFromHand{at_random: true}`, and "that
  card's mana value" reads the revealed card as `Observed`, as eighteen other
  random hand reveals do, among them Planeswalker's Fury and Planeswalker's
  Mirth with the same following sentence. It was a mandatory
  `PayCost{Reveal{random: true}}` followed by `If PaymentChosen`, though the
  text has no cost and no "if you do".

- `U0052` (normalization): Rotfeaster Maggot's "that card's toughness" reads
  the exiled target, `ObjectProperty{object: Target}`, as Surge to Victory,
  Healing Technique and four other "target card … that card's" records do. It
  was `Earlier{what: Exiled, result: null}`, which names no exile.

- `U0053` (correction): Hesitation's "When a player casts a spell" triggers on
  every player's spell, `players: All`, as 115 other "a player casts" records
  read, Standstill among them. It was `One(AbilityController)`.

- `U0054` (normalization): Voltage Surge and Undergrowth, which offer an
  optional additional cost and then ask whether it was paid, are one
  `CostBranch{keyword: AdditionalCost}` whose paid spell is the `If`'s then
  branch and whose unpaid spell is its otherwise branch, as fourteen other such
  spells are recorded, Burning Curiosity and Osseous Exhale among them. They
  were a separate optional `Casting` cost and a `Spell` testing
  `CostPaid(Additional)`. Graven Archfiend and Katara, Seeking Revenge are
  permanents whose later abilities test the cost, and stay unchanged.

- `U0055` (correction): Chameleon Spirit's "power and toughness are each
  equal to the number of permanents of the chosen color your opponents
  control" is a characteristic-defining static (CR 604.3), as the six other
  creatures whose power or toughness counts a value chosen as they enter are
  recorded, Entropic Specter and Pallimud among them. It was `Ordinary`.

- `U0056` (normalization): "If you've cast another [quality] spell this turn",
  tested while this spell is cast, excludes this spell with
  `Not(SameObject(Source))` in Gigastorm Titan, Mage Duel, Patrician's Scorn
  and Rally the Monastery, as nine other records read it, among them the
  casting restrictions of Hewed Stone Retainers and Talara's Battalion. They
  recorded the bare spell filter. Sage of the Skies excludes its triggering
  spell from a cast trigger and stays unchanged.

- `U0057` (correction): Chief Magistrate of Mercadia's "for each creature
  token you control, create a token that's a copy of it" copies creature
  tokens, `AllOf([Creature, Token, ControlledBy(You)])`, as Rhys the Redeemed
  records the same words. The `Token` filter was missing.

- `U0058` (correction): in "you may have Kimahri become a copy of that
  creature", Kimahri is the affected object. The record made the tapped target
  a copy of itself.

- `U0059` (correction): Renegade Doppelganger's copy has no exception. Its
  reminder says it loses this ability (CR 707.2); the record kept it with
  `AddCurrentAbility`, which other records write "except it has this ability".

- `U0060` (normalization): a choice among cards in exile names the exile pool,
  `Zone(Shared(Exile))` with its filter, as 33 records write it (Ethereal
  Forager, Koh, the Face Stealer). 18 records, among them Bane Alley Broker and
  Gustha's Scepter, wrapped the same pool in `InZone(..., Exile)`, which chooses
  among the same cards. Moves of exiled cards keep `InZone`, as 73 records
  write them.

- `U0061` (correction): Kenessos, Priest of Thassa and Lantern of Revealing say
  "you may put it on the bottom of your library" for a card not put onto the
  battlefield. Both records made that move mandatory; Bucolic Ranch records the
  same words as an optional move.

- `U0062` (correction): Fecund Greenshell's trigger needs another creature
  "you control". The record dropped the controller from the entering
  creature's filter; Risen Reef records the same words with `ControlledBy`.

- `U0063` (normalization): Nissa's Revelation reads the revealed card's power
  and toughness from that reveal, `ObjectProperty(Query{Observed}, ...)`, as 36
  records read revealed-card values. The record used the result-less
  `Earlier{what:Revealed}` placeholder.

- `U0064` (correction): Disruptor Flute's "Spells with the chosen name cost
  {3} more to cast" names no caster. The record taxed only its controller's
  spells; Gloom records the same unqualified wording for every player, and
  Anointed Peacekeeper adds a caster only where the words name one.

- `U0065` (correction): Kindred Charge copies each creature "you control" of
  the chosen type. The record dropped the controller and copied every
  player's creatures; Rhys the Redeemed, Second Harvest and Saheeli, the
  Gifted record the same words with `ControlledBy`.

- `U0066` (normalization): Molten Echoes and Mordor on the March grant haste
  to, and exile, the copy token as the creation's batch, `Created(...)`, as
  Flameshadow Conjuring records Molten Echoes' exact follow-up and Felhide
  Spiritbinder, Cadric, Soul Kindler and Séance record "It gains haste" and
  "Exile it". The records named only the creation's first object.

- `U0067` (normalization): Lazotep Quarry, Nahiri, the Unforgiving and The
  Scarab God exile a target card and create "a copy of it". The records
  copied the exile's result; Restore Relic, The Cloning of Shredder and
  Abyssal Harvester copy the exiled `Target` for the same words. "A copy of
  that card" keeps the exile receipt (Anikthea, Hand of Erebos, Tamiyo,
  Compleated Sage).

- `U0068` (normalization): the five Alchemy Gates (Gate to Manorborn and its
  cycle) print "{T}: Add {G}." on its own line, outside reminder
  parentheses. The records dropped that printed ability; Abandoned Air
  Temple and Ba Sing Se record printed mana lines as Activated abilities.
  This preserves wording: the basic land types already derive these intrinsic
  abilities. The card-context comparison and consumer projection in the
  [2026-11-16 contract](../../docs/reported-issues-20261116.md#intrinsic-basic-land-mana-32)
  prevent a printed manifestation from becoming a second activation.

- `U0069` (correction, #31): Mimeoplasm's recorded creature arrivals must also
  have arrived in Exile. The result identity, After view and multiplier remain.
- `U0070` (normalization, #33): five redundant live Exile-query wrappers on
  Gustha's Scepter, Northampton Farm, Vault 13 and Ugin's Labyrinth are removed
  with the exact same-zone proof. Historical arrival guards remain distinct.
- `U0071` (normalization, #33): Chandra retains its already-proved singleton
  result without a vacuous filter; 49 immediate branch moves on 32 explicitly
  named faces, including Esper Maduin, use their own reveal observations.
- `U0072` (correction, #19): Naked Singularity and Reality Twist each keep one
  replacement identity and choose among applicable outputs for the full amount.
- `U0073` (correction, #20): Tetravus keeps Token and uses the creating ability's
  linked text/acquisition, rather than every creation by the source object.
  The same explicit creation links cover Arcane Artisan, Dual Nature, Saproling
  Burst, Tombstone Stairwell and Faerie Artisans; Faerie excludes its whole
  current creation batch.
- `U0074` (correction, #21): Krovikan Vampire tests pre-death views and projects
  their first surviving graveyard incarnations when its query resolves.
- `U0075` (correction, #22): Wood Elemental counts linked historical entry
  sacrifices through its current words, preserving the entering incarnation.

- `U0076` (normalization, #33): Inscribed Tablet, Kaervek's Purge, Kalitas and
  Saw in Half use the existing singleton receipt test only after their own
  at-most-one proof. Phases, IDs, negation and later effects are preserved.
- `U0077` (correction): Lizard, Connors's Curse's "loses all abilities and
  becomes a green Lizard creature with base power and toughness 4/4" replaces
  card types with `SetCardTypes([Creature])` under CR 205.1a, matching Oko,
  Thief of Crowns; the record used `AddCardType(Creature)`.

These contracts and their scoped proofs are documented in
[the 2026-11-16 issue audit](../../docs/reported-issues-20261116.md).

The remaining reported reference/lowering issues and their current dispositions
are tracked in [the issue audit](../../docs/reported-issues-20261113.md) and
its follow-ups of [2026-11-14](../../docs/reported-issues-20261114.md) and
[2026-11-15](../../docs/reported-issues-20261115.md) and
[2026-11-16](../../docs/reported-issues-20261116.md). A report's
open status does not create a corpus equality exception.

## Open in the grammar

No face differs in the explicitly allowed population at this checkpoint.

Satya, Aetherflux Genius now reads "its mana value" as the sum over `Created`,
matching the retained reference. A request to create one token can produce
several after replacements. The grammar supplies the batch property directly,
with a printing inverse; it rejects a competing `ObjectProperty(Result{index:0})`
reading. No reference update or AST schema change was needed for this fix.

### U0078: Preserve the triggering or targeted player identity rather than substituting AbilityController.
- Faces (6): Bamboozle, Blinkmoth Urn, Cheering Crowd, Cruel Reality, Paradox Haze, Sleep
- Evidence: Bamboozle puts the rest on top of 'their [target player's] library', not AbilityController's; Blinkmoth Urn triggers at 'each player's first main phase', not only AbilityController's; Cheering Crowd, Cruel Reality, and Paradox Haze act on the triggering player (EventSubject), not AbilityController; Sleep skips untapping during 'that player's next untap step' (Target(0,0)), not ControllerOf(Affected).

### U0079: Preserve player-bound and graveyard-owner scopes on 'their', 'your', and 'each other player' references.
- Faces (8): Anya, Merciless Angel, Damping Engine, Endless Detour, Lord of the Forsaken, The Council of Four, Thoughtbound Primoc, Underworld Cerberus, Wild Mammoth
- Evidence: Anya compares against each bound opponent's starting life total (Bound(0), not Acting); Endless Detour targets any 'card in a graveyard', not only AbilityController's graveyard; Rootcoil Creeper and Lord of the Forsaken restrict mana to spells cast 'from your graveyard' (One(AbilityController), not All); Underworld Cerberus and Grave Sifter return cards from each bound player's own graveyard (One(Bound(0)), not All); The Council of Four checks a player's second card 'during their turn' (One(Active), not All); Thoughtbound Primoc, Wild Mammoth, and Damping Engine compare against 'each other player' (OthersThan(Bound(0)), not All).

### U0080: Preserve explicit chooser and controller clauses on library placement and battlefield entry.
- Faces (3): Hinder, Not Forgotten, The Cyber-Controller
- Evidence: Hinder and Not Forgotten specify 'your choice of the top or bottom of its owner's library' (AbilityController chooses, not Owners); The Cyber-Controller puts milled creature cards onto the battlefield 'under your control' (Player(AbilityController), not Owner).

### U0081: Countering 'all abilities' targets stack objects of kind Ability, not Spell.
- Faces (1): Kadena's Silencer
- Evidence: Summary Dismissal says 'counter all abilities' and Kadena's Silencer says 'counter all abilities your opponents control'; MTGish exported object kind Spell instead of Ability on both.

### U0082: Preserve swapped or misbound object, attachment, and result references.
- Faces (6): Flames of Moradin, Karona, False God Avatar, Keen Duelist, Oko, the Trickster, Technomancer, Three Tree Battalion
- Evidence: Oko, the Trickster prevents damage dealt 'to him' (Source, not Target(0,0)); Ogre Geargrabber attaches Target(0,0) to Source (MTGish swapped attachments and host); Karona, False God Avatar exchanges Result(0,0) with Result(1,0) (MTGish referenced Result(0,1)); Keen Duelist loses life equal to the other player's revealed card (Recorded(0), not Recorded(1)); Three Tree Battalion perpetually modifies the conjured duplicate (result 3, not result 2); Flames of Moradin duplicates nontoken artifacts destroyed by result 0 (Matching(Transition(0, Before)), not all Battlefield artifacts); Technomancer chooses artifact creature cards 'from your graveyard' (Zone(Graveyard), not only the 3 milled cards in Recorded(0)).

### U0083: Preserve comparison operators, aggregations, and boolean connectives (NotEqual, Less, Maximum, AnyOf, Not).
- Faces (7): Decree of Annihilation, Jackdaw Savior, Katsumasa, the Animator, Mentor's Guidance, Skull Raid, Slime Against Humanity, Squall, Gunblade Duelist
- Evidence: Iridian Maelstrom checks 'isn't all colors' (NotEqual, not Equal); Skull Raid ('fewer than two') and Jackdaw Savior ('lesser mana value') test strict Less, not LessOrEqual; Timely Reinforcements ('fewer creatures than an opponent') compares against Maximum opponent creature count, not Minimum; Decree of Annihilation ('artifacts, creatures, and lands'), Squall ('power or toughness equal'), Mentor's Guidance ('planeswalker, Cleric, Druid, Shaman, Warlock, or Wizard'), and Slime Against Humanity ('Oozes or named Slime Against Humanity') are disjunctions (AnyOf), not conjunctions (AllOf); Katsumasa checks 'if it's not a Vehicle' (Not(Subtype(Vehicle)), not Subtype(Vehicle)).

### U0084: Preserve explicit face-down visibility, secret choice, play-permission terms, choice costs, and optional wrappers.
- Faces (8): Cunning Azurescale, Divining Dive, Elder Brain, Expensive Taste, Sheoldred's Assimilator, Torment of Hailfire, Wheel of Misfortune, Whispersteel Dagger
- Evidence: Expensive Taste exiles cards 'face down' (FaceDown, not FaceUp); Cunning Azurescale and Divining Dive 'secretly choose' (secret: true); Wheel of Misfortune restricts to '0 or greater' (options: [Comparison(GreaterOrEqual, 0)]); Whispersteel Dagger has no exile replacement (into_exile: false); Elder Brain lets you spend mana as any color at normal cost (cost: Normal, spend_as: AnyColor, not Free); Torment of Hailfire offers a Choice between sacrificing or discarding, not paying both; Hope-Ender Coatl counters 'unless they pay {1}' (If(Not(PaymentChosen(0)))), not unconditionally; Expedited Inheritance ('may exile') and Sheoldred's Assimilator ('you may conjure') are Optional.

### U0085: Preserve explicit characteristic, subtype, and property checks (BasePower/BaseToughness, Toughness, Spacecraft, CreatureTypes, AddCardType(Creature)).
- Faces (3): Evolving Adaptive, Menagerie Curator, Network Marauder
- Evidence: Angelic Aberration checks 'base power or toughness 1 or less' (BasePower/BaseToughness); Evolving Adaptive compares toughness against this creature's Toughness, not Power; Network Marauder buffs 'Spacecraft cards', not Equipment; Menagerie Curator checks 'share a creature type' with a creature card in your library (SharesWithAny(CreatureTypes), not SharesNameWithAny); Waker of the Wilds animates target land into a '0/0 Elemental creature' (including AddCardType(Creature)).

### U0086: Preserve explicit control, ownership, and CardType(Creature) filter conjuncts.
- Faces (7): Benevolent Blessing, Grand Ossuary, Mirror of the Forebears, Olag, Ludevic's Hubris, Purging Scythe, Tamanoa, Trial of Agony
- Evidence: Grand Ossuary ('creatures they control'), Mirror of the Forebears ('target creature you control'), Tamanoa and Disciples of the Inferno ('noncreature source you control'), Trial of Agony ('controlled by the same opponent'), Benevolent Blessing ('Auras and Equipment you control'), Shaun ('creature you control'), and Rift Elemental ('suspended card you own') omitted or inverted their controller/owner conjunct; Olag, Purging Scythe, High Score, Primal Empathy, and Éomer omitted CardType(Creature); Szat's Will omitted OpponentsOf scope and Creature card filter on exiled cards.

### U0087: Preserve explicit zone, turn-window, event-subject, and self-exclusion constraints.
- Faces (7): Boltbender, Corpse Appraiser, Ever-Watching Threshold, Molten Lavamancer, Nicol Bolas, Dragon-God, Psychic Battle, Seshiro the Anointed Avatar
- Evidence: Harsh Mentor triggers on abilities 'on the battlefield' (InZone(Battlefield)); Corpse Appraiser checks 'put into exile this way' (InZone(Exile)); Seshiro the Anointed Avatar excludes 'the battlefield or the stack' (adding Not(InZone(Stack))); Molten Lavamancer triggers 'during your turn' (Filtered(PlayerIsActive(AbilityController))); Ever-Watching Threshold tests 'if they attacked' (Attack.by: One(EventSubject)); Psychic Battle triggers on any player choosing targets without an unprinted Psychic Battle exclusion; Boltbender ('other spells or abilities'), Nicol Bolas, Dragon-God ('all other planeswalkers'), and Archfiend of Ifnir / Curator of Mysteries / Horror of the Broken Lands ('cycle or discard another card') require self-exclusion.

### U0088: Restore omitted controller, zone, non-self, and target-kind filter predicates
- Faces (13): Ancient Lumberknot, Bedrock Tortoise, Chef's Kiss, Emissary of Grudges, From the Catacombs, Harsh Mentor, Holy Frazzle-Cannon, Noble Banneret, Orzhov Advokist, Penregon Besieged, Tapestry Warden, The Muckslinger Gang, This Is How It Ends
- Evidence: Oracle wording explicitly constrains controller ("you control", "they control", "opponents control"), zone ("from your graveyard", "from a graveyard", "other than the battlefield or the stack", "on the battlefield"), non-self ("another", "other"), or target kind ("permanent or player", "you or a permanent you control"), whereas MTGish gold dropped or inverted those filter terms.

### U0089: Correct inverted AllOf/AnyOf connectives, erroneous Subtype(Saga), and bogus ColorCount filter
- Faces (4): Coveted Falcon, Focus Fire, Herigast, Erupting Nullkite, Providence of Night
- Evidence: Coveted Falcon says "you own but don't control" (conjunction, not disjunction); Focus Fire says "creatures and/or Spacecraft" (disjunction, not conjunction); Herigast says "Each creature spell you cast" (not Saga); Providence of Night says "with hybrid mana in its mana cost" (monocolored hybrid cards like {2/W} qualify without ColorCount > 1).

### U0090: Restore missing Not negation on Fastbond and non-mana activated ability triggers
- Faces (6): Battlemage's Bracers, Fastbond, Harsh Mentor, Illusionist's Bracers, Kurkesh, Onakke Ancient, Rings of Brighthearth
- Evidence: Fastbond says "if it wasn't the first land you played this turn" and Battlemage's Bracers, Harsh Mentor, Illusionist's Bracers, Kurkesh, Onakke Ancient, and Rings of Brighthearth all say "if it isn't a mana ability", whereas MTGish gold omitted Not(...) and encoded the positive predicate.

### U0091: Use strict Less comparison for "fewer than" and "lesser mana value"
- Faces (2): Scrap Trawler, Vraska, Betrayal's Sting
- Evidence: Vraska, Betrayal's Sting says "fewer than nine poison counters" and Scrap Trawler says "with lesser mana value", both strict inequalities (<), whereas MTGish gold encoded LessOrEqual (<=).

### U0092: Correct event-player and villainous-choice actor bindings
- Faces (5): Blood Clock, Fraying Line, Glimmervoid Basin, Infernal Genesis, Overabundance
- Evidence: Glimmervoid Basin, Overabundance, Blood Clock, Fraying Line, and Infernal Genesis explicitly refer to the triggering player ("that player", "they"), not AbilityController; Great Intelligence's Plan says "or you may cast a spell from your hand", assigning decider/actor/hand to AbilityController rather than Target(0).

### U0093: Correct object reference identities in damage, pump, granted upkeep cost, hexproof, exchange, and Coin of Fate
- Faces (5): Coin of Fate, Cultural Exchange, Jilt, Orim's Thunder, Tam, Mindful First-Year
- Evidence: Jilt and Orim's Thunder deal damage from the spell (Source), not Target(0); Ent's Fury pumps Target(0) before fighting Target(1); granted upkeep abilities on Essence Leak and Disruption Aura reference their own holder (Source); Tam grants hexproof from each affected creature's own colors (Affected); Cultural Exchange swaps Selected(0) and Selected(1); Coin of Fate returns Unselected(0).

### U0094: Correct trigger scope, timing conditions, and turn/combat durations
- Faces (5): Canyon Vaulter, Elesh Norn, Mother of Machines, Reckless Velocitaur, Ria Ivor, Bane of Bladehold, Valiant Farewell
- Evidence: Elesh Norn suppresses abilities controlled by opponents for any entering permanent; Canyon Vaulter and Reckless Velocitaur trigger only during your main phase; Tifa checks first combat phase of your turn; Ria Ivor lasts EndOfCombat ("this combat"); Valiant Farewell's delayed trigger expires EndOfTurn ("this turn").

### U0095: Restore optionality, up-to/any-number counts, disjunction cost, and suspend guard
- Faces (6): Expedited Inheritance, Expose the Culprit, Hag of Ceaseless Torment, Heroes' Hangout, Taigam, Master Opportunist, Trade Secrets
- Evidence: Capricopian ("may reselect"), Trade Secrets ("draw up to four"), Worldpurge ("up to seven"), Expose the Culprit ("any number of"), Expedited Inheritance ("may exile"), Hag of Ceaseless Torment ("sacrifices ... or discards"), Taigam ("If it doesn't have suspend"), and Heroes' Hangout (unconditional play permission without spurious empty PayCost) all directly contradict the MTGish export.

### U0096: Correct Opal-Eye RedirectDamage, Arvinox AnyColor spend_as, and Soundwave Legendary tokens
- Faces (3): Arvinox, the Mind Flail, Opal-Eye, Konda's Yojimbo, Soundwave, Superior Captain
- Evidence: Opal-Eye redirects damage to itself rather than preventing it; Arvinox grants "spend mana as though it were mana of any color"; Soundwave explicitly creates legendary Ravage and legendary Laserbeak tokens.

### U0097: Restore omitted Not negation on Dose of Dawnglow, Wilhelt, Desynchronization, and undamaged hexproof guards
- Faces (5): Desynchronization, Dose of Dawnglow, Karakyk Guardian, Oyaminartok, Polar Werebear, Palladia-Mors, the Ruiner
- Evidence: Dose of Dawnglow ("if it isn't your main phase"), Wilhelt ("if it didn't have decayed"), Desynchronization ("not historic"), and Karakyk Guardian/Oyaminartok/Palladia-Mors ("as long as it hasn't dealt damage yet") are all negated in Oracle text, whereas MTGish gold omitted Not(...).

### U0098: Restore omitted filter predicates (Modified, non-self, Land, Creature, ControlledBy, FaceDown, and Extreme population)
- Faces (8): Dispersal, Lethal Throwdown, Might Makes Right, Monument to Perfection, Spy Network, The Peregrine Dynamo, Tidus, Yuna's Guardian, Zinnia, Valley's Voice
- Evidence: Oracle text explicitly requires modified (Lethal Throwdown), non-self/second target (The Peregrine Dynamo, Tidus), Land/Creature type (Monument to Perfection, Zinnia, Might Makes Right), ControlledBy(You) (Minas Tirith Garrison, Orphans of the Wheat), FaceDown (Spy Network), and among all permanents they control (Dispersal).

### U0099: Fix swapped Repeat.times and Incubate.amount on The Argent Etchings and Glistening Dawn
- Faces (1): The Argent Etchings
- Evidence: The Argent Etchings says "Incubate 2 five times" (times=5, amount=2) and Glistening Dawn says "Incubate X twice, where X is the number of lands you control" (times=2, amount=land count), whereas MTGish swapped the two operands.

### U0100: Correct player scope and controller bindings (Kynaios, Okaun, Zndrsplt, Otaria, Death in Heaven, Reality Shift, Barroom Brawl, Jhoira)
- Faces (7): Barroom Brawl, Jhoira, Weatherlight Corsair, Kynaios and Tiro of Meletis, Okaun, Eye of Chaos, Otaria, Reality Shift, Zndrsplt, Eye of Wisdom
- Evidence: Oracle text specifies opponents (Kynaios), any player (Okaun, Zndrsplt, Otaria), under your control (Death in Heaven), target controller's library (Reality Shift), opponent to your left (Barroom Brawl), and You lose life (Jhoira).

### U0101: Restore activation timing restrictions and up-to-one mode minimum 0
- Faces (1): Hama, the Bloodbender
- Evidence: Blighted Nightmare says "Activate only as a sorcery"; Katara says "Activate only during your turn"; Hama says "during your turn"; Ravager Wurm and Dreamshackle Geist say "choose up to one" (minimum 0, not 1).

### U0102: Correct unless-cost disjunctions, Emissary's Ploy choice 2, and Michelangelo OneForEach constraint
- Faces (4): Emissary's Ploy, Michelangelo, Improviser, Perforating Artist, Polygraph Orb
- Evidence: Perforating Artist and Polygraph Orb offer an OR choice between sacrificing and discarding; Emissary's Ploy says "choose 1, 2, or 3" (gold omitted 2); Michelangelo says "a creature card and/or a land card" (at most one of each, not two lands or two creatures).

### U0103: Fix otherwise branch scoping, suspend guard, and missing player protection on Faith's Shield
- Faces (3): Amy's Home, Bre of Clan Stoutarm, Guidelight Pathmaker
- Evidence: Amy's Home checks "If it doesn't have suspend"; Bre of Clan Stoutarm and Guidelight Pathmaker say "Otherwise, put it into your hand"; Faith's Shield grants protection to "you and each permanent you control" at 5 or less life.

### U0104: Fix Volo CreatureTypes check, Volcano Hellion unpreventable damage, Coram land+spell permissions, and Sleuth Instructor single trigger
- Faces (4): Coram, the Undertaker, Sleuth Instructor, Volcano Hellion, Volo, Guide to Monsters
- Evidence: Volo tests creature types rather than colors; Volcano Hellion says "The damage can't be prevented"; Coram permits both playing a land AND casting a spell each turn; Sleuth Instructor is one ability with "This ability triggers only once."

