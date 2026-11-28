//! A resolution-time counter choice has a selection receipt independent of
//! which counters a replacement effect allows the placement to put.
use super::{
    trace,
    walk::{Uses, Visit, Walker, lossless},
};
use crate::{ir::Instruction, ir_schema::Ty};
use serde_json::{Value, json};

fn max_one(count: &Value) -> bool {
    (count["AsManyAsCan"] == json!({"kind":"Constant","args":1}))
        || (count["UpTo"] == json!({"kind":"Constant","args":1}))
}

pub(super) fn may_need(op: &Value) -> bool {
    if op["kind"] != "PutCounters" {
        return false;
    }
    let Some([placement]) = op["args"]["placements"].as_array().map(Vec::as_slice) else {
        return false;
    };
    max_one(&placement["selection"]["count"])
        || (placement["recipients"]["kind"] == "Objects"
            && placement["recipients"]["args"]["kind"] == "One"
            && placement["recipients"]["args"]["args"]["kind"] == "Result")
}

fn placement(instruction: &Value) -> Option<&Value> {
    let operation = &instruction["args"]["operation"];
    if instruction["kind"] != "Effect" || operation["kind"] != "PutCounters" {
        return None;
    }
    let [placement] = operation["args"]["placements"].as_array()?.as_slice() else {
        return None;
    };
    Some(placement)
}

fn grant_subject(instruction: &Value) -> Option<&Value> {
    let operation = &instruction["args"]["operation"];
    let args = &operation["args"];
    if instruction["kind"] != "Effect"
        || operation["kind"] != "CreateContinuousEffect"
        || args["evaluation"] != "Snapshot"
        || args["affected"]["Objects"].is_null()
    {
        return None;
    }
    let changes = args["changes"].as_array()?;
    (!changes.is_empty() && changes.len() <= 8).then_some(&args["affected"]["Objects"])
}

fn object_for_choice(objects: &Value, result: u64) -> bool {
    let object = &objects["args"];
    objects["kind"] == "One"
        && object["kind"] == "Result"
        && object["args"]["phase"] == "After"
        && object["args"]["index"] == 0
        && trace::number(&object["args"]["result"]) == Some(result)
}

fn direct_program(
    pick: &Value,
    counter: &Value,
    tail: &[Value],
    uses: &Uses,
    tracing: bool,
) -> Option<(Value, bool)> {
    let selection = &pick["args"]["operation"];
    let args = &selection["args"];
    if pick["kind"] != "Effect"
        || selection["kind"] != "ChooseObjects"
        || args["player"]["kind"] != "AbilityController"
        || !max_one(&args["count"])
        || args["constraints"] != json!([])
        || args["objects"]["kind"] != "Zone"
        || args["objects"]["args"]["scope"]["Shared"] != "Battlefield"
        || args["objects"]["args"]["kind"] != "Permanent"
    {
        return None;
    }
    let choice = trace::number(&pick["args"]["result"])?;
    let spec = placement(counter)?;
    if !spec["selection"].is_null()
        || spec["recipients"]["kind"] != "Objects"
        || !object_for_choice(&spec["recipients"]["args"], choice)
        || spec["amount"]["kind"] != "Constant"
        || uses.definitions.get(&choice) != Some(&1)
        || lossless::<Instruction>(pick, tracing).is_none()
        || lossless::<Instruction>(counter, tracing).is_none()
    {
        return None;
    }
    let grant = tail.first().is_some_and(|instruction| {
        let op = &instruction["args"]["operation"]["args"];
        matches!(op["changes"].as_array().map(Vec::as_slice), Some([change])
                if change["kind"] == "GrantKeyword")
            && grant_subject(instruction).is_some_and(|objects| object_for_choice(objects, choice))
    });
    if uses.total.get(&choice) != Some(&(2 + usize::from(grant))) {
        return None;
    }
    let mut combined = counter.clone();
    let placement = &mut combined["args"]["operation"]["args"]["placements"][0];
    placement["recipients"]["args"] = args["objects"].clone();
    placement["selection"] = json!({"player":args["player"],"count":args["count"]});
    Some((combined, grant))
}

