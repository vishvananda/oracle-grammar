# A player's last turn in event history

Arboria distinguishes a player who put a nontoken permanent onto the battlefield
from the player under whose control that permanent entered. For example, a
Flickerwisp controlled by one player can return another player's permanent under
its owner's control. Arboria's published ruling explicitly says it cares who
put the permanent onto the battlefield.

MTGish Source preserves this distinction, but its Rust reducer currently maps
both `PutAPermanentOnBattleDuringTheirLastTurn` and
`HadAPermanentEnterTheBattlefieldUnderTheirControlLastTurn` to
`PlayerHistory::PermanentEntered`. The backend interprets that as the entering
controller. Changing that emitter to use the actor would break the other Source
form. Cantrip already records the actor in `ZoneChange::put_by`; no new engine
feature is needed.

The generic `Expr::EventHistory` already represents the required
`PlayerEvent::PutsOntoBattlefield`, including the player and nontoken filter.
Its window previously could not name that player's most recent completed turn:
`LastTurn` means the immediately preceding global turn, while
`SinceLastTurnOf` selects a different interval. `HistoryWindow::LastTurnOf`
preserves the distinction, including multiplayer games, extra turns and a
player who has not yet had a turn. Existing `ObjectWindow::LastTurnOf` establishes
the same meaning for the older history forms.

The new variant appends to the enum. All five existing binary tags retain
their values. Its payload has the same type as `SinceLastTurnOf`, so it does not
increase the existing enum storage bound or add a heap allocation. Schema 191
freezes exactly that one descriptor addition. Both named history windows now
validate their player bindings in event counts and `Moment::LastEvent`; the
older validator had not checked the window's own references.

Adding a public enum variant changes the schema and breaks exhaustive Rust
matches, so this prepares model/contract/grammar version 0.2.0 rather than
silently changing 0.1.0. The published 0.1.0 crates and face bundles remain
immutable. The publishing owner must review and release the new packages
before MTGish switches its registry pins; this change does not introduce local
dependency replacements or publish a release.

After publication, the consumer correction should lower the "put" form to an
occurrence test over `PutsOntoBattlefield` and `LastTurnOf(the_player)`, retain
controller semantics for the distinct "entered under their control" form, and
prove the differing-player case in a native scenario. The grammar's MTGish
reference update belongs to that consumer adoption, with explicit changed-face
evidence. The original Source parse for Arboria is correct and does not require
an upstream MTGish parser bug report.
