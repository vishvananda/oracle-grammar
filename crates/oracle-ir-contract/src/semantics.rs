//! Rules-derived contracts shared by all Oracle IR producers and consumers.
use crate::ir::*;
use crate::keyword_actions::KeywordKind;
use crate::movement::legacy::EntryState;

/// References guaranteed by every possible occurrence of an event matcher.
/// Derived from the typed matcher rather than serialized as independent flags.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct EventBindings {
    pub object: bool,
    pub permanent: bool,
    pub spell: bool,
    pub player: bool,
    pub amount: bool,
    /// Actual counters placed by the matched event, retained through wrappers.
    pub counters_placed: bool,
    pub moved: bool,
    /// The new incarnation is in hand, though Source calls it a graveyard card.
    pub moved_to_hand: bool,
    /// A matched Ring temptation retains its choice fact.
    pub ring_choice: bool,
    pub damage: bool,
    pub damage_source: bool,
    pub damage_source_permanent: bool,
    pub damage_recipient: bool,
    /// One player or object took the damage, whichever it was.
    pub damage_any_recipient: bool,
    pub damage_objects: bool,
    pub damage_players: bool,
    pub damage_subject: Option<DamageSubject>,
    pub discard: bool,
    pub discarded: bool,
    pub activation_x: bool,
    pub block_pair: bool,
    pub group: bool,
    pub stack_object: bool,
    pub other: bool,
    pub other_player: bool,
    pub moved_owner: bool,
    /// The distinguished event object was this Aura's enchanted permanent as it
    /// left the battlefield; enables the Aura's separate CR 400.7f reference.
    pub enchanted_left: bool,
}

