//! Oracle coin procedures inferred from scoped outcome observations.
//!
//! The typed flow analysis follows MTGish coin_policies.rs at published31026c7.
//! It runs only on newly bound grammar syntax; edited current IR stays explicit.
use crate::model::{
    human::document,
    ir::*,
    ir_schema::{Fields, Schema, Ty, TypeDef},
};
use serde_json::Value;
use std::collections::{BTreeMap, BTreeSet};
use std::sync::OnceLock;

pub(super) fn receipt(root: &Value, path: &[super::Step]) -> Result<Value, String> {
    fn at<'a>(mut value: &'a Value, path: &[super::Step]) -> Option<&'a Value> {
        for step in path {
            value = match step {
                super::Step::Key(key) => value.get(key)?,
                super::Step::Index(index) => value.get(*index)?,
            };
        }
        Some(value)
    }
    for depth in (0..path.len()).rev() {
        let node = at(root, &path[..depth]).ok_or("coin receipt path")?;
        if depth > 0
            && matches!(
                crate::traverse::boundary(node),
                crate::traverse::Boundary::Ability
                    | crate::traverse::Boundary::Mode
                    | crate::traverse::Boundary::Holder
                    | crate::traverse::Boundary::Lexical
            )
        {
            break;
        }
        // Consecutive physical-outcome clauses share the batch, rather than
        // nesting the second clause inside the first outcome's consequence.
        // A fresh batch inside that consequence is found before this boundary.
        if node["kind"] == "If"
            && node["args"]["condition"]["kind"] == "Compare"
            && node["args"]["condition"]["args"]["left"]["kind"] == "CountCoinOutcomes"
            && matches!(path.get(depth), Some(super::Step::Key(k)) if k == "args")
            && matches!(path.get(depth + 1), Some(super::Step::Key(k)) if k == "then")
        {
            return Err(
                "a separate coin outcome clause starts outside the preceding outcome body".into(),
            );
        }
        if let super::Step::Index(index) = path[depth]
            && matches!(depth.checked_sub(1).and_then(|i| path.get(i)), Some(super::Step::Key(k)) if k == "instructions")
        {
            for before in node
                .as_array()
                .ok_or("coin instruction sequence")?
                .iter()
                .take(index)
                .rev()
            {
                if before["kind"] == "Effect" && before["args"]["operation"]["kind"] == "FlipCoins"
                {
                    return before["args"]["result"]
                        .as_u64()
                        .map(serde_json::Value::from)
                        .ok_or_else(|| "coin batch needs a bound result".to_string());
                }
            }
        }
    }
    Err("both coins requires a preceding coin batch in this scope".into())
}

#[cfg(test)]
#[path = "coin_protocol_tests.rs"]
mod tests;

