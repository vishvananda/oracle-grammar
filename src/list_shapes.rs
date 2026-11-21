//! Static list-length and element-tag bounds derived from `@sem` declarations.
//!
//! Every bound is a sound over-approximation computed by monotone fixed-point
//! iteration over the grammar's rules and print inverses. Pruning list rules
//! and splice splits against these bounds drops only candidates whose children
//! could never unify, preserving every derivation and order.
use crate::grammar::{Rule, RuleId, Sym};
use crate::pattern::{Item, Pattern, passes_through};
use rustc_hash::FxHashMap as HashMap;
use serde_json::Value;
use smallvec::SmallVec;

const TAG_NON_OBJ: u16 = 0;
const TAG_OTHER_OBJ: u16 = 1;
const TAG_SYM: u16 = 2;
const TAG_EFFECT_SYM: u16 = 3;
const FIRST_INTERNED: u16 = 4;
const TAG_UNSEEN_EFFECT: u16 = 510;
const TAG_UNSEEN_OTHER: u16 = 511;

/// 512-bit set of element tags.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub(crate) struct TagMask([u64; 8]);

impl TagMask {
    pub(crate) const EMPTY: Self = Self([0; 8]);
    pub(crate) const ALL: Self = Self([u64::MAX; 8]);

    #[inline]
    pub(crate) fn single(bit: u16) -> Self {
        let mut words = [0u64; 8];
        words[(bit >> 6) as usize] = 1u64 << (bit & 63);
        Self(words)
    }

    #[inline]
    pub(crate) fn insert(&mut self, bit: u16) {
        self.0[(bit >> 6) as usize] |= 1u64 << (bit & 63);
    }

    #[inline]
    pub(crate) fn union(self, other: Self) -> Self {
        Self([
            self.0[0] | other.0[0],
            self.0[1] | other.0[1],
            self.0[2] | other.0[2],
            self.0[3] | other.0[3],
            self.0[4] | other.0[4],
            self.0[5] | other.0[5],
            self.0[6] | other.0[6],
            self.0[7] | other.0[7],
        ])
    }

    #[inline]
    pub(crate) fn contains(self, bit: u16) -> bool {
        (self.0[(bit >> 6) as usize] & (1u64 << (bit & 63))) != 0
    }

    #[inline]
    pub(crate) fn intersects(self, other: Self) -> bool {
        ((self.0[0] & other.0[0])
            | (self.0[1] & other.0[1])
            | (self.0[2] & other.0[2])
            | (self.0[3] & other.0[3])
            | (self.0[4] & other.0[4])
            | (self.0[5] & other.0[5])
            | (self.0[6] & other.0[6])
            | (self.0[7] & other.0[7]))
            != 0
    }

    #[inline]
    pub(crate) fn is_all(self) -> bool {
        self.0 == [u64::MAX; 8]
    }

    #[inline]
    pub(crate) fn is_empty(self) -> bool {
        self.0 == [0; 8]
    }
}

/// Intern table mapping `(kind, op_kind)` pairs to compact `u16` tag bits.
#[derive(Debug, Default)]
pub(crate) struct TagTable {
    kinds: HashMap<String, u16>,
    effect_ops: HashMap<String, u16>,
    next: u16,
    all_effects: TagMask,
}

impl TagTable {
    fn new() -> Self {
        let mut all_effects = TagMask::single(TAG_EFFECT_SYM);
        all_effects.insert(TAG_UNSEEN_EFFECT);
        Self {
            kinds: HashMap::default(),
            effect_ops: HashMap::default(),
            next: FIRST_INTERNED,
            all_effects,
        }
    }

    fn intern_kind(&mut self, kind: &str) -> u16 {
        if let Some(&bit) = self.kinds.get(kind) {
            return bit;
        }
        if self.next >= TAG_UNSEEN_EFFECT {
            return TAG_UNSEEN_OTHER;
        }
        let bit = self.next;
        self.next += 1;
        self.kinds.insert(kind.to_string(), bit);
        bit
    }

    fn intern_effect_op(&mut self, op_kind: &str) -> u16 {
        if let Some(&bit) = self.effect_ops.get(op_kind) {
            return bit;
        }
        if self.next >= TAG_UNSEEN_EFFECT {
            return TAG_UNSEEN_EFFECT;
        }
        let bit = self.next;
        self.next += 1;
        self.effect_ops.insert(op_kind.to_string(), bit);
        self.all_effects.insert(bit);
        bit
    }

