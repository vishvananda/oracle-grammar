//! The triggering action is distinct from the callback's result allocations.
use super::{Scope, Step, at_path, bind_scope};
use crate::model::ir::ResultId;
use serde_json::{Value, json};

#[derive(Clone)]
pub(super) struct Receipt {
    result: ResultId,
    lexical_entry: u64,
    draw: bool,
    moved: Option<super::reflexive_move::Moved>,
}

/// Bind the header before the callback allocates any body results. Transparent
/// Program/mode wrappers still share their cursor without creating a receipt.
fn initialize(inner: &mut Value, scope: &mut Scope, parent: Option<&Value>) -> Result<(), String> {
    if inner["kind"] != "ReflexiveTrigger" {
        return Ok(());
    }
    let inherited = scope.reflexive_receipt.take();
    let mut performed = inner
        .pointer_mut("/args/performed")
        .ok_or("a reflexive trigger needs its performed action")?
        .take();
    bind_scope(&mut performed, scope)?;
    let result = serde_json::from_value(performed.clone())
        .map_err(|_| "a reflexive performed action needs a native ResultId")?;
    let draw = parent.is_some_and(|root| super::addresses::draw_result(root, &performed, true))
        || inherited.is_some_and(|receipt| receipt.draw && json!(receipt.result) == performed);
    scope.reflexive_receipt = Some(Receipt {
        result,
        lexical_entry: scope.results.lexical_issued(),
        draw,
        moved: None,
    });
    inner["args"]["performed"] = performed;
    Ok(())
}

pub(super) fn bind(
    node: &mut Value,
    scope: &mut Scope,
    parent: Option<&Value>,
) -> Result<Value, String> {
    bind_with_move(node, scope, parent, None)
}

fn bind_with_move(
    node: &mut Value,
    scope: &mut Scope,
    parent: Option<&Value>,
    moved: Option<super::reflexive_move::Moved>,
) -> Result<Value, String> {
    let mut inner = node["args"]
        .get_mut(0)
        .ok_or("a reflexive trigger needs its contents")?
        .take();
    let mut nested = scope.nested();
    nested.target_base = scope.target_base + scope.targets.len();
    nested.results = scope.results.clone();
    nested.reflexive_receipt = scope.reflexive_receipt.clone();
    initialize(&mut inner, &mut nested, parent)?;
    if let Some(moved) = moved
        && let Some(receipt) = nested.reflexive_receipt.as_mut()
    {
        receipt.moved = Some(moved);
    }
    bind_scope(&mut inner, &mut nested)?;
    scope.results = nested.results;
    Ok(inner)
}

/// This immediate quantity cannot jump across an independently written action.
/// Generated value definitions do not consume lexical result positions.
fn reference(scope: &Scope) -> Result<Value, String> {
    let receipt = scope
        .reflexive_receipt
        .as_ref()
        .ok_or("no immediate reflexive performed action")?;
    if receipt.draw {
        return Err("that many after drawing requires a captured requested amount".into());
    }
    if scope.results.lexical_issued() > receipt.lexical_entry.saturating_add(1) {
        return Err("a reflexive quantity follows an independent callback action".into());
    }
    Ok(json!(receipt.result))
}

