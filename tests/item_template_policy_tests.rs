use d2r_core::data::item_codes::{ItemTemplate, ITEM_TEMPLATES};
use d2r_core::data::repository::DataRepository;
use d2r_core::domain::item::serialization::item_template;
use d2r_macros::hypothesis_test;
use serde_json::{json, Value};

fn fields(t: &ItemTemplate) -> Value {
    json!({
        "code": t.code, "name": t.name, "item_type": t.item_type,
        "item_type2": t.item_type2, "width": t.width, "height": t.height,
        "is_armor": t.is_armor, "is_weapon": t.is_weapon,
        "is_stackable": t.is_stackable, "max_stack": t.max_stack,
        "has_durability": t.has_durability,
        "has_quest_difficulty": t.has_quest_difficulty,
    })
}

#[hypothesis_test]
fn legacy_template_public_lookup_preserves_observed_results() {
    // Freeze current public lookup behavior, including unresolved legacy misses.
    assert_eq!(item_template("us g").unwrap().code, "jav");
    assert_eq!(item_template(" \tus g\n").unwrap().code, "jav");
    for code in ["wa2", "rhd", " wa2 ", "rhd\t", "", "US G", "ww"] {
        assert!(item_template(code).is_none(), "legacy input {code:?}");
    }
    assert_eq!(item_template("gcw").unwrap().code, "gcw");
    assert_eq!(item_template("gcw").unwrap().name, "Chipped Diamond");
    assert_eq!(item_template("pik ").unwrap().code, "pik");
    assert_eq!(item_template("cap ").unwrap().code, "cap");
    assert!(DataRepository::get_item_template("us g").is_none());
    assert!(DataRepository::get_item_template("pik ").is_none());
    assert_eq!(
        DataRepository::get_item_template("pik").unwrap().code,
        "pik"
    );

    let mut inputs = Vec::new();
    for t in ITEM_TEMPLATES {
        inputs.extend([
            t.code.to_owned(),
            format!(" {} ", t.code),
            format!("\t{}\r\n", t.code),
            format!("\u{2003}{}\u{3000}", t.code),
        ]);
    }
    let edges = [
        "us g",
        "us g ",
        "\tus g\n",
        "wa2",
        "wa2 ",
        "rhd",
        " rhd\t",
        "jav",
        "pik",
        "pik ",
        "cap",
        "cap ",
        "",
        " ",
        "plain",
        "US G",
        "WA2",
        "RHD",
        "ww",
        "gcw",
        "r13",
        "r15",
        "us  g",
        "us\tg",
        "us g\0",
        "unlisted_template_code",
    ];
    inputs.extend(edges.iter().map(|s| s.to_string()));
    let rows: Vec<Value> = inputs
        .iter()
        .map(|code| {
            let actual = item_template(code);
            let projected = actual.map(fields);
            let first_index = actual.map(|value| {
                let index = ITEM_TEMPLATES
                    .iter()
                    .position(|t| t.code == value.code)
                    .unwrap();
                assert_eq!(
                    fields(value),
                    fields(&ITEM_TEMPLATES[index]),
                    "first matching row for {code:?}"
                );
                index
            });
            json!({"input": code, "template": projected, "first_matching_index": first_index})
        })
        .collect();
    assert_eq!(rows.len(), ITEM_TEMPLATES.len() * 4 + edges.len());
    let report = json!({"schema":1, "generated_rows":ITEM_TEMPLATES.len(),
        "lookup_calls":rows.len(), "whitespace_modes":4, "edges":edges.len(), "rows":rows});
    if let Some(output) = std::env::var_os("D2R_REFACTOR_TEMPLATE_OUTPUT") {
        let path = std::path::PathBuf::from(output);
        assert!(!path.exists());
        std::fs::write(path, serde_json::to_vec_pretty(&report).unwrap()).unwrap();
    }
}
