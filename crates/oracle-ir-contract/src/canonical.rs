//! The normal form equality needs: result, slot and binding ids renumbered in
//! the order they are defined, and commutative lists sorted. Two cards that
//! differ only in lowering order are equal here. The walk reads the IR's own
//! type table, so a new field of an id type is covered without a change here.
//! Spellings the lowerer reaches by different routes that mean the same thing
//! (`All` holding `Any`, a zone scope narrowed by its filter, a one-object
//! `InSet`, an open count bounded by the total divided) reduce to one form.
use crate::ir::Card;
use crate::ir_schema::{Fields, Schema, Ty, TypeDef};
use serde_json::{Value, json};
use std::collections::{HashMap, HashSet};
use std::sync::OnceLock;

mod combat_pairs;
mod entry;
mod events;
mod filters;
mod fixed_mana;
mod immediate_values;
#[cfg(test)]
mod immediate_values_tests;
mod immediate_views;
#[cfg(test)]
mod issue_sets_tests;
mod joins;
mod keywords;
mod mana_uses;
#[cfg(test)]
mod mode_results;
mod movement;
mod observed_hands;
pub mod observed_top;
mod paid_source;
#[cfg(test)]
mod paid_source_tests;
mod programs;
pub mod resolution_payments;
#[cfg(test)]
mod result_scopes;
mod scalar_reads;
mod set_views;
mod sets;
mod single_choices;
#[cfg(test)]
mod single_choices_tests;
mod snapshots;
mod source_events;
#[cfg(test)]
mod source_events_tests;
mod spell_paragraphs;
mod target_pairs;
mod trace;
pub mod trigger_batches;
mod trigger_branches;
mod untap;
mod visibility;
mod walk;
pub use fixed_mana::literal as literal_mana;
pub use immediate_views::print_views as immediate_parameter_print_views;
use movement::groups as movement_groups;
pub use set_views::print_views as object_population_print_views;
pub use trace::MappedCard;
pub use visibility::branch_sites as branch_receipt_sites;
pub use visibility::export_sites as exported_receipt_sites;
#[cfg(test)]
mod tests;

pub fn schema() -> Result<&'static Schema, String> {
    static SCHEMA: OnceLock<Result<Schema, String>> = OnceLock::new();
    #[cfg(all(target_arch = "wasm32", feature = "compiled-schema"))]
    let load = frozen_schema;
    #[cfg(not(all(target_arch = "wasm32", feature = "compiled-schema")))]
    let load = Schema::of_ir;
    SCHEMA.get_or_init(load).as_ref().map_err(Clone::clone)
}

#[cfg(all(feature = "compiled-schema", any(target_arch = "wasm32", test)))]
fn frozen_schema() -> Result<Schema, String> {
    use crate::ir::SCHEMA_VERSION;
    #[derive(serde::Deserialize)]
    struct Descriptor {
        schema_version: u32,
        schema: Schema,
    }
    let source = crate::ir_schema::frozen_json(SCHEMA_VERSION).ok_or("missing frozen schema")?;
    let descriptor: Descriptor = serde_json::from_str(source).map_err(|error| error.to_string())?;
    if descriptor.schema_version != SCHEMA_VERSION {
        return Err("frozen schema version mismatch".into());
    }
    Ok(descriptor.schema)
}

#[cfg(all(test, feature = "compiled-schema"))]
#[test]
fn frozen_runtime_schema_matches_the_source_contract() {
    assert_eq!(
        serde_json::to_value(frozen_schema().unwrap()).unwrap(),
        serde_json::to_value(Schema::of_ir().unwrap()).unwrap()
    );
}

/// Lists whose order carries no meaning.
fn commutative(ty: &str, variant: &str) -> bool {
    matches!(
        (ty, variant),
        (
            "ObjectFilter" | "Condition" | "NumberTest",
            "AllOf" | "AnyOf"
        )
    )
}

type JoinGroup = HashMap<u64, Vec<(u64, u64)>>;
struct TargetScope {
    map: Option<HashMap<u64, u64>>,
    optional: HashSet<u64>,
    single_battlefield: HashSet<u64>,
}
#[derive(Clone)]
struct ResultScope {
    map: Option<HashMap<u64, u64>>,
    definitions: HashMap<u64, u64>,
    joins: Vec<JoinGroup>,
    floor: u64,
}
#[derive(Default)]
struct Ids {
    /// Old id to new id, per id type, in this ability's scope.
    maps: HashMap<&'static str, HashMap<u64, u64>>,
    optional_targets: HashSet<u64>,
    single_battlefield_targets: HashSet<u64>,
    /// Which ability this is, in visiting order.
    visit: usize,
    /// Unread definitions still need distinct ids for executable, valid IR.
    unread: u64,
    selections: u64,
    /// Definition identities, distinct from the numeric labels a future local
    /// invocation may reuse. Allocation counters survive a closure's scope.
    result_definitions: HashMap<u64, u64>,
    join_groups: Vec<JoinGroup>,
    results: u64,
    live_results: u64,
}

/// Used results count up from zero; unread definitions count down in lexical
/// order. Their original lowering ids have no meaning, but aliasing them all
/// would violate the IR's unique-definition contract.
const UNUSED_START: u64 = u32::MAX as u64;