fn schema() -> &'static Schema {
    static S: OnceLock<Schema> = OnceLock::new();
    S.get_or_init(|| Schema::of_ir().expect("coin schema"))
}
fn ptr(path: &str, key: &str) -> String {
    format!("{path}/{}", key.replace('~', "~0").replace('/', "~1"))
}
fn variant(v: &Value, adjacent: bool) -> Option<&str> {
    if adjacent {
        v["kind"].as_str()
    } else {
        v.as_str()
            .or_else(|| v.as_object()?.keys().next().map(String::as_str))
    }
}
/// Typed child enumeration: text, draft-form labels and opaque data are never
/// mistaken for semantic operations or observations by matching their spelling.
fn children(ty: &Ty, value: &Value, path: &str) -> Vec<(Ty, Value, String)> {
    fn fields(f: &Fields, v: &Value, p: &str, out: &mut Vec<(Ty, Value, String)>) {
        match f {
            Fields::Unit => {}
            Fields::Named(fs) => {
                for (k, t) in fs {
                    out.push((t.clone(), v[k].clone(), ptr(p, k)));
                }
            }
            Fields::Tuple(ts) => {
                for (i, t) in ts.iter().enumerate() {
                    out.push((
                        t.clone(),
                        if ts.len() == 1 {
                            v.clone()
                        } else {
                            v[i].clone()
                        },
                        if ts.len() == 1 {
                            p.into()
                        } else {
                            ptr(p, &i.to_string())
                        },
                    ));
                }
            }
        }
    }
    let mut out = vec![];
    match ty {
        Ty::Option(t) if !value.is_null() => out.push(((**t).clone(), value.clone(), path.into())),
        Ty::Vec(t) => {
            for (i, v) in value.as_array().into_iter().flatten().enumerate() {
                out.push(((**t).clone(), v.clone(), ptr(path, &i.to_string())));
            }
        }
        Ty::Tuple(ts) => {
            for (i, t) in ts.iter().enumerate() {
                out.push((t.clone(), value[i].clone(), ptr(path, &i.to_string())));
            }
        }
        Ty::Named(name) => match schema().types.get(name) {
            Some(TypeDef::Struct(f)) => fields(f, value, path, &mut out),
            Some(TypeDef::Enum { adjacent, variants }) => {
                if let Some(n) = variant(value, *adjacent)
                    && let Some((_, f)) = variants.iter().find(|(n2, _)| n2 == n)
                {
                    fields(
                        f,
                        &value[if *adjacent { "args" } else { n }],
                        &ptr(path, if *adjacent { "args" } else { n }),
                        &mut out,
                    );
                }
            }
            _ => {}
        },
        _ => {}
    };
    out
}
#[derive(Clone)]
struct Producer {
    protocol: CoinProtocol,
    locked: bool,
    faces: bool,
    winner: bool,
}
#[derive(Clone, Default)]
struct History {
    last: BTreeSet<String>,
    players: BTreeSet<String>,
    objects: BTreeSet<String>,
}
impl History {
    fn merge(&mut self, other: Self) {
        self.last.extend(other.last);
        self.players.extend(other.players);
        self.objects.extend(other.objects);
    }
}
struct Analysis {
    definitions: BTreeMap<usize, Vec<String>>,
    uses: BTreeMap<String, usize>,
    producers: BTreeMap<String, Producer>,
    source: bool,
    forced: BTreeSet<String>,
}
impl Analysis {
    fn collect(&mut self, ty: &Ty, v: &Value, p: &str, binding: Option<usize>) {
        let effect = matches!(ty,Ty::Named(n) if n=="Instruction") && v["kind"] == "Effect";
        let binding = if effect {
            self.uses.get(&format!("{p}/args/result")).copied()
        } else {
            binding
        };
        if matches!(ty,Ty::Named(n) if n=="Operation")
            && matches!(
                v["kind"].as_str(),
                Some("FlipCoin" | "FlipCoins" | "FlipCoinSeries")
            )
        {
            let protocol: CoinProtocol =
                serde_json::from_value(v["args"]["protocol"].clone()).expect("typed protocol");
            let series = v["kind"] == "FlipCoinSeries";
            let forced_source =
                self.source && series && v["args"]["repetition"].get("EachObject").is_none();
            self.producers.insert(
                p.into(),
                Producer {
                    protocol,
                    locked: self.forced.contains(p)
                        || forced_source
                        || self.source && protocol == CoinProtocol::Uncalled,
                    faces: false,
                    winner: false,
                },
            );
            if let Some(b) = binding {
                self.definitions.entry(b).or_default().push(p.into());
            }
        }
        for (t, v, p) in children(ty, v, p) {
            self.collect(&t, &v, &p, binding);
        }
    }
    fn constrain(&mut self, paths: &BTreeSet<String>, outcome: &CoinOutcome) {
        for p in paths {
            if let Some(d) = self.producers.get_mut(p) {
                match outcome {
                    CoinOutcome::Won | CoinOutcome::Lost => d.winner = true,
                    _ => d.faces = true,
                }
            }
        }
    }
    fn receipt(&mut self, path: &str, outcome: &CoinOutcome) {
        if let Some(b) = self.uses.get(path) {
            let paths = self
                .definitions
                .get(b)
                .cloned()
                .unwrap_or_default()
                .into_iter()
                .collect();
            self.constrain(&paths, outcome);
        }
    }
    fn instruction(&mut self, v: &Value, p: &str, h: &mut History) -> bool {
        let ty = &Ty::Named("Instruction".into());
        let a = &v["args"];
        let q = format!("{p}/args");
        match v["kind"].as_str() {
            Some("Effect") => {
                self.node(
                    &Ty::Named("Operation".into()),
                    &a["operation"],
                    &format!("{q}/operation"),
                    h,
                );
                return true;
            }
            Some("Group") => {
                self.node(&Ty::Named("Program".into()), a, &q, h);
                return true;
            }
            Some("If") => {
                self.node(
                    &Ty::Named("Condition".into()),
                    &a["condition"],
                    &format!("{q}/condition"),
                    &mut h.clone(),
                );
                let mut left = h.clone();
                let mut right = h.clone();
                self.node(
                    &Ty::Named("Program".into()),
                    &a["then"],
                    &format!("{q}/then"),
                    &mut left,
                );
                self.node(
                    &Ty::Named("Program".into()),
                    &a["otherwise"],
                    &format!("{q}/otherwise"),
                    &mut right,
                );
                left.merge(right);
                *h = left;
                return true;
            }
            Some("ChooseOne") => {
                let mut all = History::default();
                for (i, body) in a["options"].as_array().into_iter().flatten().enumerate() {
                    let mut child = h.clone();
                    self.node(
                        &Ty::Named("Program".into()),
                        body,
                        &format!("{q}/options/{i}"),
                        &mut child,
                    );
                    all.merge(child);
                }
                *h = all;
                return true;
            }
            Some("ForEachPlayer" | "ForEachObject" | "Optional" | "RepeatableWithLimit") => {
                // Non-body inputs read the incoming history, never a future flip.
                for (t, v, p2) in children(ty, v, p) {
                    if p2 != format!("{q}/body") {
                        self.node(&t, &v, &p2, &mut h.clone());
                    }
                }
                let mut child = h.clone();
                self.node(
                    &Ty::Named("Program".into()),
                    &a["body"],
                    &format!("{q}/body"),
                    &mut child,
                );
                if v["kind"] == "ForEachPlayer" {
                    let mut body = &a["body"];
                    let mut at = format!("{q}/body");
                    while let Some(ins) = body["instructions"]
                        .as_array()
                        .filter(|x| x.len() == 1)
                        .map(|x| &x[0])
                    {
                        if ins["kind"] == "Group" {
                            body = &ins["args"];
                            at.push_str("/instructions/0/args");
                            continue;
                        }
                        if ins["kind"] == "Effect" && ins["args"]["operation"]["kind"] == "FlipCoin"
                        {
                            let batch =
                                BTreeSet::from([format!("{at}/instructions/0/args/operation")]);
                            // This loop defines a new batch even when its
                            // player set is empty. An earlier batch is not
                            // an alternative outcome of this instruction.
                            child.players = batch.clone();
                            child.last = batch;
                            *h = child;
                            return true;
                        }
                        break;
                    }
                }
                h.merge(child);
                return true;
            }
            Some("Repeatable") => {
                self.node(&Ty::Named("Program".into()), a, &q, h);
                return true;
            }
            _ => {}
        }

        false
    }
    fn node(&mut self, ty: &Ty, v: &Value, p: &str, h: &mut History) {
        let Ty::Named(name) = ty else {
            for (t, v, p) in children(ty, v, p) {
                self.node(&t, &v, &p, &mut h.clone());
            }
            return;
        };
        if name == "Ability" || name == "Mode" {
            for (t, v, p) in children(ty, v, p) {
                self.node(&t, &v, &p, &mut History::default());
            }
            return;
        }
        if name == "Program" {
            for (i, v) in v["instructions"]
                .as_array()
                .into_iter()
                .flatten()
                .enumerate()
            {
                self.node(
                    &Ty::Named("Instruction".into()),
                    v,
                    &format!("{p}/instructions/{i}"),
                    h,
                );
            }
            return;
        }
        if name == "Instruction" && self.instruction(v, p, h) {
            return;
        }

        if name == "Operation" {
            if self.producers.contains_key(p) {
                for (t, v, p2) in children(ty, v, p) {
                    self.node(&t, &v, &p2, &mut h.clone());
                }
                h.last = BTreeSet::from([p.into()]);
                if v["kind"] == "FlipCoinSeries"
                    && v["args"]["repetition"].get("EachObject").is_some()
                {
                    h.objects = h.last.clone();
                }
                if v["kind"] == "FlipCoinSeries" {
                    let series: CoinRepetition =
                        serde_json::from_value(v["args"]["repetition"].clone()).unwrap();
                    if matches!(
                        series,
                        CoinRepetition::Until(CoinStop::Lose | CoinStop::LoseOrPlayerStops)
                            | CoinRepetition::UpTo {
                                stop: CoinStop::Lose | CoinStop::LoseOrPlayerStops,
                                ..
                            }
                    ) {
                        self.constrain(&h.last, &CoinOutcome::Won);
                    }
                }
                return;
            }
            if v["kind"] == "WithSelections" {
                for (t, v, p2) in children(ty, v, p) {
                    if p2 == format!("{p}/args/action") {
                        self.node(&t, &v, &p2, h);
                    } else {
                        self.node(&t, &v, &p2, &mut h.clone());
                    }
                }
                return;
            }
            if v["kind"] == "InOrder" {
                for (t, v, p2) in children(ty, v, p) {
                    if let Ty::Vec(inner) = t {
                        for (i, v) in v.as_array().into_iter().flatten().enumerate() {
                            self.node(&inner, v, &ptr(&p2, &i.to_string()), h);
                        }
                    }
                }
                return;
            }
            if v["kind"] == "Together" {
                let mut all = h.clone();
                for (t, v, p2) in children(ty, v, p) {
                    if let Ty::Vec(inner) = t {
                        for (i, v) in v.as_array().into_iter().flatten().enumerate() {
                            let mut child = h.clone();
                            self.node(&inner, v, &ptr(&p2, &i.to_string()), &mut child);
                            all.merge(child);
                        }
                    }
                }
                *h = all;
                return;
            }
        }
        if name == "Expr" && v["kind"] == "CountCoinOutcomes" {
            let outcome: CoinOutcome =
                serde_json::from_value(v["args"]["outcome"].clone()).unwrap();
            if v["args"]["source"] == "Latest" {
                self.constrain(&h.last, &outcome);
            } else {
                self.receipt(&format!("{p}/args/source/Receipt"), &outcome);
            }
        }
        if name == "Condition" && v["kind"] == "CoinFlip" {
            let o = serde_json::from_value(v["args"]["outcome"].clone()).unwrap();
            self.receipt(&format!("{p}/args/result"), &o);
        }
        if name == "Condition" && v["kind"] == "CoinCameUp" {
            let o = serde_json::from_value(v["args"]["outcome"].clone()).unwrap();
            self.constrain(
                if v["args"]["player"].is_null() {
                    &h.objects
                } else {
                    &h.players
                },
                &o,
            );
        }
        for (t, v, p) in children(ty, v, p) {
            let mut private = if name == "Operand"
                && v.is_object()
                && matches!(&t, Ty::Named(n) if n == "Program")
            {
                History::default()
            } else {
                h.clone()
            };
            self.node(&t, &v, &p, &mut private);
        }
    }
}

