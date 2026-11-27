use super::object_notes::is_controller;
use serde_json::Value;

fn conjuncts<'a>(filter: &'a Value, terms: &mut Vec<&'a Value>, budget: &mut usize) -> bool {
    if *budget == 0 {
        return false;
    }
    *budget -= 1;
    if filter["kind"] == "AllOf" {
        return filter["args"].as_array().is_some_and(|items| {
            !items.is_empty() && items.iter().all(|term| conjuncts(term, terms, budget))
        });
    }
    terms.push(filter);
    true
}

fn noncreature(filter: &Value) -> bool {
    filter["kind"] == "Not"
        && filter["args"]["kind"] == "CardType"
        && filter["args"]["args"] == "Creature"
}

fn own_scope(filter: &Value) -> Option<&'static str> {
    let count = &filter["args"];
    if filter["kind"] == "NthCast" {
        return (is_controller(&count["by"])
            && count["ordinal"]["kind"] == "Constant"
            && count["ordinal"]["args"] == 1
            && count["scope"] == "Turn")
            .then_some("OwnFirstSpellConvoke");
    }
    let mut terms = Vec::new();
    if !conjuncts(filter, &mut terms, &mut 24) {
        return None;
    }
    let cast = terms.iter().position(|term| {
        term["kind"] == "Cast"
            && is_controller(&term["args"]["by"])
            && term["args"]["from"].is_null()
    })?;
    terms.remove(cast);
    match terms.as_slice() {
        [term] if term["kind"] == "CardType" && term["args"] == "Creature" => {
            Some("OwnCreatureConvoke")
        }
        [term] if term["kind"] == "CardType" && term["args"] == "Artifact" => {
            Some("OwnArtifactConvoke")
        }
        [term] if noncreature(term) => Some("OwnNoncreatureConvoke"),
        [a, b]
            if [a, b]
                .iter()
                .any(|term| term["kind"] == "Supertype" && term["args"] == "Legendary")
                && [a, b]
                    .iter()
                    .any(|term| term["kind"] == "CardType" && term["args"] == "Creature") =>
        {
            Some("OwnLegendaryCreatureConvoke")
        }
        _ => None,
    }
}

fn player_scope(players: &Value, change: &Value) -> Option<&'static str> {
    let rule = &change["args"]["SpellsHaveAbilities"];
    let [grant] = rule["abilities"]["args"].as_array()?.as_slice() else {
        return None;
    };
    if change["kind"] != "PlayerRule"
        || rule["abilities"]["kind"] != "List"
        || grant["kind"] != "Term"
        || grant["args"]["family"] != "SpellEffect"
        || !grant["args"]["operands"]
            .as_array()
            .is_some_and(Vec::is_empty)
        || !rule["from"].is_null()
        || !rule["nth_each_turn"].is_null()
    {
        return None;
    }
    let filter = &rule["spells"];
    if grant["args"]["form"] == "Delve" {
        return (is_controller(players) && filter["kind"] == "Any").then_some("OwnDelve");
    }
    if grant["args"]["form"] != "Convoke" {
        return None;
    }
    if players["kind"] == "All" && noncreature(filter) {
        return Some("AllNoncreatureConvoke");
    }
    if !is_controller(players) || filter["kind"] != "AllOf" {
        return None;
    }
    let [a, b] = filter["args"].as_array()?.as_slice() else {
        return None;
    };
    ([a, b].into_iter().any(noncreature)
        && [a, b].iter().any(|term| {
            term["kind"] == "Cast"
                && term["args"]["by"].is_null()
                && term["args"]["from"]["Shared"] == "Exile"
        }))
    .then_some("OwnExiledNoncreatureConvoke")
}

pub(super) fn needs_bound_scope(topic: &str) -> bool {
    matches!(
        topic,
        "OwnCreatureConvoke"
            | "OwnArtifactConvoke"
            | "OwnNoncreatureConvoke"
            | "OwnLegendaryCreatureConvoke"
            | "OwnFirstSpellConvoke"
            | "AllNoncreatureConvoke"
            | "OwnExiledNoncreatureConvoke"
            | "OwnDelve"
    )
}

pub(super) fn grant_note(ability: &Value) -> Option<&'static str> {
    let args = &ability["args"];
    if ability["kind"] != "Static" {
        return None;
    }
    let [change] = args["changes"].as_array()?.as_slice() else {
        return None;
    };
    let objects = &args["affected"]["Objects"];
    if objects["kind"] == "Zone"
        && objects["args"]["kind"] == "Spell"
        && objects["args"]["scope"]["Shared"] == "Stack"
        && change["kind"] == "GrantAbility"
        && change["args"]["kind"] == "Mechanic"
        && change["args"]["args"]["mechanic"] == "Convoke"
    {
        return own_scope(&objects["args"]["filter"]);
    }
    player_scope(&args["affected"]["Players"], change)
}
