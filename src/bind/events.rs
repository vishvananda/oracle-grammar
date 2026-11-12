//! Matched-event referents and the current incarnation of moved objects.
use super::{Scope, ability_kind, holder, json, names_self, presentation};
use serde_json::Value;

pub(super) fn inner(event: &Value) -> Option<&Value> {
    for wrapper in ["FirstEachTurn", "FirstForObjectEachTurn", "FirstEachGame"] {
        if let Some(inner) = event.get(wrapper) {
            return Some(inner);
        }
    }
    ["Filtered", "NthEachTurn", "Caused", "WhileActivating"]
        .into_iter()
        .find_map(|wrapper| event.get(wrapper).and_then(|v| v.get("event")))
}

/// The cost part a later `the sacrificed creature` points at.
fn named_cost_part(ability: &Value) -> Option<u64> {
    let parts = ability["args"]["cost"]["parts"].as_array()?;
    let index = parts
        .iter()
        .position(|part| part.get("Sacrifice").is_some())?;
    u64::try_from(index).ok()
}

#[derive(Clone, Copy)]
pub(super) enum Quantity {
    Amount,
    Objects,
}

impl Quantity {
    pub(super) fn expression(self) -> Value {
        match self {
            Self::Amount => json!({"kind":"EventAmount"}),
            Self::Objects => json!({"kind":"CountObjects","args":{"kind":"EventGroup"}}),
        }
    }
}

/// Object batches count their matched group; numerical events retain their amount.
fn event_quantity(event: &Value) -> Option<Quantity> {
    if group_count(event) {
        Some(Quantity::Objects)
    } else {
        event_has_amount(event).then_some(Quantity::Amount)
    }
}

fn group_count(event: &Value) -> bool {
    if let Some(inner) = inner(event) {
        return group_count(inner);
    }
    if let Some(cases) = event["AnyOf"].as_array() {
        return !cases.is_empty() && cases.iter().all(group_count);
    }
    let Some(batch) = event.get("Batch") else {
        return false;
    };
    (batch["per"] != "Subject" || batch["event"].get("Pair").is_some())
        && object_batch(&batch["event"])
}

fn object_batch(event: &Value) -> bool {
    if let Some(inner) = inner(event) {
        return object_batch(inner);
    }
    if let Some(cases) = event["AnyOf"].as_array() {
        return !cases.is_empty() && cases.iter().all(object_batch);
    }
    [
        "Attack",
        "AttackUnblocked",
        "ZoneChange",
        "ZoneChangeFromElsewhere",
        "Object",
        "Pair",
        "Spell",
        "BecomesTarget",
    ]
    .iter()
    .any(|kind| event.get(kind).is_some())
        || matches!(
            event["Player"]["event"].as_str(),
            Some("EntersUnderControl" | "CreatesToken" | "Sacrifices")
        )
}

/// Whether the event exposes a numerical quantity at all.
fn event_has_amount(event: &Value) -> bool {
    if let Some(inner) = inner(event) {
        return event_has_amount(inner);
    }
    if let Some(cases) = event["AnyOf"].as_array() {
        return cases.iter().any(event_has_amount);
    }
    event.get("Damage").is_some()
        || event["Discard"]["grouping"] == "Player"
        || event["Batch"].is_object()
        || matches!(
            event["Player"]["event"].as_str(),
            Some("GainsLife" | "LosesLife")
        )
}

fn has_object(event: &Value) -> bool {
    object_availability(event, true)
}

fn object_availability(event: &Value, native_player: bool) -> bool {
    if let Some(inner) = inner(event) {
        return object_availability(inner, native_player);
    }
    if let Some(cases) = event["AnyOf"].as_array() {
        return !cases.is_empty()
            && cases
                .iter()
                .all(|case| object_availability(case, native_player));
    }
    if let Some(name) = event["Primitive"]["name"].as_str() {
        return crate::model::semantics::row_bindings(name).object;
    }
    if controller_turn(event) {
        return true;
    }
    if native_player && let Some(available) = super::player_event::object_binding(event) {
        return available;
    }
    !["Turn", "State", "SagaChapter"]
        .iter()
        .any(|kind| event.get(kind).is_some())
}