impl Ids {
    fn define(
        &mut self,
        kind: &'static str,
        value: &mut Value,
        read: Option<&HashSet<u64>>,
        tracing: bool,
    ) {
        let Some(old) = (if tracing {
            trace::number(value)
        } else {
            value.as_u64()
        }) else {
            return;
        };
        if kind == "selection" {
            let new = self.selections;
            self.selections += 1;
            self.maps.entry(kind).or_default().insert(old, new);
            trace::set_number(value, new);
            return;
        }
        if kind == "result" {
            let definition = self.results;
            self.results += 1;
            // Preserve invalid duplicate definitions for validation to reject.
            if let Some(new) = self.maps.get(kind).and_then(|map| map.get(&old)) {
                trace::set_number(value, *new);
                return;
            }
            self.result_definitions.insert(old, definition);
            let new = if read.is_some_and(|read| !read.contains(&definition)) {
                let new = UNUSED_START - self.unread;
                self.unread += 1;
                new
            } else {
                let new = self.live_results;
                self.live_results += 1;
                new
            };
            self.maps.entry(kind).or_default().insert(old, new);
            trace::set_number(value, new);
            return;
        }
        let map = self.maps.entry(kind).or_default();
        if kind == "object_binding" {
            // Object iteration permits a nested binding to shadow the same
            // label. Its inputs used the outer map; only its body sees this one.
            let next = map.values().max().map_or(0, |n| n + 1);
            map.insert(old, next);
            trace::set_number(value, next);
        } else {
            let next = map.len() as u64;
            let new = *map.entry(old).or_insert(next);
            trace::set_number(value, new);
        }
    }
}

#[derive(Clone, Copy, PartialEq)]
enum Subject {
    Other,
    /// A trigger on the source's own event: its subject is that same object.
    Source,
    /// The source's own entry: subject and controller are the source's as well.
    Entering,
}

fn subject_of(ability: &Value) -> Subject {
    let args = &ability["args"];
    if args["active"] == "SourceEntering" {
        return Subject::Entering;
    }
    events::subject(&args["event"])
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum ReferenceForm {
    Ordinary,
    Subject,
    SubjectOperands,
    SubjectCreators,
    SubjectAnnouncements,
}

impl ReferenceForm {
    fn for_field(self, owner: &str, field: &str) -> Self {
        let announcement = field == "announcement";
        let creator = matches!(
            (owner, field),
            ("Operation::CreateContinuousEffect", "source") | ("Operation::ChangeSpell", "subject")
        );
        let literal = match self {
            Self::SubjectOperands => announcement || creator,
            Self::SubjectCreators => creator,
            Self::SubjectAnnouncements => announcement,
            _ => false,
        };
        if literal { Self::Ordinary } else { self }
    }
}

struct Walker {
    schema: &'static Schema,
    binding_namespace: &'static str,
    scopes: Vec<Ids>,
    /// The role each open ability's damage event calls its subject.
    damage_subjects: Vec<Option<Value>>,
    object_batch_counts: Vec<bool>,
    /// Whether each open ability's event is about the source itself.
    subjects: Vec<Subject>,
    event_header: bool,
    reference_reductions: bool,
    /// A private print view may spell a proved Source as its event subject.
    reference_form: ReferenceForm,
    /// Keep ids and list order, for callers that read the text back out.
    keep_order: bool,
    tracing: bool,
    /// The definitions each ability reads, by visiting order. The first of the two
    /// passes fills this; the second consults it.
    read: Vec<HashSet<u64>>,
    /// Whether this is the pass that fills `read`.
    reading: bool,
    /// Only resolution-time PayCost exports receipts from its player actions.
    paying_cost: bool,
    operation_depth: usize,
    payment_depth: Option<usize>,
    /// A surrounding immediate loop gathers optional outcomes as receipts too.
    gathering_results: bool,
    visits: usize,
    join_work: usize,
    failure: Option<String>,
}

/// The role a damage event's own subject plays, through a `Filtered` wrapper.
/// Either-of damage events share a subject only when every case names the same role.
fn damage_subject(event: &Value) -> Option<Value> {
    if let Some(filtered) = event.get("Filtered") {
        return damage_subject(&filtered["event"]);
    }
    if let Some(Value::Array(events)) = event.get("AnyOf") {
        let mut subjects = events.iter().map(damage_subject);
        let first = subjects.next()??;
        return subjects
            .all(|subject| subject.as_ref() == Some(&first))
            .then_some(first);
    }
    event.get("Damage").map(|damage| damage["subject"].clone())
}

fn id_kind(name: &str) -> Option<&'static str> {
    use crate::binding_map::BindingKind;
    Some(match BindingKind::of_type(name)? {
        BindingKind::Target => "slot",
        BindingKind::Result => "result",
        BindingKind::Loop => "binding",
        BindingKind::Selection => "selection",
    })
}

impl Walker {
    fn target_scope(&self) -> Option<TargetScope> {
        self.scopes.last().map(|scope| TargetScope {
            map: scope.maps.get("slot").cloned(),
            optional: scope.optional_targets.clone(),
            single_battlefield: scope.single_battlefield_targets.clone(),
        })
    }

    fn restore_target_scope(&mut self, saved: Option<TargetScope>) {
        if let (Some(saved), Some(scope)) = (saved, self.scopes.last_mut()) {
            if let Some(slots) = saved.map {
                scope.maps.insert("slot", slots);
            } else {
                scope.maps.remove("slot");
            }
            scope.optional_targets = saved.optional;
            scope.single_battlefield_targets = saved.single_battlefield;
        }
    }

    fn result_scope(&self) -> Option<ResultScope> {
        self.scopes.last().map(|s| ResultScope {
            map: s.maps.get("result").cloned(),
            definitions: s.result_definitions.clone(),
            joins: s.join_groups.clone(),
            floor: s.results,
        })
    }
    fn restore_result_scope(&mut self, saved: Option<ResultScope>) {
        if let (Some(saved), Some(scope)) = (saved, self.scopes.last_mut()) {
            if let Some(map) = saved.map {
                scope.maps.insert("result", map);
            } else {
                scope.maps.remove("result");
            }
            scope.result_definitions = saved.definitions;
            scope.join_groups = saved.joins;
        }
    }

