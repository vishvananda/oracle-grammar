# A proved source event subject

Comparison may replace the current event's distinguished `EventSubject` with
`Source` when its matching filter requires that exact object. The proof accepts
`SameObject(Source)`, an intersection containing that restriction, or a nonempty
union whose every branch requires it. The event and all its predicates remain
unchanged. No binding IDs or their origins are removed.

The supported events are Object, ZoneChange, Pair, Counters, Attack and Spell,
plus Damage with a singular distinguished role. Damage's Source role requires
Source or SourceRecipient grouping and a source-only sources filter. Its
Recipient role requires Recipient or SourceRecipient grouping and an Objects
recipient filter requiring the source. A matching filter cannot create a role
that the grouping does not bind. Explicit Damage references to that same role
reach the same normal form in one pass.

Filtered and FirstEachTurn preserve the inner proof. Every AnyOf event branch
must prove the identity. Batch is accepted only with per-Subject grouping and
an inner proof. Work is bounded to 128 visits, depth 32 and 64 alternatives or
conjuncts. Exceeding a bound conservatively retains the explicit reference.

Each ability establishes its own source and event context. Created triggers
establish their independently matched event frame; their matcher still captures
the creator's references. Event-pattern headers, including EventHistory, keep
explicit references. Deferred, reflexive and replacement closures retain the
existing conservative reference policy. Ordinary authored print operands keep
their EventSubject form, and printing edited IR must verify using its current
event and body operands. SourceEntering keeps its separate entry contract.

AttachedTo(Source), unrestricted other creatures or spells, mixed alternatives,
non-Subject batches, grouped damage without a singular role and malformed extra
fields provide no proof. These cases remain distinct supported readings.

Printing retains the ordinary form and adds at most four subject views from the
current event and operands. Literal Source roles in announcements and in effect
creation or spell modification can each retain their declared form. Native
templates receive these views as well as lexical unbinding. Under the shared
source proof, an explicit subject spelling precedes named variants so nested
expansion cannot exhaust its bound before reaching that spelling. Every accepted
text still reparses to the original current meaning.

Trigger-branch comparison must use the shared event frame. If one branch proves
Source identity, its current Source references can be expressed as EventSubject
for this comparison. The typed lift leaves independent abilities, future bodies
and event headers untouched, and only commits when every non-event operand then
matches. Identical explicit Source bodies keep their original form. A per-pair
8,192-visit budget bounds this optional comparison; exhaustion keeps the explicit
branches. This preserves a joined trigger's normal form when one of its separate
branches gains the stronger source proof.

On the immutable 32,755-face allowed retirement population, the proof merges 26
encodings across 25 faces: 24 change from two readings to one, and Scourge of
Valkas changes from four to two. Every old reading validates before comparison;
all 33,536 resulting readings validate and remain canonical, with no missing or
added meanings. Recognition and reference equality remain complete. The seven
reviewed cases without a source identity proof keep their multiple readings.

All 26 readings on the 25 merging faces retain exact printing and verified
default printing with full card context. Flayer of the Hatebound is an additional
passing control. Radagast the Brown retains exact printing; its full-card default
print failure is reproduced with the previous binary. The separate legacy IR
printing path still verifies exact and default output for all 27 audited faces.