/// A step named through a permanent's controller captures both that permanent
/// and that player. Ordinary turn events still have no object subject.
fn controller_turn(event: &Value) -> bool {
    if let Some(inner) = inner(event) {
        return controller_turn(inner);
    }
    if let Some(events) = event["AnyOf"].as_array() {
        return !events.is_empty() && events.iter().all(controller_turn);
    }
    event["Turn"]["players"]["kind"] == "One"
        && event["Turn"]["players"]["args"]["kind"] == "ControllerOf"
}

/// Header descriptions select the triggering object; they must not replace
/// the body's captured `that creature` / `that player` with a live attachment.
pub(super) fn bind_controller_turn_header(
    ability: &mut Value,
    scope: &Scope,
) -> Result<(), String> {
    if !matches!(ability_kind(ability), Some("Triggered" | "CreateTrigger"))
        || !controller_turn(&ability["args"]["event"])
    {
        return Ok(());
    }
    let mut header = ability["args"]["event"].take();
    let mut prefix = scope.nested();
    prefix.objects = vec![json!({"kind": "Source"})];
    prefix.others.clear();
    prefix.players.clear();
    prefix.ambiguous_owner = None;
    prefix.references.clear();
    prefix.named_object = None;
    prefix.pronoun = None;
    super::bind_scope(&mut header, &mut prefix)?;
    ability["args"]["event"] = header;
    Ok(())
}

pub(super) fn object_event(event: &Value) -> Option<&Value> {
    if let Some(inner) = inner(event) {
        return object_event(inner);
    }
    if let Some(cases) = event.get("AnyOf").and_then(Value::as_array) {
        let first = object_event(cases.first()?)?;
        let owned = mentions(first, "OwnedBy");
        return cases
            .iter()
            .all(|event| {
                object_event(event).is_some_and(|objects| mentions(objects, "OwnedBy") == owned)
            })
            .then_some(first);
    }
    event
        .get("ZoneChange")
        .or_else(|| event.get("ZoneChangeFromElsewhere"))
        .or_else(|| event.get("Object"))
        .map(|inner| &inner["objects"])
}

fn mentions(value: &Value, kind: &str) -> bool {
    match value {
        Value::Object(object) => {
            value.get("kind").and_then(Value::as_str) == Some(kind)
                || object.values().any(|v| mentions(v, kind))
        }
        Value::Array(items) => items.iter().any(|v| mentions(v, kind)),
        _ => false,
    }
}

/// A joined pair event names the other participant with the same description
/// only when every branch supplies that description. Never pick one branch.
fn pair_other_filter(event: &Value) -> Option<&Value> {
    if let Some(inner) = inner(event) {
        return pair_other_filter(inner);
    }
    if let Some(cases) = event["AnyOf"].as_array() {
        let first = pair_other_filter(cases.first()?)?;
        return cases
            .iter()
            .all(|case| pair_other_filter(case) == Some(first))
            .then_some(first);
    }
    if matches!(
        event["Pair"]["event"].as_str(),
        Some("Blocks" | "BecomesBlockedBy")
    ) {
        event.pointer("/Pair/others")
    } else {
        None
    }
}

