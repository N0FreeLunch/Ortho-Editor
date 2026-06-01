use d2r_core::item::{HuffmanTree, Item};
use d2r_core::verify::args::{ArgError, ArgParser, ArgSpec};
use serde::Serialize;
use std::env;
use std::fs;

#[derive(Serialize)]
struct ProbeResult {
    status: String,
    offset: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    item_index: Option<usize>,
    #[serde(skip_serializing_if = "Option::is_none")]
    item_code: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    label: Option<String>,
}

fn main() -> anyhow::Result<()> {
    let mut parser = ArgParser::new("d2item_probe");
    parser = parser.description("Probes bit-level semantics of items in a D2R save file.");
    parser.add_spec(ArgSpec::positional(
        "fixture",
        "Path to the .d2s fixture file",
    ));
    parser.add_spec(
        ArgSpec::option(
            "offset",
            Some('o'),
            Some("offset"),
            "Bit offset to probe (relative to JM payload)",
        )
        .required(),
    );

    let parsed = match parser.parse(env::args_os().skip(1).collect()) {
        Ok(p) => p,
        Err(ArgError::Help(h)) => {
            println!("{}", h);
            return Ok(());
        }
        Err(ArgError::Error(e)) => {
            anyhow::bail!("error: {}\n\n{}", e, parser.usage());
        }
    };

    let fixture_path = parsed.get("fixture").unwrap();
    let offset_str = parsed.get("offset").unwrap();
    let offset: u64 = offset_str
        .parse()
        .map_err(|_| anyhow::anyhow!("Invalid offset: {}", offset_str))?;
    let is_json = parsed.is_json();

    let bytes = fs::read(fixture_path)?;
    let huffman = HuffmanTree::new();
    // Defaulting to alpha=true for D2R as per typical verifier patterns
    let alpha = true;

    let items = match Item::read_player_items(&bytes, &huffman, alpha) {
        Ok(items) => items,
        Err(e) => {
            if is_json {
                println!(
                    "{}",
                    serde_json::to_string(&ProbeResult {
                        status: "error_parsing_items".to_string(),
                        offset,
                        item_index: None,
                        item_code: None,
                        label: Some(format!("{:?}", e)),
                    })?
                );
                return Ok(());
            } else {
                anyhow::bail!("Failed to read player items: {:?}", e);
            }
        }
    };

    let mut found_item = None;
    for (i, item) in items.iter().enumerate() {
        if offset >= item.range.start && offset < item.range.end {
            found_item = Some((i, item));
            break;
        }
    }

    if let Some((idx, item)) = found_item {
        let semantic = item.query_bit(offset);
        let result = ProbeResult {
            status: if semantic.is_some() {
                "ok".to_string()
            } else {
                "unmapped".to_string()
            },
            offset,
            item_index: Some(idx),
            item_code: Some(item.code.clone()),
            label: semantic.map(|s| s.label),
        };

        if is_json {
            println!("{}", serde_json::to_string(&result)?);
        } else {
            println!("Hit found at offset {}:", offset);
            println!("  Item Index: {}", idx);
            println!("  Item Code:  {}", item.code);
            println!(
                "  Label:      {}",
                result.label.as_deref().unwrap_or("Unmapped/Unknown")
            );
        }
    } else {
        let result = ProbeResult {
            status: "out_of_item_range".to_string(),
            offset,
            item_index: None,
            item_code: None,
            label: None,
        };
        if is_json {
            println!("{}", serde_json::to_string(&result)?);
        } else {
            println!("Offset {} is out of range for all items.", offset);
        }
    }

    Ok(())
}
