# Print reuse

A retirement batch changes a few dozen rules, but its full allowed-population check
prints every face again. Most rows come out byte for byte as before. Print reuse
carries a face's row over from the previous accepted print when the trace of that
print shows that no difference between the two grammars can reach the face. The
first trace-only or shadow batch prints everything.

## Traces

`score --trace-out=FILE` prints as usual and also writes one line per row, in row
order, recording what the face's print consulted (`src/print_trace.rs`):

- every rule asked to print a value, reached by a looked-up value's head, or read
  while ranking alternatives;
- the hashes of the first one to eight discrimination steps of every looked-up value,
  and of every node within twelve steps of its root, with the path to the node;
- every token of every text the scoreboard tokenized, original or printed, and
  every pair of adjacent tokens;
- every word the tokenizer looked up in the vocabulary;
- the most steps and the deepest call of any parse.

Tracing is thread-local and does not change output; rows are byte-identical with
and without it. The file header names the format, the grammar fingerprint, a hash
of the scoreboard executable, the score options and a core of the rules at least
half of the faces consulted. Each line stores its rules as their difference from
that core. Id lists are base64 of LEB128 gaps.

## Carrying a row over

`--reuse-rows=ROWS --reuse-traces=TRACES --reuse-grammar=DIR` read a previous
run: its rows, its trace file and the grammar directory it printed under.
`src/print_reuse.rs` compares the two grammars by rule name (`name#n` for
duplicates) and finds:

- **strict changes**: removed rules, and rules whose alternatives, `@sem`, type,
  preferences, inverse tables or word entries differ. The printer records a rule it
  asks before checking what the rule reaches, so a rule whose meaning starts with
  symbols (`Instruction::[Effect{result:@next, operation:@sacrificed_result(…)}]`)
  is consulted by most faces. A face that consulted a strict rule with a meaning in
  both grammars is reprinted only when one of its looked-up values could fit the
  rule under the old or the new grammar (see zone rules below for what fitting
  means). Reading is checked apart, by the parse changes: a changed meaning or type
  requires every alternative. A rule without a meaning, or whose meaning is `()`,
  is bounded by its owners: the nearest rules with another meaning on every way up
  through the alternatives that name it. The scoreboard prints from `card` and
  `card_text` and renders any other rule only as a child of an alternative it
  renders, so such a rule prints only inside an owner's print of a looked-up value.
  A `()` meaning is not bound by itself, since a child its alternative binds to
  nothing is printed from null, which nearly every print looks up. A face is
  reprinted when one of its values could fit an owner under the grammar that has
  it; when the way up meets `card`, `card_text` or a rule nothing names, every
  face that consulted the rule is reprinted;
- **zone rules**: pass-through rules that kept their own meaning but gained, lost or
  changed children. A face that consulted one is reprinted when it consulted a lost
  child, or when a child now reaches a seed (a rule with a meaning) that it did not
  reach before, or that changed, and that seed could fit one of the face's looked-up
  values. A seed the child reaches in both grammars with its meaning unchanged fits
  the same values as before; a child under a new alternative reaches every seed
  anew, even one the print asked elsewhere. A seed could fit a value when, for one
  of its patterns (its meaning, or an inverse it also prints), the value's leading
  steps follow the pattern's and the value holds every node that every value the
  pattern prints holds: the pattern's own nodes and, through its holes and splices,
  the nodes its children require (a greatest fixed point over the grammar). A
  transparent symbol such as `@sacrificed_result(…)` matches what its argument
  matches or a value already marked with it, so a node required under one is also
  met by that symbol's own node. `@unordered` keeps a list's nodes, and
  `@either_event(a, b)` matches `{"AnyOf": [a, b]}` in either order. `@and` deals a
  value's conjuncts, the nodes under its nested `AllOf` lists, out among its
  arguments: an argument that must be one object kind other than `AllOf` or `Any`
  takes one conjunct, so its nodes are met at some depth of those lists, or the
  value nests an `AllOf` list past the deepest depth the trace keeps them for. A
  conjunction inverse, which prints a declared `AllOf` or `AnyOf` this way, keeps
  the meaning's holes. `@entering_controller` matches a source-entry replacement as
  it is or with each `ControllerOf(EventSubject)` under its body read as
  `AbilityController`, so such a node may also be the original. A seed no
  alternative can fill, such as a disabled `never` rule, fits nothing. A seed the
  child no longer reaches, or one under a lost alternative, reprints a face that
  consulted it (or the lost child) only when it could fit one of the face's values
  under the old grammar: the trace names every rule the print's index offered, not
  only those that printed. When a search passes 32,768 descendants, every face that
  consulted the rule is reprinted;
