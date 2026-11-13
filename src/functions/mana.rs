//! A mana-option set has one meaning and finitely many printed orders.
use crate::model::ir::ManaColor;
use crate::pattern::{Bindings, Pattern};
use serde_json::Value;

fn rank(color: &ManaColor) -> u8 {
    match color {
        ManaColor::White => 0,
        ManaColor::Blue => 1,
        ManaColor::Black => 2,
        ManaColor::Red => 3,
        ManaColor::Green => 4,
        ManaColor::Colorless => 5,
        ManaColor::Chosen => 6,
    }
}

fn colors(value: &Value) -> Result<Vec<ManaColor>, String> {
    let list = value.as_array().ok_or("mana options need a color list")?;
    if list.is_empty() || list.len() > 5 {
        return Err("mana options need one to five distinct colors".into());
    }
    let mut colors: Vec<ManaColor> =
        serde_json::from_value(value.clone()).map_err(|e| e.to_string())?;
    colors.sort_by_key(rank);
    if colors.windows(2).any(|pair| pair[0] == pair[1]) {
        return Err("mana options repeat a color".into());
    }
    Ok(colors)
}

pub(crate) fn mana_options(value: &Value) -> Result<Value, String> {
    serde_json::to_value(colors(value)?).map_err(|e| e.to_string())
}

fn permutations(colors: &mut [ManaColor], at: usize, out: &mut Vec<Value>) {
    if at == colors.len() {
        out.push(serde_json::to_value(colors).expect("native colors serialize"));
        return;
    }
    for i in at..colors.len() {
        colors.swap(at, i);
        permutations(colors, at + 1, out);
        colors.swap(at, i);
    }
}

pub(crate) fn unify_mana_options<'p, 'v>(
    args: &'p [Pattern],
    value: &'v Value,
    bindings: Bindings<'p, 'v>,
) -> Vec<Bindings<'p, 'v>> {
    let [inner] = args else {
        return Vec::new();
    };
    let Ok(mut colors) = colors(value) else {
        return Vec::new();
    };
    let mut orders = Vec::new();
    // Five distinct colors have at most 120 orders, independent of input size.
    permutations(&mut colors, 0, &mut orders);
    orders
        .iter()
        .flat_map(|order| inner.unify_made(order, &bindings))
        .collect()
}

pub(crate) fn unify_mana<'p, 'v>(
    name: &str,
    args: &'p [Pattern],
    value: &'v Value,
    bindings: Bindings<'p, 'v>,
) -> Vec<Bindings<'p, 'v>> {
    match name {
        "mana_options" => unify_mana_options(args, value, bindings),
        "literal_mana_sequence" => super::literal_mana::unify(args, value, bindings),
        "fixed_mana" | "variable_mana" => super::unify_cost_guard(name, args, value, bindings),
        _ => Vec::new(),
    }
}
