//! Regression tests for print-loop accelerations: static list shapes, interned
//! renderer tokens + bitmask-gated style rules, within-face `FaceMemo`, and
//! cross-candidate `ParseMemo` + per-ability bind caching.
use crate::engine::Engine;
use crate::print_reuse::Saved;
use crate::print_trace;
use crate::render::FaceMemo;
use crate::rtoken::RToken;
use crate::style::{broken_rules, broken_rules_rtokens, mark_flags, word_flags};
use crate::tokens::{self, Names, Token};

fn engine() -> &'static Engine {
    crate::test_support::engine()
}

#[test]
fn guarded_trigger_prints_current_operands_at_resolution() {
    let engine = engine();
    let name = "Test Elk";
    let types = ["Legendary".into(), "Creature".into()];
    let text = "When Test Elk enters, create a Treasure token if an opponent controls more lands than you. If an opponent has more life than you, you gain 4 life. If an opponent has more cards in hand than you, draw a card.";
    let parsed = engine.parse_types(name, text, &types).unwrap();
    assert_eq!(parsed.readings.len(), 1);
    let original = &parsed.readings[0];
    let trail = engine
        .trail_of_types(name, &types, original, text)
        .unwrap()
        .unwrap();
    assert_eq!(
        engine
            .print_trail_types(name, &types, original, &trail)
            .unwrap()
            .as_deref(),
        Some(text)
    );
    let mut edited = original.clone();
    edited[0]["args"]["body"]["instructions"][1]["args"]["then"]["instructions"][0]["args"]["operation"]
        ["args"]["amount"]["args"] = serde_json::json!(7);
    let printed = engine.render(name, &edited, &types, 1).unwrap();
    assert_eq!(printed.len(), 1);
    assert!(printed[0].contains("gain 7 life"), "{printed:?}");
    let reparsed = engine.parse_types(name, &printed[0], &types).unwrap();
    assert_eq!(
        reparsed.readings,
        vec![crate::engine::canonical_abilities(&edited).unwrap()]
    );
    assert!(reparsed.readings[0][0]["args"]["intervening_if"].is_null());
    let replayed = engine
        .print_trail_types(name, &types, &edited, &trail)
        .unwrap()
        .unwrap();
    assert_eq!(replayed, text.replace("gain 4 life", "gain 7 life"));
}

#[test]
fn print_index_preserves_spliced_list_matches_with_fixed_ends() {
    use crate::grammar::Rule;
    use crate::heads::{Net, Values};
    use crate::model::ir_schema::Ty;
    use crate::pattern::{Item, Pattern};
    use serde_json::{Value, json};

    let first = json!({"kind":"If","args":{}});
    let last = json!({"kind":"Effect","args":{}});
    let pattern = Pattern::Object(vec![(
        "instructions".into(),
        Pattern::List(vec![
            Item::One(Pattern::Value(first.clone())),
            Item::Splice("middle".into(), Ty::Named("Instruction".into())),
            Item::One(Pattern::Object(vec![
                ("kind".into(), Pattern::Value(json!("Effect"))),
                ("args".into(), Pattern::Value(json!({}))),
            ])),
        ]),
    )]);
    let net = Net::build(&[Rule {
        name: "list".into(),
        alternatives: Vec::new(),
        sem: Some(pattern),
        ty: None,
        prefer: Vec::new(),
    }]);
    let mut values = Values::default();
    for (list, fits) in [
        (json!([first, last]), true),
        (json!([first, Value::Null, last]), true),
        (json!([last, last]), false),
        (json!([first, first]), false),
        (json!([first]), false),
        (Value::Null, false),
    ] {
        let (_, reach) = values.id(&net, &json!({"instructions":list}));
        assert_eq!(net.reaches(&reach, 0), fits, "{list}");
    }
}

fn to_rtokens(engine: &Engine, tokens: &[Token]) -> (Vec<RToken>, Vec<String>, u64) {
    let table = &engine.grammar.word_table;
    let mut dyn_words = Vec::new();
    let mut rtoks = Vec::with_capacity(tokens.len());
    let mut flags = 0u64;
    for tok in tokens {
        match tok {
            Token::Word(w) => {
                if let Some(&id) = table.ids.get(w.as_str()) {
                    flags |= table.word_flags[id as usize];
                    rtoks.push(RToken::Word(id));
                } else {
                    flags |= word_flags(w);
                    let id = dyn_words.len() as u32;
                    dyn_words.push(w.clone());
                    rtoks.push(RToken::Dynamic(id));
                }
            }
            Token::Number(n) | Token::Grouped(n) => rtoks.push(RToken::Number(*n)),
            Token::Mark(c) => {
                flags |= mark_flags(*c);
                rtoks.push(RToken::Mark(*c));
            }
            Token::SelfName(idx) => {
                flags |= crate::style::F_SELF_NAME;
                rtoks.push(RToken::SelfName(*idx));
            }
            Token::Newline => rtoks.push(RToken::Newline),
        }
    }
    (rtoks, dyn_words, flags)
}

