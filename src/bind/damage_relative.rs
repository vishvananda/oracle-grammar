//! A bare recipient possessive can name a different explicitly written dealer.
use super::{Found, Scope, Step, filters, references};
use serde_json::Value;

pub(super) fn bind(
    root: &Value,
    scope: &Scope,
    item: &Found,
    introduced: &[references::TargetIntroduction],
    prior: &references::Contexts,
) -> Result<Value, String> {
    if item.filter == filters::Context::Outside
        && let Some(source) = explicit_source(root, &item.path)?
    {
        return Ok(source);
    }
    scope.damage_pronoun("it", root, item, introduced, prior)
}

pub(crate) fn ordinary_name(name: &str) -> &str {
    if name == "damage_relative_object" {
        "it"
    } else {
        name
    }
}

/// A trailing bare stat cannot belong to a player recipient. When that player
/// is the explicit dealer's owner/controller, the stat retains that dealer.
pub(super) fn possessive_amount(root: &Value, path: &[Step]) -> Result<Option<Value>, String> {
    if path.len() > 128 {
        return Err("damage possessive amount depth exceeds128".into());
    }
    let [
        operation @ ..,
        Step::Key(a),
        Step::Key(amount),
        Step::Key(b),
        Step::Index(1),
        Step::Key(c),
        Step::Index(0),
        Step::Key(d),
        Step::Key(object),
    ] = path
    else {
        return Ok(None);
    };
    if a != "args"
        || amount != "amount"
        || b != "args"
        || c != "args"
        || d != "args"
        || object != "object"
    {
        return Ok(None);
    }
    let mut value = root;
    for step in operation {
        value = match step {
            Step::Key(key) => value.get(key),
            Step::Index(index) => value.get(*index),
        }
        .ok_or("damage possessive amount path")?;
    }
    if value["kind"] != "DealDamage"
        || super::symbol_name(&value["args"]["amount"]) != Some("recipient_value")
        || super::symbol_name(&value["args"]["amount"]["args"][1]) != Some("its_stat")
    {
        return Ok(None);
    }
    let mut source = value["args"]["source"].clone();
    filters::strip(&mut source);
    let recipients = &value["args"]["recipients"];
    let player = &recipients["args"]["args"];
    Ok((matches!(
        source["kind"].as_str(),
        Some("AttachedTo" | "GrantingObject")
    ) && recipients["kind"] == "Players"
        && recipients["args"]["kind"] == "One"
        && matches!(player["kind"].as_str(), Some("OwnerOf" | "ControllerOf"))
        && player["args"] == source)
        .then_some(source))
}

fn explicit_source(root: &Value, path: &[Step]) -> Result<Option<Value>, String> {
    if path.len() > 128 {
        return Err("damage recipient context depth exceeds128".into());
    }
    let mut value = root;
    let mut dealer = None;
    for (index, step) in path.iter().enumerate() {
        if matches!(
            value["kind"].as_str(),
            Some("DealDamage" | "DealDamageParts")
        ) {
            dealer = None;
            if matches!(path.get(index), Some(Step::Key(k)) if k == "args")
                && matches!(path.get(index + 1), Some(Step::Key(k)) if k == "recipients" || k == "parts")
            {
                let mut source = value["args"]["source"].clone();
                filters::strip(&mut source);
                // The card's own damage retains the preceding target/event.
                // Its host and a granted ability's creator are distinct roles.
                if matches!(
                    source["kind"].as_str(),
                    Some("AttachedTo" | "GrantingObject")
                ) {
                    let typed: crate::model::ir::ObjectRef = serde_json::from_value(source)
                        .map_err(|why| format!("damage recipient source must be bound: {why}"))?;
                    dealer = Some(serde_json::to_value(typed).expect("typed damage source"));
                }
            }
        }
        value = match step {
            Step::Key(key) => value.get(key),
            Step::Index(index) => value.get(*index),
        }
        .ok_or("damage recipient context path")?;
    }
    Ok(dealer)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn path(field: &str) -> Vec<Step> {
        vec![Step::Key("args".into()), Step::Key(field.into())]
    }

    #[test]
    fn holders_keep_the_existing_object_and_explicit_hosts_and_creators_are_typed() {
        for (source, expected) in [
            (json!({"kind":"Source"}), None),
            (
                json!({"kind":"AttachedTo","args":{"kind":"Source"}}),
                Some(json!({"kind":"AttachedTo","args":{"kind":"Source"}})),
            ),
            (
                json!({"kind":"GrantingObject"}),
                Some(json!({"kind":"GrantingObject"})),
            ),
        ] {
            let root = json!({"kind":"DealDamage","args":{"source":source,"recipients":{}}});
            assert_eq!(
                explicit_source(&root, &path("recipients")).unwrap(),
                expected
            );
        }
    }

    #[test]
    fn unrelated_fields_and_nested_ordinary_damage_do_not_inherit_the_outer_source() {
        let root = json!({"kind":"DealDamage","args":{"source":{"kind":"GrantingObject"},
            "amount":{},"recipients":{"kind":"DealDamage","args":{
                "source":{"kind":"Source"},"recipients":{}}}}});
        assert_eq!(explicit_source(&root, &path("amount")).unwrap(), None);
        let mut nested = path("recipients");
        nested.extend(path("recipients"));
        assert_eq!(explicit_source(&root, &nested).unwrap(), None);
    }

    #[test]
    fn trailing_literal_stats_follow_the_dealer_when_the_recipient_is_its_controller() {
        let source = json!({"kind":"AttachedTo","args":{"kind":"Source"}});
        let mut root = json!({"kind":"DealDamage","args":{"source":source,"recipients":{
            "kind":"Players","args":{"kind":"One","args":{"kind":"ControllerOf","args":source}}},
            "amount":{"$sym":"recipient_value","args":[{}, {"$sym":"its_stat","args":[{
                "kind":"ObjectProperty","args":{"object":{"$sym":"it","args":[]},
                "property":"Toughness","information":"CurrentOrLastKnown"}}]}]}}});
        let path = vec![
            Step::Key("args".into()),
            Step::Key("amount".into()),
            Step::Key("args".into()),
            Step::Index(1),
            Step::Key("args".into()),
            Step::Index(0),
            Step::Key("args".into()),
            Step::Key("object".into()),
        ];
        assert_eq!(possessive_amount(&root, &path).unwrap(), Some(source));
        root["args"]["recipients"]["args"]["args"]["args"] = json!({"kind":"Source"});
        assert_eq!(possessive_amount(&root, &path).unwrap(), None);
    }
}
