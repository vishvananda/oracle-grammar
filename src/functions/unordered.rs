//! A spelling-only inverse for explicitly annotated commutative fixed lists.
//! Forward evaluation is identity: this module never sorts emitted IR.
use crate::pattern::{Bindings, Item, Pattern};
use serde_json::Value;

const MAX_WORK: usize = 4096;
const MAX_CANDIDATES: usize = 128;

pub(crate) fn unify<'p, 'v>(
    args: &'p [Pattern],
    value: &'v Value,
    bindings: Bindings<'p, 'v>,
) -> Vec<Bindings<'p, 'v>> {
    let [inner @ Pattern::List(pieces)] = args else {
        return Vec::new();
    };
    let Value::Array(values) = value else {
        return Vec::new();
    };
    if !(2..=4).contains(&pieces.len())
        || pieces
            .iter()
            .filter(|piece| matches!(piece, Item::Splice(..)))
            .count()
            > 1
        || super::branch_bounds(value).is_err()
    {
        return Vec::new();
    }
    // The annotation never removes the old positional inverse for longer
    // spliced lists. Only the additional commutative search is capped at four.
    if !(2..=4).contains(&values.len()) {
        return inner.unify(value, bindings);
    }
    // Enumerate every bijection of the actual list, then reuse ordinary list
    // matching for all holes and the optional single splice. The splice's
    // captured sequence is never sorted independently.
    let mut permutations = vec![Vec::new()];
    for _ in values {
        let mut next = Vec::new();
        for prefix in permutations {
            for index in 0..values.len() {
                if !prefix.contains(&index) {
                    let mut extended = prefix.clone();
                    extended.push(index);
                    next.push(extended);
                }
            }
        }
        permutations = next;
    }
    let mut solutions = Vec::new();
    let mut work = 0;
    for permutation in permutations {
        work += 1;
        if work > MAX_WORK {
            return Vec::new();
        }
        let ordered = Value::Array(
            permutation
                .into_iter()
                .map(|index| values[index].clone())
                .collect(),
        );
        for found in inner.unify_made(&ordered, &bindings) {
            work += 1;
            // Reject the entire operation on overflow, never select a reading.
            if work > MAX_WORK || solutions.len() == MAX_CANDIDATES {
                return Vec::new();
            }
            solutions.push(found);
        }
    }
    solutions
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::ir_schema::{Schema, Ty};
    use crate::pattern::{Syntax, elaborate, parse};
    use serde_json::json;

    fn pattern(text: &str) -> Pattern {
        elaborate(
            &parse(text).unwrap().1,
            &Ty::Vec(Box::new(Ty::Named("ObjectFilter".into()))),
            &Schema::of_ir().unwrap(),
        )
        .unwrap()
    }
    #[test]
    fn forward_keeps_exact_list_and_inverse_reads_permuted_current_holes() {
        let p = pattern("@unordered([CardType(Creature), $other])");
        let input =
            json!([{"kind":"State","args":"Enchanted"},{"kind":"CardType","args":"Creature"}]);
        let b = p.unify(&input, Bindings::new());
        assert_eq!(b.len(), 1);
        assert_eq!(b[0].get("other"), Some(&input[0]));
        assert_eq!(p.fill(&b[0], 0).unwrap(), json!([input[1], input[0]]));
        let edited =
            json!([{"kind":"CardType","args":"Artifact"},{"kind":"CardType","args":"Creature"}]);
        let b = p.unify(&edited, Bindings::new());
        assert_eq!(b[0].get("other"), Some(&edited[0]));
    }
    #[test]
    fn all_bijections_and_duplicate_multiplicity_are_preserved() {
        for n in 2..=4 {
            let args = (0..n)
                .map(|i| format!("$h{i}"))
                .collect::<Vec<_>>()
                .join(",");
            let p = pattern(&format!("@unordered([{args}])"));
            let values = Value::Array((0..n).map(|i| json!({"kind":"CardType","args":(["Creature","Artifact","Land","Enchantment"][i])})).collect());
            assert_eq!(
                p.unify(&values, Bindings::new()).len(),
                (1..=n).product::<usize>()
            );
        }
        let p = pattern("@unordered([CardType(Creature), CardType(Creature)])");
        assert!(p.unify(&json!([{"kind":"CardType","args":"Creature"},{"kind":"CardType","args":"Artifact"}]), Bindings::new()).is_empty());
        assert!(
            p.unify(
                &json!([{"kind":"CardType","args":"Creature"}]),
                Bindings::new()
            )
            .is_empty()
        );
    }
    #[test]
    fn one_splice_retains_every_unmatched_member_and_its_chosen_sequence() {
        let p = pattern("@unordered([CardType(Creature), ...$rest])");
        let values = json!([{"kind":"CardType","args":"Artifact"},
            {"kind":"CardType","args":"Creature"},{"kind":"CardType","args":"Land"}]);
        let found = p.unify(&values, Bindings::new());
        assert_eq!(found.len(), 2);
        for b in &found {
            let rest = b.get("rest").unwrap().as_array().unwrap();
            assert_eq!(rest.len(), 2);
            assert!(rest.contains(&values[0]) && rest.contains(&values[2]));
            let printed = p.fill(b, 0).unwrap();
            assert_eq!(printed[0], values[1]);
            assert_eq!(&printed.as_array().unwrap()[1..], rest);
        }
    }
    #[test]
    fn contract_rejects_wrong_type_arity_splice_and_unbounded_length() {
        let schema = Schema::of_ir().unwrap();
        let ty = Ty::Vec(Box::new(Ty::Named("ObjectFilter".into())));
        for text in [
            "@unordered()",
            "@unordered([Any])",
            "@unordered([Any,Any,Any,Any,Any])",
            "@unordered([...$x,...$y])",
        ] {
            assert!(
                elaborate(&parse(text).unwrap().1, &ty, &schema).is_err(),
                "{text}"
            );
        }
        let syntax = Syntax::Symbol(
            "unordered".into(),
            vec![Syntax::List(vec![Syntax::Int(1), Syntax::Int(2)])],
        );
        assert!(elaborate(&syntax, &Ty::Named("Expr".into()), &schema).is_err());
    }
    #[test]
    fn candidate_overflow_refuses_whole_inverse_and_duplicates_are_not_dropped() {
        let p = pattern("@unordered([@and($left,$right), ...$rest])");
        let large = json!({"kind":"AllOf","args":(0..10).map(|_| json!({"kind":"CardType","args":"Creature"})).collect::<Vec<_>>()});
        assert!(
            p.unify(
                &json!([large,{"kind":"CardType","args":"Land"}]),
                Bindings::new()
            )
            .is_empty()
        );
        let p = pattern("@unordered([CardType(Creature),CardType(Creature)])");
        assert_eq!(p.unify(&json!([{"kind":"CardType","args":"Creature"},{"kind":"CardType","args":"Creature"}]), Bindings::new()).len(), 2);
    }
    #[test]
    fn long_spliced_forward_lists_keep_the_old_positional_inverse() {
        let p = pattern("@unordered([CardType(Creature), ...$rest])");
        let values = Value::Array(
            (0..5)
                .map(|_| json!({"kind":"CardType","args":"Creature"}))
                .collect(),
        );
        let found = p.unify(&values, Bindings::new());
        assert_eq!(found.len(), 1);
        assert_eq!(p.fill(&found[0], 0).unwrap(), values);
    }
    #[test]
    fn input_bounds_fail_closed_before_matching_or_cloning() {
        let p = pattern("@unordered([$a,$b])");
        let mut deep = json!(null);
        for _ in 0..130 {
            deep = json!([deep]);
        }
        assert!(
            p.unify(&json!([deep,{"kind":"Any"}]), Bindings::new())
                .is_empty()
        );
    }
}
