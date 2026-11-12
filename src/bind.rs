//! Resolves the reference symbols patterns leave behind, and undoes that for
//! rendering. English refers back with `it` and `target`; the IR uses slots and
//! result ids. Work runs in reading order, by the token each symbol started on.
use serde_json::{Value, json};

pub(crate) mod activation_property;
mod addresses;
pub(crate) mod attached_counter_other;
mod branches;
mod combat;
mod conditional;
mod controller_role;
mod created;
mod damage_actor;
pub(crate) mod damage_reflexive;
pub(crate) mod damage_relative;
pub(crate) mod departure_controller;
mod drafted;
mod events;
pub(crate) mod exile_followups;
mod exile_returns;
mod exiled_card;
mod fight_counterpart;
mod filters;
mod optional_entered_card;
mod optional_exile_target;
mod payment_move_after;
mod player_event;
pub(crate) mod return_grants;
pub(crate) mod returned_conditionals;
mod search_move;
mod selections;
pub(crate) mod target_contrast;
mod target_surface;
use events::scope_for;
mod guards;
pub(crate) mod linked_card_actions;
mod loop_card_focus;
pub(crate) mod member_focus;
mod presentation;
mod qualified;
pub(crate) use filters::operand as filter_operand;
use guards::{
    blocks_itself, cancels_before_choice, condition_placement, has_cost_as_action,
    life_margin_order, protection_domain_mismatch, single_holder_affected,
};
mod coin_protocol;
mod holder;
mod move_after;
mod movement;
mod objects;
mod other;
mod owner_return;
mod paid_self_exile;
mod payments;
mod per_recipient;
pub(crate) mod pump_counts;
mod references;
mod reflexive_move;
mod reflexive_receipts;
mod reflexive_x;
mod resolution_receipts;
mod results;
mod revealed_card;
mod sacrifice_actor;
mod sacrificed;
mod spell_focus;
mod transitions;
mod unresolved_leave_player;
mod value_snapshot;
mod variables;
use references::{acted_on, collect};

#[derive(Debug, Clone)]
struct Found {
    at: u64,
    /// Operand references, target introductions, then transparent focus wrappers.
    order: u8,
    path: Vec<Step>,
    filter: filters::Context,
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum Step {
    Key(String),
    Index(usize),
}

fn symbol_name(value: &Value) -> Option<&str> {
    value.get("$sym").and_then(Value::as_str)
}

fn at_path<'a>(mut value: &'a mut Value, path: &[Step]) -> Option<&'a mut Value> {
    for step in path {
        value = match step {
            Step::Key(key) => value.get_mut(key)?,
            Step::Index(index) => value.get_mut(*index)?,
        };
    }
    Some(value)
}

#[derive(Default, Clone)]
struct Scope {
    targets: Vec<Value>,
    references: Vec<qualified::Referent>,
    /// Reflexive triggers retain outer slots and declare fresh local addresses.
    target_base: usize,
    /// Reserve outer targets, including ones written after a created trigger.
    target_floor: usize,
    results: results::Results,
    reflexive_receipt: Option<reflexive_receipts::Receipt>,
    defined_x: bool,
    /// Conservative snapshot guard inherited by lexical continuations.
    snapshot_payment: bool,
    possible_announced_x: bool,
    objects: Vec<Value>,
    /// The event or target remains available by name after a source mention.
    named_object: Option<Value>,
    /// What `that creature` can name that `it` does not: the other party to an event.
    others: Vec<Value>,
    players: Vec<Value>,
    /// A plural owner phrase cannot supply one player until a new player is named.
    ambiguous_owner: Option<usize>,
    controller_role: Option<controller_role::Role>,
    /// Explicit provenance: an own-object event supplied only a default player.
    default_object_player: Option<usize>,
    /// The direct top-level ability owns this card-reference guard; never inherited.
    root_card_scope: bool,
    /// Whether the event carries a number for `that much`.
    event_quantity: Option<events::Quantity>,
    event: events::Frame,
    /// The distinguished event object is provably this ability holder.
    other_source_event: bool,
    /// The exact distinguished damage role proven to be the holder.
    other_source_damage: Option<crate::model::ir::DamageSubject>,
    /// Combat descriptions retain their attacker when ordinary pronoun focus changes.
    defending_attacker: Option<crate::model::ir::ObjectRef>,
    /// Generated player loops cannot shadow a binding already declared by this ability.
    binding_floor: u64,
    leaving: bool,
    card_name: String,
    /// Which part of an activation cost sacrificed the object it names.
    cost_index: Option<u64>,
    /// The instruction whose choice `it` names across later instructions.
    marked: Option<results::ResultId>,
    /// What `it` names after a trigger's condition, where that differs from `objects`.
    pronoun: Option<Value>,
    /// How many changes of ability holder separate this source from the card.
    holders: u16,
    /// The latest counter choice's result and whether it has at most one object.
    counter_choice: Option<(u64, bool)>,
    /// The most recent manifest/cloak result and whether it is at most one card.
    manifest_result: Option<(u64, bool)>,
    /// Latest token batch, independent of the ability holder and source.
    created_result: Option<(u64, bool)>,
    /// Losing an inferred Player object supplies no default card antecedent.
    objectless_player_event: bool,
    /// Latest drafted card group; only one actor drafting once supplies one card.
    drafted_result: Option<(u64, bool)>,
    exiled_card: Option<exiled_card::Receipt>,
    sacrificed_result: Option<sacrificed::Information>,
}

