//! Qualify written cascade explanations by the spell population granted cascade.
use super::evidence::effects;
use super::mechanic_notes::target_slot;
use super::object_notes::is_controller;
use serde_json::Value;

pub(super) fn spell_effect(value: &Value, mechanic: &str) -> bool {
    value["kind"] == "Term"
        && value["args"]["family"] == "SpellEffect"
        && value["args"]["form"] == mechanic
}

fn grants_cascade(ability: &Value) -> bool {
    ability["kind"] == "Mechanic" && ability["args"]["mechanic"] == "Cascade"
}

pub(super) fn mentions(ability: &Value, mut visit: impl FnMut(&'static str) -> bool) -> bool {
    if ability["kind"] != "Static" {
        return false;
    }
    let args = &ability["args"];
    let Some([change]) = args["changes"].as_array().map(Vec::as_slice) else {
        return false;
    };
    let affected = &args["affected"];
    match change["kind"].as_str() {
        Some("GrantAbility") if grants_cascade(&change["args"]) => {
            object_grant(&affected["Objects"], &mut visit)
        }
        Some("PlayerRule") if is_controller(&affected["Players"]) => {
            player_grant(&change["args"]["SpellsHaveAbilities"], &mut visit)
        }
        Some("GrantAbility") if affected["Objects"]["kind"] == "One" => {
            affected_player_next_spell(&change["args"]) && visit("CascadePlayerNextSpell")
        }
        _ => false,
    }
}

fn object_grant(objects: &Value, visit: &mut impl FnMut(&'static str) -> bool) -> bool {
    let args = &objects["args"];
    if objects["kind"] != "Zone" || args["scope"]["Shared"] != "Stack" || args["kind"] != "Spell" {
        return false;
    }
    let filter = &args["filter"];
    let operands = &filter["args"];
    match filter["kind"].as_str() {
        Some("NthCast")
            if first(&operands["ordinal"])
                && operands["spells"]["kind"] == "Any"
                && is_controller(&operands["by"])
                && operands["scope"] == "Turn" =>
        {
            visit("CascadeFirstSpell")
        }
        Some("AllOf") if commander_cast(operands) => visit("CascadeCommander"),
        _ => false,
    }
}

fn commander_cast(filter: &Value) -> bool {
    let Some([a, b]) = filter.as_array().map(Vec::as_slice) else {
        return false;
    };
    let commander = |f: &Value| f["kind"] == "State" && f["args"] == "Commander";
    let cast = |f: &Value| {
        f["kind"] == "Cast" && is_controller(&f["args"]["by"]) && f["args"]["from"].is_null()
    };
    commander(a) && cast(b) || cast(a) && commander(b)
}

fn player_grant(grant: &Value, visit: &mut impl FnMut(&'static str) -> bool) -> bool {
    if grant["abilities"]["kind"] != "List" {
        return false;
    }
    let from = &grant["from"];
    let filter = &grant["spells"];
    let ordinal = &grant["nth_each_turn"];
    match grant["abilities"]["args"].as_array().map(Vec::as_slice) {
        Some([only]) if spell_effect(only, "Cascade") => {
            (first(ordinal)
                && (from == "Exile" && filter["kind"] == "Any"
                    || from.is_null()
                        && filter["kind"] == "Cast"
                        && filter["args"]["from"]["Shared"] == "Exile"
                        && filter["args"]["by"].is_null())
                && visit("CascadeFirstExiledSpell"))
                || ordinal.is_null()
                    && from == "Hand"
                    && filter["kind"] == "CardType"
                    && filter["args"] == "Enchantment"
                    && visit("CascadeEnchantmentFromHand")
        }
        Some([a, b]) if spell_effect(a, "Cascade") && spell_effect(b, "Cascade") => {
            ordinal.is_null() && visit("CascadeTwiceOnGrant")
        }
        _ => false,
    }
}

fn affected_player_next_spell(ability: &Value) -> bool {
    let ability = super::evidence::unscoped(ability);
    if ability["kind"] != "Activated" {
        return false;
    }
    let mut body = effects(&ability["args"]["body"]);
    let (Some(mana), Some(change), None) = (body.next(), body.next(), body.next()) else {
        return false;
    };
    let args = &change["args"];
    let next = &args["subject"]["Next"];
    let players = &mana["args"]["players"];
    mana["kind"] == "AddMana"
        && players["kind"] == "One"
        // The inverse may place the announcement marker on either reference.
        && target_slot(&players["args"]).is_some_and(|slot| {
            target_slot(&next["player"]) == Some(slot)
        })
        && change["kind"] == "ChangeSpell"
        && next["spells"]["kind"] == "Any"
        && next["this_turn"] == true
        && next["from_hand"] == false
        && matches!(args["changes"].as_array().map(Vec::as_slice),
            Some([grant]) if grant["kind"] == "GrantAbility" && grants_cascade(&grant["args"]))
}

fn first(value: &Value) -> bool {
    value["kind"] == "Constant" && value["args"] == 1
}
