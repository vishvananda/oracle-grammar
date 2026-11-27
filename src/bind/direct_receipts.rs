//! Direct actions whose referents are supplied by a preceding instruction.
use super::symbol_name;
use serde_json::{Value, json};

// The unqualified perpetual followup names the returned incarnation of the
// optional singleton target. It follows the actual move to its owner's hand.
pub(super) fn returned_target_pump(ability: &mut Value) {
    if ability["kind"] != "Triggered" {
        return;
    }
    let args = &mut ability["args"];
    let Some([movement, perpetually]) = args["body"]["instructions"].as_array().map(Vec::as_slice)
    else {
        return;
    };
    let moved = &movement["args"]["operation"];
    let pumping = &perpetually["args"]["operation"];
    let target = &args["announcement"]["targets"][0];
    let objects = &moved["args"]["objects"];
    let offered = &pumping["args"]["objects"];
    let same_offset = |set: &Value| {
        set == &json!({"kind":"Targets","args":target["slot"]})
            || set
                == &json!({"kind":"One","args":{"kind":"Target","args":{"slot":target["slot"],"index":0}}})
    };
    if movement["kind"] == "Effect"
        && moved["kind"] == "Move"
        && moved["args"]["qualifiers"] == json!([])
        && moved["args"]["destination"].get("OwnersHand").is_some()
        && perpetually["kind"] == "Effect"
        && pumping["kind"] == "CreatePerpetualEffect"
        && pumping["args"]["changes"].as_array().is_some_and(
            |changes| matches!(changes.as_slice(), [change] if change["kind"] == "Pump"),
        )
        && same_offset(objects)
        && same_offset(offered)
        && target["kind"] == json!({"Object":{"kind":"Permanent","zones":["Battlefield"]}})
        && target["count"] == json!({"UpTo":{"kind":"Constant","args":1}})
    {
        let id = movement["args"]["result"].clone();
        args["body"]["instructions"][1]["args"]["operation"]["args"]["objects"] =
            json!({"kind":"Transition","args":{"result":id,"phase":"After"}});
    }
}

// An own tap-symbol cost proves the named source is a permanent. A zone
// filter over just that object and one permanent target is the same move.
pub(super) fn exile_own_tapped_source(ability: &mut Value) {
    if ability["kind"] != "Activated" || ability["args"]["cost"]["parts"] != json!(["TapSymbol"]) {
        return;
    }
    let args = &mut ability["args"];
    let Some([instruction]) = args["body"]["instructions"].as_array().map(Vec::as_slice) else {
        return;
    };
    let operation = &instruction["args"]["operation"];
    let move_args = &operation["args"];
    let objects = &move_args["objects"];
    let zone = &objects["args"];
    let target = &args["announcement"]["targets"][0];
    if instruction["kind"] != "Effect"
        || operation["kind"] != "Move"
        || move_args["qualifiers"] != json!([])
        || move_args["destination"]
            != json!({"Exile":{"visibility":"FaceUp","arrival":{"counters":[],"states":[]}}})
        || objects["kind"] != "Zone"
        || zone["scope"]["Shared"] != "Battlefield"
        || zone["kind"] != "Permanent"
        || zone["filter"]["kind"] != "AnyOf"
        || target["kind"] != json!({"Object":{"kind":"Permanent","zones":["Battlefield"]}})
        || target["count"] != json!({"Exactly":{"kind":"Constant","args":1}})
    {
        return;
    }
    let source = json!({"kind":"Source"});
    let announced = json!({"kind":"Target","args":{"slot":target["slot"],"index":0}});
    let Some(choices) = zone["filter"]["args"].as_array() else {
        return;
    };
    if choices.len() != 2
        || ![&source, &announced]
            .iter()
            .all(|object| choices.contains(&json!({"kind":"SameObject","args":object})))
    {
        return;
    }
    args["body"]["instructions"][0]["args"]["operation"]["args"]["objects"] = json!({"kind":"Union","args":[{"kind":"One","args":source},{"kind":"One","args":announced}]});
}