    #[inline]
    pub(crate) fn tag_of_value(&self, value: &Value) -> u16 {
        let Some(object) = value.as_object() else {
            return TAG_NON_OBJ;
        };
        if object.contains_key("$sym") {
            return TAG_SYM;
        }
        let Some(kind) = object.get("kind").and_then(Value::as_str) else {
            return TAG_OTHER_OBJ;
        };
        if kind == "Effect" {
            let Some(op) = object
                .get("args")
                .and_then(|a| a.get("operation"))
                .and_then(Value::as_object)
            else {
                return TAG_EFFECT_SYM;
            };
            if op.contains_key("$sym") {
                return TAG_EFFECT_SYM;
            }
            let Some(op_kind) = op.get("kind").and_then(Value::as_str) else {
                return TAG_EFFECT_SYM;
            };
            return self
                .effect_ops
                .get(op_kind)
                .copied()
                .unwrap_or(TAG_UNSEEN_EFFECT);
        }
        self.kinds.get(kind).copied().unwrap_or(TAG_UNSEEN_OTHER)
    }

    #[inline]
    pub(crate) fn slice_tags(&self, items: &[Value]) -> SmallVec<[u16; 16]> {
        let mut out = SmallVec::with_capacity(items.len());
        for item in items {
            out.push(self.tag_of_value(item));
        }
        out
    }
}

/// Sound over-approximation of every `Value::Array` a rule or hole can print.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum ListShape {
    Empty,
    Array {
        min_len: usize,
        max_len: Option<usize>,
        /// Superset of `items[0]` tags across all non-empty arrays matched.
        first: TagMask,
        /// Superset of `items[len - 1]` tags across all non-empty arrays matched.
        last: TagMask,
        /// Every matched array contains an element in each required mask.
        required: Vec<TagMask>,
    },
}

impl ListShape {
    pub(crate) fn any() -> Self {
        Self::Array {
            min_len: 0,
            max_len: None,
            first: TagMask::ALL,
            last: TagMask::ALL,
            required: Vec::new(),
        }
    }

    pub(crate) fn empty_array() -> Self {
        Self::Array {
            min_len: 0,
            max_len: Some(0),
            first: TagMask::EMPTY,
            last: TagMask::EMPTY,
            required: Vec::new(),
        }
    }

    pub(crate) fn single(tag: TagMask) -> Self {
        let mut required = Vec::new();
        if !tag.is_all() {
            required.push(tag);
        }
        Self::Array {
            min_len: 1,
            max_len: Some(1),
            first: tag,
            last: tag,
            required,
        }
    }

    #[inline]
    pub(crate) fn min_len(&self) -> usize {
        match self {
            Self::Empty => usize::MAX,
            Self::Array { min_len, .. } => *min_len,
        }
    }

    #[inline]
    pub(crate) fn max_len(&self) -> Option<usize> {
        match self {
            Self::Empty => Some(0),
            Self::Array { max_len, .. } => *max_len,
        }
    }

    pub(crate) fn union(&self, other: &Self) -> Self {
        match (self, other) {
            (Self::Empty, x) | (x, Self::Empty) => x.clone(),
            (
                Self::Array {
                    min_len: min_a,
                    max_len: max_a,
                    first: first_a,
                    last: last_a,
                    required: req_a,
                },
                Self::Array {
                    min_len: min_b,
                    max_len: max_b,
                    first: first_b,
                    last: last_b,
                    required: req_b,
                },
            ) => {
                let mut required = Vec::new();
                if *min_a > 0 && *min_b > 0 {
                    for mask in req_a {
                        if req_b.contains(mask) && !required.contains(mask) && required.len() < 6 {
                            required.push(*mask);
                        }
                    }
                }
                Self::Array {
                    min_len: (*min_a).min(*min_b),
                    max_len: max_a.zip(*max_b).map(|(a, b)| a.max(b)),
                    first: first_a.union(*first_b),
                    last: last_a.union(*last_b),
                    required,
                }
            }
        }
    }