    fn restore_with_exports(&mut self, saved: Option<ResultScope>, exports: &HashSet<u64>) {
        let kept: Vec<_> = self
            .scopes
            .last()
            .into_iter()
            .flat_map(|s| {
                exports.iter().filter_map(|old| {
                    Some((
                        *old,
                        *s.maps.get("result")?.get(old)?,
                        *s.result_definitions.get(old)?,
                    ))
                })
            })
            .collect();
        self.restore_result_scope(saved);
        if let Some(scope) = self.scopes.last_mut() {
            for (old, new, definition) in kept {
                scope.maps.entry("result").or_default().insert(old, new);
                scope.result_definitions.insert(old, definition);
            }
        }
    }

    fn use_id(&mut self, kind: &'static str, value: &mut Value) {
        let Some(old) = (if self.tracing {
            trace::number(value)
        } else {
            value.as_u64()
        }) else {
            return;
        };
        if self.keep_order {
            return;
        }
        for scope in self.scopes.iter().rev() {
            if let Some(new) = scope.maps.get(kind).and_then(|m| m.get(&old)) {
                if self.reading
                    && kind == "result"
                    && let Some(definition) = scope.result_definitions.get(&old)
                {
                    self.read[scope.visit].insert(*definition);
                }
                trace::set_number(value, *new);
                return;
            }
        }
    }

    fn walk(&mut self, value: &mut Value, ty: &Ty, defining: bool) {
        let operation = matches!(ty, Ty::Named(name) if name == "Operation");
        if operation {
            self.operation_depth += 1;
        }
        self.walk_inner(value, ty, defining);
        if operation {
            self.operation_depth -= 1;
        }
    }
    fn walk_inner(&mut self, value: &mut Value, ty: &Ty, defining: bool) {
        if self.failure.is_some() {
            return;
        }
        match ty {
            Ty::Opaque => {}
            Ty::Vec(inner) => {
                if let Value::Array(items) = value {
                    for item in items.iter_mut() {
                        let local =
                            (**inner == Ty::Named("Program".into())).then(|| self.result_scope());
                        self.walk(item, inner, false);
                        if let Some(local) = local {
                            self.restore_result_scope(local);
                        }
                    }
                    if **inner == Ty::Named("Instruction".into()) {
                        splice_groups(items);
                    } else if **inner == Ty::Named("Ability".into()) {
                        merge_statics(items, self.tracing);
                        if !self.keep_order {
                            trigger_branches::merge(items, self.schema, self.tracing);
                        }
                    }
                }
            }
            Ty::Option(inner) => {
                if !value.is_null() {
                    self.walk(value, inner, defining);
                }
            }
            Ty::Tuple(parts) => {
                if let Value::Array(items) = value {
                    for (item, part) in items.iter_mut().zip(parts) {
                        self.walk(item, part, false);
                    }
                }
            }
            Ty::Named(name) => self.named(value, name, defining),
        }
    }

    fn named(&mut self, value: &mut Value, name: &str, defining: bool) {
        // Presentation wraps the same ability/keyword, without adding an id,
        // event-subject or ability-holder scope of its own.
        let empty = crate::presentation::empty(value, name);
        if let Some(inner) = crate::presentation::content(value, name) {
            self.named(inner, name, defining);
            if empty {
                *value = inner.take();
            }
            return;
        }
        if let Some(kind) = id_kind(name) {
            let kind = if kind == "binding" {
                self.binding_namespace
            } else {
                kind
            };
            if defining {
                if let (Some(scope), false) = (self.scopes.last_mut(), self.keep_order) {
                    let read = (!self.reading).then(|| &self.read[scope.visit]);
                    scope.define(kind, value, read, self.tracing);
                }
            } else {
                self.use_id(kind, value);
            }
            return;
        }
        // Modes inherit enclosing receipts and targets, but local definitions
        // cannot escape into sibling modes or the common ability body.
        let mode_slots = (name == "Mode").then(|| self.target_scope());
        let local_results = (name == "Mode"
            || name == "Operand" && value["kind"] == "Program"
            || name == "EventChange" && value.get("Then").is_some()
            || name == "Instruction" && value["kind"] == "Repeatable")
            .then(|| self.result_scope());
        let exported = if name == "Instruction" && value["kind"] == "Repeatable" {
            visibility::exports(&value["args"], true)
        } else {
            HashSet::new()
        };
        let previous_gathering = self.gathering_results;
        if name == "Instruction" && value["kind"] == "Repeatable" {
            self.gathering_results = true;
        }
        let scoped = name == "Ability";
        if scoped {
            self.gathering_results = false;
        }
        if scoped {
            if !self.keep_order {
                keywords::conditional_flash(value);
            }
            if !self.keep_order
                && value["kind"] == "Triggered"
                && let Some((_, fields)) = self.schema.variant("Ability", "Triggered")
            {
                trigger_batches::capped_occurrence(value);
                entry::header(&mut value["args"], fields, self.schema);
            }
            self.scopes.push(Ids {
                visit: self.visits,
                ..Ids::default()
            });
            self.visits += 1;
            if self.reading {
                self.read.push(HashSet::new());
            }
            self.damage_subjects
                .push(damage_subject(&value["args"]["event"]));
            self.object_batch_counts
                .push(trigger_batches::amount_is_group_size(
                    &value["args"]["event"],
                ));
            self.subjects.push(subject_of(value));
        }
        let schema = self.schema;
        match schema.types.get(name) {
            Some(TypeDef::Struct(fields)) => {
                self.fields(value, fields, name);
                if name == "TargetSpec" {
                    bound_divided_count(value);
                    if let (Some(slot), Some(scope)) =
                        (trace::number(&value["slot"]), self.scopes.last_mut())
                    {
                        if value["count"] == json!({"UpTo":{"kind":"Constant","args":1}}) {
                            scope.optional_targets.insert(slot);
                        } else {
                            scope.optional_targets.remove(&slot);
                        }
                        if sets::single_battlefield_target(value) {
                            scope.single_battlefield_targets.insert(slot);
                        } else {
                            scope.single_battlefield_targets.remove(&slot);
                        }
                    }
                }
            }
            Some(TypeDef::Enum { adjacent, .. }) => self.variant(value, name, *adjacent),
            None => {}
        }
        if scoped {
            self.scopes.pop();
            self.damage_subjects.pop();
            self.object_batch_counts.pop();
            self.subjects.pop();
        }
        if let Some(saved) = local_results {
            self.restore_with_exports(saved, &exported);
        }
        if let Some(previous) = mode_slots {
            self.restore_target_scope(previous);
        }
        self.gathering_results = previous_gathering;
    }

