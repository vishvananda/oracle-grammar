//! Objects in the normalized semantic model.
use super::*;

#[derive(Clone, Debug, Serialize, Deserialize, bincode::Encode, bincode::Decode)]
pub enum AttackDefenders {
    Any,
    Objects(ObjectFilter),
    Players(PlayerSet),
    /// A battle is not included merely because a player controls/protects it.
    PlayersAndPlaneswalkers(PlayerSet),
    /// The defending player, including the protector of a battle (CR 508.5).
    DefendingPlayers(PlayerSet),
}

#[derive(Clone, Debug, Serialize, Deserialize, bincode::Encode, bincode::Decode)]
pub enum EffectSubject {
    Objects(ObjectSet),
    /// Spells that match, including ones cast later.
    EachSpell(ObjectFilter),
    /// The next matching spell this player casts.
    NextSpell {
        spells: ObjectFilter,
        caster: PlayerRef,
        this_turn: bool,
        from: Option<EventZone>,
    },
    Players(PlayerSet),
}

#[derive(Clone, Debug, Serialize, Deserialize, bincode::Encode, bincode::Decode)]
pub enum Information {
    CurrentOrLastKnown,
    Captured(LoopBinding),
}

#[derive(Clone, Debug, Serialize, Deserialize, bincode::Encode, bincode::Decode)]
pub enum Property {
    /// X recorded on this spell, including copied choices. Distinct from an
    /// unrelated activation's announced X or an entering object's cast history.
    SpellX,
    ColorCount,
    Power,
    Toughness,
    ManaValue,
    BasePower,
    BaseToughness,
    Loyalty,
    /// CR 702.161: the starting intensity plus what intensify added.
    Intensity,
    TotalPowerAndToughness,
    CreatureTypeCount,
    CardTypeCount,
    ColorSymbolCount,
    /// Cards exiled with this object.
    ExiledCount,
    /// Creatures that crewed this Vehicle this turn.
    CrewCount,
    SubtypeCount,
    SupertypeCount,
    ToxicValue,
    BushidoValue,
    RegenerationsThisTurn,
    GenericManaInCost,
    /// Number of different colors a mana source could produce.
    ManaColorsCouldProduce,
    /// Total counters of every kind on this object.
    CounterCount,
}

#[derive(Clone, Debug, Serialize, Deserialize, bincode::Encode, bincode::Decode)]
pub enum BlockRole {
    Attacker,
    Blocker,
    /// Whichever of the two is not the object the event was asked about.
    Other,
}