/// What a trigger's event makes available to `it`, `that creature` and `that player`.
/// Canonical form surfaces a damage event's own role as EventSubject only
/// through `Filtered`; under any other wrapper that role is spelled directly.
fn event_referents(event: &Value, scope: &mut Scope, surfaced: bool) {
    if let Some(inner) = inner(event) {
        event_referents(inner, scope, surfaced && event.get("Filtered").is_some());
        return;
    }
    let role = |name: &str| {
        if surfaced {
            json!({"kind": "Damage", "args": name})
        } else {
            json!({"kind": "EventSubject"})
        }
    };
    let subject = json!({"kind":"EventSubject"});
    let spell = super::spell_focus::facts(event);
    // Chapters, state checks and ordinary turn events introduce no new object.
    if has_object(event) {
        scope.objects.push(subject.clone());
        scope.defending_attacker = Some(crate::model::ir::ObjectRef::EventSubject);
    }
    match object_event(event) {
        // A permanent's event: its player is the one who held it.
        Some(objects) if mentions(objects, "OwnedBy") => scope
            .players
            .push(json!({"kind": "OwnerOf", "args": subject})),
        Some(_) => scope
            .players
            .push(json!({"kind": "ControllerOf", "args": subject})),
        // "Attack with a creature an opponent owns": that player owns the attacker.
        None if event
            .get("Attack")
            .is_some_and(|attack| mentions(&attack["attackers"], "OwnedBy")) =>
        {
            scope
                .players
                .push(json!({"kind": "OwnerOf", "args": subject}));
        }
        None if event.get("BecomesTarget").is_some() => scope.players.push(json!({
            "kind": "ControllerOf",
            "args": {"kind": "EventStackObject"},
        })),
        None => scope.players.push(subject),
    }
    scope.event_quantity = event_quantity(event);
    let damage = &event["Damage"];
    if damage["subject"] == "Source" {
        scope.objects.push(role("Source"));
    }
    if damage["recipients"].get("Objects").is_some() {
        if damage["subject"] == "Recipient" {
            scope.objects.push(role("Recipient"));
        } else {
            scope
                .others
                .push(json!({"kind": "Damage", "args": "Recipient"}));
        }
    }
    if event["Pair"]["event"] == "BecomesBlockedBy" {
        scope
            .others
            .push(json!({"kind": "BlockPair", "args": "Blocker"}));
    } else if event["AnyOf"][0].get("Pair").is_some() || event.get("Pair").is_some() {
        scope
            .others
            .push(json!({"kind": "BlockPair", "args": "Other"}));
    }
    scope.named_object = scope.object();
    if has_object(event) {
        let filter = super::qualified::selector(event).unwrap_or(&Value::Null);
        let moved =
            event.get("ZoneChange").is_some() || event.get("ZoneChangeFromElsewhere").is_some();
        scope.references.push(super::qualified::event(
            scope.named_object.clone().unwrap(),
            filter,
            moved,
            spell.subject,
        ));
    }
    if spell.stack {
        scope.references.push(super::qualified::event(
            json!({"kind":"EventStackObject"}),
            &json!({"kind":"Any"}),
            false,
            true,
        ));
    }
    if spell.damage_source {
        scope.references.push(super::qualified::event(
            json!({"kind":"Damage","args":"Source"}),
            &json!({"kind":"Any"}),
            false,
            true,
        ));
    }
    if let Some(filter) = pair_other_filter(event)
        && let Some(reference) = scope.others.last()
    {
        scope.references.push(super::qualified::event(
            reference.clone(),
            filter,
            false,
            false,
        ));
    }
}

pub(super) fn scope_for(ability: &Value, card_name: &str, holders: u16) -> Scope {
    let ability = presentation::core(ability);
    let cast_x = super::variables::entry(ability);
    let triggered = matches!(ability_kind(ability), Some("Triggered" | "CreateTrigger"));
    let event = if triggered {
        Frame::of(&ability["args"]["event"])
    } else {
        Frame::default()
    };
    let mut scope = Scope {
        card_name: card_name.to_string(),
        objects: vec![json!({"kind": "Source"})],
        leaving: event.left_source,
        objectless_player_event: triggered
            && object_availability(&ability["args"]["event"], false)
            && !has_object(&ability["args"]["event"]),
        other_source_event: triggered && super::other::source_event(&ability["args"]["event"]),
        other_source_damage: triggered
            .then(|| super::other::source_damage(&ability["args"]["event"]))
            .flatten(),
        event,
        cost_index: named_cost_part(ability),
        holders,
        defined_x: super::payments::declared(&ability["args"]["cost"]),
        possible_announced_x: cast_x
            || ability_kind(ability) == Some("Spell")
            || ability["args"]["event"]["ZoneChange"]["to"] == "Battlefield"
                && holder::self_event(&ability["args"]["event"], holders),
        ..Scope::default()
    };
    if triggered {
        event_referents(&ability["args"]["event"], &mut scope, true);
        // A granted or token holder's own event names that holder as its
        // subject, so `it` reads the event as it does in a printed ability.

        // `If this artifact has ..., put a counter on it`: the condition named the card last.
        let condition = &ability["args"]["intervening_if"];
        if mentions(condition, "Source") || names_self(condition) {
            scope.pronoun = Some(json!({"kind": "Source"}));
        }
    }
    scope
}