    fn variant(&mut self, value: &mut Value, name: &str, adjacent: bool) {
        if name == "Expr"
            && value["kind"] == "EventAmount"
            && value.as_object().is_some_and(|fields| fields.len() == 1)
            && !self.keep_order
            && !self.event_header
            && self.reference_reductions
            && self.object_batch_counts.last() == Some(&true)
        {
            *value = json!({"kind":"CountObjects","args":{"kind":"EventGroup"}});
        }
        let schema = self.schema;
        if name == "EventSpec" && !self.keep_order {
            entry::player(value, schema, self.tracing);
        }
        let variant = if adjacent {
            value
                .get("kind")
                .and_then(Value::as_str)
                .map(str::to_string)
        } else {
            match &*value {
                Value::Object(object) if object.len() == 1 => object.keys().next().cloned(),
                _ => None,
            }
        };
        let Some(variant) = variant else { return };
        let Some((_, fields)) = schema.variant(name, &variant) else {
            return;
        };
        let subject = self.subjects.last().copied().unwrap_or(Subject::Other);
        let references = self.reference_reductions && !self.event_header;
        if references
            && self.reference_form != ReferenceForm::Ordinary
            && subject == Subject::Source
            && name == "ObjectRef"
            && variant == "Source"
            && value.as_object().is_some_and(|fields| fields.len() == 1)
        {
            *value = serde_json::json!({"kind": "EventSubject"});
            return;
        }
        if references
            && name == "ObjectRef"
            && variant == "EventSubject"
            && value.as_object().is_some_and(|fields| fields.len() == 1)
            && self.names_source(subject)
        {
            *value = serde_json::json!({"kind": "Source"});
            return;
        }
        let content = if adjacent {
            value.get_mut("args")
        } else {
            value.get_mut(&variant)
        };
        if let Some(content) = content {
            self.fields(content, fields, &format!("{name}::{variant}"));
            if commutative(name, &variant) {
                if let Value::Array(items) = content {
                    flatten(items, &variant);
                    if variant == "AllOf" {
                        drop_identity(items);
                    }
                    if !self.keep_order {
                        items.sort_by_cached_key(|v| trace::key(v, self.tracing));
                    }
                }
                if let Some(only) = single(content) {
                    *value = only;
                } else if name == "ObjectFilter" && variant == "AnyOf" && !self.keep_order {
                    filters::merge_same_property_any_of(value, self.tracing);
                }
                return;
            }
            match (name, variant.as_str()) {
                ("Operation", "Keyword" | "KeywordEach") => keywords::suspect(
                    value,
                    self.scopes.last().map(|scope| &scope.optional_targets),
                ),
                ("Operation", "Move") if !self.keep_order => {
                    if let Some(scope) = self.scopes.last() {
                        sets::exile_union(value, &scope.single_battlefield_targets, self.tracing);
                    }
                }
                ("Operation", "CreateContinuousEffect") => untap::controlled_population(value),
                ("ManaUse", "Either") => {
                    mana_uses::casting_alternatives(value, self.keep_order, self.tracing);
                }
                ("ObjectFilter", "InSet") => one_object_set(value),
                ("ObjectFilter", "Not") if !self.keep_order => filters::negated_condition(value),
                ("ObjectSet", "Zone") => {
                    narrow_zone_scope(content_of(value));
                    if let Some(filter) = battlefield_filter_mut(value) {
                        drop_permanent_types(filter);
                    }
                }
                ("ObjectSet", "InZone") => repeated_current_zone(value),
                ("ObjectSet", "Matching") => sets::matching(value, self.keep_order, self.tracing),
                ("EventSpec", "ZoneChange") => {
                    if value["ZoneChange"]["to"] == "Battlefield" {
                        drop_permanent_types(&mut value["ZoneChange"]["objects"]);
                    }
                }
                ("EventSpec", "Player") => {
                    if !self.keep_order && value["Player"]["event"] == "EntersUnderControl" {
                        drop_permanent_types(&mut value["Player"]["objects"]);
                    }
                }
                ("EventSpec", "Batch") if !self.keep_order => {
                    trigger_batches::attack_minimum(value)
                }
                ("EventSpec", "AnyOf") if !self.keep_order => {
                    if let Some(Value::Array(events)) = value.get_mut("AnyOf") {
                        events.sort_by_cached_key(|v| trace::key(v, self.tracing));
                    }
                }
                ("ObjectRef", "Damage") => {
                    if references
                        && value.as_object().is_some_and(|fields| fields.len() == 2)
                        && self.damage_subjects.last().cloned().flatten().as_ref()
                            == value.get("args")
                    {
                        *value = if self.names_source(subject) {
                            serde_json::json!({"kind": "Source"})
                        } else {
                            serde_json::json!({"kind": "EventSubject"})
                        };
                    }
                }
                ("PlayerRef", "ControllerOf") => {
                    if references
                        && subject == Subject::Entering
                        && value["args"]["kind"] == "Source"
                    {
                        *value = serde_json::json!({"kind": "AbilityController"});
                    }
                }
                ("Recipients", "Union") => self.merge_permanent_groups(value),
                _ => {}
            }
        }
    }

    /// A printer reads `it` back from the event's subject, so only comparison merges them.
    fn names_source(&self, subject: Subject) -> bool {
        subject == Subject::Entering || (subject == Subject::Source && !self.keep_order)
    }

