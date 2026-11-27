//! The reminder addresses the plotted card's owner. Prove the ownership
//! through the printed cast, payment, or library selection and exile.
use super::evidence::step_operation;
use super::object_notes::{filter_proves, is_controller};
use super::receipts::{
    instructions, nonland, own_zone, owned_nonland_hand_exile, producer, subject_query,
};
use serde_json::Value;

pub(super) fn plots_for_controller(ability: &Value) -> bool {
    let args = &ability["args"];
    let Some(steps) = instructions(&args["body"]) else {
        return false;
    };
    steps.iter().enumerate().any(|(index, step)| {
        plot_cast(step, &args["event"]["Spell"])
            || plot_paid_card(step, &steps[..index])
            || plot_library_card(step, &steps[..index])
    })
}

fn plot_effect(step: &Value) -> Option<&Value> {
    let op = step_operation(step)?;
    (op["kind"] == "EffectOn").then(|| &op["args"])
}

fn term(value: &Value, family: &str, form: &str) -> bool {
    value["kind"] == "Term" && value["args"]["family"] == family && value["args"]["form"] == form
}

fn any_effect(op: &Value, matches: impl Fn(&Value) -> bool) -> bool {
    op["effects"]
        .as_array()
        .is_some_and(|effects| effects.iter().take(16).any(matches))
}

fn plot_subject(step: &Value) -> Option<&Value> {
    let effect = plot_effect(step)?;
    any_effect(effect, |term_value| {
        term(term_value, "ExiledCardEffect", "IsPlotted")
    })
    .then(|| &effect["subject"]["Objects"])
}

fn plot_cast(step: &Value, cast: &Value) -> bool {
    if cast["event"] != "Cast" || !is_controller(&cast["players"]) {
        return false;
    }
    let Some(effect) = plot_effect(step) else {
        return false;
    };
    let objects = &effect["subject"]["Objects"];
    objects["kind"] == "One"
        && objects["args"]["kind"] == "EventSubject"
        && filter_proves(
            &cast["spells"],
            &|predicate| {
                let hand = &predicate["args"]["from"]["Players"];
                let players = &hand["players"];
                predicate["kind"] == "Cast"
                    && hand["zone"] == "Hand"
                    && players["kind"] == "One"
                    && players["args"]["kind"] == "CasterOf"
                    && players["args"]["args"]["kind"] == "Candidate"
            },
            0,
            &mut 256,
        )
        && any_effect(effect, |effect| {
            term(effect, "SpellEffect", "AsResolves")
                && effect["args"]["operands"][0]["kind"] == "List"
                && effect["args"]["operands"][0]["args"]
                    .as_array()
                    .is_some_and(|items| {
                        items
                            .iter()
                            .take(16)
                            .any(|item| term(item, "ResolveAction", "ExileResolvingSpellAndPlotIt"))
                    })
        })
}

fn plot_paid_card(step: &Value, prefix: &[Value]) -> bool {
    let condition = &step["args"]["condition"];
    if step["kind"] != "If" || condition["kind"] != "PaymentChosen" {
        return false;
    }
    let Some([grant]) = instructions(&step["args"]["then"]) else {
        return false;
    };
    let Some(objects) = plot_subject(grant).and_then(subject_query) else {
        return false;
    };
    let receipt = &objects["args"];
    let payment = &condition["args"];
    if objects["kind"] != "CostObjects"
        || receipt["payment"]["Resolution"] != *payment
        || receipt["view"] != "Performed"
        || receipt["phase"] != "After"
    {
        return false;
    }
    let Some([part]) = receipt["component"].as_array().map(Vec::as_slice) else {
        return false;
    };
    let Some(index) = part.as_u64().filter(|&n| n < 32) else {
        return false;
    };
    producer(prefix, payment).is_some_and(|op| {
        op["kind"] == "PayCost"
            && op["args"]["player"]["kind"] == "AbilityController"
            && owned_nonland_hand_exile(&op["args"]["cost"], index as usize)
    })
}

fn plot_library_card(step: &Value, prefix: &[Value]) -> bool {
    let Some(subject) = plot_subject(step) else {
        return false;
    };
    let result = &subject["args"];
    if subject["kind"] != "One"
        || result["kind"] != "Result"
        || result["args"]["phase"] != "After"
        || result["args"]["index"] != 0
    {
        return false;
    }
    let Some(movement) = producer(prefix, &result["args"]["result"]) else {
        return false;
    };
    let objects = &movement["args"]["objects"];
    if movement["kind"] != "Move"
        || !movement["args"]["destination"]["Exile"].is_object()
        || objects["kind"] != "Selected"
    {
        return false;
    }
    let Some(choice) = producer(prefix, &objects["args"]) else {
        return false;
    };
    let objects = &choice["args"]["objects"];
    let in_zone = &objects["args"]["objects"];
    let observed = &in_zone["args"]["objects"];
    choice["kind"] == "ChooseObjects"
        && objects["kind"] == "Matching"
        && filter_proves(&objects["args"]["filter"], &nonland, 0, &mut 256)
        && in_zone["kind"] == "InZone"
        && in_zone["args"]["zone"] == "Library"
        && observed["kind"] == "Observed"
        && producer(prefix, &observed["args"]).is_some_and(|op| {
            let objects = &op["args"]["objects"];
            let zone = &objects["args"]["objects"];
            op["kind"] == "LookAt"
                && objects["kind"] == "Take"
                && zone["kind"] == "Zone"
                && own_zone(&zone["args"]["scope"]["Players"], "Library")
        })
}