/// A scoped ability has its own event. Modes and lexical closures inherit it.
/// A reflexive trigger owns targets while sharing its parent's result cursor.
pub(super) fn bind_reflexive(node: &mut Value, scope: &mut Scope) -> Result<Value, String> {
    super::reflexive_receipts::bind(node, scope, None)
}

pub(super) fn bind_nested(node: &mut Value, parent: &mut Scope) -> Result<Value, String> {
    let mut inner = node["args"]
        .get_mut(0)
        .ok_or("a scope needs its contents")?
        .take();
    let created = ability_kind(&inner) == Some("CreateTrigger");
    let departure_controller = super::departure_controller::prepare(&mut inner)?;
    if created {
        holder::event_sources(&mut inner, parent.holders)?;
        // The creating effect announces targets in the event/duration. Targets
        // in the future body belong to that trigger's own announcement.
        let mut header = json!({"event":inner["args"]["event"].take(),
            "duration":inner["args"]["duration"].take()});
        super::bind_scope(&mut header, parent)?;
        inner["args"]["event"] = header["event"].take();
        inner["args"]["duration"] = header["duration"].take();
    }
    let mut nested = if matches!(
        ability_kind(presentation::core(&inner)),
        Some("Triggered" | "CreateTrigger")
    ) {
        holder::event_sources(&mut inner, parent.holders)?;
        scope_for(&inner, &parent.card_name, parent.holders)
    } else {
        parent.nested()
    };
    super::departure_controller::seed(&mut nested, departure_controller);
    nested.binding_floor = parent.binding_floor;
    if created {
        nested.target_base = parent.target_floor;
        nested.results = parent.results.clone();
        // A created trigger installs its own event. Keep its typed event
        // capabilities when inheriting the creator's targets and receipts.
        let event_references = std::mem::take(&mut nested.references);
        nested.references = parent.references.clone();
        nested.references.extend(event_references);
        nested.defined_x = parent.defined_x;
        nested.snapshot_payment = parent.snapshot_payment;
        nested.possible_announced_x = parent.possible_announced_x;
        nested.cost_index = parent.cost_index;
        nested.marked = parent.marked;
        nested.counter_choice = parent.counter_choice;
        nested.manifest_result = parent.manifest_result;
        nested.created_result = parent.created_result;
        nested.drafted_result = parent.drafted_result;
    }
    super::bind_scope(&mut inner, &mut nested)?;
    if created {
        parent.results = nested.results;
        parent.target_floor = parent.target_floor.max(nested.target_floor);
    }
    Ok(inner)
}

#[derive(Clone, Default)]
enum Destination {
    #[default]
    Absent,
    Zone(Value),
    Public,
    Discarded,
}

/// Binding metadata, not spelling: which incarnation an event makes available.
#[derive(Clone, Default)]
pub(super) struct Frame {
    before: bool,
    own: bool,
    left_source: bool,
    enchanted_left: bool,
    partial_enchanted_left: bool,
    destination: Destination,
}

fn source_filter(value: &Value) -> bool {
    value == &json!({"kind":"SameObject","args":{"kind":"Source"}})
        || value["kind"] == "AllOf"
            && value["args"]
                .as_array()
                .is_some_and(|parts| parts.iter().any(source_filter))
}

fn enchanted_filter(value: &Value, bound: bool) -> bool {
    if value["kind"] != "SameObject" {
        return false;
    }
    let mut object = &value["args"];
    let mut enchanted = bound;
    while matches!(
        super::symbol_name(object),
        Some("remember_operand" | "remember_object" | "enchanted_operand")
    ) {
        enchanted |= super::symbol_name(object) == Some("enchanted_operand");
        let Some(inner) = object["args"].get(0) else {
            return false;
        };
        object = inner;
    }
    enchanted && *object == json!({"kind":"AttachedTo","args":{"kind":"Source"}})
}

