//! Temporary changes retain the operands that produced their characteristics.
use super::*;

#[test]
fn a_mana_color_change_uses_the_produced_mana_result() {
    let mut frame = compositional(
        "Draw a card. Add one mana of any color. This creature becomes that color until end of turn.",
    );
    let body = &mut frame[0]["args"]["body"]["instructions"];
    assert_eq!(
        body[2]["args"]["operation"]["args"]["changes"][0]["args"]["mana"],
        body[1]["args"]["result"]
    );
    for option in body[1]["args"]["operation"]["args"]["mana"]["ChooseOne"]
        .as_array_mut()
        .unwrap()
    {
        option["Amount"]["amount"] = json!({"kind":"Constant","args":2});
    }
    let text = printed_types(&frame, &["Sorcery"]).to_lowercase();
    assert!(text.contains("that color"), "{text}");
    assert!(
        text.contains("two mana") || text.contains("2 mana"),
        "{text}"
    );
}

#[test]
fn a_pump_and_type_replacement_print_edited_creature_types() {
    let mut frame =
        compositional("This creature gets +3/+3 and becomes a Bear Berserker until end of turn.");
    let changes =
        &mut frame[0]["args"]["body"]["instructions"][0]["args"]["operation"]["args"]["changes"];
    changes[1]["args"]["types"]["Listed"] = json!(["Elf", "Wizard"]);
    let text = printed_types(&frame, &["Creature"]);
    assert!(text.contains("Elf") && text.contains("Wizard"), "{text}");
    assert!(
        !text.contains("Bear") && !text.contains("Berserker"),
        "{text}"
    );
}