    /// Damage to two groups of permanents is damage to the permanents of either.
    fn merge_permanent_groups(&self, value: &mut Value) {
        let Some(items) = value["args"].as_array() else {
            return;
        };
        let slots = self
            .scopes
            .last()
            .filter(|_| !self.keep_order)
            .map(|scope| &scope.single_battlefield_targets);
        let objects = items
            .iter()
            .map(|item| (item["kind"] == "Objects").then_some(&item["args"]));
        if let Some(zone) = sets::battlefield_union(objects, slots, self.keep_order, self.tracing) {
            *value = json!({"kind":"Objects","args":zone});
        }
    }

    fn fields(&mut self, value: &mut Value, fields: &Fields, owner: &str) {
        let saved = self.binding_namespace;
        if let Some(namespace) = crate::binding_roles::namespace(owner) {
            self.binding_namespace = namespace;
        }
        self.fields_inner(value, fields, owner);
        self.binding_namespace = saved;
    }
    fn fields_inner(&mut self, value: &mut Value, fields: &Fields, owner: &str) {
        match fields {
            Fields::Unit => {}
            Fields::Tuple(types) => {
                if let [only] = types.as_slice() {
                    self.walk(value, only, false);
                } else if let Value::Array(items) = value {
                    for (item, ty) in items.iter_mut().zip(types) {
                        self.walk(item, ty, false);
                    }
                }
            }
            Fields::Named(types) => self.named_fields(value, fields, owner, types),
        }
    }
    fn field_order<'a>(
        types: &'a [(String, Ty)],
        owner: &str,
        selection: bool,
    ) -> Vec<&'a (String, Ty)> {
        let late = |field: &str| {
            selection && field == "binding" || owner == "Instruction::Effect" && field == "result"
        };
        let mut ordered: Vec<_> = types
            .iter()
            .filter(|(field, _)| !late(field))
            .chain(types.iter().filter(|(field, _)| late(field)))
            .collect();
        if owner == "Ability::Activated" {
            // Activation-cost modifiers can inspect announced targets.
            // Establish those labels before visiting the modifiers.
            ordered.sort_by_key(|(field, _)| match field.as_str() {
                "announcement" => 1,
                "limits" => 2,
                "body" => 3,
                _ => 0,
            });
        }
        ordered
    }
    fn named_fields(
        &mut self,
        value: &mut Value,
        fields: &Fields,
        owner: &str,
        types: &[(String, Ty)],
    ) {
        let binding_namespace = self.binding_namespace;

        // Announced targets belong to the created invocation. It can
        // capture the creator's targets, but cannot define slots for a
        // later sibling trigger or the creating program.
        let created_slots = matches!(
            owner,
            "Operation::CreateTrigger" | "Operation::ReflexiveTrigger"
        )
        .then(|| self.target_scope());
        let mut branch_joins = JoinGroup::new();
        // A loop or predicate binds only its own body. Keeping those
        // ids in the ability-wide map makes a later sibling depend on
        // traversal order, including when commutative events move.
        let binding = types
            .iter()
            .any(|(field, ty)| field == "binding" && *ty == Ty::Named("LoopBinding".into()));
        let previous_binding = binding
            .then(|| {
                self.scopes
                    .last()
                    .map(|s| s.maps.get(binding_namespace).cloned())
            })
            .flatten();
        let previous_selection = (owner == "Operation::WithSelections").then(|| {
            self.scopes
                .last()
                .and_then(|s| s.maps.get("selection").cloned())
        });
        if owner == "Operation::CreateTrigger" && !self.keep_order {
            entry::header(value, fields, self.schema);
        }
        let created = events::created(owner, value);
        let old_reductions = self.reference_reductions;
        let old_count = if owner == "Operation::CreateTrigger" {
            self.object_batch_counts
                .push(trigger_batches::amount_is_group_size(&value["event"]));
            true
        } else {
            false
        };
        if matches!(
            owner,
            "Operation::CreateDelayedTrigger"
                | "Operation::CreateReplacement"
                | "Operation::ReflexiveTrigger"
        ) {
            // Closures may retain the creator's event or install a new
            // event. Preserve their explicit references rather than
            // equating roles without proof of that binding context.
            self.reference_reductions = false;
        }
        // A preparation step's inputs see the earlier bindings. Its
        // own binding starts only after those inputs have been read.
        let selection = owner == "SelectionStep";
        let ordered = Self::field_order(types, owner, selection);
        for (field, ty) in ordered {
            if let Some(item) = value.get_mut(field) {
                if owner == "Cost" && field == "parts" && self.paying_cost {
                    if let (Value::Array(parts), Ty::Vec(inner)) = (item, ty) {
                        self.payment_parts(parts, inner);
                    }
                    continue;
                }
                if owner == "Operation::JoinResults" && field == "results" {
                    self.join_ids(item);
                    continue;
                }
                if owner == "Instruction::ChooseOne" && field == "options" {
                    if let (Value::Array(items), Ty::Vec(inner)) = (item, ty) {
                        self.choice_branches(items, inner);
                    }
                    continue;
                }

                if owner == "Ability::Saga" && field == "chapters" {
                    if let (Value::Array(items), Ty::Vec(inner)) = (item, ty) {
                        for chapter in items {
                            self.walk(chapter, inner, false);
                        }
                    }
                    continue;
                }
                let old_header = self.event_header;
                let old_reference_form = self.reference_form;
                self.reference_form = self.reference_form.for_field(owner, field);
                if field == "event" {
                    self.event_header = true;
                }
                // Delayed/replacement/reflexive bodies inherit earlier
                // definitions, but do not define results in the creator.
                let local_results = ((field == "body"
                    && matches!(
                        owner,
                        "Operation::CreateDelayedTrigger"
                            | "Operation::CreateReplacement"
                            | "Operation::ReflexiveTrigger"
                            | "Operation::CreateTrigger"
                            | "EntryState::ExiledForWith"
                            | "ExileState::ExiledForWith"
                            | "Instruction::Optional"
                            | "CostPart::PlayerAction"
                            | "Instruction::ForEachPlayer"
                            | "Instruction::ForEachObject"
                            | "Instruction::RepeatableWithLimit"
                    ))
                    || owner == "Instruction::If"
                        && matches!(field.as_str(), "then" | "otherwise"))
                .then(|| self.result_scope());
                let exported = if field == "body" {
                    match owner {
                        "Instruction::ForEachPlayer" | "Instruction::ForEachObject" => {
                            visibility::exports(item, true)
                        }
                        "Instruction::Optional" => {
                            visibility::exports(item, self.gathering_results)
                        }
                        "CostPart::PlayerAction" if self.paying_cost => {
                            visibility::exports(item, false)
                        }
                        _ => HashSet::new(),
                    }
                } else {
                    HashSet::new()
                };
                let old_gathering = self.gathering_results;
                if field == "body" {
                    match owner {
                        "Instruction::ForEachPlayer" | "Instruction::ForEachObject" => {
                            self.gathering_results = true
                        }
                        "Operation::CreateDelayedTrigger"
                        | "Operation::CreateReplacement"
                        | "Operation::ReflexiveTrigger"
                        | "Operation::CreateTrigger"
                        | "EntryState::ExiledForWith"
                        | "ExileState::ExiledForWith"
                        | "Instruction::RepeatableWithLimit"
                        | "CostPart::PlayerAction" => self.gathering_results = false,
                        _ => {}
                    }
                } else if owner == "Instruction::If"
                    && matches!(field.as_str(), "then" | "otherwise")
                {
                    self.gathering_results = false;
                }
                let old_cost = self.paying_cost;
                let old_payment = self.payment_depth;
                if owner == "Instruction::Effect" && field == "operation" {
                    self.payment_depth =
                        (item["kind"] == "PayCost").then_some(self.operation_depth + 1);
                }
                self.paying_cost = owner == "Operation::PayCost"
                    && field == "cost"
                    && self.payment_depth == Some(self.operation_depth)
                    || self.paying_cost && owner == "Cost" && field == "parts";
                if let Some(frame) = created.as_ref().filter(|_| {
                    matches!(field.as_str(), "body" | "announcement" | "intervening_if")
                        || matches!(
                            owner,
                            "EntryState::ExiledForWith" | "ExileState::ExiledForWith"
                        ) && field == "objects"
                }) {
                    self.walk_created(item, ty, frame);
                } else {
                    self.walk(item, ty, defines(owner, field));
                }
                self.paying_cost = old_cost;
                self.payment_depth = old_payment;
                self.gathering_results = old_gathering;
                if let Some(saved) = local_results {
                    if owner == "Instruction::If"
                        && let Some(saved) = &saved
                    {
                        self.collect_join_branch(saved.floor, &mut branch_joins);
                    }
                    self.restore_with_exports(saved, &exported);
                }
                self.event_header = old_header;
                self.reference_form = old_reference_form;
            }
        }
        if owner == "Instruction::If"
            && let Some(scope) = self.scopes.last_mut()
        {
            scope.join_groups.push(branch_joins);
        }
        if old_count {
            self.object_batch_counts.pop();
        }
        self.reference_reductions = old_reductions;
        if let Some(previous) = created_slots {
            self.restore_target_scope(previous);
        }
        if let (Some(previous), Some(scope)) = (previous_selection, self.scopes.last_mut()) {
            if let Some(map) = previous {
                scope.maps.insert("selection", map);
            } else {
                scope.maps.remove("selection");
            }
        }
        if let (Some(previous), Some(scope)) = (previous_binding, self.scopes.last_mut()) {
            match previous {
                Some(map) => {
                    scope.maps.insert(binding_namespace, map);
                }
                None => {
                    scope.maps.remove(binding_namespace);
                }
            }
        }
    }

    /// Created trigger bodies have their own matched event. Their matcher and
    /// duration still capture the creator's context. Their invocation's local
    /// results cannot escape into the creator; allocation remains deterministic.
    fn walk_created(&mut self, value: &mut Value, ty: &Ty, frame: &events::Frame) {
        let old_header = self.event_header;
        let old_reductions = self.reference_reductions;
        self.subjects.push(frame.subject);
        self.damage_subjects.push(frame.damage.clone());
        self.event_header = false;
        self.reference_reductions = true;
        self.walk(value, ty, false);
        self.subjects.pop();
        self.damage_subjects.pop();
        self.event_header = old_header;
        self.reference_reductions = old_reductions;
    }
}