#[derive(Clone, Debug, Serialize, Deserialize, bincode::Encode, bincode::Decode)]
#[serde(tag = "kind", content = "args")]
pub enum ObjectRef {
    /// One reviewed rules-defined reference; see `rewrites/primitives.json5`.
    Primitive {
        name: String,
        operands: Vec<Operand>,
    },
    /// The object carrying a parameterized keyword. Inside a granted ward
    /// definition this remains the recipient, rather than the granting source.
    AbilityHolder,
    /// The card after a matched discard, wherever replacements sent it.
    Discarded,
    /// A singular role in the matched damage occurrence, when its grouping
    /// guarantees one object. Distinct from the source of this ability.
    Damage(DamageSubject),
    /// Index an ordered query, or index zero of a statically singular cost
    /// history. Missing entries select nothing; plural unordered histories
    /// cannot silently choose an arbitrary first object.
    Query {
        objects: Box<ObjectSet>,
        index: u32,
    },
    /// The object currently tested by the innermost object filter.
    Candidate,
    Source,
    /// The current permanent host of this attachment, in the query's view.
    /// Selects nothing when unattached. Last-known attachment information needs
    /// an explicit captured view rather than treating a departed source as live.
    AttachedTo(Box<ObjectRef>),
    /// The object whose characteristic change is currently being evaluated.
    Affected,
    Target {
        slot: TargetSlot,
        index: u16,
    },
    /// The matched event's distinguished object: e.g. the entering/leaving,
    /// attacking, or damaged permanent, or the spell that was cast/copied.
    EventSubject,
    /// A creature of the attacker and blocker paired by a block event.
    BlockPair(BlockRole),
    /// The first public-zone incarnation produced by the matched zone-changing
    /// event (CR 400.7e), including a sacrifice whose destination was replaced.
    /// Selects nothing once that incarnation leaves its destination; never
    /// follows a second zone change or a move into a hidden zone.
    EventDestination,
    /// The object chosen by this ability, or by a linked ability of this source.
    Chosen,
    Bound(LoopBinding),
    Result {
        result: ResultId,
        phase: ObjectPhase,
        index: u32,
    },
    /// The spell or ability that made this event's subject a target (603.2).
    EventStackObject,
    /// The permanent chosen for a damage-recipient slot. A player chosen
    /// there leaves this undefined, so tests of it are false.
    RecipientObject {
        slot: TargetSlot,
        index: u16,
    },
    /// The second object of a two-object event: the host of an attachment,
    /// the crewed vehicle, the exploited creature.
    EventOther,
    /// The object a linked ability of the source made.
    MadeHere,
    /// What this player picked in a loop that gave every player a choice.
    ChosenBy {
        result: ResultId,
        chooser: PlayerRef,
    },
    /// The permanent whose effect gave the source this ability, or made it.
    GrantingObject,
    /// The creature `object` is paired with (CR 702.95b), if any.
    PairedWith(Box<ObjectRef>),
    /// The one object this spell targets.
    SoleTargetOf(Box<ObjectRef>),
    /// What a preceding instruction named, whatever kind of result it holds.
    Recorded {
        result: ResultId,
        by: Option<PlayerRef>,
    },
    /// What an earlier step acted on, read from the card's words.
    Earlier {
        what: EarlierObjects,
        by: Option<PlayerRef>,
    },
    /// The permanent an ability on the stack came from.
    SourceOf(Box<ObjectRef>),
    /// "That" object: whatever the row around this one names.
    Scoped,
    RingBearer(PlayerRef),
    HauntedBy(Box<ObjectRef>),
    /// The nth object this player chose.
    NthChosen {
        index: u32,
        by: Option<PlayerRef>,
    },
    /// The other one of `among`, given the ones `excluding` names. Either of
    /// them when `excluding` names them all, as Forge's Spellweaver Helix.
    TheOther {
        among: Box<ObjectSet>,
        excluding: Box<ObjectSet>,
    },
    /// The object at this end of a set that holds an order of its own: a pile
    /// shuffled as it was exiled, or a history, whose `Top` is its earliest.
    AtEnd {
        objects: Box<ObjectSet>,
        end: SequenceEnd,
    },
    /// The Aura's first owner's-graveyard incarnation after its watched enchanted
    /// permanent left the battlefield (CR 400.7f). Selects nothing after that
    /// incarnation moves again, or when a replacement sent the Aura elsewhere.
    /// The Aura and host name their original battlefield objects, independently
    /// of the source's current attachment or the host's destination incarnation.
    AuraAfterEnchantedLeft {
        aura: Box<ObjectRef>,
        host: Box<ObjectRef>,
    },
    /// The actual arrival in `zone` from one direct singular movement receipt.
    /// A move prevented or redirected elsewhere gives no object. This preserves
    /// that arrival incarnation and permits current or last-known information;
    /// it never follows the object into a later incarnation. If there is no
    /// arrival, or the requested characteristic has no value, the read is
    /// undefined: it does not use the pre-move object or silently become zero.
    MoveArrivalAt {
        result: ResultId,
        zone: EventZone,
    },
}

#[derive(Clone, Debug, Serialize, Deserialize, bincode::Encode, bincode::Decode)]
pub enum ObjectPhase {
    Before,
    After,
}

