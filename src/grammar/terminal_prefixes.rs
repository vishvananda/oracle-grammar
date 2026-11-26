//! Compile shared, binding-free terminal prefixes after lexical validation.
use super::{Alternative, Class, Rule, RuleId, Sym};
use crate::starts::{Look, PlainTerminal, Starts};
use rustc_hash::FxHashMap as HashMap;
use smallvec::SmallVec;

const DEPTH: usize = 32;
const COMPILE_WORK: usize = 262_144;

#[derive(Debug, Default, Clone, Copy)]
pub(crate) struct Span {
    start: u32,
    len: u8,
}

#[derive(Debug, Default)]
pub(crate) struct TerminalPrefixes {
    rules: Vec<Box<[Span]>>,
    paths: Vec<u32>,
    terminals: Vec<PlainTerminal>,
}

impl TerminalPrefixes {
    pub(super) fn build(rules: &[Rule], starts: &mut Starts) -> Self {
        let mut trie = Self::default();
        let mut edges = HashMap::default();
        let mut remaining = COMPILE_WORK;
        for rule in rules {
            if rule.alternatives.len() > remaining {
                break;
            }
            remaining -= rule.alternatives.len();
            let mut spans = Vec::new();
            for (index, alt) in rule.alternatives.iter().enumerate() {
                let len = prefix_len(rule, alt, remaining.min(DEPTH));
                remaining -= len;
                if len < 2 {
                    continue;
                }
                if spans.is_empty() {
                    spans.resize(rule.alternatives.len(), Span::default());
                }
                spans[index] = trie
                    .insert(&alt.syms[..len], starts, &mut edges)
                    .unwrap_or_default();
            }
            trie.rules.push(spans.into_boxed_slice());
        }
        trie
    }

    fn insert(
        &mut self,
        syms: &[Sym],
        starts: &mut Starts,
        edges: &mut HashMap<(Option<u32>, PlainTerminal), u32>,
    ) -> Option<Span> {
        let mut parent = None;
        let mut path = SmallVec::<[u32; DEPTH]>::new();
        for sym in syms {
            let expected = starts.plain_terminal(sym)?;
            let key = (parent, expected);
            let next = if let Some(&node) = edges.get(&key) {
                node
            } else {
                let node = u32::try_from(self.terminals.len()).ok()?;
                self.terminals.push(expected);
                edges.insert(key, node);
                node
            };
            path.push(next);
            parent = Some(next);
        }
        let span = Span {
            start: u32::try_from(self.paths.len()).ok()?,
            len: u8::try_from(path.len()).ok()?,
        };
        self.paths.extend(path);
        Some(span)
    }

    pub(crate) fn rule(&self, rule: RuleId) -> &[Span] {
        self.rules.get(rule as usize).map_or(&[], Box::as_ref)
    }

    pub(crate) fn path(&self, spans: &[Span], alternative: u32) -> &[u32] {
        let Some(span) = spans.get(alternative as usize) else {
            return &[];
        };
        let start = span.start as usize;
        &self.paths[start..start + usize::from(span.len)]
    }

    pub(crate) fn matches(&self, node: u32, look: Look) -> bool {
        self.terminals[node as usize].matches(look)
    }
}

fn prefix_len(rule: &Rule, alt: &Alternative, limit: usize) -> usize {
    alt.syms
        .iter()
        .zip(&alt.holes)
        .take(limit)
        .take_while(|(sym, hole)| {
            matches!(
                sym,
                Sym::Word(_) | Sym::Mark(_) | Sym::Class(Class::Newline)
            ) || (**sym == Sym::Class(Class::SelfName) && rule.sem.is_some() && hole.is_none())
        })
        .count()
}