    pub(crate) fn concat(&self, other: &Self) -> Self {
        match (self, other) {
            (Self::Empty, _) | (_, Self::Empty) => Self::Empty,
            (
                Self::Array {
                    min_len: min_a,
                    max_len: max_a,
                    first: first_a,
                    last: last_a,
                    required: req_a,
                },
                Self::Array {
                    min_len: min_b,
                    max_len: max_b,
                    first: first_b,
                    last: last_b,
                    required: req_b,
                },
            ) => {
                let first = if *min_a >= 1 {
                    *first_a
                } else {
                    first_a.union(*first_b)
                };
                let last = if *min_b >= 1 {
                    *last_b
                } else {
                    last_a.union(*last_b)
                };
                let mut required = Vec::new();
                let mut push = |m: TagMask| {
                    if !m.is_all() && !m.is_empty() && !required.contains(&m) && required.len() < 6
                    {
                        required.push(m);
                    }
                };
                if *min_a >= 1 {
                    for &m in req_a {
                        push(m);
                    }
                    push(*first_a);
                }
                if *min_b >= 1 {
                    for &m in req_b {
                        push(m);
                    }
                    push(*first_b);
                }
                Self::Array {
                    min_len: min_a.saturating_add(*min_b),
                    max_len: max_a.zip(*max_b).and_then(|(a, b)| a.checked_add(b)),
                    first,
                    last,
                    required,
                }
            }
        }
    }

    pub(crate) fn require_non_empty(&self) -> Self {
        match self {
            Self::Empty => Self::Empty,
            Self::Array {
                min_len,
                max_len,
                first,
                last,
                required,
            } => {
                if *max_len == Some(0) {
                    return Self::Empty;
                }
                Self::Array {
                    min_len: (*min_len).max(1),
                    max_len: *max_len,
                    first: *first,
                    last: *last,
                    required: required.clone(),
                }
            }
        }
    }

    pub(crate) fn unordered(&self) -> Self {
        match self {
            Self::Empty => Self::Empty,
            Self::Array {
                min_len,
                max_len,
                required,
                ..
            } => Self::Array {
                min_len: *min_len,
                max_len: *max_len,
                first: TagMask::ALL,
                last: TagMask::ALL,
                required: required.clone(),
            },
        }
    }

    #[inline]
    pub(crate) fn matches_tags(&self, tags: &[u16]) -> bool {
        let Self::Array {
            min_len,
            max_len,
            first,
            last,
            required,
        } = self
        else {
            return false;
        };
        let len = tags.len();
        if len < *min_len || max_len.is_some_and(|m| len > m) {
            return false;
        }
        if len > 0 && (!first.contains(tags[0]) || !last.contains(tags[len - 1])) {
            return false;
        }
        if !required.is_empty() {
            let mut present = TagMask::EMPTY;
            for &tag in tags {
                present.insert(tag);
            }
            for &req in required {
                if !present.intersects(req) {
                    return false;
                }
            }
        }
        true
    }
}

/// Precomputed list-shape constraints for every rule in a grammar.
#[derive(Debug, Default)]
pub(crate) struct ListShapes {
    pub(crate) tags: TagTable,
    rule_shapes: Vec<ListShape>,
    rule_holes: Vec<Vec<(String, ListShape)>>,
}