impl Scope {
    fn object(&self) -> Option<Value> {
        self.objects.last().cloned()
    }
    fn object_pronoun(&self, in_condition: bool) -> Result<Value, String> {
        self.pronoun
            .clone()
            .filter(|_| !in_condition)
            .or_else(|| self.object())
            .ok_or("no object for this pronoun".into())
    }
    fn named(&self) -> Option<Value> {
        self.others.last().or(self.objects.last()).cloned()
    }
    fn player(&self) -> Option<Value> {
        if self.ambiguous_owner == Some(self.players.len()) {
            return None;
        }
        self.players.last().cloned()
    }
    /// A nested scope reads the outer referents and owns its slots and results.
    fn nested(&self) -> Self {
        Self {
            targets: Vec::new(),
            references: self.references.clone(),
            target_base: 0,
            target_floor: 0,
            results: results::Results::default(),
            reflexive_receipt: None,
            defined_x: self.defined_x,
            snapshot_payment: self.snapshot_payment,
            possible_announced_x: self.possible_announced_x,
            objects: self.objects.clone(),
            named_object: self.named_object.clone(),
            others: self.others.clone(),
            players: self.players.clone(),
            ambiguous_owner: self.ambiguous_owner,
            controller_role: self.controller_role.clone(),
            default_object_player: None,
            root_card_scope: false,
            event_quantity: self.event_quantity,
            event: self.event.clone(),
            other_source_event: self.other_source_event,
            other_source_damage: self.other_source_damage,
            defending_attacker: self.defending_attacker.clone(),
            binding_floor: self.binding_floor,
            leaving: self.leaving,
            card_name: self.card_name.clone(),
            cost_index: self.cost_index,
            marked: self.marked,
            pronoun: self.pronoun.clone(),
            holders: self.holders,
            counter_choice: self.counter_choice,
            manifest_result: self.manifest_result,
            created_result: self.created_result,
            objectless_player_event: self.objectless_player_event,
            drafted_result: self.drafted_result,
            exiled_card: self.exiled_card.clone(),
            sacrificed_result: self.sacrificed_result.clone(),
        }
    }
}

fn ability_kind(value: &Value) -> Option<&str> {
    value.get("kind").and_then(Value::as_str)
}

/// Whether a value holds the unbound `@self`.
fn names_self(value: &Value) -> bool {
    match value {
        Value::Object(object) => {
            matches!(symbol_name(value), Some("self" | "named_source"))
                || object.values().any(names_self)
        }
        Value::Array(items) => items.iter().any(names_self),
        _ => false,
    }
}

/// Resolve one ability in place.
pub fn bind_ability(ability: &mut Value, card_name: &str) -> Result<(), String> {
    let ability = presentation::core_mut(ability);
    target_contrast::prepare(ability)?;
    let departure_controller = departure_controller::prepare(ability)?;
    damage_reflexive::prepare(ability)?;
    sacrifice_actor::prepare(ability)?;
    member_focus::prepare(ability)?;
    move_after::prepare(ability)?;
    condition_placement(ability)?;
    holder::event_sources(ability, 0)?;
    unresolved_leave_player::prepare(ability)?;
    let paid_self_exile = paid_self_exile::prepare(ability)?;
    let mut scope = scope_for(ability, card_name, 0);
    scope.root_card_scope = true;
    if controller_role::default_player(ability, &scope) {
        scope.default_object_player = Some(scope.players.len());
    }
    departure_controller::seed(&mut scope, departure_controller);
    scope.binding_floor = per_recipient::binding_floor(ability)?;
    bind_scope(ability, &mut scope)?;
    target_contrast::lower(ability)?;
    payments::bind(ability)?;
    paid_self_exile::lower(ability, paid_self_exile)?;
    if has_cost_as_action(ability) {
        return Err("a payment with a cost of its own is that cost".into());
    }
    if cancels_before_choice(ability) {
        return Err("a declined replacement keeps its event".into());
    }
    if single_holder_affected(ability) {
        return Err("a single holder's pump names that holder".into());
    }
    if blocks_itself(ability) {
        return Err("a creature cannot be required to block itself".into());
    }
    if protection_domain_mismatch(ability) {
        return Err("protection names the domain its quality proves".into());
    }
    if life_margin_order(ability, false) {
        return Err("a life margin keeps its restriction's operand order".into());
    }
    variables::bind(ability)?;
    exile_followups::lower(ability)?;
    return_grants::lower(ability)?;
    returned_conditionals::lower(ability, &mut scope)?;
    selections::bind(ability)?;
    coin_protocol::lower(ability)?;
    crate::functions::definitions::resolve(ability)?;
    spell_focus::check_spell_grants(ability)
}