#[derive(Clone, Debug, Serialize, Deserialize, bincode::Encode, bincode::Decode)]
#[serde(tag = "kind", content = "args")]
pub enum ObjectSet {
    /// Original identities in a LookAt result; never re-run its source query.
    Observed(ResultId),
    /// Preserve the input sequence; ordinary filtering does not reveal cards.
    Matching {
        objects: Box<ObjectSet>,
        filter: ObjectFilter,
    },
    /// Preserve the left input's sequence and original object identities.
    Difference {
        objects: Box<ObjectSet>,
        excluding: Box<ObjectSet>,
    },
    /// Distinct objects occupying this role in the matched damage occurrence.
    Damage(DamageSubject),
    /// Private library/graveyard queries retain top-to-bottom order; filtering
    /// preserves that order. Several players' zones have no shared ordering.
    Zone {
        scope: ZoneScope,
        kind: ObjectKind,
        filter: ObjectFilter,
    },
    /// Set union, with duplicates removed; this is not an iteration.
    Union(Vec<ObjectSet>),
    Take {
        objects: Box<ObjectSet>,
        count: Expr,
        from: SequenceEnd,
    },
    /// Actual transitions, not all objects originally requested by an effect.
    /// Empty when the action sat in an optional body or cost that never ran.
    Transition {
        result: ResultId,
        phase: ObjectPhase,
    },
    One(ObjectRef),
    /// Targets legal at the start of this resolution, retaining their
    /// announced incarnations for later instructions without rechecking
    /// targeting restrictions as the resolving effect changes the game.
    Targets(TargetSlot),
    /// Intersect a reference/result set with current zone membership. A saved
    /// object reference alone does not imply the object is still on the field.
    InZone {
        objects: Box<ObjectSet>,
        zone: EventZone,
    },
    /// Objects chosen by an earlier choice, search, or single counter
    /// placement with a selection. A counter replacement can change what was
    /// placed without changing the identities chosen for the placement.
    Selected(ResultId),
    /// Objects returned by a preceding token creation; distinct from a choice.
    Created(ResultId),
    /// Object history for one exact payment component. An unpaid component,
    /// unchosen alternative, or omitted optional cost has an empty history. `Inputs`
    /// retains the designated original identities and their expected-zone/LKI
    /// view; `Performed` retains participants of the corresponding rules action
    /// after replacement processing. Paying a cost does not imply the action
    /// occurred (118.11-12). Before/After distinguish original and resulting
    /// incarnations, never a fresh zone query. Input histories use Before.
    /// During announcement, Inputs denotes the casting/activation proposal's
    /// planned inputs (601.5); payment must use those same choices. Afterward
    /// it denotes the recorded identities, so changes to a live selector cannot
    /// redirect it. Performed action history is unavailable before payment.
    /// A copied spell/ability uses the original's cost objects (707.10), even
    /// though the copy pays no cost; a resolution-time payment is its own frame.
    CostObjects {
        payment: PaymentRef,
        component: Vec<u32>,
        view: CostObjectView,
        phase: ObjectPhase,
    },
    /// Every object able to be the source of damage (CR 609.7a) that matches.
    DamageSources(ObjectFilter),
    /// The objects an earlier continuous effect in this resolution applied to.
    Affected(ResultId),
    /// Every object of a one-or-more event, grouped under one trigger (603.2c).
    EventGroup,
    /// The top card of each of these players' libraries.
    LibraryTops(PlayerSet),
    /// Objects a preceding choice could have selected and did not, by the
    /// candidates it offered; each player's own in a per-player choice.
    Unselected(ResultId),
    /// What this player's share of an action moved: their run of a `ForEachPlayer`
    /// body, or the part of a batch they performed.
    MovedBy {
        result: ResultId,
        phase: ObjectPhase,
        player: PlayerRef,
    },
    /// The copies a `CopyCards` made, wherever they were made.
    Copies(ResultId),
    /// The objects of the enclosing `ForEachObject` this payment was not made for.
    Unpaid(ResultId),
    /// A reviewed set with no reduction.
    Primitive {
        name: String,
        operands: Vec<Operand>,
    },
    /// What a preceding instruction named, whatever kind of result it holds.
    Recorded(ResultId),
    /// Every matching card or object, whatever zone it is in.
    AnyZone(ObjectFilter),
    /// Cards a player owns outside the game (CR 400.11).
    OutsideTheGame(ObjectFilter),
    /// Some of these at random.
    /// Draft producer/old-decoder form; canonical resolution uses Prepared.
    #[doc(hidden)]
    #[serde(rename = "Random")]
    DraftRandom {
        objects: Box<ObjectSet>,
        count: Expr,
    },
    /// Some of these, which `chooser` picks as the instruction resolves.
    /// Draft producer/old-decoder form; canonical resolution uses Prepared.
    #[doc(hidden)]
    #[serde(rename = "Chosen")]
    DraftChosen {
        chooser: PlayerSet,
        objects: Box<ObjectSet>,
        count: Count,
    },
    Earlier {
        what: EarlierObjects,
        by: Option<PlayerSet>,
    },
    /// "Those" objects: whatever the row around this one names.
    Scoped,
    /// The cards spliced onto this spell as it was cast (CR 702.47).
    SplicedOnto(ObjectRef),
    /// What this spell or ability targets, as it is being cast or on the stack.
    TargetsOf(ObjectRef),
    /// The subjects of an event in a window as they last existed, in the order
    /// it happened, as Forge's `ThisTurnCast`.
    FromHistory {
        event: Box<EventSpec>,
        window: HistoryWindow,
    },
    /// A selection in the nearest enclosing WithSelections scope.
    Prepared(SelectionId),
    /// Exact exiled incarnations linked at this object's creation. The link is
    /// noncopiable object history and survives its source leaving/ceasing to
    /// exist through LKI. Copies of the object/ability acquire no link. A card
    /// that left exile and returned is a new incarnation and is not included.
    CreationLinkedCards(ObjectRef),
    /// Match historical zone changes using the departing objects' historical
    /// characteristics, then project their first public destination incarnations.
    /// Re-evaluate the history window when this query is evaluated. A projected
    /// object must still be in the destination produced by that event: a second
    /// zone change, including leaving and returning, permanently breaks the link.
    /// This never changes FromHistory's departed-subject projection.
    FromHistoryDestinations {
        event: Box<EventSpec>,
        window: HistoryWindow,
    },
    /// Historical views of objects actually sacrificed by the linked entry
    /// ability when this exact permanent incarnation entered. Selection alone,
    /// unrelated sacrifices, failed entry attempts and earlier incarnations do
    /// not contribute. The consuming ability's linked acquisition is part of the
    /// relation, as for ExiledWith; copies do not acquire another object's history.
    /// Matching and aggregation use the sacrificed objects' historical views,
    /// interpreting predicates through the consuming source's current words.
    SacrificedAsEntered(ObjectRef),
    /// Actual recorded zone changes from this result, restricted by departure
    /// and first arrival zones, then projected to the requested incarnation.
    /// A replacement redirect affects the recorded arrival; a later movement
    /// cannot erase the original event. These are receipt predicates, not live
    /// zone membership tests. At least one zone constraint must be present.
    TransitionWhere {
        result: ResultId,
        phase: ObjectPhase,
        from: Option<EventZone>,
        to: Option<EventZone>,
    },
    /// The actual participants of the referenced fight. If a creature
    /// could not fight, the set is empty rather than the attempted pair.
    FightParticipants(ResultId),
    /// The objects announced for one participant in a per-player target slot.
    /// Keep that association from announcement (or retargeting), independently
    /// of control, current legality or the outcome of an attempted action.
    /// Reads retain the original targets' expected zones and last-known views.
    /// An unassigned participant has an empty set.
    TargetsFor {
        slot: TargetSlot,
        participant: PlayerRef,
    },
    /// Pre-move identities and their event-time last-known characteristics for
    /// every actual transition in a grouped zone-departure event. This snapshot
    /// stays available to the triggered ability after the objects change zones.
    EventGroupDeparture,
    /// Actual matching exile arrivals of an ExileLibraryUntil matching stop,
    /// recorded at arrival. Empty if no matching card actually arrived.
    ExileUntilMatches(ResultId),
    /// Distinct still-legal object targets of the selected modes, available only
    /// in the common program of an AnnouncedShared selection. Unselected modes
    /// contribute nothing; a shared target identity occurs once. The ordinary
    /// all-targets-illegal rule prevents resolution. If no modes were selected,
    /// this set is empty and the common action does nothing.
    ChosenModeTargets,
    /// First owner's-graveyard incarnations of the Auras attached to this
    /// departure event's subject immediately before it left the battlefield.
    /// The Aura must have entered that graveyard simultaneously with the host's
    /// departure or as a state-based action for no longer being attached. Each
    /// must still be that first incarnation in its owner's graveyard; replacement
    /// redirects and later moves (including a return) break the link (CR 400.7f).
    EventDepartureAuras,
    /// Ephemeral copies made by a visible CopyOracleDefinition receipt.
    OracleCopies(ResultId),
    /// The actual pre-placement recipient identities of the named PutCounters
    /// instruction, snapshotted before applying counter replacements. This
    /// includes recipients where all placements were prevented, and excludes
    /// things that became eligible only afterwards. Reads the same live object
    /// incarnations even if their characteristics or controller later change;
    /// objects that leave their zone are not followed to a new incarnation.
    CounterRecipients(ResultId),
    /// The creature chosen for this player's named Blight action (CR 701.68c).
    /// It remains the chosen creature even when counter placement is prevented
    /// or changed. No legal choice yields an empty set; a later new incarnation
    /// is not included. It is not every creature that received counters.
    Blighted(ResultId),
    /// The current winner's own revealed card from this clash, tracked through
    /// the clash placement. Later reorders in the same library preserve that
    /// incarnation (CR 400.7). A subsequent zone change ends the link.
    ClashWinningCard,
    /// The exact card transferred by the active outside-game gift callback.
    OutsideGiftedCard,
}

