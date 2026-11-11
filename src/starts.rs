//! Which alternatives can start at a token. A rule with thousands of alternatives tries
//! only those whose first token is the one in front of it, as a table compiled when the
//! grammar loads. Left recursion is refused, so rules depending on the rules that start
//! them form no cycle, and one pass in dependency order solves every FIRST set.
use crate::grammar::{Class, Rule, RuleId, Sym};
use crate::tokens::Token;
use rustc_hash::FxHashMap as HashMap;
use std::rc::Rc;

const LOWER: usize = 0;
const UPPER: usize = 1;
const NUMBER: usize = 2;
const MARK: usize = 3;
const SELF_NAME: usize = 4;
const NEWLINE: usize = 5;
const END: usize = 6;
const CLASSES: usize = 7;
/// An alternative that can derive nothing starts at any token, or at none.
const EVERY_CLASS: u8 = (1 << CLASSES) - 1;

/// The token at a position, as a table reads it.
#[derive(Debug, Clone, Copy)]
pub struct Look {
    key: Option<u32>,
    class: usize,
}

/// The tokens some rule starts with: terminals by key, and open classes as a mask.
#[derive(Debug, Default)]
struct Set {
    keys: Vec<u32>,
    open: u8,
}

/// One rule's alternatives by the token they can start at, each list in grammar order.
#[derive(Debug, Default)]
struct Table {
    /// Sorted by key, each with its list.
    named: Vec<(u32, u32)>,
    /// For each class, the list for a token no alternative names.
    open: [u32; CLASSES],
    /// Many keys share one list, so each distinct list is held once.
    lists: Vec<Box<[u32]>>,
}

#[derive(Debug, Default)]
pub struct Starts {
    words: HashMap<String, u32>,
    numbers: HashMap<u32, u32>,
    marks: HashMap<char, u32>,
    /// The class of each key.
    classes: Vec<u8>,
    tables: Vec<Table>,
}

fn class_mask(class: Class) -> u8 {
    match class {
        Class::Number => 1 << NUMBER,
        Class::Word => 1 << LOWER | 1 << UPPER,
        Class::Capitalized => 1 << UPPER,
        Class::SelfName => 1 << SELF_NAME,
        Class::Newline => 1 << NEWLINE,
    }
}

fn word_class(word: &str) -> usize {
    if word.starts_with(char::is_uppercase) {
        UPPER
    } else {
        LOWER
    }
}

impl Starts {
    fn new_key(&mut self, class: usize) -> u32 {
        let key = u32::try_from(self.classes.len()).unwrap_or(u32::MAX);
        self.classes.push(u8::try_from(class).unwrap_or(0));
        key
    }

    /// The keys a terminal matches: a numeral is a word in the grammar, a number in a text.
    fn terminal_keys(&mut self, sym: &Sym, out: &mut Vec<u32>) {
        match sym {
            Sym::Word(word) => {
                if !self.words.contains_key(word) {
                    let key = self.new_key(word_class(word));
                    self.words.insert(word.clone(), key);
                }
                out.push(self.words[word]);
                if let Ok(number) = word.parse::<u32>() {
                    if !self.numbers.contains_key(&number) {
                        let key = self.new_key(NUMBER);
                        self.numbers.insert(number, key);
                    }
                    out.push(self.numbers[&number]);
                }
            }
            Sym::Mark(mark) => {
                if !self.marks.contains_key(mark) {
                    let key = self.new_key(MARK);
                    self.marks.insert(*mark, key);
                }
                out.push(self.marks[mark]);
            }
            Sym::Class(_) | Sym::Rule(_) => {}
        }
    }

    /// The tokens an alternative starts with, and whether it can derive nothing.
    fn leading(
        &mut self,
        syms: &[Sym],
        nullable: &[bool],
        sets: &[Option<Rc<Set>>],
    ) -> (Vec<u32>, u8, bool) {
        let (mut keys, mut open) = (Vec::new(), 0);
        for sym in syms {
            match sym {
                Sym::Class(class) => open |= class_mask(*class),
                Sym::Rule(rule) => {
                    if let Some(set) = &sets[*rule as usize] {
                        keys.extend_from_slice(&set.keys);
                        open |= set.open;
                    }
                    if nullable[*rule as usize] {
                        continue;
                    }
                }
                terminal => self.terminal_keys(terminal, &mut keys),
            }
            return (keys, open, false);
        }
        (keys, open, true)
    }

