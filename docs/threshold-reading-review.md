# Reading changes accompanying numeric thresholds

This review compares every pre-batch reading with the new allowed-population
parse. Old singleton operands are materialized from the immutable pre-U0033
reference before comparison; substituting the new gold would hide the reference
normalization. The explicit mapped baseline applies only the owning-spell X
normalization described in [numeric thresholds](spell-x-thresholds.md).
The owned canonicalizer remains unchanged.

Native validity, reference equality and preservation of old parser alternatives
are separate checks. A strict preservation report is expected to identify the
26 alternatives below. They are reviewed frontend normalizations or corrections,
not a claim that strict preservation passed. New readings require their own
review; preserving old readings alone cannot establish their correctness.

| Old alternatives | Disposition and source evidence |
| --- | --- |
| 13 alternate permission creators | The current ability creates the permission. Its normal form uses Source, rather than an EventSubject or Discarded operand inherited from a different syntactic role. In an own-source trigger these can coincide; a cast spell or discarded card is a separate object. The references already use Source. |
| 7 alternate exiled-card readings | A following card test, characteristic query or permission concerns the actual exiled card. A holder Source or the preceding event's graveyard destination cannot stand for that receipt. |
| 4 conditions swallowing later sentences | Illuminate's two kicker checks are independent. Long Rest exiles itself even when the life-setting condition is false. Old Rutstein checks land, creature and other milled cards in independent sentences. |
| 1 Superior Foes payment continuation | Its old window named the triggering spell/event subject instead of the creature named after "with". Unsupported payment forms must not substitute that event subject for the card being played. |
| 1 White Sun's Twilight token recipe | The old tail consumed the token's quoted inability to block while omitting that static ability. The full token recipe retains it. |

The 13 creator alternatives belong to Araña, Heart of the Spider; Armory
Paladin; Capricious Sliver; Cleon, Merry Champion; Grotag Night-Runner; Pyre of
the World Tree; Kami of Celebration; Kellan, Planar Trailblazer; Moria Marauder;
Prophetic Flamespeaker; Tempered in Solitude; U.S.S. Enterprise-D, Galaxy-Class;
and Wiccan, Young Avenger. This is a constructor normal form for the creator
role, not a global alias between Source and EventSubject. Granted ability
holders and event frames remain independent.

The seven card alternatives belong to Chandra, Dressed to Kill; Hidetsugu and
Kairi; Kianne, Dean of Substance; Rundvelt Hordemaster; Scrib Nibblers; and two
Vance's Blasting Cannons readings. Chandra also retains permission from Exile;
the old generic graveyard permission did not describe that card's text.
The four sentence-scope alternatives are one Illuminate reading, one Long Rest
reading and two Old Rutstein readings.

During development, three duplicate PlayerAction payments bypassed the existing
hand-card Move-cost guard after a new receipt annotation wrapped the movement.
That guard now treats the annotation transparently. A Kianne movement without
its Exile zone guard and a Superior Foes permission naming the triggering event
instead of the exiled card were also rejected or corrected. These intermediate
additions are not accepted as final supported readings.

The model schema and vocabulary are unchanged. U0033 is the only reference
amendment in this batch: 28 operands on 21 faces. Other reviewed references
already had the correct creator, receipt, scope and token ability. The tracked
regressions include 24 original followup faces and all 21 normalized casting-X
references (20 exact-print controls and the pre-existing Shatterskull gap), independently varying loyalty/damage/
target count, edited numeric comparisons, and an observable prefix before a
destroyed object's Before-controller is used for a reveal-and-cast continuation.
