//! A proven one-card receipt is tested and measured as that card. After one card moves,
//! "if it's a land card" is `ObjectMatches(Result{index:0})`, not a count of the moved
//! land cards above zero, and "its mana value" is that card's property, not a sum.
//!
//! The proof comes from the receipt's operation: one target, a top card, a one-card
//! choice, observation or search. A requested amount proves nothing: a mill or discard
//! of one can become several. A loop's receipts collect every run, a branch proves
//! nothing to its siblings or its enclosing program, and an independent ability starts
//! over; created triggers and selections see the receipts around them.
use serde_json::{Value, json};
use std::collections::{BTreeMap, BTreeSet};

#[derive(Clone, Copy, Default)]
struct Objects {
    cards: bool,
    one: bool,
}

#[derive(Clone, Copy, Default)]
struct Receipt {
    before: Objects,
    after: Objects,
    selected: Objects,
}

#[derive(Clone, Default)]
struct Frame {
    results: BTreeMap<u64, Receipt>,
    written: BTreeSet<u64>,
    targets: BTreeMap<u64, Objects>,
    prepared: BTreeMap<u64, Objects>,
}

const LOOPS: [&str; 4] = [
    "ForEachPlayer",
    "ForEachObject",
    "Repeatable",
    "RepeatableWithLimit",
];
const CLOSURES: [&str; 6] = [
    "CreateTrigger",
    "CreateDelayedTrigger",
    "CreateReplacement",
    "ReflexiveTrigger",
    "WithSelections",
    "EachPlayer",
];

fn kind(node: &Value) -> &str {
    node.get("kind").and_then(Value::as_str).unwrap_or_default()
}

fn constant_one(expr: &Value) -> bool {
    *expr == json!({"kind":"Constant","args":1})
}

fn one(count: &Value) -> bool {
    ["Exactly", "UpTo", "AsManyAsCan"]
        .iter()
        .any(|bound| count.get(bound).is_some_and(constant_one))
}

/// Whether `node` is an ability, which starts a frame of its own. Other enums share
/// some of these names (an `Expr::Conditional`, an `Operation::Keyword`).
fn is_ability(node: &Value) -> bool {
    let args = &node["args"];
    match kind(node) {
        "Spell" | "Activated" | "Triggered" => args.get("body").is_some(),
        "Replacement" => args.get("event").is_some(),
        "Scoped" => args.get("ability").is_some(),
        "Conditional" => args["then"].is_array(),
        "Static" | "CostBranch" | "CostOptions" => args.is_object(),
        _ => false,
    }
}

fn is_mode(node: &Value) -> bool {
    ["announcement", "body", "weight"]
        .iter()
        .all(|key| node.get(key).is_some())
}

impl Frame {
    fn transition(&self, moved: &Value) -> Objects {
        moved["result"]
            .as_u64()
            .and_then(|result| self.results.get(&result))
            .map(|receipt| {
                if moved["phase"] == "Before" {
                    receipt.before
                } else {
                    receipt.after
                }
            })
            .unwrap_or_default()
    }

    fn objects(&self, set: &Value) -> Objects {
        let args = &set["args"];
        let numbered = |map: &BTreeMap<u64, Objects>| {
            args.as_u64()
                .and_then(|key| map.get(&key))
                .copied()
                .unwrap_or_default()
        };
        match kind(set) {
            "Zone" => Objects {
                cards: args["kind"] == "Card",
                one: false,
            },
            "One" => Objects {
                cards: match kind(args) {
                    "Target" => args["args"]["slot"]
                        .as_u64()
                        .and_then(|slot| self.targets.get(&slot))
                        .is_some_and(|target| target.cards),
                    "Query" => self.objects(&args["args"]["objects"]).cards,
                    "Result" => self.transition(&args["args"]).cards,
                    _ => false,
                },
                one: true,
            },
            "Targets" => numbered(&self.targets),
            "Prepared" => numbered(&self.prepared),
            "Take" => {
                let pool = self.objects(&args["objects"]);
                Objects {
                    cards: pool.cards,
                    one: constant_one(&args["count"]) || pool.one,
                }
            }
            "Matching" | "InZone" => self.objects(&args["objects"]),
            "Observed" | "Selected" => args
                .as_u64()
                .and_then(|result| self.results.get(&result))
                .map(|receipt| receipt.selected)
                .unwrap_or_default(),
            "Transition" => self.transition(args),
            "Union" => {
                let sets = args.as_array().map(Vec::as_slice).unwrap_or_default();
                Objects {
                    cards: !sets.is_empty() && sets.iter().all(|set| self.objects(set).cards),
                    one: sets.len() == 1 && self.objects(&sets[0]).one,
                }
            }
            _ => Objects::default(),
        }
    }

