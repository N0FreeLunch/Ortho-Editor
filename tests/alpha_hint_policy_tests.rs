use d2r_core::item::normalize_alpha_code_hint;
use d2r_macros::hypothesis_test;
use serde_json::{Value, json};

fn observe(input: &str, expected: &str, borrowed: bool) -> Value {
    let owned = input.to_owned();
    let trimmed = owned.trim();
    let actual = normalize_alpha_code_hint(&owned);
    let borrows_trimmed = actual.as_ptr() == trimmed.as_ptr() && actual.len() == trimmed.len();
    assert_eq!(actual, expected, "value for {input:?}");
    assert_eq!(borrows_trimmed, borrowed, "return provenance for {input:?}");
    json!({"input": input, "output": actual, "borrows_trimmed": borrows_trimmed})
}

fn digest_bytes(digest: &mut u64, bytes: &[u8]) {
    for byte in (bytes.len() as u64).to_le_bytes().iter().chain(bytes) {
        *digest ^= u64::from(*byte);
        *digest = digest.wrapping_mul(0x100000001b3);
    }
}

#[hypothesis_test]
fn legacy_global_hint_preserves_values_and_return_provenance() {
    // Freeze source-observed behavior independently of the migrated Data policy.
    let mut edge_rows = Vec::new();
    for (input, expected) in [
        ("us g", "jav"),
        ("k g", "jav"),
        ("7 p", "ks d"),
        ("80sc", "ks d"),
        ("lbl", "b7ts"),
        ("whp1", "hp1"),
        ("wmp1", "mp1"),
        ("mp1", "mp1"),
    ] {
        for padded in [
            input.to_owned(),
            format!(" {input} "),
            format!("\t{input}\r\n"),
            format!("\u{2003}{input}\u{3000}"),
        ] {
            edge_rows.push(observe(&padded, expected, false));
        }
    }
    // These are misses only in the global hint alias domain; gcw is a real item.
    for input in [
        "",
        " ",
        "\t\r\n",
        "\u{2003}\u{3000}",
        "ww",
        "gcw",
        "wa2",
        "rhd",
        "c8xr",
        "wucb",
        "g71",
        "wl l",
        "US G",
        "MP1",
        "WMP1",
        "us  g",
        "us\tg",
        "us g\0",
        "mp1\0",
        "hp1",
        " hp1 ",
        "jav",
        "ks d",
        "b7ts",
        "\u{cf}",
        "\u{cf}o",
        "\0",
        "unlisted_hint",
    ] {
        edge_rows.push(observe(input, input.trim(), true));
    }
    for (input, expected) in [
        ("\u{cf}O", "hp1"),
        ("\u{cf}M", "mp1"),
        (" \u{1cf}\u{14f}!\t", "hp1"),
        ("\u{100cf}\u{1004d}tail", "mp1"),
        ("\u{cf}O\0", "hp1"),
        ("\u{cf}M\u{3000}", "mp1"),
    ] {
        edge_rows.push(observe(input, expected, false));
    }

    let planes = [0, 0x100, 0x10000];
    let mut cases = 0_u64;
    let mut hp1_hits = 0_u64;
    let mut mp1_hits = 0_u64;
    let mut digest = 0xcbf29ce484222325_u64;
    for first_plane in planes {
        for second_plane in planes {
            for first in 0..=255 {
                for second in 0..=255 {
                    let input = format!(
                        "{}{}!",
                        char::from_u32(first_plane + first).unwrap(),
                        char::from_u32(second_plane + second).unwrap()
                    );
                    let trimmed = input.trim();
                    let mut chars = trimmed.chars();
                    let pair = (
                        chars.next().map(|c| c as u32 as u8),
                        chars.next().map(|c| c as u32 as u8),
                    );
                    let (expected, borrowed) = match pair {
                        (Some(0xcf), Some(0x4f)) => {
                            hp1_hits += 1;
                            ("hp1", false)
                        }
                        (Some(0xcf), Some(0x4d)) => {
                            mp1_hits += 1;
                            ("mp1", false)
                        }
                        _ => (trimmed, true),
                    };
                    let actual = normalize_alpha_code_hint(&input);
                    let borrows_trimmed =
                        actual.as_ptr() == trimmed.as_ptr() && actual.len() == trimmed.len();
                    assert_eq!(actual, expected, "matrix value {input:?}");
                    assert_eq!(borrows_trimmed, borrowed, "matrix provenance {input:?}");
                    digest_bytes(&mut digest, input.as_bytes());
                    digest_bytes(&mut digest, actual.as_bytes());
                    digest_bytes(&mut digest, &[u8::from(borrows_trimmed)]);
                    cases += 1;
                }
            }
        }
    }
    assert_eq!(cases, 589824);
    assert_eq!((hp1_hits, mp1_hits), (9, 9));
    let report = json!({
        "schema": 1,
        "evidence": "source-observed preservation, not universal format truth",
        "edge_rows": edge_rows,
        "prefix_matrix": {
            "protocol": "independent first/second planes; all low-byte pairs; suffix !; trim before scalar low-byte interpretation",
            "planes": planes,
            "cases": cases,
            "hits": hp1_hits + mp1_hits,
            "hp1": hp1_hits,
            "mp1": mp1_hits,
            "borrowed_fallback": cases - hp1_hits - mp1_hits,
            "digest_algorithm": "FNV-1a64 with u64le length-framed input/output/borrow flag",
            "digest": format!("{digest:016x}"),
            "failures": [],
        }
    });
    if let Some(output) = std::env::var_os("D2R_REFACTOR_HINT_OUTPUT") {
        let path = std::path::PathBuf::from(output);
        assert!(path.parent().unwrap().is_dir());
        let mut file = std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(path)
            .unwrap();
        serde_json::to_writer_pretty(&mut file, &report).unwrap();
    }
}