fn public(zone: &Value) -> bool {
    matches!(
        zone.as_str(),
        Some("Battlefield" | "Graveyard" | "Exile" | "Stack" | "Command" | "Ante")
    )
}

fn singular_sacrifice(objects: &Value) -> Frame {
    let own = source_filter(objects);
    Frame {
        before: true,
        own,
        left_source: own,
        // A sacrifice may be replaced by a move to a different public zone.
        // CR 400.7e follows that first destination, not an assumed graveyard.
        destination: Destination::Public,
        ..Frame::default()
    }
}

impl Frame {
    fn of(event: &Value) -> Self {
        Self::of_inner(event, false)
    }

    fn of_inner(event: &Value, bound: bool) -> Self {
        if let Some(inner) = inner(event) {
            return Self::of_inner(inner, bound);
        }
        if let Some(cases) = event.get("AnyOf").and_then(Value::as_array) {
            let frames: Vec<_> = cases.iter().map(|e| Self::of_inner(e, bound)).collect();
            let mut frame = Self {
                before: frames.iter().any(|f| f.before),
                own: !frames.is_empty() && frames.iter().all(|f| f.own),
                enchanted_left: !frames.is_empty() && frames.iter().all(|f| f.enchanted_left),
                ..Self::default()
            };
            frame.left_source = frame.own && frames.iter().any(|f| f.left_source);
            frame.partial_enchanted_left = !frame.enchanted_left
                && frames
                    .iter()
                    .any(|f| f.enchanted_left || f.partial_enchanted_left);
            let zones: Vec<_> = frames
                .iter()
                .filter_map(|f| match &f.destination {
                    Destination::Zone(zone) => Some(zone),
                    _ => None,
                })
                .collect();
            if !zones.is_empty() && zones.len() == frames.len() {
                frame.destination = if zones.iter().all(|z| **z == *zones[0]) {
                    Destination::Zone(zones[0].clone())
                } else {
                    Destination::Public
                };
            } else if !frames.is_empty()
                && frames
                    .iter()
                    .all(|f| matches!(f.destination, Destination::Zone(_) | Destination::Public))
            {
                frame.destination = Destination::Public;
            } else if !frames.is_empty()
                && frames
                    .iter()
                    .all(|f| matches!(f.destination, Destination::Discarded))
            {
                frame.destination = Destination::Discarded;
            }
            return frame;
        }
        if let Some(change) = event.get("ZoneChangeFromElsewhere") {
            return Self {
                before: change["to"] != "Battlefield",
                own: source_filter(&change["objects"]),
                destination: if public(&change["to"]) {
                    Destination::Zone(change["to"].clone())
                } else {
                    Destination::Absent
                },
                ..Self::default()
            };
        }
        if let Some(change) = event.get("ZoneChange") {
            let own = source_filter(&change["objects"]);
            let from = &change["from"];
            let to = &change["to"];
            let before = to != "Battlefield" && !from.is_null() && !(own && from != "Battlefield");
            return Self {
                before,
                own,
                left_source: own && before && from == "Battlefield",
                enchanted_left: from == "Battlefield"
                    && enchanted_filter(&change["objects"], bound),
                partial_enchanted_left: false,
                destination: if public(to) {
                    Destination::Zone(to.clone())
                } else {
                    Destination::Absent
                },
            };
        }
        if event["Object"]["event"] == "Sacrificed" {
            return singular_sacrifice(&event["Object"]["objects"]);
        }
        if event["Player"]["event"] == "Sacrifices" {
            return singular_sacrifice(&event["Player"]["objects"]);
        }
        if event["Object"]["event"] == "LeavesBattlefield" {
            let own = source_filter(&event["Object"]["objects"]);
            return Self {
                before: true,
                own,
                left_source: own,
                destination: Destination::Absent,
                ..Self::default()
            };
        }
        if event["Discard"]["grouping"] == "Card" {
            return Self {
                before: true,
                own: event["Discard"]["source_only"] == true,
                destination: Destination::Discarded,
                ..Self::default()
            };
        }
        Self::default()
    }