pub(super) fn marker(
    root: &mut Value,
    scope: &mut Scope,
    path: &[Step],
    name: &str,
    introduced: &[super::references::TargetIntroduction],
) -> Result<bool, String> {
    if name == "reflexive" {
        super::reflexive_x::check(root, path, scope, introduced)?;
        let mut node = at_path(root, path).ok_or("reflexive scope path")?.take();
        let moved = if super::reflexive_move::requested(&node) {
            let mut performed = node["args"][0]["args"]["performed"].clone();
            bind_scope(&mut performed, &mut scope.clone())?;
            let performed = serde_json::from_value(performed)
                .map_err(|_| "a reflexive performed action needs a native ResultId")?;
            super::reflexive_move::prove(root, path, performed, scope, introduced)?
        } else {
            None
        };
        let value = bind_with_move(&mut node, scope, Some(root), moved)?;
        *at_path(root, path).ok_or("reflexive scope path")? = value;
        return Ok(true);
    }
    if name == "reflexive_moved_fight_object" {
        let node = at_path(root, path).ok_or("reflexive moved fight object path")?;
        let inner = node["args"]
            .get(0)
            .ok_or("reflexive moved fight object needs one object")?;
        let bound = super::reflexive_move::actor(
            inner,
            scope
                .reflexive_receipt
                .as_ref()
                .filter(|receipt| {
                    scope.results.lexical_issued() <= receipt.lexical_entry.saturating_add(1)
                })
                .and_then(|receipt| receipt.moved.as_ref()),
            introduced,
            scope.target_base,
        )?;
        // finish_scope still owns target introduction paths under this wrapper.
        // The ordinary final annotation strip removes it after those writes.
        node["args"][0] = bound;
        return Ok(true);
    }
    if name != "reflexive_performed" {
        return Ok(false);
    }
    *at_path(root, path).ok_or("reflexive receipt path")? = reference(scope)?;
    Ok(true)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::bind::events::bind_reflexive;

    #[test]
    fn marked_pending_actor_keeps_its_original_introduction_path_without_receipt() {
        let mut root = json!({"$sym":"reflexive_moved_fight_object","at":7,"args":[{
            "kind":"Target","args":{"slot":{"$sym":"target","at":7,
                "args":[{"slot":{"$sym":"slot","at":7},"kind":"Object"}]},
                "index":0}}]});
        let original = root.clone();
        let target_path = [
            Step::Key("args".into()),
            Step::Index(0),
            Step::Key("args".into()),
            Step::Key("slot".into()),
        ];
        assert!(
            marker(
                &mut root,
                &mut Scope::default(),
                &[],
                "reflexive_moved_fight_object",
                &[]
            )
            .unwrap()
        );
        assert_eq!(root, original);
        let pending = at_path(&mut root, &target_path).unwrap();
        assert_eq!(pending["$sym"], "target");
        *pending = json!(11);
        assert_eq!(root["args"][0]["args"]["slot"], json!(11));
    }

    fn symbol(name: &str, at: u64) -> Value {
        json!({"$sym":name,"args":[],"at":at})
    }

    fn effect(operation: Value, at: u64) -> Value {
        json!({"kind":"Effect","args":{"result":symbol("next",at),"operation":operation}})
    }

    fn tap(at: u64) -> Value {
        let target = json!({"$sym":"target","at":at,"args":[{
            "slot":symbol("slot",at),"kind":{"Object":{"kind":"Permanent","zones":["Battlefield"]}},
            "count":{"UpTo":{"kind":"CountObjects","args":{"kind":"Transition",
                "args":{"result":symbol("reflexive_performed",at),"phase":"Before"}}}},
            "filter":{"kind":"CardType","args":"Creature"},"players":null,
            "distribute":null,"constraints":[],"chooser":null}]});
        effect(
            json!({"kind":"SetTapped","args":{"objects":{"kind":"Targets","args":target},"tapped":true}}),
            at,
        )
    }

    fn callback(actions: Vec<Value>, at: u64) -> Value {
        json!({"$sym":"reflexive","at":at,"args":[{"kind":"ReflexiveTrigger","args":{
            "performed":symbol("prior",at),"announcement":symbol("announcement",at),
            "body":{"instructions":actions}}}]})
    }

    fn parent(prefix: usize) -> Scope {
        let mut scope = Scope::default();
        // Earlier independent instructions, the actual action, then its callback declaration.
        for _ in 0..prefix + 2 {
            scope.results.allocate().unwrap();
        }
        scope
    }

    fn counted_result(trigger: &Value) -> &Value {
        &trigger["args"]["announcement"]["targets"][0]["count"]["UpTo"]["args"]["args"]["result"]
    }

    #[test]
    fn count_uses_actual_action_not_callback_or_body_result_after_prefixes() {
        for prefix in [0, 1, 6] {
            let mut scope = parent(prefix);
            scope.target_base = 2;
            scope.targets.push(json!({"slot":2,"kind":"Player"}));
            let mut node = callback(vec![tap(20)], 10);
            let bound = bind_reflexive(&mut node, &mut scope).unwrap();
            assert_eq!(bound["args"]["performed"], json!(prefix));
            assert_eq!(*counted_result(&bound), json!(prefix));
            assert_eq!(
                bound["args"]["body"]["instructions"][0]["args"]["result"],
                json!(prefix + 2)
            );
            assert_eq!(
                bound["args"]["announcement"]["targets"][0]["slot"],
                json!(3)
            );
        }
    }

    #[test]
    fn an_independent_callback_action_cannot_reuse_the_parent_quantity() {
        let draw = effect(
            json!({"kind":"Draw","args":{"players":{"kind":"One",
            "args":{"kind":"AbilityController"}},"amount":{"kind":"Constant","args":2}}}),
            20,
        );
        let error =
            bind_reflexive(&mut callback(vec![draw, tap(30)], 10), &mut parent(0)).unwrap_err();
        assert!(error.contains("independent callback action"), "{error}");
        assert!(reference(&Scope::default()).is_err());
    }

    #[test]
    fn generated_definitions_do_not_count_as_written_actions() {
        let mut scope = parent(0);
        scope.reflexive_receipt = Some(Receipt {
            result: ResultId(0),
            lexical_entry: 2,
            draw: false,
            moved: None,
        });
        scope.results.allocate_generated().unwrap();
        scope.results.allocate_generated().unwrap();
        scope.results.allocate().unwrap();
        assert_eq!(reference(&scope).unwrap(), json!(0));
        scope.results.allocate().unwrap();
        assert!(reference(&scope).is_err());
    }

    #[test]
    fn cursor_sharing_wrappers_inherit_but_independent_and_holder_scopes_reset() {
        let mut scope = parent(0);
        scope.reflexive_receipt = Some(Receipt {
            result: ResultId(0),
            lexical_entry: 2,
            draw: false,
            moved: None,
        });
        assert!(reference(&scope.nested()).is_err());
        let independent = crate::bind::scope_for(&json!({"kind":"Spell","args":{}}), "Test", 1);
        assert!(reference(&independent).is_err());
        let mut group = json!({"$sym":"reflexive","args":[{"instructions":[tap(20)]}],"at":10});
        let bound = bind_reflexive(&mut group, &mut scope).unwrap();
        assert_eq!(bound["instructions"][0]["args"]["result"], json!(2));
    }

    #[test]
    fn a_nested_callback_uses_its_own_performed_action() {
        let sacrifice = effect(
            json!({"kind":"Sacrifice","args":{"players":{"kind":"One",
            "args":{"kind":"AbilityController"}},"objects":{"kind":"One","args":{"kind":"Source"}}}}),
            20,
        );
        let inner = effect(callback(vec![tap(40)], 30), 30);
        let bound =
            bind_reflexive(&mut callback(vec![sacrifice, inner], 10), &mut parent(0)).unwrap();
        let nested = &bound["args"]["body"]["instructions"][1]["args"]["operation"];
        assert_eq!(bound["args"]["performed"], json!(0));
        assert_eq!(nested["args"]["performed"], json!(2));
        assert_eq!(*counted_result(nested), json!(2));
    }

    #[test]
    fn performed_header_and_inverse_are_bounded_native_result_addresses() {
        use crate::model::ir_schema::{Schema, Ty};
        use crate::pattern::{Bindings, elaborate, parse};
        let schema = Schema::of_ir().unwrap();
        let (_, syntax) = parse("@reflexive_performed").unwrap();
        let pattern = elaborate(&syntax, &Ty::Named("ResultId".into()), &schema).unwrap();
        for name in ["Expr", "TargetSlot", "SelectionId", "LoopBinding"] {
            assert!(elaborate(&syntax, &Ty::Named(name.into()), &schema).is_err());
        }
        let (_, args) = parse("@reflexive_performed(0)").unwrap();
        assert!(elaborate(&args, &Ty::Named("ResultId".into()), &schema).is_err());
        for value in [json!(0), json!(u32::MAX)] {
            assert!(!pattern.unify(&value, Bindings::new()).is_empty());
        }
        for value in [json!(-1), json!(u64::from(u32::MAX) + 1), json!("0")] {
            assert!(pattern.unify(&value, Bindings::new()).is_empty());
            let mut inner = json!({"kind":"ReflexiveTrigger","args":{"performed":value}});
            assert!(initialize(&mut inner, &mut parent(0), None).is_err());
        }
    }

    #[test]
    fn actual_draw_receipts_do_not_replace_requested_draw_amounts() {
        let draw = json!({"kind":"Effect","args":{"result":0,"operation":{"kind":"Draw",
            "args":{"players":{"kind":"One","args":{"kind":"AbilityController"}},
                "amount":{"kind":"Constant","args":2}}}}});
        let root = json!({"instructions":[draw]});
        let error = bind(
            &mut callback(vec![tap(20)], 10),
            &mut parent(0),
            Some(&root),
        )
        .unwrap_err();
        assert!(error.contains("captured requested amount"), "{error}");
    }
}