impl ListShapes {
    pub(crate) fn build(rules: &[Rule], inverses: &HashMap<RuleId, Pattern>) -> Self {
        let mut tags = TagTable::new();
        for rule in rules {
            let is_op = matches!(
                &rule.ty,
                Some(crate::model::ir_schema::Ty::Named(name)) if name == "Operation"
            );
            let is_elem = matches!(
                &rule.ty,
                Some(crate::model::ir_schema::Ty::Named(name))
                    if matches!(
                        name.as_str(),
                        "Ability" | "Instruction" | "ContinuousChange" | "Mode" | "Keyword"
                    )
            );
            if let Some(p) = &rule.sem {
                collect_tags(p, is_elem, is_op, &mut tags);
            }
        }
        for p in inverses.values() {
            collect_tags(p, false, false, &mut tags);
        }

        let n = rules.len();
        let hole_map: Vec<HashMap<&str, Option<SmallVec<[RuleId; 2]>>>> =
            rules.iter().map(rule_hole_targets).collect();

        let mut elem_tags = vec![TagMask::EMPTY; n];
        let mut op_tags = vec![TagMask::EMPTY; n];
        let mut shapes = vec![ListShape::Empty; n];

        for _ in 0..32 {
            let mut changed = false;
            for (idx, rule) in rules.iter().enumerate() {
                if rule.alternatives.is_empty() {
                    continue;
                }
                let id = idx as RuleId;
                let (new_elem, new_op, new_shape) = eval_rule(
                    rule,
                    inverses.get(&id),
                    &hole_map[idx],
                    &tags,
                    &elem_tags,
                    &op_tags,
                    &shapes,
                );
                if new_elem != elem_tags[idx] {
                    elem_tags[idx] = new_elem;
                    changed = true;
                }
                if new_op != op_tags[idx] {
                    op_tags[idx] = new_op;
                    changed = true;
                }
                if new_shape != shapes[idx] {
                    shapes[idx] = new_shape;
                    changed = true;
                }
            }
            if !changed {
                break;
            }
        }

        let mut rule_holes = Vec::with_capacity(n);
        for (idx, rule) in rules.iter().enumerate() {
            let id = idx as RuleId;
            let mut names = SmallVec::<[&str; 4]>::new();
            if let Some(p) = &rule.sem {
                collect_list_holes(p, &mut names);
            }
            if let Some(inv) = inverses.get(&id) {
                collect_list_holes(inv, &mut names);
            }
            names.sort_unstable();
            names.dedup();
            let holes = names
                .into_iter()
                .map(|name| {
                    let shape = hole_list_shape(name, &hole_map[idx], &shapes);
                    (name.to_string(), shape)
                })
                .collect();
            rule_holes.push(holes);
        }

        Self {
            tags,
            rule_shapes: shapes,
            rule_holes,
        }
    }

    #[inline]
    pub(crate) fn matches_rule(&self, rule: RuleId, value: &Value) -> bool {
        let Some(items) = value.as_array() else {
            return true;
        };
        let Some(shape) = self.rule_shapes.get(rule as usize) else {
            return true;
        };
        if matches!(
            shape,
            ListShape::Array {
                min_len: 0,
                max_len: None,
                first,
                last,
                required,
            } if first.is_all() && last.is_all() && required.is_empty()
        ) {
            return true;
        }
        let tags = self.tags.slice_tags(items);
        shape.matches_tags(&tags)
    }

    #[inline]
    pub(crate) fn holes(&self, rule: RuleId) -> &[(String, ListShape)] {
        self.rule_holes
            .get(rule as usize)
            .map_or(&[], Vec::as_slice)
    }
}

/// For each hole name in `rule`, the child rules filling it across all
/// alternatives, or `None` if any alternative leaves it unbound or fills it
/// from a token class.
fn rule_hole_targets(rule: &Rule) -> HashMap<&str, Option<SmallVec<[RuleId; 2]>>> {
    let mut map: HashMap<&str, Option<SmallVec<[RuleId; 2]>>> = HashMap::default();
    if rule.alternatives.is_empty() {
        return map;
    }
    let mut all_names: SmallVec<[&str; 8]> = SmallVec::new();
    for alt in &rule.alternatives {
        for hole in alt.holes.iter().flatten() {
            if !all_names.contains(&hole.as_str()) {
                all_names.push(hole.as_str());
            }
        }
    }
    for name in all_names {
        let mut targets: SmallVec<[RuleId; 2]> = SmallVec::new();
        let mut bounded = true;
        for alt in &rule.alternatives {
            let mut found_in_alt = false;
            for (sym, hole) in alt.syms.iter().zip(&alt.holes) {
                if hole.as_deref() == Some(name) {
                    found_in_alt = true;
                    match sym {
                        Sym::Rule(child) => {
                            if !targets.contains(child) {
                                targets.push(*child);
                            }
                        }
                        _ => bounded = false,
                    }
                }
            }
            if !found_in_alt {
                bounded = false;
            }
        }
        map.insert(name, bounded.then_some(targets));
    }
    map
}