// "If it's ..." after the search, selected move and shuffle names the card
// that actually moved. Outside an explicit search continuation it cannot
// resolve to the unrelated source merely because a shuffle does not move it.
pub(super) fn unbound_searched_card_test(ability: &Value) -> bool {
    let args = &ability["args"];
    let Some([find, moved, shuffle, test]) =
        args["body"]["instructions"].as_array().map(Vec::as_slice)
    else {
        return false;
    };
    let search = &find["args"]["operation"];
    let card = &moved["args"]["operation"];
    let condition = &test["args"]["condition"];
    search["kind"] == "Search"
        && search["args"]["choices"].as_array().is_some_and(|choices|
            matches!(choices.as_slice(), [choice] if choice["count"] == json!({"AsManyAsCan":{"kind":"Constant","args":1}})))
        && card["kind"] == "Move"
        && card["args"]["objects"]["kind"] == "Selected"
        && card["args"]["destination"].get("OwnersGraveyard").is_some()
        && shuffle["args"]["operation"]["kind"] == "Shuffle"
        && test["kind"] == "If"
        && condition["kind"] == "ObjectMatches"
        && symbol_name(&condition["args"]["object"]) == Some("current_it")
        && condition["args"]["filter"]["kind"] == "AnyOf"
        && condition["args"]["filter"]["args"].as_array().is_some_and(|arms|
            arms.len() == 2
            && arms.contains(&json!({"kind":"CardType","args":"Instant"}))
            && arms.contains(&json!({"kind":"CardType","args":"Sorcery"})))
}

// A one-card hand selection is the complete input of the following duplicate.
// The bound receipt preserves its identity even after a later choice.
pub(super) fn conjure_selected_hand_card(ability: &mut Value) {
    let ability = if ability["kind"] == "Scoped" {
        &mut ability["args"]["ability"]
    } else {
        ability
    };
    if !matches!(
        ability["kind"].as_str(),
        Some("Activated" | "Spell" | "Triggered")
    ) {
        return;
    }
    let args = &mut ability["args"];
    let Some([choose, conjure]) = args["body"]["instructions"].as_array().map(Vec::as_slice) else {
        return;
    };
    let found = &choose["args"]["operation"];
    let picked = &found["args"];
    let duplicate = &conjure["args"]["operation"];
    let card = &duplicate["args"]["card"]["Duplicate"];
    if choose["kind"] == "Effect"
        && conjure["kind"] == "Effect"
        && found["kind"] == "ChooseObjects"
        && picked["objects"]["kind"] == "Zone"
        && picked["objects"]["args"]["kind"] == "Card"
        && picked["objects"]["args"]["scope"]["Players"]["zone"] == "Hand"
        && ["Exactly", "AsManyAsCan"]
            .iter()
            .any(|shape| picked["count"][*shape] == json!({"kind":"Constant","args":1}))
        && duplicate["kind"] == "Conjure"
        && duplicate["args"]["destination"].get("Hand").is_some()
        && card
            == &json!({"kind":"One","args":{"kind":"Earlier","args":{"what":"Selected","by":null}}})
    {
        let id = choose["args"]["result"].clone();
        args["body"]["instructions"][1]["args"]["operation"]["args"]["card"]["Duplicate"] = json!({"kind":"One","args":{"kind":"Result","args":{"result":id,"phase":"After","index":0}}});
    }
}

// The color choice is evaluated only within the threshold branch. A
// following grant from that color stays in the same branch.
pub(super) fn protection_from_conditional_choice(ability: &mut Value) {
    if ability["kind"] != "Spell" {
        return;
    }
    let Some(instructions) = ability["args"]["body"]["instructions"].as_array_mut() else {
        return;
    };
    let [pump, branch, grant] = instructions.as_slice() else {
        return;
    };
    let prefix = &pump["args"]["operation"];
    let body = &branch["args"]["then"]["instructions"];
    let [choose] = body.as_array().map(Vec::as_slice).unwrap_or(&[]) else {
        return;
    };
    let offer = &choose["args"]["operation"];
    let protection = &grant["args"]["operation"];
    let choice = json!({"kind":"GrantKeyword","args":{"kind":"Protection","args":{
        "Objects":{"kind":"Any","filter":{"kind":"Chosen","args":"Color"}}
    }}});
    if pump["kind"] != "Effect"
        || prefix["kind"] != "CreateContinuousEffect"
        || branch["kind"] != "If"
        || branch["args"]["otherwise"]["instructions"] != json!([])
        || choose["kind"] != "Effect"
        || offer["kind"] != "ChooseValue"
        || offer["args"]["kind"] != "Color"
        || offer["args"]["choosers"] != json!({"kind":"One","args":{"kind":"AbilityController"}})
        || grant["kind"] != "Effect"
        || protection["kind"] != "CreateContinuousEffect"
        || protection["args"]["affected"] != prefix["args"]["affected"]
        || protection["args"]["duration"] != "EndOfTurn"
        || protection["args"]["changes"] != json!([choice])
    {
        return;
    }
    let granted = instructions.pop().expect("three instructions");
    instructions[1]["args"]["then"]["instructions"]
        .as_array_mut()
        .expect("one color choice")
        .push(granted);
}

