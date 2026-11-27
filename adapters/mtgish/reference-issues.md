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

- `U0218` gives Lagoon Breach the existing `OthersThan` controller set, while
  the target's owner still chooses where to put it in the library.
- `U0217` uses `GiveControl` for four faces with the same choice of one
  eligible opponent during resolution (CR 608.2d).
- `U0216` encodes the third library position on Shattered Ego as `Constant(3)`.
- `U0215` preserves Put Away's resolution choice to shuffle after choosing a
  legal graveyard target (CR 601.2c, 608.2d).
- `U0214` makes Selfless Exorcist determine both the damage source and its
  power from the actual public exile result (CR 400.7j, 608.2h).
- `U0213` makes Redemptor Dreadnought test for a card still in this source's
  linked exile population (CR 603.4, 607.2).
- `U0212` adds Creature on Roaring Earth and Waker of the Wilds while
  retaining Land. Arlinn, the Moon's Fury replaces her card and creature
  types with the written Werewolf creature form (CR 205.1a–b).
- `U0211` makes the seven spell-or-permanent target filters distinguish spells
  from abilities on the stack. Jeskai Revelation and Sink into Stupor use one
  union target alongside any independently written targets (CR 110.1, 112.1,
  113.1, 601.2b–c).
- `U0207` keeps the end-of-combat delayed triggers on Altaïr Ibn-La'Ahad,
  Fortune, Loyal Steed, Kavaron Harrier and Mordor Trebuchet pending if the
  end-of-combat step is skipped (CR 511.2, 603.7b, 725.2d–e).
- `U0208` makes Forth Eorlingas! match one or more simultaneous combat-damage
  recipients as one occurrence (CR 603.2c).
- `U0209` makes the two Cityscape Leveler triggers give the Powerstone to the
  targeted permanent's controller. The instruction still applies when the
  permanent is indestructible (CR 101.3, 608.2b–c, 702.12).
- `U0210` makes Aleatory schedule its draw after either outcome of the coin
  flip. Its separate printed paragraph is independent of the conditional pump
  (CR 608.2c).

- `U0202` requires the targets on Perpetual Timepiece and The Bath Song to
  be in the ability controller’s graveyard.

- `U0199` preserves Demolition Field’s controller search when the targeted land
  survives. The reference reads the controller of the targeted land, including
  last-known information after a successful destruction.
- `U0200` uses the original battlefield name for Splinter’s target artifact. A
  card that was a copy may have a different name in exile.
- `U0201` retains one written trigger with a shared body on Ambuscade Shaman and
  Ardoz, Cobbler of War. Both entry events retain the entering creature as
  their subject.

- `U0198` preserves the actual exile receipt and the written return controller
  for five optional exile-and-return abilities.

Export updates in [`updates.json`](updates.json), by id. Each names its faces,
the IR path, the value it replaces and why.

