# Receipt and spending follow-ups (schema 193)

The owned model adds three append-only variants for reports
[#36](https://github.com/vishvananda/oracle-grammar/issues/36),
[#35](https://github.com/vishvananda/oracle-grammar/issues/35) and
[#34](https://github.com/vishvananda/oracle-grammar/issues/34). Existing binary
variant ordinals remain fixed. Schema 192 has an immutable input descriptor;
migration preserves its existing meanings rather than guessing these corrections.
Reference amendments U0186–U0188 make the reviewed corrections explicitly.

`ObjectSet::TransitionWhere` restricts recorded transitions by departure and/or
first arrival zone before projecting the requested Before/After incarnation.
At least one zone restriction is required. Later movement does not erase an
outcome; replacement redirection changes which arrival was recorded. This
requires an object-transition producer. Glyph of Reincarnation uses actual
Battlefield → Graveyard outcomes, then asks about each Before identity's
controller at the last qualifying blocking event. The spell controller chooses
from that historical controller's graveyard. Choices reserve distinct original
identities with `Unpicked`, respect `CanMoveTo`, and gather into one set moved
simultaneously under the returned cards' owners' control. Failed destruction,
redirected arrivals and later graveyard movement remain separate cases.

`Expr::DamagePreventedBy` reads the lifetime accumulated total of one instantiated
damage replacement. A deferred program captures that exact shield receipt and
reads the total when it resolves, including zero or multiple prevention events.
An unrelated effect or callback-local receipt cannot provide it. Sacred Boon and
Scars of the Veteran install their shield and delayed trigger during spell
resolution; only prevention occurs in the replacement callback. Scars keeps its
creature condition at setup and retains the original target. `Performed` still
measures one event/action; `PerformedTotal` still aggregates loop executions.

`PlayerRule::SpendManaAsLimited` grants a chosen spending substitution with a
positive per-application use count. One successful eligible spell payment chosen
to use the permission consumes one use. `ManaPaymentComponent::ManaCost` applies
only to spell payments; `EntirePayment` is separate. North Star uses AnyType,
Spells(Any), ManaCost, one use and EndOfTurn. It grants no casting action, zone
permission, timing exception, discount or alternate cost. Additional and
alternative costs retain their ordinary payment rules. An edited use count
cannot print the unchanged “one spell” production. The original additional-cost
reminder is retained as presentation attached to that spending instruction; it
adds no casting action or payment exception.

The [#33 follow-up](https://github.com/vishvananda/oracle-grammar/issues/33#issuecomment-6934200147)
is a bounded comparison normal form: Matching(Matching(S,A),B) becomes
Matching(S,AllOf([A,B])). It preserves the receipt, phase, both predicates and
typed identifier provenance; authored print nesting remains available. Unknown
wrapper fields or more than 64 conjuncts prevent the proof. U0185 applies this
form to Frankenstein's Monster without dropping its Exile-arrival guard.

These contracts describe portable IR and validate scope and representation.
This repository does not execute game rules. Focused tests cover native and
binary roundtrips, migration boundaries, invalid receipt sources and filters,
current-operand printing, complete recognition and verified semantic reparse.