fn replace_all(value: &mut Value, old: &Value, new: &Value) {
    crate::traverse::replace(value, old, new);
}

fn result_reference(
    name: &str,
    root: &Value,
    path: &[Step],
    scope: &Scope,
    prior: &references::Contexts,
) -> Result<Value, String> {
    match name {
        "created_reference" | "created_reference_single" => created::reference(
            prior.before(path, scope),
            name == "created_reference_single",
            false,
        ),
        "created_batch_reference" | "created_named_reference" => {
            created::reference(scope, name == "created_named_reference", true)
        }
        "exiled_card_reference" => exiled_card::reference(prior.before(path, scope), path),
        _ => drafted::reference(root, path, scope, name == "drafted_reference_explicit"),
    }
}

fn bind_scope(root: &mut Value, scope: &mut Scope) -> Result<(), String> {
    let root = presentation::core_mut(root);
    attached_counter_other::prepare(root)?;
    holder::mark(root, scope.holders);
    events::bind_controller_turn_header(root, scope)?;
    let mut found = Vec::new();
    collect(
        root,
        &mut Vec::new(),
        &mut found,
        filters::Context::Outside,
        &mut scope.snapshot_payment,
    );
    references::reserve_targets(root, &found, scope)?;
    // A target starts its own reading, so what its filter names comes first.
    damage_reflexive::sort_references(root, &mut found);
    let mut announcements = Vec::new();
    let mut slots = Vec::new();
    let mut shares = Vec::new();
    let mut shape_objects = false;
    let mut introduced = Vec::new();
    let mut prior = references::Contexts::default();
    for item in found {
        let acted = acted_on(&item.path, root);
        prior.attachment_host(&item.path, root, scope);
        prior.condition(&item.path, root, scope);
        let name = symbol_name(at_path(root, &item.path).ok_or("binder path")?)
            .unwrap_or_default()
            .to_string();
        if bind_marker(root, scope, &item.path, &name, &introduced)? {
            continue;
        }
        let announced = name == "qualified_object" && transitions::announcement(root, &item.path);
        let created = scope.created_result.map(|(result, _)| result);
        let entered = optional_entered_card::reference(root, &item.path, &name, created)?;
        let counterpart = if name == "qualified_object" && entered.is_none() {
            fight_counterpart::reference(root, &item.path, &introduced, scope, &prior)?
        } else {
            None
        };
        let node = at_path(root, &item.path).ok_or("binder path")?;
        let replacement = if let Some(entered) = entered {
            entered
        } else {
            match name.as_str() {
                "coin_protocol" => json!("Called"),
                "coin_receipt" => coin_protocol::receipt(root, &item.path)?,
                "created_reference"
                | "created_reference_single"
                | "created_batch_reference"
                | "created_named_reference"
                | "drafted_reference"
                | "drafted_reference_explicit"
                | "exiled_card_reference" => {
                    result_reference(&name, root, &item.path, scope, &prior)?
                }
                sacrifice_actor::ACTOR => sacrifice_actor::reference(root, &item.path)?,
                "qualified_members" => qualified::members(node, scope)?,
                "group_reference" => {
                    qualified::group_reference(node, prior.before(&item.path, scope))?
                }
                "return_ref" => scope.event.return_card(node["args"][0].clone())?,
                "qualified_object" => counterpart.map_or_else(
                    || qualified::resolve(node, prior.before(&item.path, scope), announced),
                    Ok,
                )?,
                "counter_reference" | "counter_reference_single"
                    if item.filter == filters::Context::Outside =>
                {
                    objects::counter_reference(scope, name == "counter_reference_single")?
                }
                "counter_reference" | "counter_reference_single" => {
                    return Err("counter recipient reference inside a predicate".into());
                }
                "damage_relative_object" => {
                    damage_relative::bind(root, scope, &item, &introduced, &prior)?
                }
                "remember_object" | "remember_operand" | "enchanted_operand" => objects::remember(
                    node,
                    scope,
                    name == "remember_object" || acted,
                    &item.path,
                    &slots,
                )?,
                "owner_return" => owner_return::remember(node, scope, &item.path, &slots)?,
                "selection" => continue,
                "selection_id" => selections::index(node)?,
                "zone_members" => {
                    shape_objects = true;
                    continue;
                }
                "next" | "mark" | "choice_mark" | "marked" | "back" | "prior" | "last_result" => {
                    addresses::result(&name, node, scope)?
                }
                "cost_index" => json!(scope.cost_index.ok_or("no sacrifice in this cost")?),
                "share" => {
                    shares.push(item.path.clone());
                    continue;
                }
                "target" => {
                    references::introduce(
                        root,
                        scope,
                        &item,
                        &mut introduced,
                        &mut slots,
                        &mut prior,
                    )?;
                    continue;
                }
                "it" | "current_it" | "loop_card_it" if item.filter == filters::Context::Scalar => {
                    json!({"kind":"Candidate"})
                }
                "it" | "current_it" | "loop_card_it" if item.filter == filters::Context::Query => {
                    return Err("a nested predicate captures an outer filter candidate".into());
                }
                "defined_it" => scope
                    .pronoun
                    .clone()
                    .or_else(|| scope.object())
                    .ok_or("no object at the value definition")?,
                "defined_object" => scope.named().ok_or("no object at the value definition")?,
                "defined_named_object" => scope
                    .preceding()
                    .ok_or("no named object at the value definition")?,
                "defined_player" => scope.player().ok_or("no player at the value definition")?,
                "loop_card_it" => loop_card_focus::reference(prior.before(&item.path, scope))?,
                "it" | "current_it" => {
                    scope.damage_pronoun(&name, root, &item, &introduced, &prior)?
                }
                "another_target_reference" | "other_reference" => {
                    other::resolve_marker(&name, prior.before(&item.path, scope), &node["args"][0])?
                }
                "that_object" => prior
                    .before(&item.path, scope)
                    .named()
                    .ok_or("no object for this reference")?,
                "current_named_object" => revealed_card::resolve(
                    root,
                    &item.path,
                    scope.root_card_scope,
                    prior.before(&item.path, scope),
                )?,
                "named_object" => prior
                    .before(&item.path, scope)
                    .preceding()
                    .ok_or("no object for this named reference")?,
                "that_much" | "that_many" => {
                    addresses::quantity(root, scope, name == "that_many", &item.path)?
                }
                "next_slot" => json!(scope.target_base + scope.targets.len()),
                "last_slot" => json!(scope.target_base + scope.targets.len().saturating_sub(1)),
                "card_name" => json!(scope.card_name),
                "that_opponent" => {
                    controller_role::reference(root, &item.path, &introduced, scope, &prior)?
                }
                "that_player_literal" => controller_role::literal_reference(
                    root,
                    &item.path,
                    &introduced,
                    scope,
                    &prior,
                )?,
                "that_player" => {
                    per_recipient::reference(root, &item.path, &introduced, scope, &prior)?
                }
                // Naming this player makes them the one a later `their` reads.
                "defending" => {
                    let player = combat::defending(root, &item.path, scope)?;
                    scope.players.push(player.clone());
                    player
                }
                // Besides the player who acted when the event has one, else besides you.
                "other_party" if scope.player() == Some(json!({"kind": "EventSubject"})) => {
                    json!({"kind": "EventSubject"})
                }
                "other_party" => json!({"kind": "AbilityController"}),
                "self" if scope.leaving => json!({"kind": "EventSubject"}),
                "named_source" => holder::named_source(scope, acted)?,
                "self" => {
                    let source = json!({"kind":"Source"});
                    // Acted on rather than acting, it becomes what `it` means next.
                    if acted {
                        scope.objects.push(source.clone());
                    }
                    source
                }
                "holder" => holder::bind(node, scope)?,
                "scope" => events::bind_nested(node, scope)?,
                "kicker_branch" => branches::bind(node, scope)?,
                "conditional_ability" => conditional::bind(node, scope)?,
                // A reflexive trigger has its own targets but shares the ability's results.
                "reflexive" => events::bind_reflexive(node, scope)?,
                "announcement" => {
                    announcements.push(item.path.clone());
                    continue;
                }
                "slot" | "choose" => continue,
                other => return Err(format!("unbound symbol {other}")),
            }
        };
        let value = (damage_relative::ordinary_name(&name).into(), replacement);
        finish_reference(root, scope, &item, value, &prior, &slots, &introduced)?;
    }
    finish_scope(root, scope, slots, shares, announcements, shape_objects)
}

