//! A bounded print path for an ordinary trigger with separate guarded sentences.
use super::{Engine, FaceMemo, Names, Renderer, Sym, canonical_abilities, tokens};
use serde_json::Value;

/// The clauses in this form can each be rendered from one instruction. The
/// trigger's first clause needs the postposed-condition production because a
/// leading condition after the event would be read as intervening.
fn guarded_body(abilities: &Value) -> Option<(&Value, &[Value])> {
    let [ability] = abilities.as_array()?.as_slice() else {
        return None;
    };
    let args = ability.get("args")?;
    let event = args.get("event")?.get("ZoneChange")?;
    if ability["kind"] != "Triggered"
        || !args["intervening_if"].is_null()
        || !args["announcement"]["modes"].is_null()
        || !args["announcement"]["targets"].as_array()?.is_empty()
        || !args["limits"]["per_turn"].is_null()
        || !args["limits"]["total"].is_null()
        || !event["from"].is_null()
        || event["to"] != "Battlefield"
        || event["objects"]["kind"] != "SameObject"
        || event["objects"]["args"]["kind"] != "Source"
    {
        return None;
    }
    let instructions = args["body"]["instructions"].as_array()?;
    if !(3..=8).contains(&instructions.len()) || !instructions.iter().all(simple_guard) {
        return None;
    }
    Some((&args["event"], instructions))
}

fn simple_guard(instruction: &Value) -> bool {
    let branch = &instruction["args"];
    let condition = &branch["condition"];
    let Some([action]) = branch["then"]["instructions"].as_array().map(Vec::as_slice) else {
        return false;
    };
    instruction["kind"] == "If"
        && branch["otherwise"]["instructions"]
            .as_array()
            .is_some_and(Vec::is_empty)
        && condition["kind"] == "ExistsPlayer"
        && condition["args"]["players"]["kind"] == "OpponentsOf"
        && condition["args"]["players"]["args"]["kind"] == "AbilityController"
        && condition["args"]["condition"]["kind"] == "Compare"
        && action["kind"] == "Effect"
}

impl Engine {
    pub(super) fn print_guarded_trigger(
        &self,
        abilities: &Value,
        card: (&str, &[String]),
        memo: Option<&mut FaceMemo>,
    ) -> Option<String> {
        guarded_body(abilities)?;
        let wanted = canonical_abilities(abilities).ok()?;
        let (event, instructions) = guarded_body(&wanted)?;
        let text = self.emit_guarded_trigger(card, event, instructions)?;
        self.reads_as_memo(memo, card, &text, &wanted)
            .then_some(text)
    }

    fn emit_guarded_trigger(
        &self,
        (name, types): (&str, &[String]),
        event: &Value,
        instructions: &[Value],
    ) -> Option<String> {
        let rule = |name| self.grammar.names.get(name).copied();
        let event_when = rule("event_when")?;
        let first_clause = rule("clause_if_after")?;
        let later_clause = rule("clause_if")?;
        // Use the join terminals declared for a plain trigger and its program.
        let [Sym::Rule(_), Sym::Mark(join), Sym::Rule(_)] = self.grammar.rules
            [rule("triggered_ability")? as usize]
            .alternatives
            .first()?
            .syms
            .as_slice()
        else {
            return None;
        };
        let [Sym::Rule(_), Sym::Mark(end)] = self.grammar.rules
            [rule("program_sentence_plain")? as usize]
            .alternatives
            .first()?
            .syms
            .as_slice()
        else {
            return None;
        };
        let names = Names::for_card(name, types);
        let mut renderer = Renderer::new(&self.grammar, &self.style, 4, types.to_vec())
            .with_name(name)
            .with_self_name(names.printed(types));
        let header = renderer.render(event_when, event);
        let mut printed = renderer.tokens_of(header.first()?);
        printed.push(tokens::Token::Mark(*join));
        for (index, instruction) in instructions.iter().enumerate() {
            let goal = if index == 0 {
                first_clause
            } else {
                later_clause
            };
            let clause = renderer.render(goal, &Value::Array(vec![instruction.clone()]));
            printed.extend(renderer.tokens_of(clause.first()?));
            printed.push(tokens::Token::Mark(*end));
        }
        Some(tokens::detokenize(
            &tokens::agree_articles(&printed),
            &names,
        ))
    }
}