fn has_coins(value: &Value) -> bool {
    if matches!(
        value["kind"].as_str(),
        Some("FlipCoin" | "FlipCoins" | "FlipCoinSeries")
    ) {
        return true;
    }
    match value {
        Value::Object(fields) => fields.values().any(has_coins),
        Value::Array(items) => items.iter().any(has_coins),
        _ => false,
    }
}

pub(super) fn lower(ability: &mut Value) -> Result<(), String> {
    crate::functions::branch_bounds(ability)?;
    if !has_coins(ability) {
        return Ok(());
    }
    let typed: Ability = serde_json::from_value(ability.clone()).map_err(|e| e.to_string())?;
    let card = UnifiedCard {
        schema_version: SCHEMA_VERSION,
        layout: Layout::Extension {
            namespace: "org.oracle-grammar".into(),
            name: "CoinSyntax".into(),
        },
        faces: vec![],
        data: vec![Field {
            name: "Ability".into(),
            value: Operand::Ability(Box::new(typed)),
        }],
    };
    let doc = document::from_card(&card)?;
    let mut value = serde_json::to_value(&card).map_err(|e| e.to_string())?;
    let mut analysis = Analysis {
        definitions: BTreeMap::new(),
        uses: doc
            .uses()
            .iter()
            .map(|u| (u.path.clone(), u.binding))
            .collect(),
        producers: BTreeMap::new(),
        source: true,
        forced: BTreeSet::new(),
    };
    let ty = Ty::Named("UnifiedCard".into());
    analysis.collect(&ty, &value, "", None);
    analysis.node(&ty, &value, "", &mut History::default());
    for (path, producer) in analysis.producers {
        let protocol = if producer.locked {
            producer.protocol
        } else if producer.faces && !producer.winner {
            CoinProtocol::Uncalled
        } else {
            CoinProtocol::Called
        };
        *value
            .pointer_mut(&format!("{path}/args/protocol"))
            .ok_or("coin protocol path disappeared")? =
            serde_json::to_value(protocol).map_err(|e| e.to_string())?;
    }
    *ability = value["data"][0]["value"]["args"].take();
    Ok(())
}
