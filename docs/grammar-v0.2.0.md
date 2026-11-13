# Grammar 0.2.0

This release integrates the named-player history model PR, grammar simplification
and all reported issue fixes #4–#18. It advances the grammar and both owned model
packages to 0.2.0 and the AST schema to 191. Existing 0.1.0 packages and release
assets remain immutable.

## AST compatibility

HistoryWindow::LastTurnOf(PlayerRef) names a player's most recent completed turn.
It differs from the preceding global turn and from SinceLastTurnOf. The public
enum addition requires the incompatible 0.2.0 release. Existing binary tags keep
their values; schema 190 has an explicit identity migration through its frozen
descriptors. Player bindings are validated in history queries and historical
player snapshots. Normal builds use owned workspace/registry crates, without
MTGish packages or dependency replacements.

Reference preparation filters the explicitly allowed population, migrates its
original schema with the owned contract, then applies recorded amendments.
Current-epoch variants never carry old schema headers. The immutable reference
export, its lock and extraction provenance remain unchanged.

## Grammar and corrected meanings

Shared values, mana clauses, untap rules, exchanges, two-phase shuffle actions
and symbolic definitions retire 165 mined rules cumulatively. Reference shapes
for equivalent concepts are normalized with scoped proofs; actual identity,
window, choice, observation and completion errors are corrected independently.
Every amendment is recorded with exact operands and evidence in
[the issue audit](reported-issues-20261113.md). Satya measures the complete created
token batch. Arboria now observes the putting actor in the named player's last
turn. The other issue corrections preserve prevention direction and source,
Aura attachment eligibility, independent exchanges, fixed untap players,
original destroy targets, actual reveal/exile receipts, direct destroy legality,
historical damage participants, observed nonlands, per-player shuffled counts,
current-combat attacks and Shapeshifter's intrinsic CDA formula.

## Release artifacts

The code tag is grammar-v0.2.0. The existing release workflow verifies/packages
all three crates and prepares native faces before publishing. The matching face
bundle uses schema 191 and the unchanged SHA-pinned original Oracle snapshot
20261112220158. Its all-record export population is authorized only for release
export; no held-out diagnostics or rule authoring use that population. Code and
data manifests retain exact producer/exporter commits, package/schema versions,
input scope and checksums. Native face success does not imply reference agreement,
printing or semantic reparse; those release-export gates remain separately named.


## Validation

All eight repository gates pass: 787 frontend tests (one ignored), 102 model and
contract tests (four ignored), and 73 Python tests. The immutable allowed audit
population has 32,755 equal faces and 33,914 native-valid readings. All 53 issue
and control faces retain exact keyed printing and verified defaults. The native
preservation audit explicitly accounts for all corrected meanings: 48 old wrong
readings removed, 42 corrected additions and no unrelated delta. Six removed
alternatives already had a retained correct target reading. Schema migration
preserves all other operands and identities; canonical gold is idempotent.
These audits use the explicitly allowed population; the complete release-source
export reports its own scope and outcomes independently.