// Each selection belongs to an independently announced controller. The
// exchange acts on the two selected sets, including when either set is empty.
pub(super) fn exchange_selected_sets(ability: &mut Value) {
    if ability["kind"] != "Spell" {
        return;
    }
    let args = &mut ability["args"];
    let Some([first, second, swap]) = args["body"]["instructions"].as_array().map(Vec::as_slice)
    else {
        return;
    };
    let first_choice = &first["args"]["operation"];
    let second_choice = &second["args"]["operation"];
    let exchange = &swap["args"]["operation"];
    let exchange_args = &exchange["args"];
    let first_set = json!({"kind":"Selected","args":first["args"]["result"]});
    let second_set = json!({"kind":"Selected","args":second["args"]["result"]});
    let player = &exchange_args["second"]["args"];
    let counted = json!({"kind":"InZone","args":{"objects":first_set,"zone":"Battlefield"}});
    if first["kind"] != "Effect"
        || second["kind"] != "Effect"
        || swap["kind"] != "Effect"
        || first_choice["kind"] != "ChooseObjects"
        || second_choice["kind"] != "ChooseObjects"
        || first_choice["args"]["player"] != second_choice["args"]["player"]
        || first_choice["args"]["count"]["UpTo"]["kind"] != "AmongPlayers"
        || second_choice["args"]["count"]["AsManyAsCan"]
            != json!({"kind":"CountObjects","args":counted})
        || exchange["kind"] != "Exchange"
        || exchange_args["kind"] != "Control"
        || !exchange_args["until"].is_null()
        || exchange_args["first"] != json!({"kind":"Objects","args":second_set})
        || exchange_args["second"]["kind"] != "Player"
        || player["kind"] != "Target"
        || !second_choice["args"]["objects"]["args"]["filter"]["args"]
            .as_array()
            .is_some_and(|parts| {
                parts.contains(&json!({"kind":"ControlledBy","args":{"kind":"One","args":player}}))
            })
    {
        return;
    }
    let exchange = &mut args["body"]["instructions"][2]["args"]["operation"]["args"];
    exchange["first"] = json!({"kind":"Objects","args":first_set});
    exchange["second"] = json!({"kind":"Objects","args":second_set});
}

// A PerPlayer target slot names up to one creature per opponent. Its
// continuation cannot read an ordinary first target and the source controller.
pub(super) fn loses_distributed_recipient(ability: &Value) -> bool {
    if ability["kind"] != "Triggered" {
        return false;
    }
    let args = &ability["args"];
    let Some([exile, life]) = args["body"]["instructions"].as_array().map(Vec::as_slice) else {
        return false;
    };
    let Some([target]) = args["announcement"]["targets"]
        .as_array()
        .map(Vec::as_slice)
    else {
        return false;
    };
    let participants = &target["count"]["PerPlayer"];
    let movement = &exile["args"]["operation"];
    let gain = &life["args"]["operation"];
    let amount = &gain["args"]["amount"];
    exile["kind"] == "Effect"
        && movement["kind"] == "Move"
        && movement["args"]["destination"]["Exile"]["visibility"] == "FaceUp"
        && movement["args"]["objects"] == json!({"kind":"Targets","args":target["slot"]})
        && participants["players"]
            == json!({"kind":"OpponentsOf","args":{"kind":"AbilityController"}})
        && participants["each"] == json!({"UpTo":{"kind":"Constant","args":1}})
        && life["kind"] == "Effect"
        && gain["kind"] == "GainLife"
        && gain["args"]["players"]["kind"] == "One"
        && gain["args"]["players"]["args"]["kind"] == "ControllerOf"
        && matches!(
            gain["args"]["players"]["args"]["args"]["kind"].as_str(),
            Some("Source" | "EventSubject")
        )
        && amount["kind"] == "ObjectProperty"
        && amount["args"]["property"] == "Power"
        && amount["args"]["object"]
            == json!({"kind":"Target","args":{"slot":target["slot"],"index":0}})
}