/// A group only nests a sequence; its instructions run in the sequence around it.
fn splice_groups(items: &mut Vec<Value>) {
    if !items.iter().any(|item| item["kind"] == "Group") {
        return;
    }
    let mut flat = Vec::with_capacity(items.len());
    for item in items.drain(..) {
        match item["args"]["instructions"].as_array() {
            Some(inner) if item["kind"] == "Group" => flat.extend(inner.iter().cloned()),
            _ => flat.push(item),
        }
    }
    *items = flat;
}

/// Two statics in a row for the same objects are one static with both changes.
fn merge_statics(items: &mut Vec<Value>, tracing: bool) {
    let mut merged: Vec<Value> = Vec::with_capacity(items.len());
    for item in items.drain(..) {
        if let Some(last) = merged.last_mut() {
            let same = item["kind"] == "Static"
                && last["kind"] == "Static"
                && trace::same(&item["args"]["kind"], &last["args"]["kind"], tracing)
                && trace::same(
                    &item["args"]["affected"],
                    &last["args"]["affected"],
                    tracing,
                );
            if same {
                if tracing {
                    for field in ["kind", "affected"] {
                        trace::merge(&mut last["args"][field], &item["args"][field]);
                    }
                }
                let more = item["args"]["changes"]
                    .as_array()
                    .cloned()
                    .unwrap_or_default();
                if let Some(changes) = last["args"]["changes"].as_array_mut() {
                    changes.extend(more);
                }
                continue;
            }
        }
        merged.push(item);
    }
    *items = merged;
}