fn read_preceding_counter(objects: &Value, result: u64) -> bool {
    let args = &objects["args"];
    if matches!(objects["kind"].as_str(), Some("Affected" | "Selected")) {
        return trace::number(args) == Some(result);
    }
    objects["kind"] == "One"
        && args["kind"] == "Query"
        && args["args"]["index"] == 0
        && args["args"]["objects"]["kind"] == "Affected"
        && trace::number(&args["args"]["objects"]["args"]) == Some(result)
}

fn grant_selected(counter: &Value, grant: &mut Value, tracing: bool) -> bool {
    let Some(spec) = placement(counter) else {
        return false;
    };
    let label = &counter["args"]["result"];
    let Some(result) = trace::number(label) else {
        return false;
    };
    if !max_one(&spec["selection"]["count"])
        || grant_subject(grant).is_none_or(|objects| {
            objects["kind"] == "Selected" || !read_preceding_counter(objects, result)
        })
        || lossless::<Instruction>(counter, tracing).is_none()
        || lossless::<Instruction>(grant, tracing).is_none()
    {
        return false;
    }
    grant["args"]["operation"]["args"]["affected"]["Objects"] =
        json!({"kind":"Selected","args":label});
    true
}

pub(super) fn program(items: &mut Vec<Value>, uses: &Uses, tracing: bool) -> bool {
    let mut changed = false;
    // Rewrites consume only their own choice, leaving earlier use counts and
    // bindings valid for the next independently proven pair.
    for i in (1..items.len()).rev() {
        let (before, after) = items.split_at_mut(i);
        if let Some((mut combined, shares_grant)) =
            direct_program(&before[i - 1], &after[0], &after[1..], uses, tracing)
        {
            if shares_grant {
                after[1]["args"]["operation"]["args"]["affected"]["Objects"] =
                    json!({"kind":"Selected","args":combined["args"]["result"]});
            }
            after[0] = combined.take();
            items.remove(i - 1);
            changed = true;
        } else {
            changed |= grant_selected(&before[i - 1], &mut after[0], tracing);
        }
    }
    changed
}

fn selected_grant(counter: &Value, grant: &Value) -> Option<u64> {
    let placement = placement(counter)?;
    let result = counter["args"]["result"].as_u64()?;
    let objects = grant_subject(grant)?;
    (max_one(&placement["selection"]["count"])
        && objects["kind"] == "Selected"
        && objects["args"] == result)
        .then_some(result)
}

fn mentions(value: &Value, depth: usize, remaining: &mut usize) -> bool {
    if depth > 128 || *remaining == 0 {
        return false;
    }
    *remaining -= 1;
    match value {
        Value::Object(fields) => fields.values().any(|v| mentions(v, depth + 1, remaining)),
        Value::Array(items) => {
            items
                .windows(2)
                .any(|p| selected_grant(&p[0], &p[1]).is_some())
                || items.iter().any(|v| mentions(v, depth + 1, remaining))
        }
        _ => false,
    }
}

#[derive(Default)]
struct PrintSites(Vec<(String, Value)>);

impl Visit for PrintSites {
    fn leave(
        &mut self,
        value: &mut Value,
        ty: &Ty,
        _: bool,
        path: &str,
        _: &mut Walker<'_>,
    ) -> Result<(), String> {
        if matches!(ty, Ty::Named(name) if name == "Program")
            && let Some(items) = value["instructions"].as_array()
        {
            for (i, pair) in items.windows(2).enumerate() {
                if self.0.len() >= 4 {
                    break;
                }
                if let Some(result) = selected_grant(&pair[0], &pair[1]) {
                    self.0.push((
                        format!(
                            "{path}/instructions/{}/args/operation/args/affected/Objects",
                            i + 1
                        ),
                        json!({"kind":"Affected","args":result}),
                    ));
                }
            }
        }
        Ok(())
    }
}

/// Spell a proved counter recipient with the ordinary counter-reference rules.
pub fn print_views(ability: &Value) -> Result<Vec<Value>, String> {
    if !mentions(ability, 0, &mut 131_072) {
        return Ok(Vec::new());
    }
    let mut original = ability.clone();
    let mut sites = PrintSites::default();
    Walker::new(super::schema()?).walk(
        &mut original,
        &Ty::Named("Ability".into()),
        false,
        "",
        0,
        &mut sites,
    )?;
    super::set_views::verified_views(original, &sites.0)
}