const KEYWORD_ACTIONS: &[(&str, KeywordKind)] = &[
    (
        "Action::ChooseADamageNumber",
        KeywordKind::ChooseDamageNumber,
    ),
    ("Action::ChooseASector", KeywordKind::ChooseSector),
    ("Action::ClaimThePrize", KeywordKind::ClaimPrize),
    (
        "Action::ChooseProtectionFromAColorOrFromArtifact",
        KeywordKind::ChooseProtection,
    ),
    ("Action::SetStake", KeywordKind::SetStake),
    ("Action::DoubleTheStake", KeywordKind::DoubleStake),
    ("Action::ChooseCopyFromCopies", KeywordKind::ChooseCopy),
    (
        "Action::PutTopOfPlanarDeckOnBottomOfPlanarDeck",
        KeywordKind::TopOfPlanarDeckToBottom,
    ),
    (
        "Action::RevealTopCardOfPlanarDeck",
        KeywordKind::RevealTopOfPlanarDeck,
    ),
    (
        "Action::RevealCardsFromTheTopOfPlanarDeckUntilRevealAPlaneCardThenChaosEnsuresOnThatPlaneThenPutAllCardsOnBottomInAnyOrder",
        KeywordKind::PlanarDeckChaos,
    ),
    (
        "Action::RevealCardsFromTheTopOfPlanarDeckUntilRevealAPlaneCardThenPlaneswalkToItWhileNotPlaneswalkingAwayAndPutTheRestOnBottomInAnyOrder",
        KeywordKind::PlanarDeckPlaneswalkWithoutLeaving,
    ),
    (
        "Action::RevealCardsFromTheTopOfPlanarDeckUntilRevealNumberPlaneCardsAndPutAPlaneCardFromAmongOnTopAndTheRestOnBottomInARandomOrder",
        KeywordKind::PlanarDeckPickPlane,
    ),
    (
        "Action::RevealCardsFromTheTopOfPlanarDeckUntilRevealNumberPlaneCardsSimultaneouslyPlaneswalkToThemThenPutOnBottomInAnyOrder",
        KeywordKind::PlanarDeckPlaneswalkToEach,
    ),
    (
        "Action::RevealTheTopNumberCardsOfPlanarDeckAndTriggerEachCHAOSAbilityThenPutOnBottomOfPlanarDeckInAnyOrder",
        KeywordKind::PlanarDeckChaosEach,
    ),
    (
        "Action::ShuffleLibraryIfSearched",
        KeywordKind::ShuffleIfSearched,
    ),
    (
        "Action::RerollAnyNumberOfTheStoredD6Results",
        KeywordKind::RerollStoredDice,
    ),
    (
        "Action::RollNumberD6AndStoreTheResult",
        KeywordKind::RollAndStoreD6,
    ),
    (
        "Action::SetRepeatablePlayers",
        KeywordKind::SetRepeatablePlayers,
    ),
    ("Action::SetSchemeInMotion", KeywordKind::SetSchemeInMotion),
    (
        "Action::ReorderPlayersGraveyard",
        KeywordKind::ReorderGraveyard,
    ),
    (
        "Action::DiscardAnyNumberOfCardsAtRandom",
        KeywordKind::DiscardAnyNumberAtRandom,
    ),
    (
        "Action::PutTheRemainingLibraryCardsOnTheBottomOfLibraryInARandomOrder",
        KeywordKind::RemainingToBottomAtRandom,
    ),
    ("Action::TransformPermanent", KeywordKind::Transform),
    ("Action::Fight", KeywordKind::Fight),
    ("Action::CollectEvidence", KeywordKind::CollectEvidence),
    ("Action::Investigate", KeywordKind::Investigate),
    ("Action::Proliferate", KeywordKind::Proliferate),
    ("Action::Amass", KeywordKind::Amass),
    ("Action::ExploreWithPermanent", KeywordKind::Explore),
    ("Action::TemptWithRing", KeywordKind::RingTempts),
    ("Action::CreatureConnives", KeywordKind::Connive),
    ("Action::GoadCreature", KeywordKind::Goad),
    ("Action::Earthbend", KeywordKind::Earthbend),
    ("Action::VentureIntoTheDungeon", KeywordKind::Venture),
    ("Action::ManifestDread", KeywordKind::ManifestDread),
    ("Action::Recruit", KeywordKind::Recruit),
    ("Action::Epic", KeywordKind::Epic),
    ("Action::Paradigm", KeywordKind::Paradigm),
    (
        "Action::TakeANumberOfExtraTurns",
        KeywordKind::TakeExtraTurn,
    ),
    ("Action::Monstrosity", KeywordKind::Monstrosity),
    ("Action::Discover", KeywordKind::Discover),
    ("Action::GetEnergy", KeywordKind::GainEnergy),
    ("Action::BecomeTheMonarch", KeywordKind::BecomeMonarch),
    ("Action::LoseTheGame", KeywordKind::LoseGame),
    ("Action::WinTheGame", KeywordKind::WinGame),
    ("Action::TakeAnExtraTurn", KeywordKind::TakeExtraTurn),
    ("Action::PhaseOutPermanent", KeywordKind::PhaseOut),
    ("Action::PreparePermanent", KeywordKind::Prepare),
    ("Action::TapOrUntapPermanent", KeywordKind::TapOrUntap),
    ("Action::ExchangeControl", KeywordKind::ExchangeControl),
    ("Action::RollAD20", KeywordKind::RollDie(20)),
    ("Action::RollAD10", KeywordKind::RollDie(10)),
    ("Action::RollAD6", KeywordKind::RollDie(6)),
    ("Action::RollNumberD6", KeywordKind::RollDie(6)),
    ("Action::RollAD4", KeywordKind::RollDie(4)),
    ("Action::RollNumberD4", KeywordKind::RollDie(4)),
    ("Action::EmpowerJace", KeywordKind::EmpowerJace),
    ("Action::Incubate", KeywordKind::Incubate),
    ("Action::Adapt", KeywordKind::Adapt),
    ("Action::Populate", KeywordKind::Populate),
    ("Action::InvestigateTimes", KeywordKind::Investigate),
    ("Action::ProliferateNumberTimes", KeywordKind::Proliferate),
    ("Action::CreatureConnivesNumber", KeywordKind::Connive),
    ("Action::Bolster", KeywordKind::Bolster),
    ("Action::Learn", KeywordKind::Learn),
    ("Action::TakeTheInitiative", KeywordKind::TakeInitiative),
    ("Action::OpenAnAttraction", KeywordKind::OpenAttraction),
    ("Action::GetNumberTickets", KeywordKind::GetTickets),
    (
        "Action::RemoveCreatureFromCombat",
        KeywordKind::RemoveFromCombat,
    ),
    ("Action::FlipPermanent", KeywordKind::Flip),
    ("Action::AwakenPermanent", KeywordKind::Awaken),
    ("Action::Planeswalk", KeywordKind::Planeswalk),
    ("Action::ConvertPermanent", KeywordKind::Convert),
    ("Action::SuspectPermanent", KeywordKind::Suspect),
    ("Action::EndureWithPermanent", KeywordKind::Endure),
    ("Action::Blight", KeywordKind::Blight),
    ("Action::TurnPermanentFaceUp", KeywordKind::TurnFaceUp),
    ("Action::TurnPermanentFaceDown", KeywordKind::TurnFaceDown),
    ("Action::DetainPermanent", KeywordKind::Detain),
    ("Action::EndTheTurn", KeywordKind::EndTheTurn),
    ("Action::TimeTravel", KeywordKind::TimeTravel),
    ("Action::IntensifyPermanent", KeywordKind::Intensify),
    ("Action::Forage", KeywordKind::Forage),
    ("Action::Ascend", KeywordKind::Ascend),
    ("Action::BecomeDay", KeywordKind::BecomeDay),
    ("Action::BecomeNight", KeywordKind::BecomeNight),
    ("Action::ChaosEnsues", KeywordKind::ChaosEnsues),
    ("Action::AirbendPermanent", KeywordKind::Airbend),
    ("Action::SaddlePermanent", KeywordKind::Saddle),
    ("Action::HarnessPermanent", KeywordKind::Harness),
    ("Action::UnattachPermanent", KeywordKind::Unattach),
    ("Action::Fateseal", KeywordKind::Fateseal),
    ("Action::SkipNextTurn", KeywordKind::Skip(SkippedPart::Turn)),
    (
        "Action::SkipNextUntapStep",
        KeywordKind::Skip(SkippedPart::UntapStep),
    ),
    (
        "Action::SkipNextDrawStep",
        KeywordKind::Skip(SkippedPart::DrawStep),
    ),
    (
        "Action::SkipNextCombatPhase",
        KeywordKind::Skip(SkippedPart::CombatPhase),
    ),
    (
        "Action::SkipNextNumberTurns",
        KeywordKind::Skip(SkippedPart::Turn),
    ),
    (
        "Action::SkipNextCombatPhaseThisTurn",
        KeywordKind::Skip(SkippedPart::CombatPhaseThisTurn),
    ),
    (
        "Action::SkipAllCombatPhasesTheirNextTurn",
        KeywordKind::Skip(SkippedPart::AllCombatPhasesNextTurn),
    ),
    ("Action::RollAD8", KeywordKind::RollDie(8)),
    ("Action::RollAD12", KeywordKind::RollDie(12)),
    ("Action::AirbendSpell", KeywordKind::Airbend),
    ("Action::Assimilate", KeywordKind::Assimilate),
    ("Action::PopulateNumberTimes", KeywordKind::Populate),
    ("Action::TimeTravelNumberTimes", KeywordKind::TimeTravel),
    (
        "Action::ManifestDreadNumberTimes",
        KeywordKind::ManifestDread,
    ),
    ("Action::OpenNumberAttractions", KeywordKind::OpenAttraction),
    (
        "Action::ExploreWithPermanentNumberTimes",
        KeywordKind::Explore,
    ),
    ("Action::PhaseInPermanent", KeywordKind::PhaseIn),
    ("Action::UnlockADoorOfPermanent", KeywordKind::UnlockDoor),
    (
        "Action::LockOrUnlockADoorOfPermanent",
        KeywordKind::LockOrUnlockDoor,
    ),
    ("Action::SetCreatureAsBlocked", KeywordKind::BecomeBlocked),
    (
        "Action::RemoveCreatureFromCombatAndUnblockBlockers",
        KeywordKind::RemoveFromCombatUnblocking,
    ),
    ("Action::ReverseTurnOrder", KeywordKind::ReverseTurnOrder),
    ("Action::LoseUnspentMana", KeywordKind::LoseUnspentMana),
    (
        "Action::DoubleEachTypeOfUnspentMana",
        KeywordKind::DoubleUnspentMana,
    ),
    ("Action::DrawTheGame", KeywordKind::DrawGame),
    ("Action::ShuffleGraveyard", KeywordKind::ShuffleGraveyard),
    ("Action::RollThePlanarDie", KeywordKind::RollPlanarDie),
    (
        "Action::RollToVisitAttractions",
        KeywordKind::VisitAttractions,
    ),
    ("Action::EndTheCombatPhase", KeywordKind::EndCombat),
    (
        "Action::RollNumberD20AndIgnoreAllButHighest",
        KeywordKind::RollKeepHighest(20),
    ),
    (
        "Action::RollTwoD4AndChooseAnOrder",
        KeywordKind::RollTwoInOrder(4),
    ),
    (
        "Action::RollTwoD6AndChooseAnOrder",
        KeywordKind::RollTwoInOrder(6),
    ),
    (
        "Action::RollTwoD8AndChooseAnOrder",
        KeywordKind::RollTwoInOrder(8),
    ),
    (
        "Action::RollTwoD10AndChooseAnOrder",
        KeywordKind::RollTwoInOrder(10),
    ),
    (
        "Action::RollTwoD12AndChooseAnOrder",
        KeywordKind::RollTwoInOrder(12),
    ),
    (
        "Action::TakeAnExtraTurnAfterNextTurn",
        KeywordKind::TakeExtraTurnAfterNext,
    ),
    (
        "Action::TakeAnExtraTurnAndSkipUntapStepOfThatTurn",
        KeywordKind::TakeExtraTurnSkippingUntap,
    ),
    ("Action::PlayAMagicSubgame", KeywordKind::PlaySubgame),
    (
        "Action::ThereIsAnAdditionalCombatPhase",
        KeywordKind::AdditionalPhase(AddedPhases::Combat),
    ),
    (
        "Action::ThereIsAnAdditionalCombatPhaseAndAnAdditionalMainPhase",
        KeywordKind::AdditionalPhase(AddedPhases::CombatAndMain),
    ),
    (
        "Action::ThereIsAnAdditionalBeginningPhase",
        KeywordKind::AdditionalPhase(AddedPhases::Beginning),
    ),
];