/// Indexing one currently filtered object already enforces this exact zone.
/// Keep the inner query: unlike EventSubject/Source, it selects nothing after
/// the object leaves. No history view or mismatched zone is simplified here.
fn repeated_current_zone(value: &mut Value) {
    if !value.as_object().is_some_and(|fields| fields.len() == 2)
        || !value["args"]
            .as_object()
            .is_some_and(|fields| fields.len() == 2)
    {
        return;
    }
    let outer = &value["args"];
    let one = &outer["objects"];
    let scope = &one["args"]["scope"];
    if one["kind"] == "Zone"
        && outer["zone"].is_string()
        && (scope["Shared"] == outer["zone"] || scope["Players"]["zone"] == outer["zone"])
    {
        // A live query already guarantees its exact zone. Retain the whole
        // query, including its player, linked-object predicates and origins.
        *value = one.clone();
        return;
    }
    let reference = &one["args"];
    let query = &reference["args"];
    let objects = &query["objects"];
    if one["kind"] == "One"
        && reference["kind"] == "Query"
        && query["index"] == 0
        && objects["kind"] == "InZone"
        && objects["args"]["objects"]["kind"] == "One"
        && objects["args"]["zone"] == outer["zone"]
    {
        *value = one.clone();
    }
}

/// `All([a, All([b])])` means `All([a, b])`, for `All` and for `Either`.
fn flatten(items: &mut Vec<Value>, variant: &str) {
    let mut flat = Vec::with_capacity(items.len());
    for item in items.drain(..) {
        match (item.get("kind").and_then(Value::as_str), item.get("args")) {
            (Some(kind), Some(Value::Array(inner)))
                if kind == variant && item.as_object().is_some_and(|fields| fields.len() == 2) =>
            {
                flat.extend(inner.iter().cloned());
            }
            _ => flat.push(item),
        }
    }
    *items = flat;
}

/// `All` of nothing but `Any` keeps one, so the list still means `Any`.
fn drop_identity(items: &mut Vec<Value>) {
    let any = |item: &Value| {
        item.as_object().is_some_and(|fields| fields.len() == 1)
            && item.get("kind").and_then(Value::as_str) == Some("Any")
    };
    if items.iter().any(|item| !any(item)) {
        items.retain(|item| !any(item));
    } else {
        items.truncate(1);
    }
}

fn content_of(value: &mut Value) -> &mut Value {
    match value.get("args") {
        Some(_) => &mut value["args"],
        None => value,
    }
}

/// `InSet(One(x))` is `SameObject(x)`.
fn one_object_set(value: &mut Value) {
    if value["args"]["kind"] == "One" {
        let object = value["args"]["args"].take();
        *value = serde_json::json!({"kind": "SameObject", "args": object});
    }
}

/// Every object on the battlefield is a permanent, so naming all six types
/// narrows nothing there. Other zones keep it: it is what `permanent card` means.
fn drop_permanent_types(filter: &mut Value) {
    let names_all = |item: &Value| {
        let Some(cases) = item["args"].as_array() else {
            return false;
        };
        item["kind"] == "AnyOf"
            && cases.len() == 6
            && [
                "Artifact",
                "Battle",
                "Creature",
                "Enchantment",
                "Land",
                "Planeswalker",
            ]
            .iter()
            .all(|name| {
                cases
                    .iter()
                    .any(|case| case["kind"] == "CardType" && case["args"] == *name)
            })
    };
    if names_all(filter) {
        *filter = serde_json::json!({"kind": "Any"});
        return;
    }
    if filter["kind"] != "AllOf" {
        return;
    }
    if let Some(items) = filter["args"].as_array_mut() {
        items.retain(|item| !names_all(item));
    }
    if let Some(only) = single(&filter["args"]) {
        *filter = only;
    }
}

/// A scope over all players narrowed by an `InZone` of that zone is that
/// player's zone, however the filter is nested.
fn narrow_zone_scope(zone: &mut Value) {
    let wide = |scope: &Value| scope["Players"]["players"]["kind"] == "All";
    if !wide(&zone["scope"]) {
        return;
    }
    let same_zone = |filter: &Value, scope: &Value| {
        filter["kind"] == "InZone" && filter["args"]["Players"]["zone"] == scope["Players"]["zone"]
    };
    let scope = zone["scope"].clone();
    if same_zone(&zone["filter"], &scope) {
        zone["scope"] = zone["filter"]["args"].clone();
        zone["filter"] = serde_json::json!({"kind": "Any"});
        return;
    }
    if zone["filter"]["kind"] != "AllOf" {
        return;
    }
    let Some(items) = zone["filter"]["args"].as_array_mut() else {
        return;
    };
    let Some(index) = items.iter().position(|item| same_zone(item, &scope)) else {
        return;
    };
    let narrowed = items.remove(index);
    if let Some(only) = single(&zone["filter"]["args"]) {
        zone["filter"] = only;
    }
    zone["scope"] = narrowed["args"].clone();
}

/// Divided among any number of targets: at most one target per point.
fn bound_divided_count(spec: &mut Value) {
    if spec["count"] == "AnyNumber" && !spec["distribute"].is_null() {
        spec["count"] = serde_json::json!({"UpTo": spec["distribute"].clone()});
    }
}

/// A commutative list of one item means that item.
fn single(content: &Value) -> Option<Value> {
    match content {
        Value::Array(items) if items.len() == 1 => items.first().cloned(),
        _ => None,
    }
}

/// Every other position of an id type reads an id defined at one of these.
fn defines(owner: &str, field: &str) -> bool {
    field == "binding"
        || (owner == "TargetSpec" && field == "slot")
        || (owner == "Instruction::Effect" && field == "result")
}