    /// Solve `rule` after the rules that start it.
    fn solve(
        &mut self,
        rule: RuleId,
        rules: &[Rule],
        nullable: &[bool],
        sets: &mut [Option<Rc<Set>>],
    ) {
        if sets[rule as usize].is_some() {
            return;
        }
        let mut set = Set::default();
        for alternative in &rules[rule as usize].alternatives {
            for sym in &alternative.syms {
                let Sym::Rule(child) = sym else { break };
                self.solve(*child, rules, nullable, sets);
                if !nullable[*child as usize] {
                    break;
                }
            }
            let (keys, open, _) = self.leading(&alternative.syms, nullable, sets);
            set.keys.extend(keys);
            set.open |= open;
        }
        set.keys.sort_unstable();
        set.keys.dedup();
        sets[rule as usize] = Some(Rc::new(set));
    }

    fn table(&mut self, rule: &Rule, nullable: &[bool], sets: &[Option<Rc<Set>>]) -> Table {
        let mut by_key: HashMap<u32, Vec<u32>> = HashMap::default();
        let mut open: [Vec<u32>; CLASSES] = Default::default();
        for (index, alternative) in (0u32..).zip(&rule.alternatives) {
            let (mut keys, mask, empty) = self.leading(&alternative.syms, nullable, sets);
            keys.sort_unstable();
            keys.dedup();
            for key in keys {
                by_key.entry(key).or_default().push(index);
            }
            let mask = if empty { EVERY_CLASS } else { mask };
            for (class, list) in open.iter_mut().enumerate() {
                if mask & (1 << class) != 0 {
                    list.push(index);
                }
            }
        }
        let mut table = Table::default();
        let mut held: HashMap<Vec<u32>, u32> = HashMap::default();
        let mut intern = |list: Vec<u32>, lists: &mut Vec<Box<[u32]>>| -> u32 {
            *held.entry(list).or_insert_with_key(|list| {
                lists.push(list.as_slice().into());
                u32::try_from(lists.len() - 1).unwrap_or(u32::MAX)
            })
        };
        for (class, list) in open.iter().enumerate() {
            table.open[class] = intern(list.clone(), &mut table.lists);
        }
        for (key, mut list) in by_key {
            list.extend_from_slice(&open[usize::from(self.classes[key as usize])]);
            list.sort_unstable();
            list.dedup();
            table.named.push((key, intern(list, &mut table.lists)));
        }
        table.named.sort_unstable();
        table
    }

    pub fn build(rules: &[Rule], nullable: &[bool]) -> Self {
        let mut starts = Self::default();
        let mut sets = vec![None; rules.len()];
        for rule in 0..rules.len() {
            starts.solve(
                u32::try_from(rule).unwrap_or(u32::MAX),
                rules,
                nullable,
                &mut sets,
            );
        }
        starts.tables = rules
            .iter()
            .map(|rule| starts.table(rule, nullable, &sets))
            .collect();
        starts
    }

    /// How a table reads this token, or the end of the text.
    pub fn look(&self, token: Option<&Token>) -> Look {
        match token {
            None => Look {
                key: None,
                class: END,
            },
            Some(Token::Word(word)) => Look {
                key: self.words.get(word).copied(),
                class: word_class(word),
            },
            Some(Token::Number(number)) => Look {
                key: self.numbers.get(number).copied(),
                class: NUMBER,
            },
            Some(Token::Mark(mark)) => Look {
                key: self.marks.get(mark).copied(),
                class: MARK,
            },
            Some(Token::SelfName(_)) => Look {
                key: None,
                class: SELF_NAME,
            },
            Some(Token::Newline) => Look {
                key: None,
                class: NEWLINE,
            },
        }
    }

    /// The alternatives of `rule` that can start at this token, in grammar order.
    pub fn candidates(&self, rule: RuleId, look: Look) -> &[u32] {
        let table = &self.tables[rule as usize];
        let named = look.key.and_then(|key| {
            let found = table.named.binary_search_by_key(&key, |(key, _)| *key);
            found.ok().map(|at| table.named[at].1)
        });
        let list = named.unwrap_or(table.open[look.class]);
        &table.lists[list as usize]
    }
}
