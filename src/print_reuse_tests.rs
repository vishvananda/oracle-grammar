use super::*;
use crate::engine::Engine;
use crate::print_trace::{self, literal_key, pair_key};
use crate::test_support::engine;

/// The default grammar with one more ability spelling: a new rule under a pass-through
/// rule, which is how retirement adds seeded rules.
fn with_ability(rule: &str, body: &str, sem: &str) -> Engine {
    let mut card = crate::engine::module_source("card").unwrap();
    card = card.replacen("ability ::=", &format!("ability ::= {rule} |"), 1);
    card.push_str(&format!("\n/* @sem {sem} */\n{rule} ::= {body}\n"));
    Engine::load_with(&[("card".into(), card)]).unwrap()
}

const GAIN_TWO: &str = "Ability::Spell{announcement:_{targets:[]}, body:_{instructions:[Group{instructions:[Effect{result:7, operation:GainLife{players:One(AbilityController), amount:Constant(2)}}]}]}}";

/// Read and print a card as the scoreboard does, and keep what it consulted.
fn traced(engine: &Engine, text: &str) -> Saved {
    let ((), trace) = print_trace::record(|| {
        let reading = engine.parse("Example", text).unwrap().readings.remove(0);
        let printed = engine.print_keyed("Example", &reading, &[], &[]).unwrap();
        assert_eq!(printed.as_deref(), Some(text));
    });
    Saved::from_trace(&trace)
}

#[test]
fn an_unchanged_grammar_carries_every_traced_card_over() {
    let engine = engine();
    let changes = Changes::between(&engine.grammar, &engine.grammar, Limits::default());
    assert_eq!(
        (
            changes.summary.changed,
            changes.summary.added,
            changes.summary.removed
        ),
        (0, 0, 0)
    );
    let saved = traced(engine, "Flying");
    assert!(!saved.rules.is_empty() && !saved.prefixes.is_empty() && !saved.words.is_empty());
    assert_eq!(changes.verdict(&saved), None);
    assert_eq!(changes.renumber(&saved), Some(saved));
}