fn normalize(value: &mut Value, keep_order: bool) -> Result<(), String> {
    normalize_with_trace(value, keep_order, false)
}
fn normalize_with_trace(value: &mut Value, keep_order: bool, tracing: bool) -> Result<(), String> {
    if tracing {
        trace::movement_groups(value)?;
    } else {
        movement_groups(value)?;
    }
    walk_normal_form(value, keep_order, tracing)?;
    let joined = !keep_order && spell_paragraphs::join_closed_draws(value, schema()?, tracing);
    let reduced = programs::normalize(value, schema()?, tracing, keep_order)?;
    if !keep_order && (joined || reduced) {
        // Program reductions and joins can change local definition scopes.
        // One final pass assigns identities in the new scopes.
        walk_normal_form(value, keep_order, tracing)?;
    }
    Ok(())
}

fn walk_normal_form(value: &mut Value, keep_order: bool, tracing: bool) -> Result<(), String> {
    walk_reference_form(value, keep_order, tracing, ReferenceForm::Ordinary)
}

fn walk_reference_form(
    value: &mut Value,
    keep_order: bool,
    tracing: bool,
    reference_form: ReferenceForm,
) -> Result<(), String> {
    let mut walker = Walker {
        schema: schema()?,
        binding_namespace: "binding",
        scopes: Vec::new(),
        damage_subjects: Vec::new(),
        object_batch_counts: Vec::new(),
        subjects: Vec::new(),
        event_header: false,
        reference_reductions: true,
        reference_form,
        keep_order,
        tracing,
        read: Vec::new(),
        reading: !keep_order,
        paying_cost: false,
        operation_depth: 0,
        payment_depth: None,
        gathering_results: false,
        visits: 0,
        join_work: 0,
        failure: None,
    };
    let abilities = Ty::Vec(Box::new(Ty::Named("Ability".into())));
    if !keep_order {
        walker.walk(&mut value.clone(), &abilities, false);
        if let Some(error) = walker.failure.take() {
            return Err(error);
        }
        walker.reading = false;
        walker.visits = 0;
        walker.join_work = 0;
    }
    walker.walk(value, &abilities, false);
    match walker.failure {
        Some(error) => Err(error),
        None => Ok(()),
    }
}

/// The canonical form of serialized abilities, in place.
pub fn abilities(value: &mut Value) -> Result<(), String> {
    normalize(value, false)
}

/// Meaning comparison explicitly removes presentation before canonicalizing.
/// Ordinary `abilities` and `reduced` preserve it for exact round trips.
pub fn semantic_abilities(value: &mut Value) -> Result<(), String> {
    crate::presentation::erase_abilities(value)?;
    abilities(value)
}

/// The same reductions with ids and list order left as lowered: the order is
/// what a printer reads the card's wording back from.
pub fn reduced(value: &mut Value) -> Result<(), String> {
    normalize(value, true)
}

/// At most four additional print views of current operands, retaining IDs/order.
/// Only a proved source event can spell Source as its distinguished subject;
/// event headers and independent/deferred event contexts keep their own roles.
/// Literal creator and announcement Source roles can each retain their form.
/// The caller retains the ordinary form and verifies every generated text.
pub fn event_subject_print_views(value: &Value) -> Result<Vec<Value>, String> {
    let mut views = Vec::new();
    for form in [
        ReferenceForm::SubjectOperands,
        ReferenceForm::SubjectCreators,
        ReferenceForm::SubjectAnnouncements,
        ReferenceForm::Subject,
    ] {
        let mut view = value.clone();
        walk_reference_form(&mut view, true, false, form)?;
        if view != *value && !views.contains(&view) {
            views.push(view);
        }
    }
    Ok(views)
}

/// Whether the distinguished object is the source, with its role available.
/// This structural identity proof does not replace event validation; unknown
/// or over-budget shapes conservatively give no proof.
pub fn event_subject_names_source(event: &Value) -> bool {
    source_events::names_source(event)
}

pub fn card(card: &Card) -> Result<Card, String> {
    let mut value = serde_json::to_value(card).map_err(|e| e.to_string())?;
    let abilities = value.get_mut("abilities").ok_or("card has no abilities")?;
    self::abilities(abilities)?;
    serde_json::from_value(value).map_err(|e| e.to_string())
}

/// Full-face meaning comparison, with the card context that derives intrinsic
/// basic-land mana. A retained printed manifestation and its omission compare
/// alike; ordinary `card` preserves the written definition for printing.
pub fn semantic_card(card: &Card) -> Result<Card, String> {
    crate::validate::card(card)?;
    let mut compared = card.clone();
    compared.abilities = crate::intrinsic::nonintrinsic_printed_abilities(card)
        .cloned()
        .collect();
    let mut abilities = serde_json::to_value(&compared.abilities).map_err(|e| e.to_string())?;
    semantic_abilities(&mut abilities)?;
    compared.abilities = serde_json::from_value(abilities).map_err(|e| e.to_string())?;
    Ok(compared)
}

/// Only shared, permanent battlefield queries imply the permanent type domain.
fn battlefield_filter(value: &Value) -> Option<&Value> {
    (value["kind"] == "Zone"
        && value["args"]["scope"] == json!({"Shared":"Battlefield"})
        && value["args"]["kind"] == "Permanent")
        .then(|| &value["args"]["filter"])
}
fn battlefield_filter_mut(value: &mut Value) -> Option<&mut Value> {
    battlefield_filter(value)?;
    value.get_mut("args")?.get_mut("filter")
}

/// Canonicalize every face and metadata ability in a unified record.
pub fn unified(card: &crate::ir::UnifiedCard) -> Result<crate::ir::UnifiedCard, String> {
    trace::unified(card)
}
/// The same canonical form with a complete typed-ID provenance map, including
/// removed, cloned and coalesced sites. Tracing is checked against the plain form.
pub fn mapped_unified(card: &crate::ir::UnifiedCard) -> Result<MappedCard, String> {
    trace::mapped(card)
}