fn finish_reference(
    root: &mut Value,
    scope: &mut Scope,
    item: &Found,
    candidate: (String, Value),
    prior_targets: &references::Contexts,
    introductions: &[(Vec<Step>, usize)],
    introduced: &[references::TargetIntroduction],
) -> Result<(), String> {
    let (name, replacement) = candidate;
    let name = loop_card_focus::ordinary_name(name);
    created::check_scalar(root, &item.path, &replacement, scope)?;
    drafted::check_scalar(root, &item.path, &name, &replacement, scope)?;
    let replacement = reference_context(&name, replacement, item, prior_targets, scope, root)?;
    let replacement = transitions::reference(
        &name,
        root,
        &item.path,
        &transitions::Introductions {
            slots: introductions,
            keys: introduced,
            base: scope.target_base,
        },
        &scope.event,
        replacement,
        scope.counter_choice.map(|(result, _)| result),
    )?;
    if matches!(name.as_str(), "next" | "mark" | "choice_mark") {
        resolution_receipts::record(root, &item.path, &replacement, scope)?;
    }
    *at_path(root, &item.path).ok_or("binder path")? = replacement;
    Ok(())
}

fn bind_marker(
    root: &mut Value,
    scope: &mut Scope,
    path: &[Step],
    name: &str,
    introduced: &[references::TargetIntroduction],
) -> Result<bool, String> {
    Ok(activation_property::marker(root, scope, path, name)?
        || damage_reflexive::bind_marker(root, scope, path, name, introduced)?
        || move_after::bind_marker(root, scope, path, name, introduced)?
        || reflexive_receipts::marker(root, scope, path, name, introduced)?
        || member_focus::marker(root, path, name)?
        || pump_counts::marker(root, path, name)?
        || value_snapshot::marker(root, scope, path, name)?
        || drafted::marker(root, scope, path, name)?
        || exiled_card::marker(root, scope, path, name, introduced)?
        || search_move::marker(root, scope, path, name, introduced)?
        || created::bind_marker(root, scope, path, name)?
        || sacrificed::marker(root, scope, path, name)?
        || references::bind_link(root, path, name)?
        || payments::marker(root, scope, path, name)?)
}