#[test]
fn rtoken_style_rules_match_token_style_rules() {
    let eng = engine();
    let cases = [
        (
            "Grizzly Bears",
            &["Creature"][..],
            "Whenever Grizzly Bears attacks, draw a card and lose 1 life.",
        ),
        (
            "Grizzly Bears",
            &["Creature"],
            "Put two +1/+1 counter on target creature. Target creatures gets +1/+1 until end of turn.",
        ),
        (
            "Grizzly Bears",
            &["Creature"],
            "You may have until end of turn target creature gain flying.",
        ),
        (
            "Grizzly Bears",
            &["Creature"],
            "You may target creature gets +2/-0 until end of turn.",
        ),
        (
            "Grizzly Bears",
            &["Creature"],
            "Cast this creature from your hand. Grizzly Bears has flying until end of turn.",
        ),
        (
            "Sword of Test",
            &["Artifact", "Equipment"],
            "Enchanted creature gets +1/+1 and has flying.",
        ),
        (
            "Aura of Test",
            &["Enchantment", "Aura"],
            "Equipped permanent gets +2/+2.",
        ),
        (
            "Test Mage",
            &["Creature"],
            "Sacrifice this creature: This creature deals 2 damage to them.",
        ),
        (
            "Test Mage",
            &["Creature"],
            "Add {G}{G}. Spend this mana only to cast creature spells or activate abilities.",
        ),
        (
            "Test Mage",
            &["Creature"],
            "Target player reveals a card or permanent card, where X is its owner's life total.",
        ),
        (
            "Test Mage",
            &["Creature"],
            "If it was kicked, target attacking creature gains \"When this dies, draw a card.\"",
        ),
    ];
    for (name, raw_types, text) in cases {
        let names = Names::of(name);
        let tokens = tokens::tokenize(text, &names, &eng.grammar.vocabulary).unwrap();
        let types: Vec<String> = raw_types.iter().map(|s| (*s).to_string()).collect();
        let (rtoks, dyn_words, flags) = to_rtokens(eng, &tokens);
        let expected = broken_rules(&tokens, &types);
        let actual =
            broken_rules_rtokens(&rtoks, flags, &types, &eng.grammar.word_table, &dyn_words);
        assert!(
            (expected - actual).abs() < 1e-4,
            "broken_rules mismatch for {text:?}: expected={expected} actual={actual}"
        );
    }
}

#[test]
fn guided_unification_matches_unguided_on_parsed_cards() {
    let eng = engine();
    let cards = [
        (
            "Test Dual",
            "Flying\nWhen Test Dual enters, draw a card, then discard a card.\n{T}: Add {G}.",
        ),
        (
            "Test Modal",
            "Choose one —\n• Draw two cards.\n• Target creature gets +2/+2 until end of turn.",
        ),
    ];
    for (name, text) in cards {
        let parsed = eng.parse(name, text).unwrap();
        let abilities = &parsed.readings[0];
        let mut stack = vec![abilities];
        let mut values = Vec::new();
        while let Some(v) = stack.pop() {
            if v.is_array() {
                values.push(v);
            }
            match v {
                serde_json::Value::Array(items) => stack.extend(items),
                serde_json::Value::Object(map) => stack.extend(map.values()),
                _ => {}
            }
        }
        for value in values {
            let mut reaches = crate::heads::Values::default();
            let (_, reach) = reaches.id(&eng.grammar.net, value);
            for rule in 0..eng.grammar.rules.len() as u32 {
                if !eng.grammar.net.reaches(&reach, rule)
                    || eng.grammar.rules[rule as usize].sem.is_none()
                {
                    continue;
                }
                for native in [false, true] {
                    let unguided = eng.grammar.print_unify(rule, value, native);
                    let guided = eng.grammar.print_unify_guided(rule, value, native);
                    assert!(
                        guided.len() <= unguided.len(),
                        "guided returned extra bindings for rule {} native={native}",
                        eng.grammar.rules[rule as usize].name
                    );
                    if guided.len() < unguided.len() {
                        // Every pruned binding must have at least one unrenderable child hole.
                        let def = &eng.grammar.rules[rule as usize];
                        let style = crate::style::Model::default();
                        let mut renderer =
                            crate::render::Renderer::new(&eng.grammar, &style, 8, vec![])
                                .with_literal_inverses(native);
                        for bindings in &unguided {
                            let all_children_render = def.alternatives.iter().any(|alt| {
                                alt.syms.iter().zip(&alt.holes).all(|(sym, hole)| {
                                    let (crate::grammar::Sym::Rule(child), Some(h)) = (sym, hole)
                                    else {
                                        return true;
                                    };
                                    bindings
                                        .get(h)
                                        .is_some_and(|v| !renderer.render(*child, v).is_empty())
                                })
                            });
                            if all_children_render {
                                assert!(
                                    !guided.is_empty(),
                                    "guided pruned a renderable binding for rule {} native={native}",
                                    def.name
                                );
                            }
                        }
                    }
                }
            }
        }
    }
}

