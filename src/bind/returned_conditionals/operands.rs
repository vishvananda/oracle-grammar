use serde_json::{Value, json};
pub(super) fn scalar(result: u32) -> Value {
    json!({"kind":"Result","args":{"result":result,"phase":"After","index":0}})
}
pub(super) fn group(result: u32) -> Value {
    json!({"kind":"Transition","args":{"result":result,"phase":"After"}})
}
pub(super) fn retarget(operation: &Value, from: &Value, to: &Value) -> Option<Value> {
    let mut operation = operation.clone();
    let kind = operation["kind"].as_str()?.to_owned();
    let args = &mut operation["args"];
    match kind.as_str() {
        "PutCounters" => {
            let p = args["placements"].as_array_mut()?;
            if p.is_empty() || p.len() > 4096 {
                return None;
            }
            for p in p {
                if p["recipients"] != json!({"kind":"Objects","args":from})
                    || p["amount"]["kind"] != "Constant"
                    || !p["selection"].is_null()
                {
                    return None;
                }
                p["recipients"]["args"] = to.clone();
            }
        }
        "SetTapped" if args["objects"] == *from => args["objects"] = to.clone(),
        _ => return None,
    }
    Some(operation)
}
pub(super) fn actions(body: &Value, from: &Value, to: &Value) -> Option<Vec<Value>> {
    let items = body["instructions"].as_array()?;
    if items.len() > 4096 {
        return None;
    }
    items
        .iter()
        .map(|i| {
            if i["kind"] != "Effect" {
                return None;
            }
            let mut i = i.clone();
            i["args"]["operation"] = retarget(&i["args"]["operation"], from, to)?;
            Some(i)
        })
        .collect()
}