#[test]
fn a_new_rule_meaning_what_a_card_says_reprints_only_that_card() {
    let old = engine();
    let new = with_ability("reuse_settle_case", r#""settle" ".""#, GAIN_TWO);
    let changes = Changes::between(&old.grammar, &new.grammar, Limits::default());
    assert_eq!((changes.summary.added, changes.summary.hooks), (1, 1));
    // The new rule cannot read either text, but it can print what the first one means.
    assert_eq!(
        changes.verdict(&traced(old, "You gain 2 life.")),
        Some("a changed child can fit a looked-up value")
    );
    assert_eq!(changes.verdict(&traced(old, "Flying")), None);
}

#[test]
fn a_new_alternative_that_can_read_a_text_reprints_it() {
    let old = engine();
    let new = with_ability("reuse_flying_case", r#""flying" "again""#, GAIN_TWO);
    let changes = Changes::between(&old.grammar, &new.grammar, Limits::default());
    let saved = traced(old, "Flying");
    // `again` keeps the alternative from reading `Flying`, so only the words it shares
    // with the text would not be enough.
    assert_eq!(changes.verdict(&saved), None);
    let new = with_ability("reuse_flying_case", r#""flying""#, GAIN_TWO);
    let changes = Changes::between(&old.grammar, &new.grammar, Limits::default());
    assert_eq!(
        changes.verdict(&saved),
        Some("a changed alternative can read its text")
    );
}

#[test]
fn required_words_hold_in_every_derivation() {
    let new = with_ability("reuse_settle_case", r#""settle" ".""#, GAIN_TWO);
    let required = required_words(&new.grammar);
    let rule = new.grammar.names["reuse_settle_case"];
    let mut expected = vec![word_key("settle"), word_key(".")];
    expected.sort_unstable();
    let pairs = vec![pair_key(word_key("settle"), word_key("."))];
    assert_eq!(
        required[rule as usize],
        Some(Needs {
            words: expected,
            pairs
        })
    );
    // A pass-through rule requires only what all of its alternatives share.
    let ability = new.grammar.names["ability"];
    assert!(
        required[ability as usize]
            .as_ref()
            .is_some_and(|needs| !needs.words.contains(&word_key("settle")))
    );
}

#[test]
fn reordered_alternatives_have_no_common_order() {
    assert_eq!(
        common(&[1, 2, 3], &[1, 3, 4]),
        Some((vec![true, false, true], vec![true, true, false]))
    );
    assert_eq!(common(&[1, 2], &[2, 1]), None);
}

#[test]
fn carried_keys_follow_moved_alternatives() {
    use serde_json::json;
    let old = engine();
    let new = with_ability("reuse_settle_case", r#""settle" ".""#, GAIN_TWO);
    let changes = Changes::between(&old.grammar, &new.grammar, Limits::default());
    // A new first alternative moves every other one up, and a sorted key resorts.
    let mut row = json!({"name": "Example", "key": ["ability.9", "ability.10", "card.0"]});
    changes.rekey(&mut row).unwrap();
    assert_eq!(row["key"], json!(["ability.10", "ability.11", "card.0"]));
    let mut trail = json!({"key": ["card.0", "ability.10", "@name.0"], "ordered": true});
    changes.rekey(&mut trail).unwrap();
    assert_eq!(trail["key"], json!(["card.0", "ability.11", "@name.0"]));
    // Going back, the first alternative is gone, and so is the rule it named.
    let back = Changes::between(&new.grammar, &old.grammar, Limits::default());
    let mut removed = json!({"key": ["ability.0"]});
    assert_eq!(
        back.rekey(&mut removed),
        Err("the key names a removed alternative")
    );
    let mut named = json!({"key": ["reuse_settle_case.0"]});
    assert_eq!(back.rekey(&mut named), Err("the key names a changed rule"));
    let mut elsewhere = json!({"key": [], "first": "ability.3"});
    assert_eq!(
        changes.rekey(&mut elsewhere),
        Err("the row names a changed rule outside its key")
    );
}

#[test]
fn a_changed_alternative_needs_its_adjacent_words_in_order() {
    let old = engine();
    let new = with_ability("reuse_life_gain_case", r#""life" "gain""#, GAIN_TWO);
    let changes = Changes::between(&old.grammar, &new.grammar, Limits::default());
    let (life, gain, two) = (word_key("life"), word_key("gain"), word_key("2"));
    let mut words = vec![life, gain, two];
    words.sort_unstable();
    // "You gain 2 life" holds both words, but never "life" just before "gain".
    let mut saved = Saved {
        words,
        pairs: vec![pair_key(gain, two), pair_key(two, life)],
        ..Saved::default()
    };
    saved.pairs.sort_unstable();
    assert_eq!(changes.required_hit(&saved), None);
    saved.pairs.push(pair_key(life, gain));
    saved.pairs.sort_unstable();
    // The new alternative of `ability` needs what its new child needs; the child,
    // reached only through it, adds no requirement of its own.
    assert_eq!(changes.required_hit(&saved), Some("ability"));
    assert_eq!(literal_key("+2"), None);
    assert_eq!(literal_key("2"), Some(word_key("2")));
}

#[test]
fn an_added_rule_needs_nothing_beyond_the_alternatives_that_name_it() {
    let old = engine();
    // A new pass-through filter reads bare words, but only under its new parent.
    let mut card = crate::engine::module_source("card").unwrap();
    card = card.replacen("ability ::=", "ability ::= reuse_settle_case |", 1);
    card.push_str(&format!(
        "\n/* @sem {GAIN_TWO} */\nreuse_settle_case ::= \"settle\" reuse_any_filter \".\"\n\
         reuse_any_filter ::= filter_phrase | adjective_nontype\n"
    ));
    let new = Engine::load_with(&[("card".into(), card.clone())]).unwrap();
    let changes = Changes::between(&old.grammar, &new.grammar, Limits::default());
    let flying = traced(old, "Flying");
    assert_eq!(changes.required_hit(&flying), None);
    assert!(changes.required.iter().all(|(_, rule)| rule == "ability"));
    // A rule no alternative names may be entered directly, so its own alternatives count.
    card.push_str("reuse_orphan_filter ::= filter_phrase | adjective_nontype\n");
    let new = Engine::load_with(&[("card".into(), card)]).unwrap();
    let changes = Changes::between(&old.grammar, &new.grammar, Limits::default());
    assert_eq!(changes.required_hit(&flying), Some("reuse_orphan_filter"));
}

#[test]
fn unchanged_seeds_under_a_changed_child_reprint_nothing() {
    let old = engine();
    // A new keyword-position rule under `keyword_ability`, which already prints Flying.
    let mut keywords = crate::engine::module_source("keywords").unwrap();
    keywords = keywords.replacen(
        "keyword_ability ::=",
        "keyword_ability ::= reuse_settle_case |",
        1,
    );
    keywords.push_str(&format!(
        "\n/* @sem {GAIN_TWO} */\nreuse_settle_case ::= \"settle\" \".\"\n"
    ));
    let new = Engine::load_with(&[("keywords".into(), keywords)]).unwrap();
    let changes = Changes::between(&old.grammar, &new.grammar, Limits::default());
    // Only the new seed can take a value it could not before.
    assert_eq!(changes.verdict(&traced(old, "Flying")), None);
    assert_eq!(
        changes.verdict(&traced(old, "You gain 2 life.")),
        Some("a changed child can fit a looked-up value")
    );
}

#[test]
fn a_new_rule_reprints_only_cards_holding_every_node_it_prints() {
    let old = engine();
    // Same leading steps as "You gain 2 life.", a different amount deep in the body.
    let gain_three = GAIN_TWO.replace("Constant(2)", "Constant(3)");
    let new = with_ability("reuse_settle_case", r#""settle" ".""#, &gain_three);
    let changes = Changes::between(&old.grammar, &new.grammar, Limits::default());
    assert_eq!(changes.verdict(&traced(old, "You gain 2 life.")), None);
    assert_eq!(
        changes.verdict(&traced(old, "You gain 3 life.")),
        Some("a changed child can fit a looked-up value")
    );
}

#[test]
fn a_removed_rule_reprints_only_cards_it_could_have_printed() {
    let old = with_ability("reuse_settle_case", r#""settle" ".""#, GAIN_TWO);
    let new = engine();
    let changes = Changes::between(&old.grammar, &new.grammar, Limits::default());
    assert_eq!(changes.summary.removed, 1);
    assert_eq!(
        changes.verdict(&traced(&old, "You gain 2 life.")),
        Some("a consulted alternative was removed")
    );
    assert_eq!(changes.verdict(&traced(&old, "Flying")), None);
}

#[test]
fn a_removed_rule_the_print_asked_reprints_only_cards_it_could_print() {
    // The printer records a rule it asks before checking what the rule reaches, and
    // the retired rules asked most often start with symbols such as `result:@next`.
    // Without a literal inverse, the meaning must spell the reading's own shape.
    let gain_three = "Ability::Spell{announcement:_{targets:[]}, body:_{instructions:[Effect{result:@next, operation:@sacrificed_result(Operation::GainLife{players:One(AbilityController), amount:Constant(3)})}]}}";
    let old = with_ability("reuse_gain_three", r#""settle" ".""#, gain_three);
    let new = engine();
    let changes = Changes::between(&old.grammar, &new.grammar, Limits::default());
    let removed = changes.old.id("reuse_gain_three").unwrap();
    let asked = |text: &str| {
        let mut saved = traced(&old, text);
        saved.rules.push(removed);
        saved.rules.sort_unstable();
        saved.rules.dedup();
        saved
    };
    assert_eq!(changes.verdict(&asked("You gain 2 life.")), None);
    assert!(changes.verdict(&asked("You gain 3 life.")).is_some());
}

/// Every rule a fresh print under the new grammar consults is in the carried trace.
fn assert_carried_trace_covers_a_fresh_print(old: &Engine, new: &Engine, text: &str) {
    let changes = Changes::between(&old.grammar, &new.grammar, Limits::default());
    let saved = traced(old, text);
    assert_eq!(changes.verdict(&saved), None);
    let carried = changes.renumber(&saved).unwrap();
    let fresh = traced(new, text);
    let missing: Vec<&str> = fresh
        .rules
        .iter()
        .filter(|rule| !has(&carried.rules, **rule))
        .map(|rule| changes.new.key(*rule))
        .collect();
    assert!(missing.is_empty(), "{missing:?}");
}

#[test]
fn a_carried_trace_holds_every_rule_a_fresh_print_consults() {
    let gain_three = GAIN_TWO.replace("Constant(2)", "Constant(3)");
    let added = with_ability("reuse_settle_case", r#""settle" ".""#, &gain_three);
    assert_carried_trace_covers_a_fresh_print(engine(), &added, "You gain 2 life.");
    let asked = "Ability::Spell{announcement:_{targets:[]}, body:_{instructions:[Effect{result:@next, operation:@sacrificed_result(Operation::GainLife{players:One(AbilityController), amount:Constant(3)})}]}}";
    let added = with_ability("reuse_gain_three", r#""settle" ".""#, asked);
    assert_carried_trace_covers_a_fresh_print(engine(), &added, "You gain 2 life.");
    assert_carried_trace_covers_a_fresh_print(&added, engine(), "You gain 2 life.");
    // A splice unifies with any list, so the printer asks the child it binds even
    // where the derivation prints nothing.
    let spliced = "Ability::Spell{announcement:_{targets:[]}, body:_{instructions:[...$instructions_exile_top_cast_remainder]}}";
    let added = with_ability(
        "reuse_spliced_case",
        r#"instructions_exile_top_cast_remainder "settle""#,
        spliced,
    );
    assert_carried_trace_covers_a_fresh_print(engine(), &added, "You gain 2 life.");
}

#[test]
fn a_broad_new_alternative_needs_the_words_around_its_rule() {
    let grammar = |who: &str| {
        let mut card = crate::engine::module_source("card").unwrap();
        card = card.replacen("ability ::=", "ability ::= reuse_settle_case |", 1);
        card.push_str(&format!(
            "\n/* @sem {GAIN_TWO} */\nreuse_settle_case ::= reuse_who \"settles\" \".\"\n\
             reuse_who ::= {who}\nreuse_broad ::= reuse_alpha | reuse_beta\n\
             /* @sem PlayerRef::AbilityController */\nreuse_you ::= \"you\"\n\
             /* @sem PlayerRef::AbilityController */\nreuse_alpha ::= \"alpha\"\n\
             /* @sem PlayerRef::AbilityController */\nreuse_beta ::= \"beta\"\n"
        ));
        Engine::load_with(&[("card".into(), card)]).unwrap()
    };
    let old = grammar("reuse_you");
    let new = grammar("reuse_you | reuse_broad");
    let changes = Changes::between(&old.grammar, &new.grammar, Limits::default());
    let saved = |text: &[&str]| {
        let keys: Vec<u32> = text.iter().map(|word| word_key(word)).collect();
        let mut words = keys.clone();
        words.sort_unstable();
        words.dedup();
        let mut pairs: Vec<u32> = keys.windows(2).map(|w| pair_key(w[0], w[1])).collect();
        pairs.sort_unstable();
        pairs.dedup();
        Saved {
            words,
            pairs,
            ..Saved::default()
        }
    };
    // The broad rule's derivations share no word, but a reading through the new
    // alternative holds one of them just before "settles".
    assert_eq!(changes.required_hit(&saved(&["beta", "flying", "."])), None);
    assert_eq!(
        changes.required_hit(&saved(&["alpha", "flying", "settles", "."])),
        None
    );
    assert_eq!(
        changes.required_hit(&saved(&["beta", "settles", "."])),
        Some("reuse_who")
    );
}

#[test]
fn a_changed_label_reprints_only_cards_its_owner_could_print() {
    // A literal group means `()`, and every child bound to nothing prints from null,
    // so the group fits nearly every print; the rule around it does not. A rule with
    // a group has no literal inverse, so its meaning spells the reading's own shape.
    let gain_two = "Ability::Spell{announcement:_{targets:[]}, body:_{instructions:[Effect{result:@next, operation:GainLife{players:One(AbilityController), amount:Constant(2)}}]}}";
    let grammar = |labels: &str| {
        with_ability(
            "reuse_labelled_case",
            &format!(r#"({labels}) "settles" ".""#),
            gain_two,
        )
    };
    let old = grammar(r#""alpha" | "beta""#);
    let new = grammar(r#""alpha" | "beta" | "gamma""#);
    let changes = Changes::between(&old.grammar, &new.grammar, Limits::default());
    let label = changes.old.id("reuse_labelled_case#1").unwrap();
    let asked = |text: &str| {
        let mut saved = traced(&old, text);
        saved.rules.push(label);
        saved.rules.sort_unstable();
        saved.rules.dedup();
        saved
    };
    assert_eq!(changes.verdict(&asked("Flying")), None);
    assert_eq!(
        changes.verdict(&asked("You gain 2 life.")),
        Some("a consulted rule changed")
    );
}

#[test]
fn the_most_telling_weak_child_is_spelled_out_first() {
    let grammar = |who: &str| {
        let mut card = crate::engine::module_source("card").unwrap();
        card = card.replacen("ability ::=", "ability ::= reuse_limit_case |", 1);
        card.push_str(&format!(
            "\n/* @sem {GAIN_TWO} */\nreuse_limit_case ::= {who} reuse_limit \".\"\n\
             reuse_who ::= reuse_you | reuse_broad\nreuse_broad ::= reuse_alpha | reuse_beta\n\
             reuse_limit ::= reuse_venture | reuse_attack\n\
             /* @sem PlayerRef::AbilityController */\nreuse_you ::= \"you\"\n\
             /* @sem PlayerRef::AbilityController */\nreuse_alpha ::= \"alpha\"\n\
             /* @sem PlayerRef::AbilityController */\nreuse_beta ::= \"beta\"\n\
             /* @sem PlayerRef::AbilityController */\nreuse_venture ::= \"venture\" \"twice\"\n\
             /* @sem PlayerRef::AbilityController */\nreuse_attack ::= \"attack\" \"alone\"\n"
        ));
        Engine::load_with(&[("card".into(), card)]).unwrap()
    };
    let old = grammar("reuse_you");
    let new = grammar("reuse_who");
    let changes = Changes::between(&old.grammar, &new.grammar, Limits::default());
    let saved = |text: &[&str]| {
        let keys: Vec<u32> = text.iter().map(|word| word_key(word)).collect();
        let mut words = keys.clone();
        words.sort_unstable();
        words.dedup();
        let mut pairs: Vec<u32> = keys.windows(2).map(|w| pair_key(w[0], w[1])).collect();
        pairs.sort_unstable();
        pairs.dedup();
        Saved {
            words,
            pairs,
            ..Saved::default()
        }
    };
    // Spelling out who first would leave forms holding only "you" and ".".
    assert_eq!(changes.required_hit(&saved(&["you", "flying", "."])), None);
    assert_eq!(
        changes.required_hit(&saved(&["beta", "attack", "alone", "."])),
        Some("reuse_limit_case")
    );
}

#[test]
fn a_consulted_rule_under_a_new_alternative_reprints_only_cards_it_could_print() {
    let gain_three = "Ability::Spell{announcement:_{targets:[]}, body:_{instructions:[Effect{result:@next, operation:GainLife{players:One(AbilityController), amount:Constant(3)}}]}}";
    let grammar = |pass: &str| {
        let mut card = crate::engine::module_source("card").unwrap();
        card = card.replacen("ability ::=", "ability ::= reuse_pass | reuse_other |", 1);
        card.push_str(&format!(
            "\nreuse_pass ::= {pass}\n/* @sem {gain_three} */\nreuse_settle ::= \"settle\" \".\"\n\
             /* @sem {gain_three} */\nreuse_other ::= \"rest\" \".\"\n"
        ));
        Engine::load_with(&[("card".into(), card)]).unwrap()
    };
    let old = grammar("reuse_settle");
    let new = grammar("reuse_settle | reuse_other");
    let changes = Changes::between(&old.grammar, &new.grammar, Limits::default());
    let other = changes.old.id("reuse_other").unwrap();
    let pass = changes.old.id("reuse_pass").unwrap();
    let mut saved = traced(&old, "You gain 2 life.");
    saved.rules.extend([other, pass]);
    saved.rules.sort_unstable();
    saved.rules.dedup();
    // The new alternative reaches a seed the print asked elsewhere, which cannot
    // print any value the card looked up.
    assert_eq!(changes.verdict(&saved), None);
}
