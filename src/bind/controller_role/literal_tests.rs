use super::*;

fn fixtures() -> Vec<Value> {
    serde_json::from_str(include_str!(
        "../../../tests/fixtures/controller-player-pair.json"
    ))
    .unwrap()
}
fn path(pointer: &str) -> Vec<Step> {
    pointer
        .split('/')
        .skip(1)
        .map(|p| {
            p.parse()
                .map(Step::Index)
                .unwrap_or_else(|_| Step::Key(p.into()))
        })
        .collect()
}
fn raw() -> (Value, Scope, Vec<Step>) {
    let f = fixtures().remove(0);
    let mut root = f["expected"].as_array().unwrap().last().unwrap().clone();
    let at = "/args/body/instructions/1/args/operation/args/players/args";
    *root.pointer_mut(at).unwrap() = json!({"$sym":"that_player_literal","args":[],"at":20});
    let mut spec = root["args"]["announcement"]["targets"][0].clone();
    let filter = &mut spec["filter"]["args"][1];
    *filter = json!({"$sym":"indefinite_opponent_controller","args":[filter.clone()],"at":10});
    let mut scope = Scope::default();
    scope
        .players
        .push(json!({"kind":"ControllerOf","args":{"kind":"EventSubject"}}));
    scope.default_object_player = Some(1);
    introduce(
        &mut scope,
        &json!({"kind":"Target","args":{"slot":0,"index":0}}),
        &spec,
    )
    .unwrap();
    (root, scope, path(at))
}
fn resolve(root: &Value, scope: &Scope, path: &[Step]) -> Value {
    literal_reference(root, path, &[], scope, &references::Contexts::default()).unwrap()
}
#[test]
fn controller_player_literal_authentic_player_local_player_and_nested_scope_keep_priority() {
    let (root, scope, path) = raw();
    assert_eq!(
        resolve(&root, &scope, &path),
        json!({"kind":"ControllerOf","args":{"kind":"Target","args":{"slot":0,"index":0}}})
    );
    for player in [
        json!({"kind":"EventSubject"}),
        json!({"kind":"Bound","args":7}),
        json!({"kind":"Target","args":{"slot":2,"index":0}}),
    ] {
        let mut s = scope.clone();
        s.players.push(player.clone());
        assert_eq!(resolve(&root, &s, &path), player);
    }
    let nested = scope.nested();
    assert_eq!(resolve(&root, &nested, &path), scope.player().unwrap());
    let mut untagged = scope.clone();
    untagged.default_object_player = None;
    assert_eq!(resolve(&root, &untagged, &path), scope.player().unwrap());
}

#[test]
fn controller_player_literal_requires_exact_scalar_provenance_and_terminal_direct_action() {
    let (root, scope, path) = raw();
    for changed in 0..6 {
        let mut r = root.clone();
        let mut s = scope.clone();
        match changed {
            0 => s.controller_role.as_mut().unwrap().literal = false,
            1 => s.controller_role.as_mut().unwrap().object = ObjectRef::Source,
            2 => r["args"]["body"]["instructions"]
                .as_array_mut()
                .unwrap()
                .push(json!({"kind":"Effect"})),
            3 => {
                r["args"]["body"]["instructions"][0]["args"]["operation"]["kind"] =
                    json!("Sacrifice")
            }
            4 => {
                r["args"]["body"]["instructions"][1]["args"]["operation"]["args"]["amount"] =
                    json!({"kind":"EventAmount"})
            }
            _ => r["args"]["announcement"]["modes"] = json!({"selection":"Any"}),
        }
        assert_eq!(
            resolve(&r, &s, &path),
            scope.player().unwrap(),
            "case {changed}"
        );
    }
}

#[test]
fn controller_player_literal_seed_is_positive_root_own_etb_only() {
    let (root, scope, _) = raw();
    assert!(literal::seed(&root, &scope));
    for changed in 0..5 {
        let mut r = root.clone();
        let mut s = scope.clone();
        match changed {
            0 => s.holders = 1,
            1 => r["kind"] = json!("CreateTrigger"),
            2 => {
                r["args"]["event"]["ZoneChange"]["objects"] =
                    json!({"kind":"CardType","args":"Creature"})
            }
            3 => r["args"]["intervening_if"] = json!({"kind":"Constant","args":true}),
            _ => r["args"]["event"]["ZoneChange"]["to"] = json!("Graveyard"),
        }
        assert!(!literal::seed(&r, &s));
    }
}

#[test]
fn controller_player_literal_full_producer_bound_precedes_target_clone() {
    let (mut root, scope, path) = raw();
    let mut deep = json!(null);
    for _ in 0..130 {
        deep = json!({"nested":deep});
    }
    root["unused"] = deep;
    assert!(
        literal_reference(&root, &path, &[], &scope, &references::Contexts::default()).is_err()
    );
}
