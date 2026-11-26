use crate::{
    grammar,
    model::ir_schema::Schema,
    parse::{Limits, Parser, RuleSet},
    tokens::{self, Names},
};
use serde_json::Value;
use std::rc::Rc;

const SYNTAX: &str = r#"
root ::= literal | marker | bad
/* @sem Expr::ObjectProperty{object:Source, property:Power, information:CurrentOrLastKnown} */
literal ::= "alpha" | "alpha" "23" | "alpha" "23" NEWLINE "again" | SELF_NAME "23"
/* @sem Expr::BoundValue(@prior) */
marker ::= "alpha" "23"
/* @sem Expr::Constant(7) */
bad ::= "alpha" "23" "missing"
"#;

#[derive(Debug, PartialEq)]
struct ResultAtRoot {
    values: Result<Vec<Rc<Value>>, String>,
    counters: [u64; 4],
    furthest: usize,
    expected: Vec<String>,
}

fn read(grammar: &grammar::Grammar, text: &str, limits: Limits, mode: usize) -> ResultAtRoot {
    let tokens = tokens::tokenize(text, &Names::of("Title"), &grammar.vocabulary).unwrap();
    let off = RuleSet::from_iter(grammar.names.get("bad").copied());
    let mut parser = Parser::new(grammar, &tokens, limits);
    parser = match mode {
        1 => parser.with_diagnostics(),
        2 => parser.with_audit(),
        3 => parser.without(&off),
        _ => parser,
    };
    let values = parser.whole(grammar.names["root"]);
    let counters = &parser.counters;
    ResultAtRoot {
        values,
        counters: [
            counters.steps,
            counters.rule_calls,
            counters.memo_hits,
            counters.first_prunes,
        ],
        furthest: parser.furthest,
        expected: parser.expected.clone(),
    }
}

#[test]
fn compiled_literal_reads_preserve_all_values_work_and_refusals() {
    let lexical = include_str!("../../grammar/lexical.ebnf");
    let schema = Schema::of_ir().unwrap();
    let mut grammar = grammar::load(lexical, &[("synthetic", SYNTAX)], &schema).unwrap();
    let literal = grammar.names["literal"];
    let marker = grammar.names["marker"];
    assert!(
        grammar
            .literal_reads
            .value(literal, &grammar.rules[literal as usize])
            .is_some()
    );
    assert!(
        grammar
            .literal_reads
            .value(marker, &grammar.rules[marker as usize])
            .is_none()
    );
    let mut cases = Vec::new();
    for text in [
        "Alpha",
        "Alpha 23",
        "Alpha 23\nagain",
        "Title 23",
        "Alpha 23 missing",
        "Alpha 23 extra",
    ] {
        for mode in 0..4 {
            for limits in [
                Limits::default(),
                Limits {
                    readings: 1,
                    ..Limits::default()
                },
                Limits {
                    steps: 4,
                    ..Limits::default()
                },
                Limits {
                    depth: 0,
                    ..Limits::default()
                },
            ] {
                cases.push((text, limits, mode, read(&grammar, text, limits, mode)));
            }
        }
    }
    assert_eq!(
        read(&grammar, "Alpha 23", Limits::default(), 0)
            .values
            .unwrap()
            .len(),
        2
    );
    grammar.literal_reads = Default::default();
    grammar.terminal_prefixes = Default::default();
    for (text, limits, mode, compiled) in cases {
        assert_eq!(
            compiled,
            read(&grammar, text, limits, mode),
            "{text:?} {limits:?} mode {mode}"
        );
    }
}

#[test]
fn compiled_prefixes_preserve_mixed_rules_and_step_budget_boundaries() {
    let syntax = r#"
root ::= mixed | literal
/* @sem Expr::Constant($number) */
mixed ::= "alpha" "between" "again" NUMBER | "alpha" "between" "else" NUMBER | "alpha" "between" "again" NEWLINE NUMBER
/* @sem Expr::Constant(7) */
literal ::= "alpha" "between" "again" "01" | "alpha" "between" "again" "1" | SELF_NAME "between" "1000"
"#;
    let lexical = include_str!("../../grammar/lexical.ebnf");
    let schema = Schema::of_ir().unwrap();
    let mut grammar = grammar::load(lexical, &[("synthetic", syntax)], &schema).unwrap();
    let mixed = grammar.names["mixed"];
    let spans = grammar.terminal_prefixes.rule(mixed);
    assert_eq!(grammar.terminal_prefixes.path(spans, 0).len(), 3);
    assert_eq!(grammar.terminal_prefixes.path(spans, 2).len(), 4);
    let mut cases = Vec::new();
    for text in [
        "Alpha between again 1",
        "Alpha between else 7",
        "Alpha between again\n8",
        "Alpha between again",
        "Alpha between other 7",
        "Title between 1000",
        "Title between 1,000",
    ] {
        for steps in 0..=25 {
            let limits = Limits {
                steps,
                ..Limits::default()
            };
            for mode in 0..4 {
                cases.push((text, limits, mode, read(&grammar, text, limits, mode)));
            }
        }
    }
    assert_eq!(
        read(&grammar, "Alpha between again 1", Limits::default(), 0)
            .values
            .unwrap()
            .len(),
        2
    );
    grammar.terminal_prefixes = Default::default();
    for (text, limits, mode, compiled) in cases {
        assert_eq!(
            compiled,
            read(&grammar, text, limits, mode),
            "{text:?} {limits:?} mode {mode}"
        );
    }
}