// If the condition fails, the tracked source is absent and cannot receive
// the reciprocal damage. Keep both sequential actions in the one branch.
pub(super) fn reciprocal_damage_branch(ability: &mut Value) {
    if ability["kind"] != "Activated" {
        return;
    }
    let Some(instructions) = ability["args"]["body"]["instructions"].as_array_mut() else {
        return;
    };
    let [checked, second] = instructions.as_slice() else {
        return;
    };
    let args = &checked["args"];
    let source = json!({"kind":"Source"});
    let Some([first]) = args["then"]["instructions"].as_array().map(Vec::as_slice) else {
        return;
    };
    let left = &first["args"]["operation"];
    let right = &second["args"]["operation"];
    let target = &right["args"]["source"];
    let owns_power = |operation: &Value, actor: &Value| {
        operation["args"]["amount"]
            == json!({"kind":"ObjectProperty","args":{
                "object":actor,"property":"Power","information":"CurrentOrLastKnown"
            }})
    };
    let receives = |operation: &Value, object: &Value| {
        operation["args"]["recipients"]
            == json!({"kind":"Objects","args":{"kind":"One","args":object}})
    };
    if checked["kind"] != "If"
        || !args["otherwise"]["instructions"]
            .as_array()
            .is_some_and(Vec::is_empty)
        || args["condition"]
            != json!({"kind":"ObjectMatches","args":{"object":source,
            "filter":{"kind":"InZone","args":{"Shared":"Battlefield"}}}})
        || first["kind"] != "Effect"
        || second["kind"] != "Effect"
        || left["kind"] != "DealDamage"
        || right["kind"] != "DealDamage"
        || left["args"]["source"] != source
        || target["kind"] != "Target"
        || !owns_power(left, &source)
        || !owns_power(right, target)
        || !receives(left, target)
        || !receives(right, &source)
    {
        return;
    }
    let later = instructions.pop().expect("two instructions");
    instructions[0]["args"]["then"]["instructions"]
        .as_array_mut()
        .expect("one damage instruction")
        .push(later);
}

// A delayed optional target already named by the payment must use the
// resulting hand incarnation for the conditional perpetual effect.
pub(super) fn returned_payment_pump(ability: &mut Value) {
    if ability["kind"] != "Triggered" {
        return;
    }
    let args = &mut ability["args"];
    let target = &args["announcement"]["targets"][0];
    if target["count"] != json!({"UpTo":{"kind":"Constant","args":1}})
        || target["kind"] != json!({"Object":{"kind":"Permanent","zones":["Battlefield"]}})
    {
        return;
    }
    let original = json!({"kind":"Targets","args":target["slot"]});
    let Some([_, delayed]) = args["body"]["instructions"]
        .as_array_mut()
        .map(Vec::as_mut_slice)
    else {
        return;
    };
    if delayed["kind"] != "Effect" {
        return;
    }
    let operation = &mut delayed["args"]["operation"];
    if operation["kind"] != "CreateDelayedTrigger"
        || operation["args"]["event"]
            != json!({"Turn":{"point":"EndStepBegins","players":{"kind":"All"}}})
    {
        return;
    }
    let Some([pay, guarded]) = operation["args"]["body"]["instructions"]
        .as_array_mut()
        .map(Vec::as_mut_slice)
    else {
        return;
    };
    let payment = &pay["args"]["operation"];
    let result = &pay["args"]["result"];
    let Some([grant]) = guarded["args"]["then"]["instructions"]
        .as_array()
        .map(Vec::as_slice)
    else {
        return;
    };
    let buff = &grant["args"]["operation"];
    if pay["kind"] != "Effect"
        || payment["kind"] != "PayCost"
        || payment["args"]["optional"] != false
        || payment["args"]["player"]["kind"] != "AbilityController"
        || payment["args"]["cost"]["parts"]
            != json!([{"Move":{
                "selections":[{"objects":original,"choose":{"Exactly":{"kind":"Constant","args":1}}}],
                "destination":{"OwnersHand":{"counters":[],"states":[]}}
            }}])
        || guarded["kind"] != "If"
        || guarded["args"]["otherwise"]["instructions"] != json!([])
        || guarded["args"]["condition"] != json!({"kind":"PaymentChosen","args":result})
        || grant["kind"] != "Effect"
        || buff["kind"] != "CreatePerpetualEffect"
        || buff["args"]["objects"] != original
        || !buff["args"]["changes"]
            .as_array()
            .is_some_and(|items| matches!(items.as_slice(),[c] if c["kind"]=="Pump"))
    {
        return;
    }
    guarded["args"]["then"]["instructions"][0]["args"]["operation"]["args"]["objects"] = json!({"kind":"CostObjects","args":{"payment":{"Resolution":result},
            "component":[0],"view":"Performed","phase":"After"}});
}