/// `outer` with what a reviewed row adds for its own operands.
/// Trigger rows that are one player event. Each keeps its reviewed `binds`.
const PLAYER_EVENTS: &[(&str, PlayerEvent)] = &[
    ("Trigger::WhenAPlayerGainsLife", PlayerEvent::GainsLife),
    (
        "Trigger::WhenAPlayerUntapsAnyNumberOfPermanentDuringTheirUntapStep",
        PlayerEvent::UntapsDuringUntapStep,
    ),
    (
        "Trigger::WhenAPermanentEntersTheBattlefieldUnderAPlayersControl",
        PlayerEvent::EntersUnderControl,
    ),
    (
        "Trigger::WhenAPermanentEntersTheBattlefieldUnderAPlayersControlWithoutBeingPlayed",
        PlayerEvent::EntersUnplayed,
    ),
    ("Trigger::WhenAPlayerLosesLife", PlayerEvent::LosesLife),
    ("Trigger::WhenAPlayerDrawsACard", PlayerEvent::Draws),
    ("Trigger::WhenAPlayerAttacks", PlayerEvent::Attacks),
    ("Trigger::WhenAPlayerScrys", PlayerEvent::Scries),
    ("Trigger::WhenAPlayerSurveils", PlayerEvent::Surveils),
    (
        "Trigger::WhenAPlayerCommitsACrime",
        PlayerEvent::CommitsCrime,
    ),
    (
        "Trigger::WhenAPlayerSearchesTheirLibrary",
        PlayerEvent::SearchesLibrary,
    ),
    ("Trigger::WhenAPlayerLosesTheGame", PlayerEvent::LosesGame),
    (
        "Trigger::WhenAPlayerRollsAnyNumberOfDice",
        PlayerEvent::RollsDice,
    ),
    (
        "Trigger::WhenAPlayerWinsACoinFlip",
        PlayerEvent::WinsCoinFlip,
    ),
    (
        "Trigger::WhenAPlayerSacrificesAPermanent",
        PlayerEvent::Sacrifices,
    ),
    ("Trigger::WhenAPlayerPlaysALand", PlayerEvent::PlaysLand),
    ("Trigger::WhenAPlayerUnlocksADoor", PlayerEvent::UnlocksDoor),
    ("Trigger::WhenChaosEnsues", PlayerEvent::ChaosEnsues),
    (
        "Trigger::WhenAPlayerSetsASchemeInMotion",
        PlayerEvent::SetsSchemeInMotion,
    ),
    (
        "Trigger::WhenAPlayerPlaneswalksToAPlane",
        PlayerEvent::Planeswalks,
    ),
    (
        "Trigger::WhenAPlayerEncountersAPhenomenon",
        PlayerEvent::EncountersPhenomenon,
    ),
    (
        "Trigger::WhenAPlayerFullyUnlocksARoom",
        PlayerEvent::FullyUnlocksRoom,
    ),
    (
        "Trigger::WhenAPlayerTapsAPermanentForMana",
        PlayerEvent::TapsForMana,
    ),
    (
        "Trigger::WhenDayBecomesNightOrNightBecomesDay",
        PlayerEvent::DayNightChanges,
    ),
    (
        "Trigger::WhenPlayersFinishVoting",
        PlayerEvent::FinishVoting,
    ),
    ("Trigger::AtTheBeginningOfTheGame", PlayerEvent::GameBegins),
    (
        "Trigger::WhenAPlayerLosesControlOfAPermanent",
        PlayerEvent::LosesControl,
    ),
    ("Trigger::WhenTheRingTemptsAPlayer", PlayerEvent::RingTempts),
    (
        "Trigger::WhenAPlayerCreatesAToken",
        PlayerEvent::CreatesToken,
    ),
    ("Trigger::WhenAPlayerExertsACreature", PlayerEvent::Exerts),
    (
        "Trigger::WhenAPlayerProliferates",
        PlayerEvent::Proliferates,
    ),
    (
        "Trigger::WhenAPlayerRevealsFirstCardDrawn",
        PlayerEvent::RevealsFirstCardDrawn,
    ),
    (
        "Trigger::WhenAPlayerPlaysACardFromExile",
        PlayerEvent::PlaysCardFromExile,
    ),
    (
        "Trigger::WhenAPlayerPlaysALandFromExile",
        PlayerEvent::PlaysLandFromExile,
    ),
    (
        "Trigger::WhenAPlayerPutsAPermanentOnTheBattlefield",
        PlayerEvent::PutsOntoBattlefield,
    ),
    ("Trigger::WhenAPlayerGetsEnergy", PlayerEvent::GetsEnergy),
    (
        "Trigger::WhenAPlayerBecomesTheMonarch",
        PlayerEvent::BecomesMonarch,
    ),
    (
        "Trigger::WhenAPlayerCompletesADungeon",
        PlayerEvent::CompletesDungeon,
    ),
    (
        "Trigger::WhenAPlayerTapsAPermanent",
        PlayerEvent::TapsPermanent,
    ),
    ("Trigger::WhenAPlayerPlaysACard", PlayerEvent::PlaysCard),
    (
        "Trigger::WhenAPlayerDrawsARevealedCard",
        PlayerEvent::DrawsRevealedCard,
    ),
    (
        "Trigger::WhenAPlayerDrawsASpecificCard",
        PlayerEvent::DrawsCard,
    ),
    (
        "Trigger::WhenAPlayerSeeksAnyNumberOfCards",
        PlayerEvent::Seeks,
    ),
    (
        "Trigger::WhenAPlayerVisitsAnAttraction",
        PlayerEvent::VisitsAttraction,
    ),
    ("Trigger::WhenAPlayerRollsADie", PlayerEvent::RollsADie),
    (
        "Trigger::WhenAPlayerRollsThePlanarDie",
        PlayerEvent::RollsPlanarDie,
    ),
    (
        "Trigger::WhenAPlayerShufflesTheirLibrary",
        PlayerEvent::Shuffles,
    ),
    (
        "Trigger::WhenAPlayerConjuresAnyNumberOfCards",
        PlayerEvent::Conjures,
    ),
    (
        "Trigger::WhenAPlayerConjuresAnyNumberOfOtherCards",
        PlayerEvent::ConjuresOther,
    ),
    ("Trigger::WhenAPlayerDiscovers", PlayerEvent::Discovers),
    ("Trigger::WhenAPlayerClashes", PlayerEvent::Clashes),
    ("Trigger::WhenAPlayerClashesAndWins", PlayerEvent::WinsClash),
    (
        "Trigger::WhenAPlayerInvestigates",
        PlayerEvent::Investigates,
    ),
    (
        "Trigger::WhenAPlayerCollectsEvidence",
        PlayerEvent::CollectsEvidence,
    ),
    (
        "Trigger::WhenAPlayerPutsAStickerOnAPermanent",
        PlayerEvent::PutsSticker,
    ),
    (
        "Trigger::WhenAPlayerPutsANameStickerOnAPermanent",
        PlayerEvent::PutsNameSticker,
    ),
    (
        "Trigger::WhenAPlayerPutsAnAbilityStickerOnAPermanent",
        PlayerEvent::PutsAbilitySticker,
    ),
    (
        "Trigger::WhenAPlayerPutsAnArtStickerOnAPermanent",
        PlayerEvent::PutsArtSticker,
    ),
    (
        "Trigger::WhenAPlayerPlacesASticker",
        PlayerEvent::PlacesSticker,
    ),
    (
        "Trigger::WhenAPlayerRollsToVisitTheirAttractions",
        PlayerEvent::RollsToVisit,
    ),
    (
        "Trigger::WhenAPlayerWaterEarthFireOrAirBends",
        PlayerEvent::Bends,
    ),
    (
        "Trigger::WhenAPlayerChoosesARingBearer",
        PlayerEvent::ChoosesRingBearer,
    ),
    ("Trigger::WhenAPlayerSolvesACase", PlayerEvent::SolvesCase),
    (
        "Trigger::WhenAPlayerSelectsAnyNumberOfTargetsAtRandom",
        PlayerEvent::SelectsTargetsAtRandom,
    ),
    (
        "Trigger::WhenAPlayerSelectsAModeAtRandom",
        PlayerEvent::SelectsModeAtRandom,
    ),
    (
        "Trigger::WhenAPlayerFlipsAnyNumberOfCoins",
        PlayerEvent::FlipsCoins,
    ),
    ("Trigger::WhenAPlayerForages", PlayerEvent::Forages),
    (
        "Trigger::WhenAPlayerRollsANatural20",
        PlayerEvent::RollsNatural20,
    ),
    (
        "Trigger::WhenAPlayerRollsADiesHighestNaturalResult",
        PlayerEvent::RollsHighestNatural,
    ),
    (
        "Trigger::WhenAPlayerRollsABlankOnThePlanarDie",
        PlayerEvent::RollsPlanarBlank,
    ),
    ("Trigger::WhenAPlayerForetellsACard", PlayerEvent::Foretells),
    ("Trigger::WhenAPlayerPaysLife", PlayerEvent::PaysLife),
    ("Trigger::WhenAPlayerGivesAGift", PlayerEvent::GivesGift),
    (
        "Trigger::WhenAPlayerLosesACoinFlip",
        PlayerEvent::LosesCoinFlip,
    ),
    (
        "Trigger::WhenAPlayerTurnsAPermanentFaceUp",
        PlayerEvent::TurnsFaceUp,
    ),
    (
        "Trigger::WhenAPlayerSacrificesAPermanentForEmerge",
        PlayerEvent::SacrificesForEmerge,
    ),
    (
        "Trigger::WhenAPlayerPaysAPermanentsCumulativeUpkeepCost",
        PlayerEvent::PaysCumulativeUpkeep,
    ),
    (
        "Trigger::WhenAPlayerDoesntPayAPermanentsCumulativeUpkeepCost",
        PlayerEvent::DeclinesCumulativeUpkeep,
    ),
    (
        "Trigger::WhenAPlayerMillsASpecificCard",
        PlayerEvent::MillsCard,
    ),
    (
        "Trigger::WhenAPlayerMillsAnyNumberOfCards",
        PlayerEvent::Mills,
    ),
    (
        "Trigger::WhenAPlayerManifestsDread",
        PlayerEvent::ManifestsDread,
    ),
    (
        "Trigger::WhenAPlayerFacesADilemma",
        PlayerEvent::FacesDilemma,
    ),
    (
        "Trigger::WhenAPlayerOpensAnAttraction",
        PlayerEvent::OpensAttraction,
    ),
    (
        "Trigger::WhenAPlayerClaimsThePrizeOfAnAttraction",
        PlayerEvent::ClaimsPrize,
    ),
    (
        "Trigger::WhenAPlayerDiscardsACardAtRandom",
        PlayerEvent::DiscardsAtRandom,
    ),
    (
        "Trigger::WhenAPlayerPlaysALandFromAnywhereOtherThanTheirHand",
        PlayerEvent::PlaysLandNotFromHand,
    ),
    (
        "Trigger::WhenAPlayerChoosesToPutAnyCardsOnTheBottomOfTheirLibraryWhileScrying",
        PlayerEvent::BottomsWhileScrying,
    ),
    (
        "Trigger::WhenAPlayerPlaneswalksAwayFromAPlane",
        PlayerEvent::PlaneswalksAway,
    ),
    (
        "Trigger::WhenAPlayerBecomesTheTargetOfASpellOrAbility",
        PlayerEvent::BecomesTarget,
    ),
    (
        "Trigger::WhenAPlayerBecomesTheTargetOfASpell",
        PlayerEvent::BecomesTarget,
    ),
];