#[derive(Clone, Debug, Serialize, Deserialize, bincode::Encode, bincode::Decode)]
pub enum ZoneScope {
    Shared(EventZone),
    Players { zone: EventZone, players: PlayerSet },
}

#[derive(Clone, Debug, Serialize, Deserialize, bincode::Encode, bincode::Decode)]
#[serde(tag = "kind", content = "args")]
pub enum ObjectFilter {
    /// Exiled by an ability of this object, as linked by 607.
    ExiledWith(ObjectRef),
    /// One reviewed rules-defined form; see `rewrites/primitives.json5`.
    Primitive {
        name: String,
        operands: Vec<Operand>,
    },
    /// Evaluate a reusable condition with Candidate bound to this object.
    Condition(Box<Condition>),
    /// An actual casting event, retained by the permanent the spell becomes.
    /// Both absent means "was cast". A copied spell was not cast. Origin is
    /// the original zone/owner, not current stack membership or current owner.
    /// This data is not inherited after a later zone change or blink. During
    /// casting legality/payment checks, test the current casting proposal's
    /// actor/origin, before it becomes a completed casting event.
    Cast {
        by: Option<PlayerSet>,
        from: Option<ZoneScope>,
    },
    /// The zone from which this incarnation most recently entered the
    /// battlefield, independently of whether it was cast. A reanimated card
    /// can match a graveyard entry without having been cast from there.
    EnteredFrom(ZoneScope),
    HasKeywordFamily(KeywordFamily),
    /// At least one actual target belongs to this set. For casting-cost rules
    /// inspect the completed announcement, before costs are paid.
    TargetsAny(Box<Recipients>),
    /// What a spell or ability on the stack is.
    OnStack(StackTrait),
    /// An ability whose source matches.
    AbilityOf(Box<ObjectFilter>),
    /// Includes defined abilities even while their functioning zone is inactive.
    HasActivatedAbility(ActivationFilter),
    InSet(Box<ObjectSet>),
    Name(String),
    InZone(ZoneScope),
    /// Blocked by this object.
    BlockedByObject(ObjectRef),
    /// Attacking one of these.
    AttackingDefender(Box<AttackDefenders>),
    /// Has the value of this kind chosen by this ability or as this object entered.
    Chosen(ValueKind),
    IsCard,
    Any,
    AllOf(Vec<ObjectFilter>),
    AnyOf(Vec<ObjectFilter>),
    Not(Box<ObjectFilter>),
    CardType(CardType),
    Supertype(SuperType),
    Subtype(SubType),
    Color(ManaColor),
    Colorless,
    HasKeyword(Keyword),
    ControlledBy(PlayerSet),
    OwnedBy(PlayerSet),
    SameObject(ObjectRef),
    /// Has a name in common with this object (CR 201.2a).
    SharesNameWith(ObjectRef),
    Property {
        property: Property,
        test: NumberTest,
    },
    Counters {
        kind: Option<crate::counter::CounterKind>,
        comparison: Comparison,
        value: Expr,
    },
    Token,
    Tapped,
    Attacking,
    Blocking,
    /// A creature with counters, Equipment, or an Aura controlled by its
    /// controller (the rules definition of modified).
    Modified,
    /// Dealt damage by this earlier instruction of the same resolution.
    DamagedBy(ResultId),
    /// Has at least one value of this property in common with the object.
    Shares {
        property: DistinctProperty,
        with: ObjectRef,
    },
    /// Has the same name as any of these objects.
    SharesNameWithAny(Box<ObjectSet>),
    /// Has the same name as a different object among these, never itself.
    SharesNameWithAnother(Box<ObjectSet>),
    /// Dealt more than lethal damage by this earlier instruction.
    DealtExcessBy(ResultId),
    /// Reached its current zone this turn, from this zone if one is named.
    ArrivedThisTurn {
        from: Option<EventZone>,
    },
    /// Created or put onto the battlefield by an ability of this object.
    MadeBy(ObjectRef),
    /// Has a value of this property in common with any of these objects.
    SharesWithAny {
        property: DistinctProperty,
        with: Box<ObjectSet>,
    },
    /// Was attached to this object as it last existed.
    WasAttachedTo(ObjectRef),
    /// A state the rules define for an object, which no other filter reads.
    State(ObjectState),
    AttachedTo(Box<ObjectFilter>),
    /// Blocking this attacker, not merely blocking.
    BlockingObject(ObjectRef),
    /// The spell cast at `ordinal` among the `spells` cast in `scope`, by
    /// `by` when that is named. Counts earlier casts, not this ability's life.
    /// For a matched cast-event subject, use its recorded ordinal at that cast;
    /// it remains available if the spell later leaves the stack, and later
    /// spells cannot change whether that event was the Nth one.
    NthCast {
        ordinal: Expr,
        spells: Box<ObjectFilter>,
        by: Option<PlayerSet>,
        scope: CastScope,
    },
    /// Was kicked with this kicker (CR 702.33d).
    KickedWith(Vec<Operand>),
    /// Has something matching attached to it.
    HasAttached(Box<ObjectFilter>),
    AttachedToPlayer(PlayerRef),
    /// Has the highest or lowest value of `property` among `among`.
    Extreme {
        among: Box<ObjectSet>,
        property: Property,
        highest: bool,
    },
    /// Mana from a source that matches was spent to cast this spell.
    ManaSpentFrom(Box<ObjectFilter>),
    /// Shares at least one color with this object.
    SharesColorWith(ObjectRef),
    /// The filter chosen by a `ChooseValue` of `ValueKind::Filter`.
    ChosenFilter,
    /// Shares a color with any of a set, as protection from the colors of permanents.
    SharesColorWithAny(Box<ObjectSet>),
    /// What happened to this object in a window, as Forge's `attackedThisTurn`.
    History {
        event: ObjectHistory,
        window: ObjectWindow,
    },
    /// Mana spent to cast it, narrowed by color, source or who paid.
    ManaSpent {
        color: Option<ManaColor>,
        sources: Option<Box<ObjectFilter>>,
        payers: Option<PlayerSet>,
        comparison: Comparison,
        value: Expr,
    },
    /// Cast for this alternative cost.
    AlternateCostPaid(Vec<Operand>),
    /// A printed mana cost, symbol for symbol.
    ManaCostIs(Vec<Operand>),
    ManaSymbolInCost(ManaColor),
    BlockingAny(Box<ObjectFilter>),
    BlockedByAny(Box<ObjectFilter>),
    PairedWith(Box<ObjectFilter>),
    BandedWith(ObjectRef),
    AttachedCount {
        attachments: Box<ObjectFilter>,
        comparison: Comparison,
        value: Expr,
    },
    ProtectedBy(PlayerSet),
    NameFirstPrintedIn(FirstPrinting),
    InStartingDeck(PlayerSet),
    /// Every target it could choose matches.
    /// Every declared target domain is restricted to this population, including
    /// domains in unchosen modes. Requires at least one target domain.
    CanTargetOnly(Box<ObjectFilter>),
    /// Resolving it would destroy a permanent that matches.
    WouldDestroy(Box<ObjectFilter>),
    TargetableBy(ObjectRef),
    CouldEnchant(Box<ObjectSet>),
    CouldProduce(Box<Operand>),
    CouldProduceManaOf(ObjectRef),
    ExiledBy {
        player: PlayerRef,
        with: Option<ObjectRef>,
        this_turn: bool,
    },
    /// Of the type this ability noted.
    Noted(ValueKind),
    ChosenBy {
        kind: ValueKind,
        player: PlayerRef,
    },
    /// Chosen as this object entered or resolved, for the object itself.
    ChosenOn {
        kind: ValueKind,
        object: ObjectRef,
    },
    DraftNoted(DraftNote),
    /// In a chosen pile, or the pile chosen for this object.
    InPile(Option<ObjectRef>),
    /// In the chosen sector, or this object's sector.
    InSector(Option<ObjectRef>),
    NthChosen(u32),
    AttackingSame(ObjectRef),
    CastFromAmong(Box<ObjectSet>),
    /// Shares one of these card types with this object.
    SharesTypeFrom {
        types: Vec<CardType>,
        with: ObjectRef,
    },
    SharesMostCommonColor(Box<ObjectSet>),
    /// A spell or ability with this many targets.
    TargetCount(Comparison, Expr),
    /// Has the creature type most common in this player's library.
    MostCommonCreatureTypeIn(PlayerRef),
    /// Shares a color with this mana. Colorless mana shares none.
    SharesColorWithMana(ManaOccurrence),
    /// Its mana cost could be paid with some or all of this mana.
    ManaCostPayableWith(ManaOccurrence),
    /// A target of a spell or ability on the stack that matches.
    TargetedBy(Box<ObjectFilter>),
    /// Has a color outside the color identity of this player's commander.
    ColorOutsideCommanderIdentity(PlayerRef),
    /// Candidate is a host that this Aura could legally enchant. Unlike
    /// CouldEnchant, the candidate is not the Aura. This does not target.
    CouldBeEnchantedBy(ObjectRef),
    /// Created or put onto the battlefield by the creating ability linked to
    /// this consuming ability (CR 607.2c). Identity includes the object's exact
    /// incarnation, the linked text and its acquisition (CR 607.5), not just the
    /// source object. Unrelated granted abilities and separately acquired copies
    /// never share a link. This relation includes nontokens; intersect Token when
    /// the words ask for tokens. Created(ResultId) remains a resolution receipt.
    CreatedWith(ObjectRef),
    /// Battlefield incarnation from the Assimilate action linked to this
    /// ability (CR 607). Tracks the assimilated permanent through changes of
    /// controller, type and counters, but a new zone incarnation is unrelated.
    AssimilatedWith(ObjectRef),
    /// A card that the named player designated as one of their commanders.
    /// The designation follows the card through zones and control changes.
    CommanderOf(PlayerRef),
    /// Any current rules name matches this text query. An unnamed object
    /// does not match. Apply the usual English-name view (CR 201.2).
    NameMatches(NameMatch),
    /// The character represented by this object's current card or token form.
    /// Character identity comes from the rules catalog for that form.
    RepresentsCharacter(String),
    /// A property of the physical or digital card printing across zones.
    /// Copies use their own origin and never inherit printing properties.
    PrintingHas(PrintingFeature),
    /// An actual completed cast whose payment included a matching participant
    /// in any selected action. Match the original object's pre-action view,
    /// preserving creature type and controller even when sacrifice moved it.
    /// TapForManaSpent requires that this object was actually tapped for the
    /// mana action and that its produced mana paid for this exact cast. Planned
    /// inputs, prevented actions, unspent mana, and copied spells do not match.
    CastPaymentParticipant {
        participant: Box<ObjectFilter>,
        actions: Vec<CastPaymentAction>,
    },
    /// The candidate has at least one allowed creature type that no object in
    /// `others` currently has. Types are sets: a candidate may also share other
    /// types with the population and still match. Empty allowed lists never match.
    HasUnsharedCreatureTypeFrom {
        allowed: Vec<CreatureType>,
        others: Box<ObjectSet>,
    },
    /// Whether this object represents an animal of the stated habitat. The
    /// host supplies the represented animal and habitat from its catalog;
    /// a nonanimal or an unknown habitat matches neither kind.
    RepresentsAnimal(AnimalHabitat),
    /// At least one character represented by this card or token's current
    /// form has this catalogued trait. Character identity is not inferred from
    /// creature subtypes, player identity, artwork, or card name alone.
    RepresentsCharacterTrait(CharacterTrait),
    /// Has the name chosen by the stated name choice in this resolution.
    NameFromChoice(ResultId),
    /// The current physical or digital printing credits the chosen artist.
    /// The same Oracle card may have printings with different artists.
    IllustratedByChoice(ResultId),
    /// Match an object sharing at least one color with `player`'s currently
    /// enrolled affiliation for this exact face-up `plane` visit. Resolve the
    /// affiliation's colors from the specified external catalog in this plane's
    /// context, not from the affiliation label or an object's current colors.
    /// An unassigned player has no matching color. If the catalog is unavailable,
    /// the test is unresolved; do not silently treat it as false or colorless.
    SharesPlaneAffiliationColor {
        plane: ObjectRef,
        player: PlayerRef,
        catalog: PlaneAffiliationCatalog,
    },
    /// The candidate's actual represented text has reminder text. For a cast
    /// proposal this is the chosen spell face. The host supplies facts for
    /// the actual representation. Missing data is indeterminate, including
    /// under negation, and must be obtained before deciding the predicate.
    HasReminderText,
    /// The current object has at least one of these creature types; empty
    /// allowed sets match nothing. This reads types, not represented animals.
    MatchesAnyCreatureType {
        allowed: Vec<CreatureType>,
    },
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize, bincode::Encode, bincode::Decode)]
pub enum CharacterTrait {
    Transgender,
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize, bincode::Encode, bincode::Decode)]
pub enum AnimalHabitat {
    /// Lives in the ocean or another saltwater environment.
    Marine,
    /// An animal whose habitat excludes saltwater environments.
    NonMarine,
}

