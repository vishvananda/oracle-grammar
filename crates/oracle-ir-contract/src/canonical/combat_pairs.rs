//! Compare the participants of a shared block/blocked-by trigger.
use super::{
    trace,
    walk::{Visit, Walker},
};
use crate::ir_schema::{Schema, Ty};
use serde_json::{Value, json};

const MAX_COMPARISON_WORK: usize = 8192;

fn participant(value: &Value) -> Option<(&'static str, &Value)> {
    let args = &value["args"];
    let event = &args["event"];
    let pair = &event["Pair"];
    if value["kind"] != "Triggered"
        || !args["limits"]["per_turn"].is_null()
        || !args["limits"]["total"].is_null()
        || !event.as_object().is_some_and(|fields| fields.len() == 1)
        || !pair.as_object().is_some_and(|fields| fields.len() == 3)
        || pair["objects"]["kind"] != "SameObject"
    {
        return None;
    }
    let other = match pair["event"].as_str()? {
        "Blocks" => "Attacker",
        "BecomesBlockedBy" => "Blocker",
        _ => return None,
    };
    Some((other, pair))
}

/// A direct Pair supplies both combatants. A turn-delayed body captures the
/// original pair. New event headers, triggers, and ability holders keep their
/// own roles, even when their body happens to use the same role name.
fn new_event_frame(value: &Value, ty: &Ty) -> bool {
    if !matches!(ty, Ty::Named(name) if name == "Operation") {
        return false;
    }
    match value["kind"].as_str() {
        Some("CreateTrigger" | "CreateReplacement" | "ReflexiveTrigger") => true,
        Some("CreateDelayedTrigger") => value["args"]["event"].get("Turn").is_none(),
        _ => false,
    }
}

struct PairRole<'a> {
    concrete: &'static str,
    abilities: usize,
    headers: usize,
    new_frames: usize,
    remaining: &'a mut usize,
}
impl Visit for PairRole<'_> {
    fn enter(&mut self, value: &Value, ty: &Ty) {
        *self.remaining = self.remaining.saturating_sub(1);
        self.abilities += usize::from(matches!(ty, Ty::Named(name) if name == "Ability"));
        self.headers += usize::from(
            matches!(ty, Ty::Named(name) if matches!(name.as_str(), "EventSpec" | "ReplacementEvent")),
        );
        self.new_frames += usize::from(new_event_frame(value, ty));
    }

    fn leave(
        &mut self,
        value: &mut Value,
        ty: &Ty,
        _: bool,
        _: &str,
        _: &mut Walker<'_>,
    ) -> Result<(), String> {
        if *self.remaining == 0 {
            return Err("combat role comparison work limit".into());
        }
        if self.abilities == 1
            && self.headers == 0
            && self.new_frames == 0
            && matches!(ty, Ty::Named(name) if name == "ObjectRef")
            && value.as_object().is_some_and(|fields| fields.len() == 2)
            && value["kind"] == "BlockPair"
            && value["args"] == self.concrete
        {
            value["args"] = Value::String("Other".into());
        }
        self.abilities -= usize::from(matches!(ty, Ty::Named(name) if name == "Ability"));
        self.headers -= usize::from(
            matches!(ty, Ty::Named(name) if matches!(name.as_str(), "EventSpec" | "ReplacementEvent")),
        );
        self.new_frames -= usize::from(new_event_frame(value, ty));
        Ok(())
    }
}

/// The end-of-combat return captures the other participant in this block pair.
pub(super) fn delayed_bounce(ability: &mut Value) {
    let Some((role, _)) = participant(ability) else {
        return;
    };
    let Some([delay]) = ability["args"]["body"]["instructions"]
        .as_array_mut()
        .map(Vec::as_mut_slice)
    else {
        return;
    };
    if delay["kind"] != "Effect" {
        return;
    }
    let trigger = &mut delay["args"]["operation"];
    if trigger["kind"] != "CreateDelayedTrigger"
        || trigger["args"]["event"]
            != json!({"Turn":{"point":"CombatEnds","players":{"kind":"All"}}})
        || trigger["args"]["start"] != "Immediately"
        || !trigger["args"]["expires"].is_null()
        || !trigger["args"]["intervening_if"].is_null()
    {
        return;
    }
    let Some([returning]) = trigger["args"]["body"]["instructions"]
        .as_array_mut()
        .map(Vec::as_mut_slice)
    else {
        return;
    };
    if returning["kind"] != "Effect" {
        return;
    }
    let movement = &mut returning["args"]["operation"];
    if movement["kind"] == "Move"
        && movement["args"]["qualifiers"] == json!([])
        && movement["args"]["destination"].get("OwnersHand").is_some()
        && movement["args"]["objects"]
            == json!({"kind":"One","args":{"kind":"BlockPair","args":"Other"}})
    {
        movement["args"]["objects"]["args"]["args"] = json!(role);
    }
}

pub(super) fn comparable(
    left: &Value,
    right: &Value,
    schema: &Schema,
    tracing: bool,
) -> Option<(Value, Value)> {
    let (left_role, left_pair) = participant(left)?;
    let (right_role, right_pair) = participant(right)?;
    if left_role == right_role
        || ["objects", "others"]
            .iter()
            .any(|field| !trace::same(&left_pair[*field], &right_pair[*field], tracing))
    {
        return None;
    }
    let (mut left, mut right) = (left.clone(), right.clone());
    let mut remaining = MAX_COMPARISON_WORK;
    let ability = Ty::Named("Ability".into());
    let mut walker = Walker::new(schema);
    for (value, concrete) in [(&mut left, left_role), (&mut right, right_role)] {
        let mut roles = PairRole {
            concrete,
            abilities: 0,
            headers: 0,
            new_frames: 0,
            remaining: &mut remaining,
        };
        walker
            .walk(value, &ability, false, "", 0, &mut roles)
            .ok()?;
    }
    Some((left, right))
}