- **parse changes**: for each changed or new alternative, the words and adjacent
  word pairs every derivation of it must contain (a greatest fixed point over the
  grammar). The parser matches one token per symbol, so two literals side by side
  in an alternative need those two tokens side by side. A child whose derivations
  share no word, such as `players_singular`, would let every text through, so the
  requirement is a set of sentential forms (`src/print_reuse_reads.rs`): the
  alternative inside each place the grammar names its rule, climbing up to four
  uses that add no symbols. While the forms fit 512, up to 256 times, the form with
  the fewest words is spelled out at its most telling weak child: the one whose
  alternatives' fewest words are the most. A reading through the
  alternative is a derivation from `card` or `card_text` (or a rule no alternative
  names), so it holds one form's words and pairs. A face whose texts contain all of
  the words and pairs of some form of some alternative is reprinted. An added or removed
  rule that another alternative names adds nothing here: a reading reaches it only
  through such an alternative, which is itself new, changed or removed and needs
  what the rule needs. A rule only the engine enters, such as `card`, keeps its own
  requirements;
- **vocabulary changes**: words that came or went in the vocabulary. A face is
  reprinted when its tokenizer looked one of them up;
- **headroom**: a face is reprinted when any parse used more than a quarter of the
  step limit or half the depth limit;
- **global changes**: when the word types or presentation labels differ, every
  face is reprinted.

A row carries over only when it is `equal` and `exact` and has no `printing`,
`print_frontier`, `source_context`, `syntax` or `rendered` detail, its text is
unchanged, and its derivation key can be spelled in the new grammar. Retirement
removes hook alternatives, so the indices after them shift (`mined_ability.129`
becomes `mined_ability.127`). A hook that only lost or reordered pass-through
children keeps its meaning, and the key is renumbered; a key that names a removed
alternative or any other changed rule is reprinted. The face must match by
a key over its gold record, Oracle text and source context. A trace from another
grammar, build or option set lends nothing, and neither does a run with rules
switched off by `--without`. The scoreboard then prints every face and
`--reuse-report` says why.

A carried row's trace is written in the new grammar's ids, without the rules the
new grammar removed and with every added, strict and zone rule and every rule
beneath them. A fresh print may consult those without printing anything through
them: the printer asks a rule it tries for its children, and a splice unifies with
any list. A rule outside them is asked as before, with the same values. The next
comparison checks them as it would for a fresh trace, and its checks on them ask
what a value needs to fit, so the extra rules cost little.

`--reuse-mode=shadow` prints every face and reports each prediction that the
fresh print contradicts, and, as `uncovered`, each rule a fresh print consulted
that a predicted row's carried trace lacks. `--reuse-report=FILE` writes the change summary and
the counts of reused, printed and unseen faces, with a reason for each print.

## Retirement driver

With `full_check_round_trip: gated`, `full_check_reuse` turns reuse on:

- `off` (default): no traces;
- `shadow`: trace every batch and audit every prediction with a full print;
- `on`: carry rows over from the previous checkpoint's traced print. One batch
  in `full_check_reuse_audit_every` (default 4) runs in shadow mode instead.

A previous print counts only when its batch record names its rows and the
digest of its traces, and the frozen grammar matches its input manifest. A
contradicted prediction writes `print-reuse-contradicted.json` at the campaign
root. Until someone investigates and removes it, every batch runs in shadow mode.
Batch evidence records the plan, the counts and the trace digest under
`print_reuse`.