#[derive(Clone, Debug, Serialize, Deserialize, bincode::Encode, bincode::Decode)]
pub enum NameMatch {
    /// Literal opening text, ignoring ASCII case, without a word boundary.
    Prefix(String),
    /// An English letter occurs at least once, irrespective of capitalization.
    Letter(String),
    /// The whole printed word, not a substring of another word. When enabled,
    /// the word's possessive also matches (for example, Urza and Urza's).
    /// Word boundaries and Unicode normalization follow the host's reviewed
    /// rules-name tokenizer; do not substitute ASCII-only matching.
    Word { word: String, possessive: bool },
}

/// Something that happened to an object. See `ObjectFilter::History`.
#[derive(Clone, Debug, Serialize, Deserialize, bincode::Encode, bincode::Decode)]
pub enum ObjectHistory {
    Attacked,
    AttackedDefender(Box<ObjectFilter>),
    Blocked,
    WasBlocked,
    Regenerated,
    AbilityActivated,
    Entered,
    Drawn(Option<PlayerSet>),
    Discarded,
    Milled,
    Exiled,
    ExiledFromGraveyard(PlayerSet),
    PutIntoGraveyard,
    DealtCombatDamage,
    DamagedBy(ObjectRef),
    Enlisted(Box<ObjectFilter>),
    Convoked(ObjectRef),
    Exploited(ObjectRef),
    TappedToPayFor(ObjectRef),
    DiedOrPutFromBattlefield,
    CycledOrDiscarded,
    CountersPutBy {
        kind: crate::counter::CounterKind,
        by: PlayerSet,
    },
    CountersPutWith {
        kind: crate::counter::CounterKind,
        with: ObjectRef,
    },
    CountersRemovedWith {
        kind: crate::counter::CounterKind,
        with: ObjectRef,
    },
    PhasedOutWith(ObjectRef),
    Cycled,
    Sacrificed,
    Surveilled,
    /// Put into a hand, as Forge's `ThisTurnEnteredYourHand`.
    PutIntoHand,
}