fn reference_context(
    name: &str,
    mut replacement: Value,
    item: &Found,
    prior_targets: &references::Contexts,
    scope: &mut Scope,
    root: &Value,
) -> Result<Value, String> {
    replacement = sacrificed::reference(name, replacement, &item.path, root, scope)?;
    objects::check_scalar_reference(name, &replacement, scope)?;
    exiled_card::check_scalar(&replacement, &item.path, scope)?;
    if matches!(
        name,
        "defined_it" | "defined_object" | "defined_named_object"
    ) && item.filter != filters::Context::Outside
        && prior_targets.candidate(&item.path).as_ref() == Some(&replacement)
    {
        if item.filter == filters::Context::Query {
            return Err("a nested predicate captures a defined filter candidate".into());
        }
        replacement = json!({"kind":"Candidate"});
    }
    Ok(replacement)
}

fn finish_scope(
    root: &mut Value,
    scope: &mut Scope,
    mut introductions: Vec<(Vec<Step>, usize)>,
    shares: Vec<Vec<Step>>,
    announcements: Vec<Vec<Step>>,
    shape_objects: bool,
) -> Result<(), String> {
    // A specification can itself hold references, so it moves out last.
    introductions.sort_by_key(|(path, _)| std::cmp::Reverse(path.len()));
    for (path, slot) in introductions {
        let node = at_path(root, &path).ok_or("binder path")?;
        let mut spec = node["args"][0].take();
        spec["slot"] = json!(scope.target_base + slot);
        scope.targets[slot] = spec;
        *node = json!(scope.target_base + slot);
    }
    // A share names the slot whose total it divides.
    for path in shares {
        let slot = scope
            .targets
            .iter()
            .position(|spec| !spec["distribute"].is_null())
            .ok_or("a share needs a divided target")?;
        *at_path(root, &path).ok_or("binder path")? = json!(scope.target_base + slot);
    }
    strip_choices(root);
    for path in announcements {
        let node = at_path(root, &path).ok_or("binder path")?;
        *node = json!({"targets": scope.targets.clone(), "modes": null});
    }
    per_recipient::lower(root, scope.binding_floor)?;
    filters::strip(root);
    if shape_objects {
        exile_returns::guard(root);
        objects::resolve_shapes(root)?;
    }
    scope.target_floor = scope.target_floor.max(
        scope
            .target_base
            .checked_add(scope.targets.len())
            .ok_or("target address space exhausted")?,
    );
    Ok(())
}