    fn selection(&mut self, step: &Value) {
        let pool = self.objects(&step["objects"]);
        let chooser = &step["chooser"];
        let single_chooser = *chooser == "Chance" || kind(&chooser["Players"]) == "One";
        let Some(binding) = step["binding"].as_u64() else {
            return;
        };
        let objects = Objects {
            cards: pool.cards,
            one: pool.one || one(&step["count"]) && single_chooser,
        };
        self.prepared.insert(binding, objects);
    }

    fn receipt(&self, operation: &Value) -> Receipt {
        let args = &operation["args"];
        let mut receipt = Receipt::default();
        match kind(operation) {
            "WithSelections" => {
                let mut scope = self.clone();
                for step in args["steps"].as_array().into_iter().flatten() {
                    scope.selection(step);
                }
                return scope.receipt(&args["action"]);
            }
            "Move" => {
                let input = self.objects(&args["objects"]);
                receipt.before = input;
                receipt.after = Objects {
                    cards: input.cards && args["destination"].get("Battlefield").is_none(),
                    one: input.cards && input.one,
                };
            }
            "Destroy" => receipt.before = self.objects(&args["objects"]),
            "Sacrifice" => {
                let input = self.objects(&args["objects"]);
                let chosen = kind(&args["players"]) == "One" && one(&args["choose"]);
                receipt.before = Objects {
                    cards: input.cards,
                    one: input.one || chosen,
                };
            }
            "ChooseObjects" => {
                let input = self.objects(&args["objects"]);
                receipt.selected = Objects {
                    cards: input.cards,
                    one: input.one || one(&args["count"]),
                };
            }
            "LookAt" | "Reveal" | "NoteObjects" => {
                receipt.selected = self.objects(&args["objects"]);
            }
            "Search" => {
                let choices = args["choices"].as_array().map(Vec::as_slice);
                receipt.selected = Objects {
                    cards: true,
                    one: matches!(choices, Some([choice]) if one(&choice["count"])),
                };
            }
            // A requested mill or discard of one can be replaced by several cards.
            "Mill" | "Discard" | "DiscardObjects" => {
                receipt.before.cards = true;
                receipt.after.cards = true;
            }
            _ => {}
        }
        receipt
    }

    fn target(&mut self, spec: &Value) {
        let (Some(slot), Some(count)) = (spec["slot"].as_u64(), spec.get("count")) else {
            return;
        };
        let domain = &spec["kind"]["Object"];
        let battlefield = domain["zones"]
            .as_array()
            .is_some_and(|zones| zones.iter().any(|zone| zone == "Battlefield"));
        let objects = Objects {
            cards: domain["kind"] == "Card" && !battlefield,
            one: one(count),
        };
        self.targets.insert(slot, objects);
    }
}

fn is_target_spec(node: &Value) -> bool {
    ["slot", "count", "constraints", "distribute"]
        .iter()
        .all(|key| node.get(key).is_some())
}

/// The one-card spelling of a proven singleton test or measure, or the counted
/// spelling it came from.
fn rewritten(node: &Value, scope: &Frame, lower: bool) -> Option<Value> {
    let args = &node["args"];
    let receipt = |object: &Value| json!({"kind":"Result","args":{"result":object["result"],"phase":object["phase"],"index":0}});
    let moved = |object: &Value| json!({"kind":"Transition","args":{"result":object["result"],"phase":object["phase"]}});
    let above_zero = json!({"kind":"Compare","args":{"comparison":"Greater",
        "value":{"kind":"Constant","args":0}}});
    match kind(node) {
        "Compare" if lower && args["test"] == above_zero => {
            let matching = &args["left"]["args"];
            let set = &matching["args"]["objects"];
            (kind(&args["left"]) == "CountObjects"
                && kind(matching) == "Matching"
                && kind(set) == "Transition"
                && scope.transition(&set["args"]).one)
                .then(|| {
                    json!({"kind":"ObjectMatches","args":{"object":receipt(&set["args"]),
                        "filter":matching["args"]["filter"]}})
                })
        }
        "Aggregate" if lower && args["aggregation"] == "Sum" => {
            let set = &args["objects"];
            (kind(set) == "Transition" && scope.transition(&set["args"]).one).then(|| {
                json!({"kind":"ObjectProperty","args":{"object":receipt(&set["args"]),
                    "property":args["property"],"information":"CurrentOrLastKnown"}})
            })
        }
        "ObjectMatches" | "ObjectProperty" if !lower => {
            let object = &args["object"];
            let proven = kind(object) == "Result"
                && object["args"]["index"] == 0
                && scope.transition(&object["args"]).one;
            if !proven {
                return None;
            }
            Some(if kind(node) == "ObjectMatches" {
                json!({"kind":"Compare","args":{"left":{"kind":"CountObjects","args":{
                    "kind":"Matching","args":{"objects":moved(&object["args"]),
                    "filter":args["filter"]}}},"test":above_zero}})
            } else if args["information"] == "CurrentOrLastKnown" {
                json!({"kind":"Aggregate","args":{"objects":moved(&object["args"]),
                    "property":args["property"],"aggregation":"Sum"}})
            } else {
                return None;
            })
        }
        _ => None,
    }
}