const OBJECT_EVENTS: &[(&str, ObjectEvent)] = &[
    ("Trigger::WhenAPermanentTransforms", ObjectEvent::Transforms),
    (
        "Trigger::WhenAPermanentBecomesMonstrous",
        ObjectEvent::BecomesMonstrous,
    ),
    ("Trigger::WhenACreatureMutates", ObjectEvent::Mutates),
    (
        "Trigger::WhenACreatureSpecializes",
        ObjectEvent::Specializes,
    ),
    (
        "Trigger::WhenAPermanentIsTappedForMana",
        ObjectEvent::TappedForMana,
    ),
    ("Trigger::WhenAVehicleBecoemsCrewed", ObjectEvent::Crewed),
    ("Trigger::WhenAPermanentExplores", ObjectEvent::Explores),
    (
        "Trigger::WhenAPermanentLeavesTheBattlefieldWithoutDying",
        ObjectEvent::LeavesWithoutDying,
    ),
    ("Trigger::WhenAPermanentPhasesIn", ObjectEvent::PhasesIn),
    ("Trigger::WhenAPermanentPhasesOut", ObjectEvent::PhasesOut),
    ("Trigger::WhenAPermanentConnives", ObjectEvent::Connives),
    ("Trigger::WhenAPermanentFights", ObjectEvent::Fights),
    (
        "Trigger::WhenACreatureBecomesRenowned",
        ObjectEvent::BecomesRenowned,
    ),
    ("Trigger::WhenAPermanentEvolves", ObjectEvent::Evolves),
    ("Trigger::WhenACreatureIsDevoured", ObjectEvent::Devoured),
    ("Trigger::WhenAPermanentIsDestroyed", ObjectEvent::Destroyed),
    (
        "Trigger::WhenAPermanentIsSacrificed",
        ObjectEvent::Sacrificed,
    ),
    ("Trigger::WhenAPermanentTrains", ObjectEvent::Trains),
    (
        "Trigger::WhenAPermanentsCumulativeUpkeepCostIsPaid",
        ObjectEvent::CumulativeUpkeepPaid,
    ),
    (
        "Trigger::WhenAnEchoCostOfAPermanentIsPaid",
        ObjectEvent::EchoPaid,
    ),
    (
        "Trigger::WhenAPermanentBecomesTappedToPayATeamworkCost",
        ObjectEvent::TappedForTeamwork,
    ),
    (
        "Trigger::WhenTheFinalChapterOfASagaResolves",
        ObjectEvent::FinalChapterResolves,
    ),
    (
        "Trigger::WhenTheFinalChapterOfASagaTriggers",
        ObjectEvent::FinalChapterTriggers,
    ),
];