fn collect_tags(pattern: &Pattern, as_elem: bool, as_op: bool, tags: &mut TagTable) {
    match pattern {
        Pattern::Value(v) => collect_value_tags(v, as_elem, as_op, tags),
        Pattern::Object(fields) => {
            if let Some(kind) = object_kind(fields) {
                if kind == "Effect" {
                    if let Some(op) = effect_operation(fields) {
                        collect_op_pattern_tags(op, tags);
                    }
                } else {
                    if as_elem {
                        tags.intern_kind(kind);
                    }
                    if as_op {
                        tags.intern_effect_op(kind);
                    }
                }
            }
            for (_, inner) in fields {
                collect_tags(inner, false, false, tags);
            }
        }
        Pattern::List(items) => {
            for item in items {
                if let Item::One(inner) = item {
                    collect_tags(inner, true, false, tags);
                }
            }
        }
        Pattern::Symbol(name, args) => {
            let pass = passes_through(name) || matches!(name.as_str(), "apply" | "unordered");
            for (idx, arg) in args.iter().enumerate() {
                let keep = pass && (name != "apply" || idx == 1);
                collect_tags(arg, as_elem && keep, as_op && keep, tags);
            }
        }
        Pattern::Hole(..) => {}
    }
}

fn collect_value_tags(value: &Value, as_elem: bool, as_op: bool, tags: &mut TagTable) {
    match value {
        Value::Object(map) => {
            if let Some(kind) = map.get("kind").and_then(Value::as_str) {
                if kind == "Effect" {
                    if let Some(op_kind) = map
                        .get("args")
                        .and_then(|a| a.get("operation"))
                        .and_then(|o| o.get("kind"))
                        .and_then(Value::as_str)
                    {
                        tags.intern_effect_op(op_kind);
                    }
                } else {
                    if as_elem {
                        tags.intern_kind(kind);
                    }
                    if as_op {
                        tags.intern_effect_op(kind);
                    }
                }
            }
            for v in map.values() {
                collect_value_tags(v, false, false, tags);
            }
        }
        Value::Array(items) => {
            for item in items {
                collect_value_tags(item, true, false, tags);
            }
        }
        _ => {}
    }
}

fn collect_op_pattern_tags(op: &Pattern, tags: &mut TagTable) {
    match op {
        Pattern::Value(v) => {
            if let Some(k) = v.get("kind").and_then(Value::as_str) {
                tags.intern_effect_op(k);
            }
        }
        Pattern::Object(fields) => {
            if let Some(k) = object_kind(fields) {
                tags.intern_effect_op(k);
            }
        }
        Pattern::Symbol(name, args) if passes_through(name) => {
            if let Some(inner) = args.first() {
                collect_op_pattern_tags(inner, tags);
            }
        }
        _ => {}
    }
}

fn collect_list_holes<'a>(pattern: &'a Pattern, out: &mut SmallVec<[&'a str; 4]>) {
    match pattern {
        Pattern::List(items) => {
            for item in items {
                match item {
                    Item::One(inner) => collect_list_holes(inner, out),
                    Item::Splice(name, _) => out.push(name.as_str()),
                }
            }
        }
        Pattern::Object(fields) => {
            for (_, inner) in fields {
                collect_list_holes(inner, out);
            }
        }
        Pattern::Symbol(name, args) => {
            if matches!(
                name.as_str(),
                "and_sequence" | "then_sequence" | "optional_then"
            ) {
                for arg in args {
                    if let Pattern::Hole(h, _) = arg {
                        out.push(h.as_str());
                    }
                }
            }
            for arg in args {
                collect_list_holes(arg, out);
            }
        }
        Pattern::Value(_) | Pattern::Hole(..) => {}
    }
}

fn object_kind(fields: &[(String, Pattern)]) -> Option<&str> {
    fields.iter().find_map(|(k, p)| match p {
        Pattern::Value(Value::String(s)) if k == "kind" => Some(s.as_str()),
        _ => None,
    })
}

fn effect_operation(fields: &[(String, Pattern)]) -> Option<&Pattern> {
    let args = fields
        .iter()
        .find_map(|(k, p)| (k == "args").then_some(p))?;
    match args {
        Pattern::Object(inner) => inner
            .iter()
            .find_map(|(k, p)| (k == "operation").then_some(p)),
        _ => None,
    }
}