/// A `choose` sentence has done its work once its target is announced.
fn strip_choices(value: &mut Value) {
    match value {
        Value::Object(object) => object.values_mut().for_each(strip_choices),
        Value::Array(items) => {
            items.retain(|item| symbol_name(item) != Some("choose"));
            items.iter_mut().for_each(strip_choices);
        }
        _ => {}
    }
}

/// Reserve the next slot and make the target a referent for what follows.
fn introduce(node: &Value, scope: &mut Scope) -> Result<(), String> {
    let spec = &node["args"][0];
    if !spec.is_object() {
        return Err("a target needs its specification".into());
    }
    let slot = scope.target_base + scope.targets.len();
    let reference = json!({"kind": "Target", "args": {"slot": slot, "index": 0}});
    controller_role::introduce(scope, &reference, spec)?;
    qualified::target(scope, reference.clone(), spec)?;
    scope.others.clear();
    if spec["kind"] == "Player" {
        scope.players.push(reference);
    } else {
        scope.named_object = Some(reference.clone());
        scope.objects.push(reference.clone());
        if spec["kind"] == "DamageRecipient" {
            scope.players.push(reference);
        }
    }
    scope.targets.push(Value::Null);
    Ok(())
}

/// Every place a slot is written, as a path to the number.
fn slot_uses(value: &Value, slot: u64, path: &mut Vec<Step>, out: &mut Vec<Vec<Step>>) {
    match value {
        Value::Object(object) => {
            let kind = value.get("kind").and_then(Value::as_str);
            let args = value.get("args");
            if matches!(kind, Some("Targets" | "DistributedShare"))
                && args.and_then(Value::as_u64) == Some(slot)
            {
                path.push(Step::Key("args".into()));
                out.push(path.clone());
                path.pop();
                return;
            }
            if kind == Some("Target")
                && args.and_then(|a| a.get("slot")).and_then(Value::as_u64) == Some(slot)
            {
                path.push(Step::Key("args".into()));
                path.push(Step::Key("slot".into()));
                out.push(path.clone());
                path.pop();
                path.pop();
                return;
            }
            // A created trigger owns its body's slots, while its event and
            // duration still refer to the creating ability's announced targets.
            if kind == Some("CreateTrigger") {
                for key in ["event", "duration"] {
                    path.push(Step::Key("args".into()));
                    path.push(Step::Key(key.into()));
                    slot_uses(&value["args"][key], slot, path, out);
                    path.pop();
                    path.pop();
                }
                return;
            }
            // A nested ability owns its slots.
            if opens_scope(value) {
                return;
            }
            for name in ["DifferentFromTarget", "SharesLimitWith", "Targets"] {
                if value.get(name).and_then(Value::as_u64) == Some(slot) {
                    path.push(Step::Key(name.into()));
                    out.push(path.clone());
                    path.pop();
                    return;
                }
            }
            // A damage recipient holds its slot in a record, not a kind.
            if value["Target"]["slot"].as_u64() == Some(slot) {
                path.push(Step::Key("Target".into()));
                path.push(Step::Key("slot".into()));
                out.push(path.clone());
                path.pop();
                path.pop();
                return;
            }
            for (key, item) in object {
                path.push(Step::Key(key.clone()));
                slot_uses(item, slot, path, out);
                path.pop();
            }
        }
        Value::Array(items) => {
            for (index, item) in items.iter().enumerate() {
                path.push(Step::Index(index));
                slot_uses(item, slot, path, out);
                path.pop();
            }
        }
        _ => {}
    }
}

/// The surface forms of an announced scope: each choice of which mention of a
/// slot introduces it. Also permit one correlated form when a lexical operand
/// occurs repeatedly in the semantic constructor.
fn slot_forms(root: &Value, specs: &[Value], body: &[Step]) -> Vec<Value> {
    let Some(order) = references::surface_order(specs) else {
        return Vec::new();
    };
    let mut forms = vec![root.clone()];
    for slot in order {
        let spec = &specs[slot];
        let mut next = Vec::new();
        for form in &forms {
            let mut copy = form.clone();
            let Some(program) = at_path(&mut copy, body) else {
                continue;
            };
            let mut uses = Vec::new();
            let address = spec["slot"].as_u64().unwrap_or(slot as u64);
            slot_uses(program, address, &mut Vec::new(), &mut uses);
            for path in uses {
                let mut copy = form.clone();
                let node = at_path(&mut copy, body).and_then(|b| at_path(b, &path));
                if let Some(node) = node {
                    *node = json!({"$sym": "target", "args": [spec], "at": 0});
                    next.push(copy);
                }
                if next.len() >= 64 {
                    break;
                }
            }
            if let Some(chosen) = chosen_form(form, slot, spec, body) {
                next.push(chosen);
            }
        }
        if !next.is_empty() {
            forms = next;
        }
    }
    // Keep the complete original cross-slot enumeration first. An added form
    // must never become a parent of the old per-slot loops or amplify their
    // pre-existing cap overshoot.
    if forms.len() < 64
        && let Some(correlated) = target_surface::correlated(root, specs, body)
    {
        forms.push(correlated);
    }
    forms
}

