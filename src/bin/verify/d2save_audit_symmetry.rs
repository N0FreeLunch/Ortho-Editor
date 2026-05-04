use d2r_core::data::quests::V105_QUESTS;
use d2r_core::data::waypoints::WAYPOINTS;
use std::process;

fn main() {
    println!("--- Alpha v105 Grounding Symmetry Audit ---");
    let mut failures = 0;

    // Legacy grounding anchor for metadata verification.
    // Discussion 0230 verified that physical truth shifted to 0x78 (448),
    // but generated metadata in d2r-data still uses the 415 hypothesis.
    const METADATA_GROUNDING_ANCHOR: usize = 415;

    // 1. Quest Audit
    println!("Auditing Alpha v105 Quests...");
    for (i, entry) in V105_QUESTS.iter().enumerate() {
        if entry.v105_offset < METADATA_GROUNDING_ANCHOR {
            println!("  [FAIL] Quest[{}]: {} offset {} is below grounding anchor {}", i, entry.name, entry.v105_offset, METADATA_GROUNDING_ANCHOR);
            failures += 1;
        } else {
            let relative = entry.v105_offset - METADATA_GROUNDING_ANCHOR;
            if relative % 2 != 0 {
                // Verified in Discussion 0230: Alpha v105 uses 2-byte stride (u16)
                println!("  [FAIL] Quest[{}]: {} offset {} is not 2-byte aligned relative to anchor {}", i, entry.name, entry.v105_offset, METADATA_GROUNDING_ANCHOR);
                failures += 1;
            }
        }
    }
    if failures == 0 {
        println!("  [OK] Quest grounding looks consistent.");
    }

    // 2. Waypoint Audit
    println!("Auditing Alpha v105 Waypoints...");
    // Alpha v105 has 24 bytes per difficulty for waypoints (192 bits).
    let max_ws_bit = 24 * 8; 
    for (i, entry) in WAYPOINTS.iter().enumerate() {
        if entry.ws_bit as usize >= max_ws_bit {
            println!("  [FAIL] Waypoint[{}]: {} ws_bit {} exceeds difficulty stride {}", i, entry.name, entry.ws_bit, max_ws_bit);
            failures += 1;
        }
    }
    if failures == 0 {
        println!("  [OK] Waypoint bit projections look consistent.");
    }

    if failures > 0 {
        println!("\nAudit FAILED with {} error(s).", failures);
        process::exit(1);
    } else {
        println!("\nAudit PASSED.");
    }
}