fn hole_list_shape(
    name: &str,
    hole_map: &HashMap<&str, Option<SmallVec<[RuleId; 2]>>>,
    shapes: &[ListShape],
) -> ListShape {
    let Some(Some(targets)) = hole_map.get(name) else {
        return ListShape::any();
    };
    let mut acc = ListShape::Empty;
    for &child in targets {
        acc = acc.union(&shapes[child as usize]);
    }
    acc
}

fn hole_tag_mask(
    name: &str,
    hole_map: &HashMap<&str, Option<SmallVec<[RuleId; 2]>>>,
    masks: &[TagMask],
    default: TagMask,
) -> TagMask {
    let Some(Some(targets)) = hole_map.get(name) else {
        return default;
    };
    let mut acc = TagMask::EMPTY;
    for &child in targets {
        acc = acc.union(masks[child as usize]);
    }
    acc
}

fn eval_rule(
    rule: &Rule,
    inverse: Option<&Pattern>,
    hole_map: &HashMap<&str, Option<SmallVec<[RuleId; 2]>>>,
    tags: &TagTable,
    elem_tags: &[TagMask],
    op_tags: &[TagMask],
    shapes: &[ListShape],
) -> (TagMask, TagMask, ListShape) {
    match &rule.sem {
        None => {
            let mut elem = TagMask::EMPTY;
            let mut op = TagMask::EMPTY;
            let mut shape = ListShape::Empty;
            for alt in &rule.alternatives {
                match alt.syms.first() {
                    Some(Sym::Rule(child)) => {
                        let c = *child as usize;
                        elem = elem.union(elem_tags[c]);
                        op = op.union(op_tags[c]);
                        shape = shape.union(&shapes[c]);
                    }
                    Some(_) => {
                        elem.insert(TAG_NON_OBJ);
                    }
                    None => {}
                }
            }
            (elem, op, shape)
        }
        Some(pattern) => {
            let mut elem = pattern_elem_tag(pattern, hole_map, tags, elem_tags, op_tags);
            let mut op = pattern_op_tag(pattern, hole_map, tags, op_tags);
            let mut shape = pattern_list_shape(pattern, hole_map, tags, elem_tags, op_tags, shapes);
            if let Some(inv) = inverse {
                elem = elem.union(pattern_elem_tag(inv, hole_map, tags, elem_tags, op_tags));
                op = op.union(pattern_op_tag(inv, hole_map, tags, op_tags));
                shape = shape.union(&pattern_list_shape(
                    inv, hole_map, tags, elem_tags, op_tags, shapes,
                ));
            }
            (elem, op, shape)
        }
    }
}

fn pattern_op_tag(
    pattern: &Pattern,
    hole_map: &HashMap<&str, Option<SmallVec<[RuleId; 2]>>>,
    tags: &TagTable,
    op_tags: &[TagMask],
) -> TagMask {
    match pattern {
        Pattern::Value(v) => v
            .get("kind")
            .and_then(Value::as_str)
            .and_then(|k| tags.effect_ops.get(k).copied())
            .map_or(tags.all_effects, TagMask::single),
        Pattern::Object(fields) => match object_kind(fields) {
            Some(k) => tags
                .effect_ops
                .get(k)
                .copied()
                .map_or(tags.all_effects, TagMask::single),
            None => tags.all_effects,
        },
        Pattern::Hole(name, _) => hole_tag_mask(name, hole_map, op_tags, tags.all_effects),
        Pattern::Symbol(name, args) if passes_through(name) => {
            let inner = args.first().map_or(tags.all_effects, |p| {
                pattern_op_tag(p, hole_map, tags, op_tags)
            });
            inner.union(TagMask::single(TAG_EFFECT_SYM))
        }
        Pattern::Symbol(name, args) if name == "apply" => {
            args.get(1).map_or(tags.all_effects, |p| {
                pattern_op_tag(p, hole_map, tags, op_tags)
            })
        }
        _ => tags.all_effects,
    }
}

