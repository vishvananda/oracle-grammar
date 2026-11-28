use serde_json::{Value, json};

// A damage recipient creates this token, then the source's controller
// goads that fixed creation for the rest of the game.
pub(super) fn recipient_goad(program: &mut Value, tracing: bool) -> bool {
    let Some([create, follow_up]) = program["instructions"].as_array().map(Vec::as_slice) else {
        return false;
    };
    let token_creation = &create["args"]["operation"];
    let args = &token_creation["args"];
    let operation = &follow_up["args"]["operation"];
    let operands = &operation["args"];
    let objects = &operands["objects"];
    if create["kind"] != "Effect"
        || follow_up["kind"] != "Effect"
        || token_creation["kind"] != "CreateTokens"
        || args["players"] != json!({"kind":"One","args":{"kind":"EventSubject"}})
        || !args["tokens"].as_array().is_some_and(|items| {
            matches!(items.as_slice(), [token]
                if token["amount"] == json!({"kind":"Constant","args":1})
                    && token["recipe"]["Defined"]["name"] == "FromSubtypes")
        })
        || operation["kind"] != "KeywordEach"
        || operands["actor"] != json!({"kind":"One","args":{"kind":"AbilityController"}})
        || operands["action"] != json!({"Goad":{"until":"Indefinite"}})
        || objects["kind"] != "Created"
        || !super::trace::same(&objects["args"], &create["args"]["result"], tracing)
        || super::walk::lossless::<crate::ir::Operation>(operation, tracing).is_none()
    {
        return false;
    }
    let effect = json!({"kind":"CreateContinuousEffect","args":{
        "source":{"kind":"Source"},"affected":{"Objects":objects},
        "changes":[{"kind":"Rule","args":{"GoadedBy":{"kind":"AbilityController"}}}],
        "duration":"Indefinite","evaluation":"LiveObjects"
    }});
    program["instructions"][1]["args"]["operation"] = effect;
    true
}