/// The form where the target is named by a `choose` sentence before the body.
fn chosen_form(form: &Value, slot: usize, spec: &Value, body: &[Step]) -> Option<Value> {
    let mut copy = form.clone();
    let instructions = at_path(&mut copy, body)?.get_mut("instructions")?;
    let target = json!({"kind": "Target", "args": {
        "slot": {"$sym": "target", "args": [spec], "at": 0},
        "index": 0,
    }});
    let marker = json!({"$sym": "choose", "args": [target], "at": 0});
    instructions.as_array_mut()?.insert(slot.min(1), marker);
    Some(copy)
}

fn path_of(keys: &[&str]) -> Vec<Step> {
    keys.iter().map(|k| Step::Key((*k).to_string())).collect()
}

/// Surface forms of each mode, in place of the bound modes.
fn mode_forms(forms: Vec<Value>) -> Vec<Value> {
    mode_forms_at(forms, "/args/announcement/modes/options", 0, &mut 65_536)
}

fn mode_forms_at(forms: Vec<Value>, pointer: &str, depth: usize, work: &mut usize) -> Vec<Value> {
    let mut out = forms;
    let count = out
        .first()
        .and_then(|f| f.pointer(pointer))
        .and_then(Value::as_array)
        .map_or(0, Vec::len);
    if count > 0 && depth >= 64 {
        return Vec::new();
    }
    for index in 0..count {
        let mut next = Vec::new();
        'forms: for form in &out {
            if *work == 0 {
                return Vec::new();
            }
            *work -= 1;
            let Some(mode) = form.pointer(pointer).and_then(|m| m.get(index)) else {
                continue;
            };
            let specs = mode["announcement"]["targets"]
                .as_array()
                .cloned()
                .unwrap_or_default();
            // Implied target choices can nest inside a printed mode. Each
            // child's announcement owns its targets, just like the parent.
            let variants = mode_forms_at(
                slot_forms(mode, &specs, &path_of(&["body"])),
                "/announcement/modes/options",
                depth + 1,
                work,
            );
            for variant in variants {
                if *work == 0 {
                    return Vec::new();
                }
                *work -= 1;
                let mut copy = form.clone();
                if let Some(slot) = copy.pointer_mut(pointer).and_then(|m| m.get_mut(index)) {
                    *slot = json!({"$sym": "scope", "args": [variant], "at": 0});
                }
                next.push(copy);
                if next.len() == 64 {
                    break 'forms;
                }
            }
        }
        out = next;
    }
    out
}

/// Whether a value owns an announcement. Reflexive triggers own slots too,
/// while their result ids still belong to the creating ability.
fn opens_scope(value: &Value) -> bool {
    matches!(
        ability_kind(value),
        Some("Spell" | "Activated" | "Triggered" | "ReflexiveTrigger" | "CreateTrigger")
    ) && value["args"].get("announcement").is_some()
}

/// Where the outermost abilities below `value` are.
fn nested_scopes(value: &Value, path: &mut Vec<Step>, out: &mut Vec<Vec<Step>>) {
    let mut descend = |key: Step, item: &Value| {
        path.push(key);
        if opens_scope(item) {
            out.push(path.clone());
        } else {
            nested_scopes(item, path, out);
        }
        path.pop();
    };
    match value {
        Value::Object(object) => {
            for (key, item) in object {
                descend(Step::Key(key.clone()), item);
            }
        }
        Value::Array(items) => {
            for (index, item) in items.iter().enumerate() {
                descend(Step::Index(index), item);
            }
        }
        _ => {}
    }
}

/// `forms` with each ability nested inside them replaced by its own surface forms.
fn nested_forms(forms: Vec<Value>) -> Vec<Value> {
    let mut paths = Vec::new();
    if let Some(first) = forms.first() {
        nested_scopes(first, &mut Vec::new(), &mut paths);
    }
    let mut out = forms;
    for path in paths {
        let mut next = Vec::new();
        for form in &out {
            let mut copy = form.clone();
            let Some(inner) = at_path(&mut copy, &path).map(|node| node.clone()) else {
                next.push(copy);
                continue;
            };
            // Literal recipes (for example Map tokens) print their name with a
            // complete ability in the pattern. Keep that bound form as well as
            // the forms that introduce targets in a quoted ability's text.
            let mut variants = unbind_ability(&inner);
            if !variants.contains(&inner) {
                variants.push(inner);
            }
            for variant in variants {
                if next.len() >= 64 {
                    break;
                }
                if let Some(node) = at_path(&mut copy, &path) {
                    *node = variant;
                    next.push(copy.clone());
                }
            }
        }
        out = next;
    }
    out
}

