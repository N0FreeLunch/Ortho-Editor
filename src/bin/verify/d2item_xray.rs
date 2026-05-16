use anyhow::{Context, Result};
use d2r_core::item::{HuffmanTree, Item};
use d2r_core::save::Save;
use d2r_core::verify::args::{ArgParser, ArgSpec};
use std::env;
use std::fs;

fn main() -> Result<()> {
    let mut parser = ArgParser::new("d2item_xray");
    parser.add_spec(ArgSpec::positional("fixture", "Path to the savegame fixture (.d2s)"));
    parser.add_spec(ArgSpec::option("compare", Some('c'), Some("compare"), "Path to the second fixture to compare against"));
    parser.add_spec(ArgSpec::flag("verbose", Some('v'), Some("verbose"), "Show detailed segment bitstreams"));

    let args: Vec<_> = env::args_os().skip(1).collect();
    use d2r_core::verify::args::ArgError;
    let parsed = match parser.parse(args) {
        Ok(p) => p,
        Err(ArgError::Help(h)) => {
            println!("{}", h);
            std::process::exit(0);
        }
        Err(ArgError::Error(e)) => {
            eprintln!("error: {}", e);
            eprintln!("\n{}", parser.usage());
            std::process::exit(1);
        }
    };

    let fixture_path = parsed.get("fixture").cloned().context("Fixture path required")?;
    let compare_path = parsed.get("compare").cloned();
    let verbose = parsed.is_set("verbose");

    // Force tracing on to collect segments
    unsafe {
        env::set_var("D2R_ITEM_TRACE", "1");
    }

    let bytes = fs::read(&fixture_path)
        .with_context(|| format!("Failed to read fixture: {}", fixture_path))?;
    
    let save = Save::from_bytes(&bytes)
        .context("Failed to parse save header")?;
    
    let huffman = HuffmanTree::new();
    let is_alpha = save.header.version == 105;
    
    println!("=== d2item_xray: {} ===", fixture_path);
    println!("Version: {}, Alpha: {}", save.header.version, is_alpha);

    let items = Item::read_player_items(bytes.as_slice(), &huffman, is_alpha)
        .map_err(|e| anyhow::anyhow!("{}", e))
        .context("Failed to read items from fixture")?;

    let compare_items = if let Some(path) = &compare_path {
        let compare_bytes = fs::read(path)
            .with_context(|| format!("Failed to read comparison fixture: {}", path))?;
        let items = Item::read_player_items(compare_bytes.as_slice(), &huffman, is_alpha)
            .map_err(|e| anyhow::anyhow!("{}", e))
            .context("Failed to read items from comparison fixture")?;
        Some(items)
    } else {
        None
    };

    println!("Found {} items in fixture.", items.len());

    if let Some(c_items) = &compare_items {
        println!("Found {} items in comparison fixture.", c_items.len());
        
        use d2r_core::verify::sba::flatten_item;
        let mut exp_flattened = Vec::new();
        for (i, item) in items.iter().enumerate() {
            flatten_item(item, &i.to_string(), &mut exp_flattened);
        }
        
        let mut act_flattened = Vec::new();
        for (i, item) in c_items.iter().enumerate() {
            flatten_item(item, &i.to_string(), &mut act_flattened);
        }

        for exp in &exp_flattened {
            if let Some(act) = act_flattened.iter().find(|a| a.path == exp.path && a.code == exp.code) {
                println!("\nItem {} [{}]: diffing segments", exp.path, exp.code);
                
                let max_segments = exp.segments.len().max(act.segments.len());
                for j in 0..max_segments {
                    let e_seg = exp.segments.get(j);
                    let a_seg = act.segments.get(j);
                    
                    match (e_seg, a_seg) {
                        (Some(e), Some(a)) => {
                            let status = if e.label == a.label && e.start == a.start && e.end == a.end {
                                // Check bits
                                let e_bits = &exp.bits[e.start as usize..e.end as usize];
                                let a_bits = &act.bits[a.start as usize..a.end as usize];
                                if e_bits == a_bits { " [PASS]" } else { " [VALUE DIFF]" }
                            } else {
                                " [STRUCT DIFF]"
                            };
                            
                            println!("  Segment #{}: {} vs {}{}", j, e.label, a.label, status);
                            if status != " [PASS]" || verbose {
                                let e_bits_str: String = exp.bits[e.start as usize..e.end as usize].iter().map(|&b| if b { '1' } else { '0' }).collect();
                                let a_bits_str: String = act.bits[a.start as usize..a.end as usize].iter().map(|&b| if b { '1' } else { '0' }).collect();
                                println!("    EXP: {}", e_bits_str);
                                println!("    ACT: {}", a_bits_str);
                            }
                        },
                        (Some(e), None) => println!("  Segment #{}: {} vs (MISSING)", j, e.label),
                        (None, Some(a)) => println!("  Segment #{}: (MISSING) vs {}", j, a.label),
                        (None, None) => unreachable!(),
                    }
                }
            } else {
                println!("\nItem {} [{}]: missing in comparison", exp.path, exp.code);
            }
        }
    } else {
        for (idx, item) in items.iter().enumerate() {
            println!("\nItem #{} [{}]: bits {}-{} (len={})", 
                idx, item.code, item.range.start, item.range.end, item.range.end - item.range.start);
            
            // Re-serialize to check for symmetry
            let _reserialized = item.to_bytes(0, &huffman, is_alpha).map_err(|e| anyhow::anyhow!("{}", e))?;
            
            println!("  Segments ({}):", item.segments.len());
            for seg in &item.segments {
                let indent = "  ".repeat(seg.depth + 1);
                let len = seg.end - seg.start;
                print!("{}[{:>4}..{:>4}] (len={:>2}) {:<20}", indent, seg.start, seg.end, len, seg.label);
                
                if verbose {
                    // Show bits
                    let mut bit_str = String::new();
                    for i in seg.start..seg.end {
                        if i < item.bits.len() as u64 {
                            bit_str.push(if item.bits[i as usize].bit { '1' } else { '0' });
                        }
                    }
                    print!(" | {}", bit_str);
                }
                println!();
            }

            for (c_idx, child) in item.socketed_items.iter().enumerate() {
                println!("    Socketed #{} [{}]: bits {}-{}", c_idx, child.code, child.range.start, child.range.end);
            }
        }
    }

    Ok(())
}