const PAIR_EVENTS: &[(&str, PairEvent)] = &[
    ("Trigger::WhenACreatureBlocksACreature", PairEvent::Blocks),
    (
        "Trigger::WhenACreatureBecomesBlockedByACreature",
        PairEvent::BecomesBlockedBy,
    ),
    ("Trigger::WhenACreatureCrewsAVehicle", PairEvent::Crews),
    ("Trigger::WhenACreatureSaddlesAMount", PairEvent::Saddles),
    (
        "Trigger::WhenAPermanentBecomesAttachedToAPermanent",
        PairEvent::BecomesAttachedTo,
    ),
    (
        "Trigger::WhenAPermanentExploitsAPermanent",
        PairEvent::Exploits,
    ),
    ("Trigger::WhenACreatureEnlistsACreature", PairEvent::Enlists),
    ("Trigger::WhenACreatureMentorsACreature", PairEvent::Mentors),
    (
        "Trigger::WhenAPermanentBecomesUnattachedFromAPermanent",
        PairEvent::BecomesUnattachedFrom,
    ),
    (
        "Trigger::WhenAPermanentIsChampionedWithAPermanent",
        PairEvent::ChampionedWith,
    ),
    (
        "Trigger::WhenAPermanentStationsAPermanent",
        PairEvent::Stations,
    ),
    (
        "Trigger::WhenAPermanentEntersTheBattlefieldAttachedToAPermanent",
        PairEvent::EntersAttachedTo,
    ),
    (
        "Trigger::WhenAPermanentTransformsFromIntoAPermanent",
        PairEvent::TransformsInto,
    ),
];

