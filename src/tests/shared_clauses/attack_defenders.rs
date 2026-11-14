//! A trigger on attacking a player names the attacked player as `that player`: the
//! event's second player when a player attacks, else the event's own player.
use super::*;

#[test]
fn that_player_is_the_one_attacked() {
    for (text, player) in [
        (
            "Whenever you attack a player, target creature that's attacking that player gains first strike until end of turn.",
            "EventOtherPlayer",
        ),
        (
            "Whenever one or more creatures you control that entered this turn attack a player, target creature that player controls can't block this turn.",
            "EventSubject",
        ),
    ] {
        let reading = compositional(text);
        let filter = reading[0]["args"]["announcement"]["targets"][0]["filter"].to_string();
        let named = json!({"kind": "One", "args": {"kind": player}}).to_string();
        assert!(filter.contains(&named), "{text}: {filter}");
        assert_eq!(printed(&reading), text);
    }
}