#[derive(Clone, Debug, Serialize, Deserialize, bincode::Encode, bincode::Decode)]
pub enum ObjectWindow {
    ThisTurn,
    ThisCombat,
    ThisGame,
    LastTurnOf(PlayerRef),
    SinceLastUpkeepOf(PlayerRef),
    SinceLastTurnOf(PlayerRef),
    LastCombatOf(PlayerRef),
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize, bincode::Encode, bincode::Decode)]
pub enum FirstPrinting {
    ArabianNights,
    Antiquities,
    Homelands,
}

impl ObjectFilter {
    /// Membership of the objects named by one result, independent of spelling.
    pub fn named_result(&self) -> Option<&ResultId> {
        match self {
            Self::InSet(objects) => match objects.as_ref() {
                ObjectSet::Recorded(result) => Some(result),
                _ => None,
            },
            _ => None,
        }
    }
}

impl ObjectSet {
    /// The shared battlefield contains permanents, including tokens.
    pub fn battlefield(filter: ObjectFilter) -> Self {
        crate::battlefield_query!(filter)
    }

    pub fn battlefield_filter(&self) -> Option<&ObjectFilter> {
        match self {
            crate::battlefield_query!(filter) => Some(filter),
            _ => None,
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, bincode::Encode, bincode::Decode)]
pub enum PrintingFeature {
    Foil,
    /// This edition came from a Secret Lair product.
    SecretLair,
    /// The symbol actually printed on this card, not a set in which another
    /// card with the same Oracle identity was printed. Catalog resolution must
    /// identify the symbol on this individual physical or digital printing.
    ExpansionSymbol(ExpansionSymbolKind),
    /// An original Limited Edition Alpha printing. It has no expansion symbol;
    /// this predicate excludes Beta, other symbol-less printings and reprints.
    AlphaPrinting,
    /// This actual printing belongs to a Mystery Booster set, including later
    /// sets with that product branding. This is not a name-wide property.
    MysteryBoosterSet,
    /// This actual printing is a playtest card, independent of its set.
    PlaytestCard,
}

#[derive(
    Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, bincode::Encode, bincode::Decode,
)]
pub enum CastPaymentAction {
    TapForCost,
    TapForManaSpent,
    SacrificeForCost,
}

/// Reviewed classes of actual printed expansion symbols.
#[derive(Clone, Copy, Debug, Serialize, Deserialize, bincode::Encode, bincode::Decode)]
pub enum ExpansionSymbolKind {
    /// The Foundations FDN symbol, not a different Foundations product symbol.
    Foundations,
    /// A symbol that denotes a core set; an absent symbol never matches.
    CoreSet,
}