- `U0189` corrects Break Expectations. The draft and reveal depend on the
  selected card arriving in exile. The reveal still uses the draft's receipt.
  See [issue #37](https://github.com/vishvananda/oracle-grammar/issues/37).
- `U0190` corrects the inclusive number choices on Shapeshifter, By Invitation
  Only, Expel the Interlopers, Liquid Fire, Talion, the Kindly Lord, and Yusri,
  Fortune's Flame. Both of Shapeshifter's choices offer 0 through 7, with the
  optional upkeep choice and characteristic-defining formula intact.
  See [issue #38](https://github.com/vishvananda/oracle-grammar/issues/38).
- `U0191` restores the source exclusion on the 13 permanents with Support in
  the allowed cohort. This includes three noncreature permanents, which still
  exclude themselves if they become creatures. Instant and sorcery Support
  keeps its ordinary creature target. [Wizards' Support rules](https://magic.wizards.com/en/news/feature/modern-horizons-3-mechanics#support)
- `U0192` restores the explicit `Starting intensity 0` declaration on seven
  Alchemy faces. Minthara of the Absolute also places its static pump before
  its trigger, in printed order. Audits of the amended export use those
  printed declarations in their Oracle input. [Wizards' intensity description](https://magic.wizards.com/en/news/mtg-arena/mtg-arena-state-game-alchemy-kamigawa-2022-03-16#newmechanicintensity)
- `U0193` sets Earwig Squad's unqualified search for three cards to
  `AsManyAsCan(3)`. A library with at least three cards supplies all three.
  A shorter library supplies all available cards. [Wizards' search ruling](https://magic.wizards.com/en/news/feature/spider-man-release-notes)

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

### U0105: Preserve 'for as long as you control this creature' controller condition instead of WhileOnBattlefield
- Faces (3): Aegis Angel, Kyodai, Soul of Kamigawa, Thancred Waters
- Evidence: Oracle text for Aegis Angel, Kyodai, Soul of Kamigawa, and Thancred Waters states 'gains indestructible for as long as you control [self]'; frozen gold emitted WhileOnBattlefield(Source) without the controller check.

### U0106: Execute 'Tap target creature.' unconditionally before metalcraft exile on Dispatch
- Faces (1): Dispatch
- Evidence: Dispatch reads 'Tap target creature. If you control three or more artifacts, exile that creature.'; frozen gold placed SetTapped only in the otherwise branch, failing to tap before exiling when metalcraft is active.

### U0107: Include 'that player controls' on Mordant Dragon's combat-damage trigger target
- Faces (1): Mordant Dragon
- Evidence: Mordant Dragon reads 'deal that much damage to target creature that player controls.'; frozen gold omitted ControlledBy(EventSubject), permitting targeting any creature.

### U0108: Use Affected exclusion and Creature blocker filter on Nephilim Epochal
- Faces (1): Nephilim Epochal
- Evidence: Nephilim Epochal reads 'Nephilim you control get +1/+1 for each other Nephilim you control and can't be blocked by monocolored creatures.'; frozen gold excluded Source instead of Affected and omitted CardType(Creature) on the blocker filter.

### U0109: Normalize 'third from the top' library position to Constant(3) on Teferi, Hero of Dominaria
- Faces (1): Teferi, Hero of Dominaria
- Evidence: Teferi's [-3] reads 'Put target nonland permanent into its owner's library third from the top.'; frozen gold emitted unsimplified Add(Constant(2), Constant(1)) instead of Constant(3).

### U0110: Fix Sarulf, Realm Eater mana value comparison from Less to LessOrEqual
- Faces (1): Sarulf, Realm Eater
- Evidence: Sarulf, Realm Eater says "exile each other nonland permanent with mana value less than or equal to the number of counters removed this way" (Comparison::LessOrEqual), whereas MTGish exported Comparison::Less.

### U0111: Restore AddCurrentAbility copy exception on Court of Vantress
- Faces (1): Court of Vantress
- Evidence: Court of Vantress says "you may have this enchantment become a copy of it, except it has this ability" (CopyException::AddCurrentAbility), whereas MTGish exported an empty exceptions list.

### U0112: Restore SharesNameWith predicate on Legion's End battlefield exile clause
- Faces (1): Legion's End
- Evidence: Legion's End says "all other creatures that player controls with the same name as that creature", whereas MTGish omitted SharesNameWith(Target(0, 0)) from the battlefield permanent filter.

### U0113: Fix swapped Repeat.times and Incubate.amount on Progenitor Exarch
- Faces (1): Progenitor Exarch
- Evidence: Progenitor Exarch says "When this creature enters, incubate 3 X times" (times=CastX(EventSubject), amount=Constant(3)), whereas MTGish swapped the two operands (same upstream bug fixed on The Argent Etchings and Glistening Dawn in U0099).

### U0114: Restore CardType(Creature) on Malik, Grim Manipulator secret choice pool
- Faces (1): Malik, Grim Manipulator
- Evidence: Malik, Grim Manipulator says "you and target opponent each secretly choose a creature that player controls", whereas MTGish omitted CardType(Creature) from the choice-pool filter.

### U0115: Restore CardType(Land) on Tunnel Ignus intervening-if entry history filter
- Faces (1): Tunnel Ignus
- Evidence: Tunnel Ignus says "if that player had another land enter the battlefield under their control this turn", whereas MTGish omitted CardType(Land) from the historical ZoneChange object filter. PlayerRef::EventSubject correctly names the opponent under whose control the triggering land entered.

### U0116: Restrict Chandra's Phoenix planeswalker damage trigger recipients to opponents
- Faces (1): Chandra's Phoenix
- Evidence: Chandra's Phoenix says "Whenever an opponent is dealt damage by a red instant or sorcery spell you control or by a red planeswalker you control", so both branches require opponent recipients (OpponentsOf(AbilityController)), whereas MTGish exported Players(All) on the planeswalker branch.

### U0117: Negate Thirsting Axe end-step combat-damage intervening-if predicate
- Faces (1): Thirsting Axe
- Evidence: Thirsting Axe says "At the beginning of your end step, if equipped creature didn't deal combat damage to a creature this turn, sacrifice it", whereas MTGish exported the unnegated >= 1 occurrence check.

### U0118: Model Vhati il-Dal power-or-toughness resolution choice as ChooseOne
- Faces (1): Vhati il-Dal
- Evidence: Vhati il-Dal says "Until end of turn, target creature has base power 1 or base toughness 1" (a resolution-time choice between SetPower(1) and SetToughness(1)), whereas MTGish unconditionally applied both SetPower(1) and SetToughness(1).

### U0119: Move Gilded Ambusher damage inside ForEachPlayer targeting each opponent (Bound(0))
- Faces (1): Gilded Ambusher
- Evidence: Gilded Ambusher says "each opponent exiles a nonland, nontoken permanent they control and a random nonland card from their library, then Gilded Ambusher deals damage to them equal to the total mana value of the cards exiled this way", whereas MTGish placed DealDamage outside the opponent loop with recipient AbilityController instead of Bound(0) inside the loop.

### U0120: Fix Urge to Feed counter placement to put one +1/+1 counter on each tapped Vampire
- Faces (1): Urge to Feed
- Evidence: Urge to Feed says "You may tap any number of untapped Vampire creatures you control. If you do, put a +1/+1 counter on each of those Vampires", whereas MTGish put CountObjects(tapped) counters on Target(0, 0) instead of 1 counter on each tapped Vampire (Transition(result=2, phase=Before)).

### U0121: Fix Urza, Lord Protector meld condition filter from Creature to Artifact
- Faces (1): Urza, Lord Protector
- Evidence: Urza, Lord Protector says "If you both own and control Urza, Lord Protector and an artifact named The Mightstone and Weakstone", and the exile choice already filters by CardType(Artifact), whereas MTGish exported CardType(Creature) in the condition.

### U0122: Run Hex, Kellan's Companion's battlefield exile check after the perpetual pump instead of inside the non-battlefield branch.
- Faces (1): Hex, Kellan's Companion
- Evidence: Hex, Kellan's Companion says "if Hex is on the battlefield or in exile, it perpetually gets +1/+1. Then if it's on the battlefield, exile it with a fetch counter on it." The frozen export split on InZone(Battlefield) and placed the follow-up If(InZone(Battlefield), Move(Exile)) inside the otherwise branch (not on the battlefield), making the exile dead code when Hex is on the battlefield.

### U0123: Allow Yuna's Decision's first mode to put up to one creature card and/or one land card from hand onto the battlefield via OneForEach.
- Faces (1): Yuna's Decision
- Evidence: Yuna's Decision's first mode says "you may put a creature card and/or a land card from your hand onto the battlefield." The frozen export used ChooseObjects with count AsManyAsCan(1) and no constraints, allowing only one card total rather than up to one creature card and up to one land card. Match Michelangelo, Improviser (U0102) using WithSelections with UpTo(2) and OneForEach([CardType(Creature), CardType(Land)]).

### U0124: Use CreatureTypeWord vocabulary operands in Artificial Evolution's ReplaceWord instead of ColorWord.
- Faces (1): Artificial Evolution
- Evidence: Artificial Evolution says "Change the text of target spell or permanent by replacing all instances of one creature type with another. The new creature type can't be Wall." Both implied modes correctly choose two CreatureTypes (except Wall), but their ReplaceWord operands erroneously referenced ColorWord::TheFirstChosenColorWord / ColorWord::TheSecondChosenColorWord instead of CreatureTypeWord::TheFirstChosenCreatureType / CreatureTypeWord::TheSecondChosenCreatureType (paralleling LandTypeWord::TheFirstChosenLandType / TheSecondChosenLandType on Magical Hack and Spectral Shift).

### U0125: Reveal the drafted card (result 3) rather than the earlier hand reveal (result 0) on Break Expectations.
- Faces (1): Break Expectations
- Evidence: Break Expectations says "If a card was exiled this way, that player drafts a card from Break Expectations's spellbook and reveals it." The frozen export referenced Recorded(0) (the cards revealed from the target player's hand in instruction 0) instead of Recorded(3) (the card drafted in instruction 3).

### U0126: Enforce OneForEach([Subtype(Saga), CardType(Land)]) on Eivor, Wolf-Kissed's milled-card entry choice.
- Faces (1): Eivor, Wolf-Kissed
- Evidence: Eivor, Wolf-Kissed says "You may put a Saga card and/or a land card from among them onto the battlefield." The frozen export used UpTo(2) with empty constraints over AnyOf([Saga, Land]), permitting two Sagas or two lands. Match Michelangelo, Improviser (U0102) with OneForEach([Subtype(Saga), CardType(Land)]) over Recorded(0).

### U0127: Guard Enigma Sphinx Avatar's bottom-of-library placement with If(Not(PaymentChosen)) on the optional free play.
- Faces (1): Enigma Sphinx Avatar
- Evidence: Enigma Sphinx Avatar says "You may play that card without paying its mana cost. If you don't, put that card on the bottom of your library." The frozen export placed an unconditional ReorderLibrary after PlayCards with no conditional branch. Wrap the optional PlayCards in PayCost(optional:true, PlayerAction) and guard ReorderLibrary with If(Not(PaymentChosen(1))), matching Breaching Dragonstorm, Chandra, Torch of Defiance, and Vaan, Street Thief.

### U0128: Enforce OneForEach across mana values 1, 2, and 3 on Gruesome Menagerie.
- Faces (1): Gruesome Menagerie
- Evidence: Gruesome Menagerie says "Choose a creature card with mana value 1 in your graveyard, then do the same for creature cards with mana value 2 and 3. Return those cards to the battlefield." The frozen export used UpTo(3) with empty constraints over AnyOf([MV 1, MV 2, MV 3]), permitting multiple cards of the same mana value. Enforce OneForEach across the three filters over Zone(Graveyard, Any).

### U0129: Restore CardType(Creature) and ControlledBy(One(Bound(0))) to Highcliff Felidar's greatest-power selection filter.
- Faces (1): Highcliff Felidar
- Evidence: Highcliff Felidar says "for each opponent, choose a creature with the greatest power among creatures that player controls. Destroy those creatures." The frozen export's ChooseObjects filter checked only Power == Maximum(...) and omitted both CardType(Creature) and ControlledBy(One(Bound(0))), allowing selection of permanents controlled by other players. Match Summon: Valefor's per-player maximum ChooseObjects filter.

### U0130: Check Observed(0) instead of recounting the opponent's post-discard hand on Mind Spike.
- Faces (1): Mind Spike
- Evidence: Mind Spike says "Target opponent reveals each noncreature, nonland card in their hand. You choose a card revealed this way. That player discards that card. You lose 2 life. If they didn't reveal a card this way, you draw a card." The frozen export counted current noncreature, nonland cards in the opponent's hand after the discard, causing a draw whenever the opponent had revealed and discarded their sole matching card. Count Observed(0) from the Reveal instruction instead.

### U0131: Include all four chosen permanents (results 0, 2, 4, 6) in Nature Demands an Offering's shuffle player filter.
- Faces (1): Nature Demands an Offering
- Evidence: Nature Demands an Offering says "target opponent chooses a creature you don't control and puts it on top of its owner's library, then repeats this process for an artifact, an enchantment, and a land. Then the owner of each permanent chosen this way shuffles." The frozen export checked only Recorded(6) (the land), omitting the chosen creature (0), artifact (2), and enchantment (4). Union all four Recorded receipts.

### U0132: Count nonland cards exiled across all three Repeatable exile steps (results 0, 1, 2) on Rally the Horde.
- Faces (1): Rally the Horde
- Evidence: Rally the Horde exiles the top card of your library three times per iteration, repeats if the third isn't a land, then creates a 1/1 red Warrior token "for each nonland card exiled this way." The frozen export counted only Transition(result: 2, After), dropping cards exiled by steps 0 and 1. Union Transition(0, After), Transition(1, After), and Transition(2, After).

### U0133: Put Retribution's -1/-1 counter on the unsacrificed target creature via Difference(Targets(0), Transition(0, Before)).
- Faces (1): Retribution
- Evidence: Retribution says "Choose two target creatures controlled by the same opponent. That player chooses and sacrifices one of those creatures. Put a -1/-1 counter on the other." The frozen export put counters on Targets(0) (both targets) rather than Difference(Targets(0), Transition(result: 0, phase: Before)).

### U0134: Exile the top card of the playing player (Affected) library rather than AbilityController on Share the Spoils.
- Faces (1): Share the Spoils
- Evidence: Share the Spoils reads "During each player's turn, that player may play a land or cast a spell from among cards exiled with this enchantment ... When they do, exile the top card of their library." The static effect affects Players(All) and already uses One(Affected) for during; frozen gold wrongly used One(AbilityController) for the library owner in the triggered effect.

### U0135: Grant suspend to both cards exiled by Sinister Concierge (self cost and target) that do not already have suspend.
- Faces (1): Sinister Concierge
- Evidence: Sinister Concierge reads "you may exile it and put three time counters on it. If you do, exile up to one target creature and put three time counters on it. Each card exiled this way that doesn't have suspend gains suspend." Frozen gold granted suspend only to Transition(1, After), omitting Sinister Concierge itself (CostObjects(Resolution(0), [0], Performed, After)) and omitting the Not(HasKeywordFamily(Suspend)) filter.

### U0136: Guard Systems Override delayed counter-removal trigger on actually putting charge counters (If you do).
- Faces (1): Systems Override
- Evidence: Systems Override reads "If it's a Spacecraft, put ten charge counters on it. If you do, remove ten charge counters from it at the beginning of the next end step." Frozen gold created the delayed trigger unconditionally inside the Spacecraft branch without checking Performed(3) > 0.

### U0137: Reveal the chosen creature cards in both branches of Thunderous Debut before checking bargain.
- Faces (1): Thunderous Debut
- Evidence: Thunderous Debut reads "Look at the top twenty cards of your library. You may reveal up to two creature cards from among them. If this spell was bargained, put the revealed cards onto the battlefield. Otherwise, put the revealed cards into your hand. Then shuffle." Frozen gold duplicated LookAt/ChooseObjects/Shuffle under If(Bargained) and omitted Reveal in the bargained branch (and added destination CanMoveTo constraints before revealing).

### U0138: Condition each player choice on Two Streams Facility chaos trigger on their previous choice so green anchor and red waterfall swap.
- Faces (1): Two Streams Facility
- Evidence: Two Streams Facility AB[3] reads "Whenever chaos ensues, each player who last chose green anchor chooses red waterfall, and vice versa." Frozen gold copied AB[0] unrestricted ChooseValue([Green Anchor, Red Waterfall]) with no VotedFor condition.

### U0139: Guard Unlucky Cabbage Merchant bottoming itself and shuffling on actually searching (If you search your library this way).
- Faces (1): Unlucky Cabbage Merchant
- Evidence: Unlucky Cabbage Merchant reads "you may search your library for a basic land card and put it onto the battlefield tapped. If you search your library this way, put this creature on the bottom of its owner's library, then shuffle." Frozen gold placed Move(Source -> Bottom) and Shuffle unconditionally outside Optional, so declining the search still bottomed the creature and shuffled.

### U0140: Move the revealed creature card into hand in Whiskervale Forerunner off-turn branch before reordering the rest.
- Faces (1): Whiskervale Forerunner
- Evidence: Whiskervale Forerunner reads "You may put it onto the battlefield if it's your turn. If you don't put it onto the battlefield, put it into your hand. Put the rest on the bottom of your library in a random order." In the PlayerIsActive otherwise branch (not your turn), frozen gold ran only ReorderLibrary on all Observed(0) cards and dropped moving Selected(1) into hand.

### U0141: Restrict Glen Elendra second target controller history predicate to Combat damage instead of Any damage.
- Faces (1): Glen Elendra
- Evidence: Glen Elendra reads "exchange control of target creature you control that dealt combat damage to a player this combat and target creature that player controls." Target 0 uses Damage(kind: Combat), while frozen gold Target 1 wrongly used Damage(kind: Any).

### U0142: Test that the countered ability came from a permanent and destroy SourceOf(Recorded(0)) on Green Slime.
- Faces (1): Green Slime
- Evidence: Green Slime reads "counter target activated or triggered ability from an artifact or enchantment source. If a permanent's ability is countered this way, destroy that permanent." Frozen gold tested Matching(Recorded(0), Any) and destroyed Recorded(0) (the stack ability itself) instead of filtering by AbilityOf(InZone(Shared(Battlefield))) and destroying SourceOf(Recorded(0)).

### U0143: Restrict Itazura, Lingering Wick hand choice to exiled cards still in Exile (not cast this way).
- Faces (1): Itazura, Lingering Wick
- Evidence: Itazura, Lingering Wick reads "they may cast a spell from among those cards without paying its mana cost. You put a card from among them that wasn't cast this way into your hand." Frozen gold selected from all Transition(0, After) without excluding the cast card; wrapping with InZone(..., Exile) selects only the cards remaining in exile after the optional cast.

### U0144: Require target player to be an opponent before drawing a card on Lucy MacLean, Positively Armed.
- Faces (1): Lucy MacLean, Positively Armed
- Evidence: Lucy MacLean, Positively Armed reads "you may have target player other than its controller create a token that's a copy of it, then you draw a card if an opponent created a token this way." Frozen gold checked only PaymentChosen(0), wrongly drawing when AbilityController targeted themselves after an opponent's token entered.

### U0145: Restore target opponent hand Reveal before PlayWith on Mindclaw Shaman
- Faces (1): Mindclaw Shaman
- Evidence: Mindclaw Shaman reads 'When this creature enters, target opponent reveals their hand. You may cast an instant or sorcery spell from among those cards without paying its mana cost.'; frozen gold dropped the Reveal instruction entirely and queried the unrevealed hand directly.

### U0146: Scope each player's Fractal token creation and +1/+1 counter placement together on Oversimplify
- Faces (1): Oversimplify
- Evidence: Oversimplify reads 'Exile all creatures. Each player creates a 0/0 green and blue Fractal creature token and puts a number of +1/+1 counters on it equal to the total power of creatures they controlled that were exiled this way.'; frozen gold created all players' tokens first and then had each player put counters on every token in Recorded(1) instead of only their own created token.

### U0147: Select up to two lands for the battlefield tapped before moving the remaining land to hand on Viewpoint Synchronization
- Faces (1): Viewpoint Synchronization
- Evidence: Viewpoint Synchronization reads 'Search your library for up to three basic land cards and reveal them. Put two of them onto the battlefield tapped and the other into your hand, then shuffle.'; frozen gold chose AsManyAsCan(1) for the hand first and moved the rest to the battlefield, putting a card into hand instead of onto the battlefield whenever two or fewer cards are found.

### U0148: Guard Breeches, the Blastmaker's coin flip and reflexive triggers on paying the optional artifact sacrifice
- Faces (1): Breeches, the Blastmaker
- Evidence: Breeches, the Blastmaker reads 'you may sacrifice an artifact. If you do, flip a coin. When you win the flip, ... When you lose the flip, ...'; frozen gold executed FlipCoin and both reflexive triggers unconditionally after the optional PayCost without If(PaymentChosen(0)).

### U0149: Select a contested land controlled by the damaged player instead of the attacking creature's controller on Turf War
- Faces (1): Turf War
- Evidence: Turf War reads 'Whenever a creature deals combat damage to a player, if that player controls one or more lands with contested counters on them, that creature's controller gains control of one of those lands of their choice and untaps it.'; frozen gold filtered ChooseObjects by ControlledBy(ControllerOf(Damage(Source))) instead of ControlledBy(EventSubject).

### U0150: Place Unexpected Results' self-return inside the optional land-card branch guarded by revealing a land
- Faces (1): Unexpected Results
- Evidence: Unexpected Results reads 'If it's a land card, you may put it onto the battlefield and return Unexpected Results to its owner's hand.' Per Oracle rulings, returning Unexpected Results is part of the optional choice and does not occur if declined or if the library is empty; frozen gold placed Move(Source, OwnersHand) unconditionally in otherwise outside Optional.

### U0151: Grant free-cast permission to the exiled nonland card rather than the first exiled card on Nicol Bolas, God-Pharaoh and Stolen Goods
- Faces (2): Nicol Bolas, God-Pharaoh, Stolen Goods
- Evidence: Both faces read 'Target opponent exiles cards from the top of their library until they exile a nonland card. Until end of turn, you may cast that card without paying its mana cost.'; frozen gold emitted Result(1, After, 0) (the first exiled card, a land whenever lands are exiled before the nonland card) instead of Matching(Transition(1, After), Not(CardType(Land))).

### U0152: Fix lost enchant ability domain (graveyard creature card) and delayed sacrifice controller (reanimated creature's controller) on Animate Dead
- Faces (1): Animate Dead
- Evidence: Animate Dead loses 'enchant creature card in a graveyard' (not battlefield permanent) and its delayed trigger makes 'that creature's controller' (ControllerOf(Result(1, After, 0))) sacrifice it, not the Aura's controller (ControllerOf(EventSubject)).

### U0153: Measure Equipment's own mana value (Source) instead of equipped creature's (Affected) in Bludgeon Brawl's granted pump
- Faces (1): Bludgeon Brawl
- Evidence: Bludgeon Brawl grants '"Equipped creature gets +X/+0," where X is that artifact's mana value.' Inside the granted static ability, Source is the Equipment artifact and Affected is the equipped creature; frozen gold emitted ObjectProperty(object: Affected, property: ManaValue).

### U0154: Take cards from the damaged player's library and put them onto the battlefield under AbilityController's control on Cybership
- Faces (1): Cybership
- Evidence: Cybership reads 'Whenever this Vehicle deals combat damage to a player, put the top two cards of that player's library onto the battlefield face down under your control.'; frozen gold took from AbilityController's library and put them onto the battlefield under Owner's control.

### U0155: Modal abilities reading 'choose up to one' permit choosing zero modes (minimum 0, not 1).
- Faces (5): Dreamshackle Geist, Ertai Resurrected, Hullbreaker Horror, Ravager Wurm, Satsuki, the Living Lore
- Evidence: Oracle text for Satsuki, the Living Lore, Ertai Resurrected, Hullbreaker Horror, Ravager Wurm, and Dreamshackle Geist reads 'choose up to one —'; frozen gold set ModeCount.minimum to 1 instead of 0.

### U0156: Retain original Target reference for post-destroy controller draw on Ertai Resurrected.
- Faces (1): Ertai Resurrected
- Evidence: Ertai Resurrected's second mode reads 'Destroy another target creature or planeswalker. Its controller draws a card.' Per SPEC.md (post-destroy references retain the original target even if destruction fails) and peer cards (Cleansing Wildfire, Faller's Faithful, Geomancer's Gambit, Ob Nixilis, the Hate-Twisted, Price of Freedom), the draw actor is ControllerOf(Target(0, 0)), not Result(0, Before).

### U0157: Preserve player-owner, graveyard-owner, and controller scopes on Rootcoil Creeper, Great Intelligence's Plan, and Death in Heaven.
- Faces (3): Death in Heaven, Great Intelligence's Plan, Rootcoil Creeper
- Evidence: Rootcoil Creeper restricts mana to spells cast from your graveyard (One(AbilityController), not All); Great Intelligence's Plan's second villainous-choice option ('or you may cast a spell from your hand') has decider, actor, and hand owner AbilityController rather than Target(0); Death in Heaven's chapter III returns cards 'under your control' (Player(AbilityController), not Owner).

### U0158: Preserve BasePower/BaseToughness checks on Angelic Aberration and NotEqual comparison on Iridian Maelstrom.
- Faces (2): Angelic Aberration, Iridian Maelstrom
- Evidence: Angelic Aberration specifies 'each with base power or toughness 1 or less' (BasePower/BaseToughness, not Power/Toughness); Iridian Maelstrom specifies 'that isn't all colors' (NotEqual, not Equal).

### U0159: Preserve explicit CardType(Creature) and controller predicates on High Score, Primal Empathy, Éomer of the Riddermark, Disciples of the Inferno, Shaun, Father of Synths, Szat's Will, and Minas Tirith Garrison.
- Faces (7): Disciples of the Inferno, High Score, Minas Tirith Garrison, Primal Empathy, Shaun, Father of Synths, Szat's Will, Éomer of the Riddermark
- Evidence: High Score, Primal Empathy, and Éomer of the Riddermark test controlling a creature with greatest power (preserving CardType(Creature)); Disciples of the Inferno restricts damage sources to 'you control'; Shaun, Father of Synths targets a creature 'you control' (One(AbilityController), not OthersThan); Szat's Will exiles all opponents' graveyards (OpponentsOf scope with filter Any) and counts only creature cards exiled this way; Minas Tirith Garrison taps untapped Humans 'you control'.

### U0160: Correct swapped Attach operands on Ogre Geargrabber, missing counter-unless payment branch on Hope-Ender Coatl, Ability counter kind on Summary Dismissal, and granted Source reference on Essence Leak.
- Faces (4): Essence Leak, Hope-Ender Coatl, Ogre Geargrabber, Summary Dismissal
- Evidence: Ogre Geargrabber attaches Target(0) (Equipment) to Source (self), not vice versa; Hope-Ender Coatl counters only if the optional {1} payment is not chosen; Summary Dismissal counters 'all abilities' (kind Ability, not Spell); Essence Leak's granted upkeep ability refers to its own holder's mana cost (ManaCostOf(Source), not AttachedTo(Source)).

### U0161: Preserve active-player turn restrictions on Tifa, Martial Artist and Katara, Water Tribe's Hope.
- Faces (2): Katara, Water Tribe's Hope, Tifa, Martial Artist
- Evidence: Tifa, Martial Artist checks 'if it's the first combat phase of your turn' (requiring PlayerIsActive(AbilityController) alongside Window(FirstCombat)); Katara, Water Tribe's Hope states 'Activate only during your turn' (limits.condition PlayerIsActive(AbilityController), not null).


### U0162: Use historical WasAttachedTo(Target(0, 0)) after zone change on Fumble and Murderous Spoils.
- Faces (2): Fumble, Murderous Spoils
- Evidence: Fumble ('Return target creature to its owner's hand. Gain control of all Auras and Equipment that were attached to it, then attach them to another creature.') and Murderous Spoils ('Destroy target nonblack creature. It can't be regenerated. You gain control of all Equipment that were attached to it.') query what was attached to the target creature immediately before it left the battlefield (CR 608.2h). Frozen gold emitted live AttachedTo(SameObject(Target(0, 0))), which is always empty once the target is in hand or graveyard; crates/oracle-ir/src/ir/objects.rs:471 defines ObjectFilter::WasAttachedTo(ObjectRef) specifically as 'Was attached to this object as it last existed' (already used by frozen gold on Hateful Eidolon and Zack Fair).

### U0163: Use AllSameProperty(Colors) instead of Not(NoCommonProperty(Colors)) on Dead Ringers.
- Faces (1): Dead Ringers
- Evidence: Dead Ringers reads 'Destroy two target nonblack creatures unless either one is a color the other isn't. They can't be regenerated.' It destroys both targets iff they have the exact same set of colors (AllSameProperty(Colors), defined in crates/oracle-ir/src/ir/selection.rs:322 as 'Every object has the same set of values of this property'). Frozen gold used Not(SubsetMeets{..., constraints:[NoCommonProperty(Colors)]}), which wrongly destroys a mono-red creature and a red-blue creature (they share red, even though the red-blue creature is blue which the red one isn't) and wrongly spares two colorless creatures (they share no colors).

### U0164: Negate SharesWithAny on Radagast the Brown's 'doesn't share a creature type' filter.
- Faces (1): Radagast the Brown
- Evidence: Radagast the Brown's Oracle text reads: 'You may reveal a creature card that doesn't share a creature type with a creature you control from among those cards and put it into your hand.' Frozen gold (both Triggered abilities) exported positive SharesWithAny without Not(...), inverting the filter to match cards that DO share a creature type. Compare Menagerie Curator and Volo, Guide to Monsters in frozen gold, which both wrap SharesWithAny in Not(...).

### U0165: Require AbilityController as caster and hand owner on Amped Raptor's 'if you cast it from your hand'.
- Faces (1): Amped Raptor
- Evidence: Amped Raptor reads: "When this creature enters, you get {E}{E}. Then if you cast it from your hand, exile cards from the top of your library until you exile a nonland card..." Under CR 109.5, "you" refers to the controller of the ability (AbilityController). Frozen gold exported Cast{by: null, from: Players{zone: Hand, players: One(CasterOf(Candidate))}}, which erroneously drops the 'you cast it' actor check and checks the caster's hand instead of the ability controller's hand (failing when an opponent casts it from their hand and it enters under your control, e.g. via Gather Specimens). All 27 other 'if you cast it from your hand' faces in the allowed corpus (Angel of the Dire Hour, Breaching Leviathan, Coal Stoker, Deathbringer Regent, Dread Cacodemon, Feasting Troll King, etc.) export Cast{by: One(AbilityController), from: Players{zone: Hand, players: One(AbilityController)}}.

### U0166: Use CountDistinct(..., Names) on Swine Rebellion's 'Boars with different names' conditions.
- Faces (1): Swine Rebellion
- Evidence: Swine Rebellion reads: "If you control three or more Boars with different names, conjure each card from the Three Pigs spellbook onto the battlefield. If you control two or fewer Boars with different names, conjure two cards of your choice from the Three Pigs spellbook into your hand, then put one of them onto the battlefield." Frozen gold exported CountObjects over controlled Boars in both conditions, completely dropping 'with different names' (incorrectly treating three identically named Boars as satisfying 'three or more Boars with different names' and failing 'two or fewer Boars with different names'). All 4 other 'if you control ... with different names' faces in the allowed corpus (Field of the Dead, Liliana's Contract, Maze's End, The Necrobloom) use CountDistinct with property Names.

### U0167: Use strict Less comparison for Gyrus, Waker of Corpses's 'with lesser power' graveyard target filter.
- Faces (1): Gyrus, Waker of Corpses
- Evidence: Gyrus, Waker of Corpses reads 'you may exile target creature card with lesser power from your graveyard.' Official Gatherer ruling (2018-07-13) confirms the targeted creature card's power must be strictly less than Gyrus's power (Less, not LessOrEqual).

### U0168: Restore player protection effect on Faith's Shield's fateful hour branch.
- Faces (1): Faith's Shield
- Evidence: Faith's Shield reads 'If you have 5 or less life, instead you and each permanent you control gain protection from the color of your choice until end of turn.' Frozen MTGish gold omitted the Players(One(AbilityController)) PlayerRule(GrantKeyword(Protection(...))) continuous effect, matching Seht's Tiger and Serra's Emissary.

### U0169: Exclude Source from Jeweled Bird's 'all other cards you own from the ante' graveyard return filter.
- Faces (1): Jeweled Bird
- Evidence: Jeweled Bird reads '{T}: Ante this artifact. If you do, put all other cards you own from the ante into your graveyard, then draw a card.' Frozen MTGish gold used OwnedBy(One(AbilityController)) without Not(SameObject(Source)), which would return Jeweled Bird itself from ante to the graveyard.

### U0170: Use NotEqual comparison for Haktos the Unscarred's 'protection from each mana value other than the chosen number'.
- Faces (1): Haktos the Unscarred
- Evidence: Haktos the Unscarred reads 'Haktos has protection from each mana value other than the chosen number.' Frozen MTGish gold inverted the predicate to Equal (same bug class as U0158).

### U0171: Reference AttachedTo(Source) rather than destroy result for Orcish Mine's 'that land's controller'.
- Faces (1): Orcish Mine
- Evidence: Orcish Mine reads 'When the last ore counter is removed from this Aura, destroy enchanted land and this Aura deals 2 damage to that land's controller.' Per CR 608.2c/609.3 and SPEC.md (same bug class as U0156 Ertai Resurrected), damage is still dealt to enchanted land's controller even if destruction fails (indestructible/regenerated); Result(0, Before, 0) would be empty.

### U0172: Remove spurious ControlledBy(AbilityController) from Jasmine Boreal of the Seven's blocker filter.
- Faces (1): Jasmine Boreal of the Seven
- Evidence: Jasmine Boreal of the Seven reads 'Creatures you control with no abilities can't be blocked by creatures with abilities.' Frozen MTGish gold erroneously copied ControlledBy(One(AbilityController)) into the BeBlocked blocker filter.

### U0173: Restrict Longhorn Firebeast and Vexing Devil willing-player damage payment to opponents.
- Faces (2): Longhorn Firebeast, Vexing Devil
- Evidence: Both Longhorn Firebeast ('any opponent may have it deal 5 damage to them') and Vexing Devil ('any opponent may have it deal 4 damage to them') specify 'any opponent', not 'any player'. Across the corpus all other 'any opponent may' cards use FirstWilling(OpponentsOf(AbilityController)); only these two faces erroneously exported FirstWilling(All).

### U0174: Represent Mordenkainen's 'Exchange your hand and library, then shuffle' as sequential Exchange then Shuffle instructions.
- Faces (1): Mordenkainen
- Evidence: Mordenkainen's -10 ability reads 'Exchange your hand and library, then shuffle. You get an emblem with "You have no maximum hand size."' Per CR 608.2c, 'then shuffle' happens sequentially after the zone exchange (matching Morality Shift, which exports sequential [Effect(Exchange), Effect(Shuffle)], not simultaneous Together).

### U0175: Restore missing negation Not(...) on Wilhelt, the Rotcleaver's 'if it didn't have decayed' intervening-if condition.
- Faces (1): Wilhelt, the Rotcleaver
- Evidence: Wilhelt, the Rotcleaver reads: 'Whenever another Zombie you control dies, if it didn't have decayed, create a 2/2 black Zombie creature token with decayed.' Frozen gold emitted `intervening_if: ObjectMatches{object: EventSubject, filter: HasKeywordFamily(Decayed)}` without `Not`, inverting the condition so Wilhelt triggers only when a decayed Zombie dies (going infinite with its own tokens) instead of when a non-decayed Zombie dies. Wrapping the filter in `Not(HasKeywordFamily(Decayed))` restores the exact Oracle meaning.

### U0176: Apply Ent's Fury's +1/+1 pump to target creature you control (Target slot 0) rather than target creature you don't control (Target slot 1).
- Faces (1): Ent's Fury
- Evidence: Ent's Fury reads: 'Put a +1/+1 counter on target creature you control if its power is 4 or greater. Then that creature gets +1/+1 until end of turn and fights target creature you don't control.' 'That creature' refers unambiguously to Target{slot: 0} (target creature you control). Frozen gold mistakenly set the CreateContinuousEffect's `affected` to `Target{slot: 1, index: 0}`, pumping the opponent's creature before the fight instead of your own creature.

### U0177: Use a CR 603.12 ReflexiveTrigger for Richlau, Headmaster's 'you may pay {1}. When you do, target artifact card in your graveyard ...' clause.
- Faces (1): Richlau, Headmaster
- Evidence: Richlau, Headmaster reads: 'At the beginning of your end step, you may pay {1}. When you do, target artifact card in your graveyard perpetually gains "This spell costs {1} less to cast." If it's a creature or Vehicle card, it perpetually gets +2/+2. Put it into your library second from the top.' Under CR 603.12, 'When you do, target ...' creates a reflexive triggered ability whose target is announced only after paying {1}, NOT when the end-step trigger goes on the stack (matching Go-Shintai of Lost Wisdom, Itzquinth, Firstborn of Gishath, and Ty Lee, Artful Acrobat in frozen gold). Frozen gold omitted ReflexiveTrigger and placed the target on the outer end-step trigger's announcement.

### U0178: Check the exiled payment card's mana value via CostObjects on Summoner's Sending instead of turn-wide exile EventHistory.
- Faces (1): Summoner's Sending
- Evidence: Summoner's Sending reads: 'At the beginning of your end step, you may exile target creature card from a graveyard. If you do, create a 1/1 white Spirit creature token with flying. Put a +1/+1 counter on it if the exiled card's mana value is 4 or greater.' Frozen gold encoded 'if the exiled card's mana value is 4 or greater' as `EventHistory{event: ZoneChange{objects: Property{ManaValue >= 4}, to: Exile}, window: ThisTurn, measure: Occurrences} >= 1`, which falsely succeeds if any unrelated card with mana value >= 4 was exiled anywhere that turn even when Summoner's Sending exiled a 1-mana creature. Testing `Query{objects: CostObjects{payment: Resolution(0), component: [0], view: Performed, phase: After}, index: 0}` directly inspects the card exiled by the payment.

### U0179: Restore missing ControlledBy(One(AbilityController)) predicate on Guild Summit's 'untapped Gates you control' choice.
- Faces (1): Guild Summit
- Evidence: Guild Summit reads: 'When this enchantment enters, you may tap any number of untapped Gates you control. Draw a card for each Gate tapped this way.' Frozen gold emitted `filter: AllOf([Not(Tapped), Subtype(Gate)])`, dropping `ControlledBy(One(AbilityController))` and allowing the controller to tap opponents' Gates (same class of dropped controller predicate fixed in U0159).

### U0180: Restore missing Not(SameObject(Source)) ('other') exclusion on Coalition Construct's perpetual ETB buff.
- Faces (1): Coalition Construct
- Evidence: Coalition Construct reads: 'As this creature enters, choose a creature type. This creature is the chosen type in addition to its other types. When this creature enters, other creatures you control of the chosen type and creature cards of that type in your hand perpetually get +1/+1.' Frozen gold omitted `Not(SameObject(Source))` from the battlefield perpetual pump filter, causing Coalition Construct (which is itself the chosen creature type via ability 2) to perpetually buff itself, directly contradicting 'other creatures you control of the chosen type'.

### U0181: Use WhichCreaturesBlock on Berserker's Frenzy's 'You choose which creatures block this turn and how those creatures block' result.
- Faces (1): Berserker's Frenzy
- Evidence: Berserker's Frenzy's 15-20 result reads verbatim: 'You choose which creatures block this turn and how those creatures block.' In frozen gold, every other card with this exact wording (Master Warcraft, Odric, Master Tactician, and Melee) uses `GameDecision::WhichCreaturesBlock` (allowing the controller to choose which creatures block, including choosing none), whereas `HowCreaturesBlock(filter)` is for cards like Brutal Hordechief ('you choose how those creatures block') that only control blocking assignments. Frozen gold exported `HowCreaturesBlock(CardType(Creature))` on Berserker's Frenzy, losing the 'which creatures block' decision.

### U0182: Fix inverted Repeat times and Incubate amount on Glistening Dawn.
- Faces (1): Glistening Dawn
- Evidence: Glistening Dawn reads 'Incubate X twice, where X is the number of lands you control.' (CR 701.52: incubate with value X two times, creating two Incubator tokens each with X +1/+1 counters). Frozen MTGish gold inverted the operands as Repeat{times: CountObjects(lands you control), body: Incubate{amount: Constant(2)}} (creating X tokens with 2 counters each) instead of Repeat{times: Constant(2), body: Incubate{amount: CountObjects(lands you control)}}.

### U0183: Use Affected instead of Source in Earnest Fellowship's SharesColorWith protection grant.
- Faces (1): Earnest Fellowship
- Evidence: Earnest Fellowship reads: 'Each creature has protection from each of its colors.' Under CR 702.16, 'each of its colors' refers to each affected creature's own colors (SharesColorWith(Affected), matching Tam, Mindful First-Year's 'has hexproof from each of its colors' grant in continuous.ebnf). Frozen MTGish gold emitted SharesColorWith(Source), which grants every creature protection from Earnest Fellowship's color (white) instead of each creature's own colors.
