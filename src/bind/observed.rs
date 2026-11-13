//! A local where clause cannot accidentally leave another X as a casting value.
use crate::traverse::{Boundary, Visit, boundary, children, rewrite};
use serde_json::Value;

pub(super) fn inline(root: &mut Value) -> Result<(), String> {
    let mut marked = false;
    let mut x = false;
    children(root, |child| {
        rewrite(child, &mut |node| {
            if matches!(
                boundary(node),
                Boundary::Ability | Boundary::Holder | Boundary::Mode
            ) {
                return Visit::Skip;
            }
            marked |= node["$sym"] == "look_inline";
            x |= crate::functions::is_x(node);
            Visit::Children
        });
    });
    if marked && x {
        return Err("a repeated look definition requires whole-program X binding".into());
    }
    if marked {
        children(root, |child| {
            rewrite(child, &mut |node| {
                if matches!(
                    boundary(node),
                    Boundary::Ability | Boundary::Holder | Boundary::Mode
                ) {
                    return Visit::Skip;
                }
                if node["$sym"] == "look_inline" {
                    *node = node["args"][0].take();
                }
                Visit::Children
            });
        });
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn look() -> Value {
        json!({"$sym":"look_inline","args":[{"kind":"LookAt","args":{"objects":{
            "kind":"Take","args":{"count":{"kind":"Constant","args":3}}
        }}}],"at":0})
    }

    #[test]
    fn inline_look_rejects_owned_and_inherited_x_but_keeps_independent_abilities() {
        let x = json!({"kind":"AnnouncedX"});
        for tail in [x.clone(), json!({"$sym":"scope","args":[x.clone()],"at":1})] {
            let mut root = json!({"instructions":[look(),tail]});
            assert!(inline(&mut root).is_err());
        }
        for child in [
            json!({"kind":"Activated","args":{"amount":x}}),
            json!({"$sym":"holder","args":[x],"at":1}),
        ] {
            let mut root = json!({"instructions":[look(),child.clone()]});
            inline(&mut root).unwrap();
            assert_eq!(root["instructions"][0]["kind"], "LookAt");
            assert_eq!(root["instructions"][1], child);
        }
    }
}