#[test]
fn face_memo_and_parse_memo_match_cold_prints_and_traces() {
    let eng = engine();
    let cards = [
        (
            "Memo Rider",
            vec!["Creature".to_string(), "Human".to_string()],
            "Flying\nWhen Memo Rider enters, target player draws a card.\n{1}{U}: Memo Rider gets +1/+0 until end of turn.",
        ),
        (
            "Memo Spell",
            vec!["Sorcery".to_string()],
            "Target creature gets +2/+2 until end of turn. Draw a card.",
        ),
    ];
    for (name, types, text) in cards {
        let parsed = eng.parse(name, text).unwrap();
        let abilities = &parsed.readings[0];

        let ((cold_def, cold_prods, cold_key, cold_narrow, cold_full), cold_trace) =
            print_trace::record(|| {
                let def = eng.print_keyed(name, abilities, &types, &[]).unwrap();
                let prods = eng.productions_of(name, abilities, &types).unwrap();
                let key = eng.derivation_of(name, abilities, text).unwrap().unwrap();
                let trimmed = Engine::trim_key(&key, &prods);
                let narrow = eng
                    .print_keyed_narrow(name, abilities, &types, &trimmed)
                    .unwrap();
                let full = eng.print_keyed(name, abilities, &types, &key).unwrap();
                (def, prods, key, narrow, full)
            });

        let ((memo_def, memo_prods, memo_key, memo_narrow, memo_full), memo_trace) =
            print_trace::record(|| {
                let mut memo = FaceMemo::new();
                let def = eng
                    .print_keyed_memo(&mut memo, name, abilities, &types, &[])
                    .unwrap();
                let prods = eng
                    .productions_of_memo(&mut memo, name, abilities, &types)
                    .unwrap();
                let key = eng.derivation_of(name, abilities, text).unwrap().unwrap();
                let trimmed = Engine::trim_key(&key, &prods);
                let narrow = eng
                    .print_keyed_narrow_memo(&mut memo, name, abilities, &types, &trimmed)
                    .unwrap();
                let full = eng
                    .print_keyed_memo(&mut memo, name, abilities, &types, &key)
                    .unwrap();
                (def, prods, key, narrow, full)
            });

        assert_eq!(cold_def, memo_def, "{name} default");
        assert_eq!(cold_prods, memo_prods, "{name} productions");
        assert_eq!(cold_key, memo_key, "{name} key");
        assert_eq!(cold_narrow, memo_narrow, "{name} narrow");
        assert_eq!(cold_full, memo_full, "{name} full");

        let cold_saved = Saved::from_trace(&cold_trace);
        let memo_saved = Saved::from_trace(&memo_trace);
        assert_eq!(cold_saved.rules, memo_saved.rules, "{name} trace rules");
        assert_eq!(
            cold_saved.prefixes, memo_saved.prefixes,
            "{name} trace prefixes"
        );
        assert_eq!(
            cold_saved.features, memo_saved.features,
            "{name} trace features"
        );
        assert_eq!(cold_saved.words, memo_saved.words, "{name} trace words");
        assert_eq!(cold_saved.pairs, memo_saved.pairs, "{name} trace pairs");
        assert_eq!(
            cold_saved.vocabulary, memo_saved.vocabulary,
            "{name} trace vocabulary"
        );
    }
}

#[cfg(feature = "cli")]
#[test]
fn audit_default_print_memo_matches_cold_round_trip() {
    let eng = engine();
    let name = "Memo Audit";
    let types = vec!["Creature".to_string()];
    let text = "Flying\nWhen Memo Audit enters, draw a card.\n{2}: Memo Audit gets +1/+1 until end of turn.";
    let parsed = eng.parse(name, text).unwrap();
    let abilities = &parsed.readings[0];
    let ((cold_def, cold_rt), cold_trace) = print_trace::record(|| {
        let def = eng.print_keyed(name, abilities, &types, &[]).unwrap();
        let rt = eng.round_trip(name, abilities, &types, 8).unwrap();
        (def, rt)
    });
    let ((memo_def, memo_rt), memo_trace) = print_trace::record(|| {
        let mut memo = FaceMemo::new();
        eng.audit_default_print_memo(&mut memo, name, abilities, &types, 8)
            .unwrap()
    });
    assert_eq!(cold_def, memo_def);
    assert_eq!(cold_rt, memo_rt);
    assert_eq!(cold_trace.depth, memo_trace.depth);
    assert!(
        memo_trace.steps <= cold_trace.steps.saturating_mul(2).max(100),
        "memo inflated parse steps: cold={} memo={}",
        cold_trace.steps,
        memo_trace.steps
    );
}