/// The surface forms of an ability, and of the abilities inside it.
pub fn unbind_ability(ability: &Value) -> Vec<Value> {
    presentation::forms(ability).unwrap_or_else(|| nested_forms(own_forms(ability)))
}

/// The surface forms of an ability's own slots and modes.
fn own_forms(ability: &Value) -> Vec<Value> {
    let mut plain;
    let mut ability = ability;
    if variables::entry(ability) || variables::activation(ability) {
        plain = ability.clone();
        if variables::surface(&mut plain).is_err() {
            return Vec::new();
        }
        ability = &plain;
    }
    let mut forms = spelled_forms(ability);
    if let Ok(Some(attached)) = attached_counter_other::surface(ability) {
        forms.extend(spelled_forms(&attached));
    }
    // Keep ordinary spellings first. Only a proven damage actor/recipient pair
    // receives the extra lexical reflexive spelling view.
    let mut reflexive = ability.clone();
    if damage_reflexive::surface(&mut reflexive) {
        forms.extend(spelled_forms(&reflexive));
    }
    let mut receipt = ability.clone();
    if exiled_card::surface(&mut receipt) {
        forms.extend(spelled_forms(&receipt));
    }
    let mut recipients = ability.clone();
    if per_recipient::surface(&mut recipients).is_ok() && recipients != *ability {
        forms.extend(spelled_forms(&recipients));
    }
    if let Some(aura) = events::Frame::of_ability(ability).aura_destination() {
        let mut returning = ability.clone();
        replace_all(&mut returning, &aura, &json!({"kind":"Source"}));
        if returning != *ability {
            forms.extend(spelled_forms(&returning));
        }
    }
    let mut inline = ability.clone();
    if selections::surface(&mut inline).is_ok() && inline != *ability {
        forms.extend(spelled_forms(&inline));
    }
    // The card's own name can stand for the event's subject only when the event is its own.
    if !names_itself(&ability["args"]["event"]) {
        return forms;
    }
    let mut named = ability.clone();
    replace_all(
        &mut named,
        &json!({"kind": "EventSubject"}),
        &json!({"kind":"Source"}),
    );
    if let Some(destination) = events::Frame::of_ability(ability).source_destination() {
        replace_all(&mut named, &destination, &json!({"kind":"Source"}));
    }
    if named == *ability {
        return forms;
    }
    let mut all = spelled_forms(&named);
    all.extend(forms);
    all
}

/// Whether an event is about the card that has the ability, not about others than it.
fn names_itself(event: &Value) -> bool {
    match event {
        Value::Object(object) => {
            object.get("kind").and_then(Value::as_str) != Some("Not")
                && (*event == json!({"kind": "SameObject", "args": {"kind": "Source"}})
                    || object.values().any(names_itself))
        }
        Value::Array(items) => items.iter().any(names_itself),
        _ => false,
    }
}

fn spelled_forms(ability: &Value) -> Vec<Value> {
    let mut ability = ability.clone();
    if payments::surface(&mut ability).is_err() {
        return Vec::new();
    }
    let specs = ability["args"]["announcement"]["targets"]
        .as_array()
        .cloned()
        .unwrap_or_default();
    let forms = mode_forms(slot_forms(&ability, &specs, &path_of(&["args", "body"])));
    let mut forms = qualified::spelling_forms(forms, &specs);
    for form in &mut forms {
        guards::resolution_surface(form);
    }
    forms
}

#[cfg(test)]
mod mode_surface_tests {
    use super::*;

    #[test]
    fn nested_modes_bound_depth_work_and_candidate_count() {
        let leaf = json!({"announcement":{"targets":[],"modes":null},"body":{}});
        let root =
            json!({"args":{"announcement":{"modes":{"options":[leaf.clone(),leaf.clone()]}}}});
        let original = root.clone();
        assert_eq!(mode_forms(vec![root.clone(); 100]).len(), 64);
        assert!(
            mode_forms_at(
                vec![root.clone()],
                "/args/announcement/modes/options",
                0,
                &mut 3,
            )
            .is_empty()
        );
        assert_eq!(root, original);
        let mut nested = leaf;
        for _ in 0..65 {
            nested = json!({"announcement":{"targets":[],"modes":{"options":[nested]}},"body":{}});
        }
        let deep = json!({"args":{"announcement":{"modes":{"options":[nested]}}}});
        assert!(mode_forms(vec![deep]).is_empty());
    }
}
