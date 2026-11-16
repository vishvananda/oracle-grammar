//! Announced target groups remain separate from resolving choices.
use super::*;

#[test]
fn announced_group_grants_print_changed_target_kind_count_and_ability() {
    let mut frame = compositional(
        "Choose any number of target creatures. Each of those creatures gains persist until end of turn.",
    );
    let body = &frame[0]["args"]["body"]["instructions"];
    assert_eq!(body.as_array().unwrap().len(), 1);
    assert_eq!(
        body[0]["args"]["operation"]["args"]["affected"],
        json!({"Objects":{"kind":"Targets","args":0}})
    );
    frame[0]["args"]["announcement"]["targets"][0]["count"] =
        json!({"Exactly":{"kind":"Constant","args":2}});
    frame[0]["args"]["announcement"]["targets"][0]["filter"] =
        json!({"kind":"CardType","args":"Artifact"});
    frame[0]["args"]["body"]["instructions"][0]["args"]["operation"]["args"]["changes"] =
        json!([{"kind":"GrantKeyword","args":{"kind":"Flying"}}]);
    let text = printed_types(&frame, &["Instant"]).to_lowercase();
    assert!(
        text.contains("artifact") && text.contains("flying"),
        "{text}"
    );
    assert!(text.contains("two") || text.contains('2'), "{text}");
}

#[test]
fn group_counter_placements_print_changed_amount_kind_and_recipients() {
    let mut frame = compositional(
        "Choose any number of target creatures. Put a +1/+1 counter on each of them.",
    );
    frame[0]["args"]["announcement"]["targets"][0]["filter"] =
        json!({"kind":"CardType","args":"Artifact"});
    let placement = &mut frame[0]["args"]["body"]["instructions"][0]["args"]["operation"]["args"]["placements"]
        [0];
    assert_eq!(
        placement["recipients"],
        json!({"kind":"Objects","args":{"kind":"Targets","args":0}})
    );
    placement["amount"] = json!({"kind":"Constant","args":2});
    placement["kind"] = json!({"Named":"ChargeCounter"});
    let text = printed_types(&frame, &["Instant"]).to_lowercase();
    assert!(
        text.contains("artifact") && text.contains("charge counters"),
        "{text}"
    );
    assert!(text.contains("two") || text.contains('2'), "{text}");
}