impl EventBindings {
    /// What every alternative of an either-event still binds.
    fn shared(a: Self, b: Self) -> Self {
        Self {
            object: a.object && b.object,
            permanent: a.permanent && b.permanent,
            spell: a.spell && b.spell,
            player: a.player && b.player,
            amount: a.amount && b.amount,
            counters_placed: a.counters_placed && b.counters_placed,
            moved: a.moved && b.moved,
            moved_to_hand: a.moved_to_hand && b.moved_to_hand,
            ring_choice: a.ring_choice && b.ring_choice,
            block_pair: a.block_pair && b.block_pair,
            group: a.group && b.group,
            stack_object: a.stack_object && b.stack_object,
            other: a.other && b.other,
            other_player: a.other_player && b.other_player,
            moved_owner: a.moved_owner && b.moved_owner,
            enchanted_left: a.enchanted_left && b.enchanted_left,
            damage: a.damage && b.damage,
            damage_source: a.damage_source && b.damage_source,
            damage_source_permanent: a.damage_source_permanent && b.damage_source_permanent,
            damage_recipient: a.damage_recipient && b.damage_recipient,
            damage_any_recipient: a.damage_any_recipient && b.damage_any_recipient,
            damage_objects: a.damage_objects && b.damage_objects,
            damage_players: a.damage_players && b.damage_players,
            damage_subject: if a.damage_subject == b.damage_subject {
                a.damage_subject
            } else {
                None
            },
            discard: a.discard && b.discard,
            discarded: a.discarded && b.discarded,
            activation_x: a.activation_x && b.activation_x,
        }
    }
}

mod same_player;
use self::same_player::chosen_kind;
pub use self::same_player::{
    makes_card, reduce_changes, reduce_rule, replacement_bindings, requires_typed_keyword,
    row_bindings, same_player, shuffle_closes, singular_operation, yields,
};

mod stack_filter;
pub use self::stack_filter::{
    counter_kind, entry_return_event, event_bindings, with_event, with_row,
};
use self::stack_filter::{event_filter, stack_filter, with_bindings};

mod event_bindings_case_2;
use self::event_bindings_case_2::{
    card_term, color_form, copy_of, event_bindings_case_2, gain_abilities, objects_filter,
    perpetual_changes, reduce_card_layer, reduce_layer, vocabulary_form,
};
pub use self::event_bindings_case_2::{delayed_reads_event, with_replacement};

mod reduce_object_static;
pub use self::reduce_object_static::reduce_entry_flag;
use self::reduce_object_static::{
    keyword_for, mana_cost, mana_terms, reduce_object_static, settable_color, vocabulary_value,
};
