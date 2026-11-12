//! A tagged audit parse keeps one derivation per reading, so derivations that differ only
//! in their seeded rules cannot multiply across a card's sentences.
use super::*;
use crate::parse::RuleSet;

#[test]
fn an_audit_parse_keeps_one_derivation_per_reading() {
    let (name, text) = (
        "Long River Lurker",
        "When this creature enters, target creature you control can't be blocked this turn. \
         Whenever that creature deals combat damage this turn, you may exile it. If you do, \
         return it to the battlefield under its owner's control.",
    );
    let audited = engine().audited(name, text, &RuleSet::default()).unwrap();
    let values: Vec<&Value> = audited.iter().map(|(value, _)| value).collect();
    for (index, value) in values.iter().enumerate() {
        assert!(!values[..index].contains(value), "repeated reading {value}");
    }
    let mut audited: Vec<Value> = values
        .iter()
        .map(|value| canonical_abilities(value).unwrap())
        .collect();
    let mut parsed = engine().parse(name, text).unwrap().readings;
    audited.sort_by_key(Value::to_string);
    parsed.sort_by_key(Value::to_string);
    assert_eq!(audited, parsed);
}