// A counted token continuation follows the exact return from the named
// graveyard to the battlefield, including when a replacement redirects a move.
pub(super) fn counted_graveyard_return(ability: &mut Value) {
    if ability["kind"] != "Spell" {
        return;
    }
    let args = &mut ability["args"];
    let target = &args["announcement"]["targets"][0];
    if target["kind"] != json!({"Object":{"kind":"Card","zones":["Graveyard"]}})
        || target["count"]["UpTo"]["kind"] != "Constant"
    {
        return;
    }
    let slot = &target["slot"];
    let Some([put, tokens]) = args["body"]["instructions"].as_array().map(Vec::as_slice) else {
        return;
    };
    let moved = &put["args"]["operation"];
    let created = &tokens["args"]["operation"];
    let result = &put["args"]["result"];
    let Some([recipe]) = created["args"]["tokens"].as_array().map(Vec::as_slice) else {
        return;
    };
    if put["kind"] != "Effect"
        || moved["kind"] != "Move"
        || moved["args"]["destination"]["Battlefield"]["arrival"]["tapped"] != true
        || moved["args"]["objects"] != json!({"kind":"Targets","args":slot})
        || moved["args"]["qualifiers"] != json!([])
        || tokens["kind"] != "Effect"
        || created["kind"] != "CreateTokens"
        || recipe["amount"]
            != json!({"kind":"CountObjects","args":{
                "kind":"Transition","args":{"result":result,"phase":"Before"}
            }})
    {
        return;
    }
    let amount = json!({"kind":"CountObjects","args":{"kind":"TransitionWhere",
        "args":{"result":result,"phase":"Before","from":"Graveyard","to":"Battlefield"}}});
    args["body"]["instructions"][1]["args"]["operation"]["args"]["tokens"][0]["amount"] = amount;
}

// A mill of one card gives the subsequent named-card test at most one
// candidate. Move the life amount onto that test's sole matched receipt.
pub(super) fn named_mill_amount(ability: &mut Value) {
    if ability["kind"] != "Spell" {
        return;
    }
    let args = &mut ability["args"];
    let Some([_, mill, test, ..]) = args["body"]["instructions"]
        .as_array_mut()
        .map(Vec::as_mut_slice)
    else {
        return;
    };
    let movement = &mill["args"]["operation"];
    let transition = json!({"kind":"Transition","args":{
        "result":mill["args"]["result"],"phase":"After"
    }});
    let controller = json!({"kind":"One","args":{"kind":"AbilityController"}});
    let amount = json!({"kind":"CountObjects","args":{"kind":"Matching","args":{
        "objects":transition,"filter":{"kind":"Chosen","args":"CardName"}
    }}});
    if mill["kind"] != "Effect"
        || movement["kind"] != "Mill"
        || movement["args"]["players"]["args"]["kind"] != "Target"
        || movement["args"]["amount"] != json!({"kind":"Constant","args":1})
        || test["kind"] != "If"
        || test["args"]["otherwise"]["instructions"] != json!([])
        || test["args"]["condition"]
            != json!({"kind":"Compare","args":{"left":amount,
                "test":{"kind":"Compare","args":{"comparison":"Greater","value":{"kind":"Constant","args":0}}}
            }})
    {
        return;
    }
    let Some([gain]) = test["args"]["then"]["instructions"]
        .as_array_mut()
        .map(Vec::as_mut_slice)
    else {
        return;
    };
    let effect = &gain["args"]["operation"];
    if gain["kind"] != "Effect"
        || effect["kind"] != "GainLife"
        || effect["args"]["players"] != controller
        || effect["args"]["amount"]
            != json!({"kind":"ObjectProperty","args":{
                "object":{"kind":"Source"},"property":"ManaValue","information":"CurrentOrLastKnown"
            }})
    {
        return;
    }
    gain["args"]["operation"]["args"]["amount"] = json!({"kind":"Aggregate","args":{
        "objects":transition,"property":"ManaValue","aggregation":"Sum"
    }});
}
