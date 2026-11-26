use crate::engine::rank_readings_by;
use serde_json::json;

#[test]
fn supplied_context_orders_all_readings_and_preserves_ties() {
    let readings = [json!([1]), json!([2]), json!([3]), json!([4])];
    let preferred = [&readings[1], &readings[3]];
    let ranked = rank_readings_by(&readings, |reading| preferred.contains(&reading));
    assert_eq!(
        ranked,
        [&readings[1], &readings[3], &readings[0], &readings[2]]
    );
    assert_eq!(readings, [json!([1]), json!([2]), json!([3]), json!([4])]);
}