fn pattern_elem_tag(
    pattern: &Pattern,
    hole_map: &HashMap<&str, Option<SmallVec<[RuleId; 2]>>>,
    tags: &TagTable,
    elem_tags: &[TagMask],
    op_tags: &[TagMask],
) -> TagMask {
    match pattern {
        Pattern::Value(v) => TagMask::single(tags.tag_of_value(v)),
        Pattern::List(_) => TagMask::single(TAG_NON_OBJ),
        Pattern::Hole(name, _) => hole_tag_mask(name, hole_map, elem_tags, TagMask::ALL),
        Pattern::Object(fields) => match object_kind(fields) {
            Some("Effect") => match effect_operation(fields) {
                Some(op) => pattern_op_tag(op, hole_map, tags, op_tags),
                None => tags.all_effects,
            },
            Some(kind) => tags
                .kinds
                .get(kind)
                .copied()
                .map_or(TagMask::ALL, TagMask::single),
            None => {
                if fields.iter().any(|(k, _)| k == "$sym") {
                    TagMask::single(TAG_SYM)
                } else {
                    TagMask::ALL
                }
            }
        },
        Pattern::Symbol(name, args) if passes_through(name) => {
            let inner = args.first().map_or(TagMask::ALL, |p| {
                pattern_elem_tag(p, hole_map, tags, elem_tags, op_tags)
            });
            inner.union(TagMask::single(TAG_SYM))
        }
        _ => TagMask::ALL,
    }
}

fn pattern_list_shape(
    pattern: &Pattern,
    hole_map: &HashMap<&str, Option<SmallVec<[RuleId; 2]>>>,
    tags: &TagTable,
    elem_tags: &[TagMask],
    op_tags: &[TagMask],
    shapes: &[ListShape],
) -> ListShape {
    match pattern {
        Pattern::Value(Value::Array(items)) => {
            if items.is_empty() {
                return ListShape::empty_array();
            }
            let mut acc = ListShape::single(TagMask::single(tags.tag_of_value(&items[0])));
            for item in &items[1..] {
                acc = acc.concat(&ListShape::single(TagMask::single(tags.tag_of_value(item))));
            }
            acc
        }
        Pattern::Value(_) | Pattern::Object(_) => ListShape::Empty,
        Pattern::Hole(name, _) => hole_list_shape(name, hole_map, shapes),
        Pattern::List(pieces) => {
            if pieces.is_empty() {
                return ListShape::empty_array();
            }
            let mut acc: Option<ListShape> = None;
            for piece in pieces {
                let part = match piece {
                    Item::One(inner) => ListShape::single(pattern_elem_tag(
                        inner, hole_map, tags, elem_tags, op_tags,
                    )),
                    Item::Splice(name, _) => hole_list_shape(name, hole_map, shapes),
                };
                acc = Some(match acc {
                    None => part,
                    Some(prev) => prev.concat(&part),
                });
            }
            acc.unwrap_or_else(ListShape::empty_array)
        }
        Pattern::Symbol(name, args) => {
            if passes_through(name) {
                return args.first().map_or_else(ListShape::any, |inner| {
                    pattern_list_shape(inner, hole_map, tags, elem_tags, op_tags, shapes)
                });
            }
            match (name.as_str(), args.as_slice()) {
                ("and_sequence", [head, tail]) => {
                    let left = pattern_list_shape(head, hole_map, tags, elem_tags, op_tags, shapes);
                    let right =
                        pattern_list_shape(tail, hole_map, tags, elem_tags, op_tags, shapes);
                    left.concat(&right)
                }
                ("then_sequence" | "optional_then", [head, tail]) => {
                    let left = pattern_list_shape(head, hole_map, tags, elem_tags, op_tags, shapes)
                        .require_non_empty();
                    let right =
                        pattern_list_shape(tail, hole_map, tags, elem_tags, op_tags, shapes)
                            .require_non_empty();
                    left.concat(&right)
                }
                ("apply", [_, body]) => {
                    pattern_list_shape(body, hole_map, tags, elem_tags, op_tags, shapes)
                }
                ("unordered", [inner @ Pattern::List(_)]) => {
                    pattern_list_shape(inner, hole_map, tags, elem_tags, op_tags, shapes)
                        .unordered()
                }
                (
                    "single_reflexive_base"
                    | "offered_action"
                    | "captured_targets"
                    | "announced_targets",
                    [inner],
                ) => pattern_list_shape(inner, hole_map, tags, elem_tags, op_tags, shapes),
                _ => ListShape::any(),
            }
        }
    }
}
