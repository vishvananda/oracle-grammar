//! A top card of one player's library or graveyard, used as a set, is the ordered slice
//! it names. "Exile the top card of your graveyard" moves `Take(Zone, 1, Top)`, which
//! "the top card" also spells as a one-card slice; the scalar `Query(Zone, 0)` stays
//! where a condition or an amount reads that card in place.
use serde_json::{Value, json};

use crate::traverse::{Visit, rewrite};

/// The slice a positional `One(Query(Zone, 0))` set names, if it is one.
fn slice(set: &Value) -> Option<Value> {
    let query = &set["args"];
    let zone = &query["args"]["objects"];
    let scope = &zone["args"]["scope"]["Players"];
    let ordered = matches!(scope["zone"].as_str(), Some("Library" | "Graveyard"));
    (set["kind"] == "One"
        && query["kind"] == "Query"
        && query["args"]["index"] == 0
        && zone["kind"] == "Zone"
        && zone["args"]["kind"] == "Card"
        && scope["players"]["kind"] == "One"
        && ordered)
        .then(|| {
            json!({"kind":"Take","args":{"objects":zone,
                "count":{"kind":"Constant","args":1},"from":"Top"}})
        })
}

/// Spell each positional top-card set as its slice.
pub(super) fn lower(abilities: &mut Value) {
    rewrite(abilities, &mut |node| match slice(node) {
        Some(take) => {
            *node = take;
            Visit::Skip
        }
        None => Visit::Children,
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    fn zone(zone: &str) -> Value {
        json!({"kind":"Zone","args":{"scope":{"Players":{"zone":zone,
            "players":{"kind":"One","args":{"kind":"AbilityController"}}}},
            "kind":"Card","filter":{"kind":"Any"}}})
    }

    #[test]
    fn a_top_card_set_is_its_slice_and_a_scalar_read_stays() {
        let read = json!({"kind":"Query","args":{"objects":zone("Graveyard"),"index":0}});
        let mut value = json!({"moved":{"kind":"One","args":read},"tested":read});
        lower(&mut value);
        assert_eq!(
            value,
            json!({"moved":{"kind":"Take","args":{"objects":zone("Graveyard"),
                "count":{"kind":"Constant","args":1},"from":"Top"}},"tested":read})
        );
    }

    #[test]
    fn an_unordered_zone_keeps_its_one_card() {
        let exile = json!({"kind":"Zone","args":{"scope":{"Shared":"Exile"},
            "kind":"Card","filter":{"kind":"Any"}}});
        let mut value = json!({"kind":"One","args":{"kind":"Query",
            "args":{"objects":exile,"index":0}}});
        let before = value.clone();
        lower(&mut value);
        assert_eq!(value, before);
        let mut hand = json!({"kind":"One","args":{"kind":"Query",
            "args":{"objects":zone("Hand"),"index":0}}});
        let before = hand.clone();
        lower(&mut hand);
        assert_eq!(hand, before);
    }
}
