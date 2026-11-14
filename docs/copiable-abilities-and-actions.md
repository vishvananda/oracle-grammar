# Copiable abilities and dependent action printing

The shared copy-token grammar now retains the token quantity, copied object and
copiable exceptions. The article in "a copy" is literal; it cannot replace the
quantity of tokens. Singular and plural copy phrases use their current amount.
Entry copies and token copies share keyword and quoted-ability exception rules.
A final quotation supplies its own sentence-ending period. A missing inner
period cannot claim a complete parse. Artifact additions also use this shared
exception list instead of a separate target-only copy recipe.

Abilities added by a copy exception introduce their copied object's holder.
The creating card's name remains distinct from "this token". Previously the
holder binder handled defined token recipes but missed abilities in copy
exceptions, allowing the creator's name to print as though it named the token.
A token copy introduces a holder even when its creator is entering. An own-source
entry copy retains the incoming source as its existing holder. These are AST
lowering corrections under the existing source and granting-object contract;
the wire schema is unchanged.

"The exiled card" in an activation body uses that activation's one proved
singular exile-payment component. Its reference is the performed After receipt,
with the actual structural cost address. It cannot be replaced by an arbitrary
card from the source's persistent exiled-with set. The old seeded spelling
hardcoded component three and included the following comma in its noun.
The shared annotation detects the cost component and retains the punctuation
in the surrounding copy phrase. It keeps the linked-card spelling when there
is no current activation-body proof, and refuses plural or ambiguous payment
components instead of choosing a card. Its inverse projects only a validated
current receipt to the grammar's own literal fallback; semantic reparse must
recover the same cost address. Tawnos's existing reference already uses that
receipt, so this correction needs no reference amendment.

Lock-or-unlock is a shared keyword action over its actual object. Its one door
is not an ignored numeric quantity. Per-population activation reductions retain
the written generic-mana factor, multiply it by the population expression and
compose with sorcery timing. This replaces whole-ability seeds that dropped
these operands. The unit reduction has the existing expression normal form.

## Default-printing regressions found by the complete audit

The complete allowed-population printing audit of checkpoint 6198db3 confirms
719 exact-wording gaps, with no lost exact or verified outputs. It separately
found two lost default prints, Silent Extraction and Water Weird. Both still
had verified keyed wording. This is why exact, any verified and default
printing remain separate measurements.

Silent Extraction exposed a binding-order error: a seek's move-receipt marker
was visited before references in the preceding random-selection filter.
The same preparation that already delays a chosen-card movement until its
filter is bound now handles seek movements too. It changes binding metadata,
not program execution order or the actual selected and moved result identities.
Water Weird's complete top-card query now has a printing preference over a
short pronoun that cannot acquire that query from its context. Candidates still
require semantic reparse; the preference adds no reading or equality exception.

Regression tests cover the seven original recovery faces, Tawnos and both
lost defaults. They also vary copy quantities, granted keywords, creator/holder
identity, activation cost addresses, reduction factors and preceding observable
actions. An entry-copy control checks the existing holder. The phrase family
runs without seeded rules and includes incomplete-quote and dropped-door-count
refusals. The AST schema, model pin and reference lock remain unchanged. No card text
is retained as a printing operand. Reference amendment U0034 records the two
singleton normalizations described below.


## Singleton payment measurements and reviewed reading changes

The existing singleton-receipt lowering now also handles a direct activation
Move-to-Exile cost with exactly one selection choosing Exactly(Constant(1)).
A sum of a property over its Activation/Performed/After receipt becomes that
card's ObjectProperty query at index zero, with CurrentOrLastKnown information.
The exact structural component is retained. The inverse supplies the aggregate
surface form from those current operands and the same cost proof; a printed
candidate must still pass semantic reparse.

U0034 normalizes the two allowed reference records using this shape: Ellie and
Alan, Paleontologists and Food Chain. The former names the mana value of the
exiled card; the latter names the mana value of the exiled creature. Both costs
choose exactly one object. Other costs, phases, receipt views, plural or optional
selections, malformed fields and independent ability frames do not qualify.
This extends frontend lowering, without changing the owned canonicalizer,
AST schema or vocabulary. It is not a general equality alias between sums and
object properties. Ellie and Alan's old fixed-component value seed is retired;
the shared property grammar retains the actual activation receipt instead.

The old alternative for Storm Elemental that queried a persistent ExiledWith
set is a lowering error: its condition names the card exiled to pay that
activation. Ellie and Alan had the same wrong persistent-link alternative in
addition to the correctly addressed aggregate. Those two alternatives are
removed. Their two correct aggregate readings (one on each normalized face)
are replaced by scalar property readings. These changes require an explicit
reading review; strict preservation of every old parser alternative is not a
passing claim for this batch.

The activation annotation remains transparent to existing positive lexical
proofs for linked-card return actions. It cannot turn an arbitrary typed zone
query, an indexed different card or an annotation with extra operands into a
literal noun proof. Voyager Staff, Identity Thief and Ugin's Labyrinth retain
their existing local-payment and return identities.

## Validation

All eight repository gates pass: formatting, model pin, hygiene, Python tooling,
Clippy, helper build, compositional families and locked release library tests.
There are 829 passing frontend tests and one ignored test, 102 passing owned
model tests and four ignored tests, and 75 passing Python tests.

The frozen allowed-population parse matches all 32,755 references; 31,714 faces
have one reading. All 33,882 returned readings are native-valid and canonical.
All source and canonical reference records validate, and reference canonicalization
is idempotent. The old Food Chain singleton is materialized from the old frozen
parser before comparison, so its amended gold cannot conceal the normalization.
Exactly four old alternatives change: the two singleton measurements are replaced
by scalar measurements, and the two wrong persistent-link alternatives are removed.
There are two added scalar readings and no other additions, removals or canonical
merges. Strict preservation reports these changes; the separate operand review
verifies each normalization and identity correction.

The rule audit matches all 32,755 faces, with 19,978 compositional faces, 11,984
remaining seeded rules, no at-risk faces and the same four attribution limits.
Five seeds are retired in this batch. The three previously protected unused
rules remain; lack of reference necessity alone does not justify removing their
printing or alternate-reading support.

On the original 850-face exact-gap cohort, 512 faces now have a verified print,
172 reproduce the original wording, and 511 have a verified default print.
This batch gains ten verified/default prints and eight exact texts, with no
verified, exact or default losses. The remaining tracked exact gaps are 678:
338 without a verified print and 340 with verified alternative wording.

The broad 1,101-face affected cohort has 1,071 verified prints, 1,057 exact texts
and 1,069 verified defaults, with no losses against the complete earlier printing
snapshot overlaid by the accepted entry/equip checkpoint. Silent Extraction and
Water Weird regain their defaults; Volrath's Shapeshifter also gains a default.
All 64 Gift controls preserve verified, exact and default printing. These
focused comparisons are separate from the complete earlier 6198db3 printing
refresh; they do not claim a fresh whole-population printing measurement.

The eight exact recoveries are Chandra, Flameshaper; Electroduplicate; Exploding
Barrel; Heat Shimmer; Keys to the House; Kindle the Inner Flame; Marina Vendrell;
and Quest for the Necropolis. Reflection of Kiki-Jiki and Kiki-Jiki, Mirror Breaker
also gain verified prints, while their original nonlegendary target wording
remains a separate adjective-grammar gap.