    pub(super) fn source_destination(&self) -> Option<Value> {
        self.own
            .then(|| self.reference(json!({"kind":"EventSubject"}), true).ok())
            .flatten()
    }

    pub(super) fn of_ability(ability: &Value) -> Self {
        // Bound IR has erased the enchanted/equipped spelling distinction.
        // This constructor only proposes spellings; semantic reparse checks
        // whether the emitted event really supplies the Aura reference.
        Self::of_inner(&ability["args"]["event"], true)
    }

    /// CR 400.7f grants the Aura its own first graveyard incarnation. This is
    /// independent of the host's destination and of the Aura's live attachment.
    pub(super) fn aura_destination(&self) -> Option<Value> {
        self.enchanted_left.then(|| {
            json!({"kind":"AuraAfterEnchantedLeft","args":{
            "aura":{"kind":"Source"},"host":{"kind":"EventSubject"}}})
        })
    }

    /// An explicit return from a graveyard tests and moves the same current card.
    pub(super) fn return_card(&self, reference: Value) -> Result<Value, String> {
        let reference = self.returning_aura(reference)?;
        if reference["kind"] == "Source" {
            return Err("graveyard return has no causal current-card incarnation".into());
        }
        self.reference(reference, true)
    }

    pub(super) fn returning_aura(&self, reference: Value) -> Result<Value, String> {
        if reference["kind"] == "Source" && self.partial_enchanted_left {
            return Err("return has no uniform Aura incarnation across event alternatives".into());
        }
        if reference["kind"] == "Source"
            && let Some(aura) = self.aura_destination()
        {
            return Ok(aura);
        }
        Ok(reference)
    }

    /// Damage by a discarded card follows that card after the discard, including
    /// replacement destinations. Battlefield departures retain their LKI source.
    pub(super) fn damage_source(&self, source: Value) -> Result<Value, String> {
        if matches!(self.destination, Destination::Discarded) {
            self.reference(source, false)
        } else {
            Ok(source)
        }
    }

    pub(super) fn reference(&self, reference: Value, current: bool) -> Result<Value, String> {
        let names_event =
            reference["kind"] == "EventSubject" || reference["kind"] == "Source" && self.own;
        if !names_event {
            return Ok(reference);
        }
        // An entering event already names the new permanent. Do not introduce
        // an extra zone check unless the wording explicitly asks for a card view.
        if !current
            && !self.before
            && matches!(&self.destination, Destination::Zone(zone) if zone == "Battlefield")
        {
            return Ok(reference);
        }
        match &self.destination {
            Destination::Zone(zone) => Ok(json!({"kind":"Query","args":{"objects":{
                "kind":"InZone","args":{"objects":{"kind":"One","args":{"kind":"EventDestination"}},"zone":zone}},"index":0}})),
            Destination::Public => Ok(json!({"kind":"EventDestination"})),
            Destination::Discarded => Ok(json!({"kind":"Discarded"})),
            Destination::Absent if !self.before => Ok(reference),
            Destination::Absent => Err(
                "the event has no uniformly available public destination for this action".into(),
            ),
        }
    }
}

#[cfg(test)]
mod player_object_availability_tests {
    use super::*;

    #[test]
    fn only_changed_player_object_inference_disables_default_card_antecedent() {
        let draw = json!({"Player":{"event":"Draws"}});
        let attack = json!({"Object":{"event":"Attacks"}});
        for (event, old, new) in [
            (draw.clone(), true, false),
            (attack.clone(), true, true),
            (json!({"AnyOf":[draw.clone(),attack]}), true, false),
            (json!({"AnyOf":[draw,{"Turn":{}}]}), false, false),
            (json!({"Player":{"event":"CreatesToken"}}), true, true),
            (
                json!({"NthEachTurn":{"event":{"AnyOf":[
                    {"Player":{"event":"Draws"}},{"Object":{"event":"Attacks"}}
                ]}}}),
                true,
                false,
            ),
            (
                json!({"Filtered":{"event":{"Player":{"event":"Draws"}}}}),
                true,
                false,
            ),
        ] {
            assert_eq!(object_availability(&event, false), old);
            assert_eq!(has_object(&event), new);
        }
    }
}