struct Pass {
    scope: Frame,
    frames: Vec<Frame>,
    lower: bool,
    changed: bool,
}

impl Pass {
    fn push(&mut self, captures: bool) {
        let mut next = if captures {
            self.scope.clone()
        } else {
            Frame::default()
        };
        next.written.clear();
        self.frames.push(std::mem::replace(&mut self.scope, next));
    }

    fn pop(&mut self) -> Frame {
        let outer = self.frames.pop().unwrap_or_default();
        std::mem::replace(&mut self.scope, outer)
    }

    fn branch(&mut self, body: Option<&mut Value>) {
        if let Some(body) = body {
            self.push(true);
            self.walk(body);
            self.pop();
        }
    }

    fn children(&mut self, node: &mut Value) {
        if let Value::Object(fields) = node {
            fields.values_mut().for_each(|value| self.walk(value));
        }
    }

    fn walk(&mut self, node: &mut Value) {
        match node {
            Value::Array(items) => items.iter_mut().for_each(|item| self.walk(item)),
            Value::Object(_) => self.object(node),
            _ => return,
        }
        if let Some(new) = rewritten(node, &self.scope, self.lower) {
            *node = new;
            self.changed = true;
        }
    }

    fn object(&mut self, node: &mut Value) {
        if is_ability(node) || is_mode(node) {
            self.push(is_mode(node));
            self.children(node);
            self.pop();
            return;
        }
        if is_target_spec(node) {
            self.scope.target(node);
        }
        let name = kind(node).to_owned();
        match name.as_str() {
            "If" => {
                if let Some(condition) = node.pointer_mut("/args/condition") {
                    self.walk(condition);
                }
                self.branch(node.pointer_mut("/args/then"));
                self.branch(node.pointer_mut("/args/otherwise"));
            }
            "ChooseOne" => {
                for key in ["decider", "offered"] {
                    if let Some(value) = node.pointer_mut(&format!("/args/{key}")) {
                        self.walk(value);
                    }
                }
                let options = node
                    .pointer_mut("/args/options")
                    .and_then(Value::as_array_mut);
                for option in options.into_iter().flatten() {
                    self.branch(Some(option));
                }
            }
            looped if LOOPS.contains(&looped) => {
                self.push(true);
                self.children(node);
                let body = self.pop();
                if looped != "RepeatableWithLimit" {
                    for id in body.written {
                        let mut receipt = body.results.get(&id).copied().unwrap_or_default();
                        receipt.before.one = false;
                        receipt.after.one = false;
                        receipt.selected.one = false;
                        self.scope.results.insert(id, receipt);
                        self.scope.written.insert(id);
                    }
                }
            }
            "Effect" if node["args"].get("operation").is_some() => {
                self.children(node);
                let args = &node["args"];
                if let Some(result) = args["result"].as_u64() {
                    let receipt = self.scope.receipt(&args["operation"]);
                    self.scope.written.insert(result);
                    self.scope.results.insert(result, receipt);
                }
            }
            closure if CLOSURES.contains(&closure) => {
                self.push(true);
                if closure == "WithSelections" {
                    let steps = node["args"]["steps"]
                        .as_array()
                        .cloned()
                        .unwrap_or_default();
                    steps.iter().for_each(|step| self.scope.selection(step));
                }
                self.children(node);
                self.pop();
            }
            _ => self.children(node),
        }
    }
}

fn run(abilities: &mut Value, lower: bool) -> bool {
    let mut pass = Pass {
        scope: Frame::default(),
        frames: Vec::new(),
        lower,
        changed: false,
    };
    pass.walk(abilities);
    pass.changed
}

/// Name each proven one-card receipt's test and measure by that card.
pub(super) fn lower(abilities: &mut Value) -> bool {
    run(abilities, true)
}

/// The counted spelling of each proven one-card test and measure. Whether any changed.
pub(super) fn surface(abilities: &mut Value) -> bool {
    run(abilities, false)
}

#[cfg(test)]
#[path = "single_receipts_tests.rs"]
mod tests;
