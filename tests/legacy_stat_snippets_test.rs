// Micro-stat snippet unit test harness
// Provides ultra-fast in-memory parsing and verification for standard 1.10+ and Alpha v105 stat bitstreams.

use bitstream_io::{BitRead, BitReader, BitWrite, BitWriter, LittleEndian};
use d2r_core::data::stat_costs::STAT_COSTS;
use d2r_core::domain::stats::{lookup_alpha_map_by_raw, stat_save_bits};
use d2r_core::init_rayon_thread_pool;
use rayon::prelude::*;
use std::io::Cursor;

/// Represents a decoded stat entry from a micro-snippet.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DecodedStatSnippet {
    pub stat_id: u32,
    pub name: String,
    pub param: u32,
    pub raw_value: i32,
    pub value: i32,
}

/// Encodes a sequence of stat entries into a standard 1.10+ raw bitstream snippet.
/// Appends a 9-bit terminator (0x1FF) and byte-aligns the final stream.
pub fn encode_standard_stat_snippet(entries: &[(u32, u32, i32)]) -> Vec<u8> {
    let mut buffer = Vec::new();
    {
        let mut writer = BitWriter::endian(&mut buffer, LittleEndian);
        for &(stat_id, param, value) in entries {
            // Write 9-bit stat ID
            writer.write_var(9, stat_id).expect("bit write stat_id");
            let stat_def = STAT_COSTS.iter().find(|s| s.id == stat_id);
            if let Some(def) = stat_def {
                if def.save_param_bits > 0 {
                    writer
                        .write_var(def.save_param_bits as u32, param)
                        .expect("bit write param");
                }
                let mut raw_val = value.wrapping_add(def.save_add) as u32;
                if def.save_bits > 0 {
                    let mask = if def.save_bits >= 32 {
                        !0u32
                    } else {
                        (1u32 << def.save_bits) - 1
                    };
                    raw_val &= mask;
                    writer
                        .write_var(def.save_bits as u32, raw_val)
                        .expect("bit write value");
                }
            } else {
                // Fallback default 9-bit value for undefined stats
                writer.write_var(9, value as u32).expect("bit write fallback");
            }
        }
        // Write 9-bit terminator (511)
        writer.write_var(9, 0x1FFu32).expect("bit write terminator");
        writer.byte_align().expect("byte align");
    }
    buffer
}

/// Parses a standard 1.10+ raw stat snippet from bytes into decoded properties.
pub fn parse_standard_stat_snippet(bytes: &[u8]) -> Vec<DecodedStatSnippet> {
    let mut reader = BitReader::endian(Cursor::new(bytes), LittleEndian);
    let mut results = Vec::new();

    while let Ok(stat_id) = reader.read_var::<u32>(9) {
        if stat_id == 0x1FF {
            break;
        }

        let stat_def = STAT_COSTS.iter().find(|s| s.id == stat_id);
        let (param, raw_value, value, name) = if let Some(def) = stat_def {
            let p = if def.save_param_bits > 0 {
                reader.read_var::<u32>(def.save_param_bits as u32).unwrap_or(0)
            } else {
                0
            };
            let raw = if def.save_bits > 0 {
                reader.read_var::<u32>(def.save_bits as u32).unwrap_or(0) as i32
            } else {
                0
            };
            let val = raw.wrapping_sub(def.save_add);
            (p, raw, val, def.name.to_string())
        } else {
            let raw = reader.read_var::<u32>(9).unwrap_or(0) as i32;
            (0, raw, raw, format!("stat_{stat_id}"))
        };

        results.push(DecodedStatSnippet {
            stat_id,
            name,
            param,
            raw_value,
            value,
        });
    }

    results
}

/// Reads a single standard stat block from an active BitReader until 0x1FF terminator.
pub fn read_standard_stat_block(reader: &mut BitReader<Cursor<&Vec<u8>>, LittleEndian>) -> Vec<DecodedStatSnippet> {
    let mut results = Vec::new();

    while let Ok(stat_id) = reader.read_var::<u32>(9) {
        if stat_id == 0x1FF {
            break;
        }

        let stat_def = STAT_COSTS.iter().find(|s| s.id == stat_id);
        let (param, raw_value, value, name) = if let Some(def) = stat_def {
            let p = if def.save_param_bits > 0 {
                reader.read_var::<u32>(def.save_param_bits as u32).unwrap_or(0)
            } else {
                0
            };
            let raw = if def.save_bits > 0 {
                reader.read_var::<u32>(def.save_bits as u32).unwrap_or(0) as i32
            } else {
                0
            };
            let val = raw.wrapping_sub(def.save_add);
            (p, raw, val, def.name.to_string())
        } else {
            let raw = reader.read_var::<u32>(9).unwrap_or(0) as i32;
            (0, raw, raw, format!("stat_{stat_id}"))
        };

        results.push(DecodedStatSnippet {
            stat_id,
            name,
            param,
            raw_value,
            value,
        });
    }

    results
}

/// Encodes a charged skill entry (Stat 204: `item_charged_skill`).
/// Param (16 bits): skill_id (9 bits) | (skill_level (6 bits) << 9)
/// Value (16 bits): current_charges (8 bits) | (max_charges (8 bits) << 8)
pub fn make_charged_skill_entry(
    skill_id: u32,
    skill_level: u32,
    current_charges: u32,
    max_charges: u32,
) -> (u32, u32, i32) {
    let param = (skill_id & 0x1FF) | ((skill_level & 0x3F) << 9);
    let val = ((current_charges & 0xFF) | ((max_charges & 0xFF) << 8)) as i32;
    (204, param, val)
}

/// Encodes a skill proc entry (e.g. Stat 195 `item_skillonattack`, 198 `item_skillonhit`, 201 `item_skillongethit`).
/// Param (16 bits): skill_id (9 bits) | (skill_level (6 bits) << 9)
/// Value (7 bits): chance percentage
pub fn make_proc_skill_entry(
    stat_id: u32,
    skill_id: u32,
    skill_level: u32,
    chance_pct: i32,
) -> (u32, u32, i32) {
    let param = (skill_id & 0x1FF) | ((skill_level & 0x3F) << 9);
    (stat_id, param, chance_pct)
}

/// Encodes an equipped aura entry (Stat 151: `item_aura`).
/// Param (9 bits): skill_id
/// Value (5 bits): aura level
pub fn make_aura_entry(skill_id: u32, aura_level: i32) -> (u32, u32, i32) {
    (151, skill_id, aura_level)
}

/// Encodes a non-class `oSkill` entry (Stat 97: `item_nonclassskill`).
/// Param (9 bits): skill_id
/// Value (6 bits): skill bonus
pub fn make_oskill_entry(skill_id: u32, skill_bonus: i32) -> (u32, u32, i32) {
    (97, skill_id, skill_bonus)
}

/// Encodes a class skill tab entry (Stat 188: `item_addskill_tab`).
/// Param (16 bits): tab_id (3 bits) | (class_id (3 bits) << 3)
/// Value (3 bits): tab bonus
pub fn make_skill_tab_entry(class_id: u32, tab_id: u32, bonus: i32) -> (u32, u32, i32) {
    let param = (tab_id & 0x7) | ((class_id & 0x7) << 3);
    (188, param, bonus)
}

/// Encodes an Alpha v105 raw stat snippet given raw Alpha stat IDs and bit widths.
pub fn encode_alpha_stat_snippet(entries: &[(u32, u32, u32)]) -> Vec<u8> {
    let mut buffer = Vec::new();
    {
        let mut writer = BitWriter::endian(&mut buffer, LittleEndian);
        for &(raw_stat_id, width, raw_value) in entries {
            writer.write_var(9, raw_stat_id).expect("bit write alpha stat_id");
            if width > 0 {
                writer.write_var(width, raw_value).expect("bit write alpha val");
            }
        }
        // Terminator (9 bits 0x1FF)
        writer.write_var(9, 0x1FFu32).expect("bit write terminator");
        writer.byte_align().expect("byte align");
    }
    buffer
}

#[test]
fn test_harness_basic_all_skills_snippet() {
    // 9-bit ID 127 (item_allskills) + 3-bit value 1 (+1 All Skills `모든 스킬`) + 9-bit 0x1FF terminator
    // ID 127 = 0b001111111 (9 bits: 1,1,1,1,1,1,1,0,0)
    // Value 1 = 0b001 (3 bits: 1,0,0)
    // Terminator = 0b111111111 (9 bits: 1,1,1,1,1,1,1,1,1)
    let encoded = encode_standard_stat_snippet(&[(127, 0, 1)]);
    let decoded = parse_standard_stat_snippet(&encoded);

    assert_eq!(decoded.len(), 1);
    assert_eq!(decoded[0].stat_id, 127);
    assert_eq!(decoded[0].name, "item_allskills");
    assert_eq!(decoded[0].value, 1);
}

#[test]
fn test_harness_strength_snippet() {
    // Stat 0 (strength `힘`), save_bits 8, save_add 32, value +5 -> raw_value 37
    let encoded = encode_standard_stat_snippet(&[(0, 0, 5)]);
    let decoded = parse_standard_stat_snippet(&encoded);

    assert_eq!(decoded.len(), 1);
    assert_eq!(decoded[0].stat_id, 0);
    assert_eq!(decoded[0].name, "strength");
    assert_eq!(decoded[0].raw_value, 37);
    assert_eq!(decoded[0].value, 5);
}

#[test]
fn test_harness_resists_snippet() {
    // Four elemental resists in one snippet:
    // Fire (39) +30%, Lightning (41) +25%, Cold (43) +20%, Poison (45) +15%
    let entries = vec![(39, 0, 30), (41, 0, 25), (43, 0, 20), (45, 0, 15)];
    let encoded = encode_standard_stat_snippet(&entries);
    let decoded = parse_standard_stat_snippet(&encoded);

    assert_eq!(decoded.len(), 4);
    assert_eq!(decoded[0].name, "fireresist");
    assert_eq!(decoded[0].value, 30);
    assert_eq!(decoded[1].name, "lightresist");
    assert_eq!(decoded[1].value, 25);
    assert_eq!(decoded[2].name, "coldresist");
    assert_eq!(decoded[2].value, 20);
    assert_eq!(decoded[3].name, "poisonresist");
    assert_eq!(decoded[3].value, 15);
}

#[test]
fn test_harness_durability_snippet() {
    // Stat 72 (durability `내구도`), Stat 73 (maxdurability `최대 내구도`)
    let entries = vec![(72, 0, 40), (73, 0, 40)];
    let encoded = encode_standard_stat_snippet(&entries);
    let decoded = parse_standard_stat_snippet(&encoded);

    assert_eq!(decoded.len(), 2);
    assert_eq!(decoded[0].stat_id, 72);
    assert_eq!(decoded[0].name, "durability");
    assert_eq!(decoded[0].value, 40);
    assert_eq!(decoded[1].stat_id, 73);
    assert_eq!(decoded[1].name, "maxdurability");
    assert_eq!(decoded[1].value, 40);
}

#[test]
fn test_harness_alpha_v105_opaque_to_standard_mappings() {
    // Test that all Alpha v105 raw stat IDs resolve to their standard 1.10+ equivalent
    let alpha_mappings = vec![
        (256, 127, "item_allskills"),
        (128, 127, "item_allskills_alpha"),
        (106, 127, "item_allskills_alt"),
        (496, 99, "item_fastergethitrate"),
        (499, 16, "item_enandefense_percent"),
        (26, 31, "item_defense_percent"),
        (312, 72, "item_durability"),
        (207, 73, "item_maxdurability"),
        (380, 194, "item_indestructible"),
        (289, 9, "maxmana"),
        (287, 9, "maxmana_alt"),
        (309, 9, "maxmana_alpha_309"),
        (114, 7, "maxlife"),
        (310, 7, "maxlife_alpha_310"),
        (311, 7, "maxlife_alpha_311"),
        (68, 20, "item_block_percent_alpha"),
        (112, 31, "item_defense_alpha"),
        (69, 45, "item_poisonresist_alpha"),
        (70, 9, "maxmana_alpha"),
        (75, 75, "item_maxdurability_percent_alpha"),
        (77, 77, "item_maxmana_percent_alpha"),
        (87, 87, "item_reducedprices_alpha"),
        (92, 92, "item_levelreq_alpha"),
        (134, 134, "item_freeze_alpha"),
    ];

    for (raw_id, expected_effective_id, expected_name_part) in alpha_mappings {
        let mapping = lookup_alpha_map_by_raw(raw_id);
        assert!(
            mapping.is_some(),
            "Alpha raw stat ID {} should be registered in forensic registry",
            raw_id
        );
        let info = mapping.unwrap();
        assert_eq!(
            info.effective_id, expected_effective_id,
            "Alpha raw stat ID {} should map to standard effective ID {}",
            raw_id, expected_effective_id
        );
        assert!(
            info.name.contains(expected_name_part),
            "Mapping name '{}' should match expected '{}'",
            info.name,
            expected_name_part
        );
    }
}

#[test]
fn test_harness_alpha_v105_micro_snippet_decoding() {
    // Construct an Alpha v105 micro-snippet with mapped stats:
    // Raw ID 256 (All Skills -> ID 127, 3 bits, save_add 0, raw 2 -> +2)
    // Raw ID 496 (Faster Hit Recovery -> ID 99, 7 bits, save_add 20, raw 50 -> +30)
    // Raw ID 26 (Enhanced Defense % -> ID 31, 11 bits, save_add 10, raw 160 -> +150)
    let alpha_snippet = encode_alpha_stat_snippet(&[
        (256, 3, 2),
        (496, 7, 50),
        (26, 11, 160),
    ]);

    // Decode using Alpha registry lookup
    let mut reader = BitReader::endian(Cursor::new(&alpha_snippet), LittleEndian);
    let mut decoded = Vec::new();
    while let Ok(raw_id) = reader.read_var::<u32>(9) {
        if raw_id == 0x1FF {
            break;
        }
        let mapping = lookup_alpha_map_by_raw(raw_id).expect("mapping must exist");
        let width = mapping
            .save_bits
            .or_else(|| stat_save_bits(mapping.effective_id))
            .unwrap_or(9);
        let raw_val = reader.read_var::<u32>(width).expect("read raw val");
        let save_add = mapping.save_add.unwrap_or_else(|| {
            STAT_COSTS
                .iter()
                .find(|s| s.id == mapping.effective_id)
                .map(|s| s.save_add)
                .unwrap_or(0)
        });
        let value = raw_val as i32 - save_add;
        decoded.push((raw_id, mapping.effective_id, mapping.name, value));
    }

    assert_eq!(decoded.len(), 3);
    assert_eq!(decoded[0].1, 127); // all_skills
    assert_eq!(decoded[0].3, 2);
    assert_eq!(decoded[1].1, 99);  // fastergethitrate
    assert_eq!(decoded[1].3, 30);
    assert_eq!(decoded[2].1, 31);  // defense percent (armorclass)
    assert_eq!(decoded[2].3, 150);
}

#[test]
fn test_harness_enigma_micro_snippet() {
    // Enigma `수수께끼` snippet:
    // +2 All Skills (127), +45% FRW (96), +1 Teleport (97), +0.75 Str/Lvl (220), +5% Max Life (76), 8% DR (36), +14 LAEK (86), 15% DTM (114), +1% MF/Lvl (240), +14 Teleport Charges (204)
    let enigma_stats = vec![
        (127, 0, 2),                              // +2 to All Skills
        (96, 0, 45),                              // +45% Faster Run/Walk
        make_oskill_entry(54, 1),                 // +1 to Teleport (skill 54)
        (220, 0, 6),                              // +0.75 Strength per Level
        (76, 0, 5),                               // Increase Maximum Life 5%
        (36, 0, 8),                               // Damage Reduced by 8%
        (86, 0, 14),                              // +14 Life After Each Kill
        (114, 0, 15),                             // 15% Damage Taken Goes to Mana
        (240, 0, 8),                              // +1% Better Chance of Getting Magic Items per Level
        make_charged_skill_entry(54, 1, 14, 14),  // Level 1 Teleport (14/14 Charges)
    ];

    let encoded = encode_standard_stat_snippet(&enigma_stats);
    let decoded = parse_standard_stat_snippet(&encoded);

    assert_eq!(decoded.len(), enigma_stats.len());
    for (actual, expected) in decoded.iter().zip(enigma_stats.iter()) {
        assert_eq!(actual.stat_id, expected.0);
        assert_eq!(actual.param, expected.1);
        assert_eq!(actual.value, expected.2);
    }
}

#[test]
fn test_harness_grief_micro_snippet() {
    // Grief `고뇌` snippet:
    // 35% Chance to cast Level 15 Venom on striking (198), +40% IAS (93), Damage +400 (111), -25% Target Defense (116), 20% Deadly Strike (141), Prevent Monster Heal (117), Ignore Target Defense (115), +2 MAEK (138), +11 LAEK (86)
    let grief_stats = vec![
        make_proc_skill_entry(198, 92, 15, 35), // 35% Chance to cast Level 15 Venom (skill 92) on striking
        (93, 0, 40),                            // +40% Increased Attack Speed
        (111, 0, 400),                          // Damage +400
        (116, 0, 25),                           // -25% Target Defense (Stat 116 value 25)
        (115, 0, 1),                            // Ignore Target's Defense
        (141, 0, 20),                           // 20% Deadly Strike
        (117, 0, 1),                            // Prevent Monster Heal
        (138, 0, 2),                            // +2 to Mana After Each Kill
        (86, 0, 11),                            // +11 Life After Each Kill
    ];

    let encoded = encode_standard_stat_snippet(&grief_stats);
    let decoded = parse_standard_stat_snippet(&encoded);

    assert_eq!(decoded.len(), grief_stats.len());
    for (actual, expected) in decoded.iter().zip(grief_stats.iter()) {
        assert_eq!(actual.stat_id, expected.0);
        assert_eq!(actual.param, expected.1);
        assert_eq!(actual.value, expected.2);
    }
}

#[test]
fn test_harness_call_to_arms_micro_snippet() {
    // Call to Arms `소집 (CTA)` snippet:
    // +1 All Skills (127), +40% IAS (93), +290% ED (16), +3 Battle Command (97), +6 Battle Orders (97), +4 Battle Cry (97), Replenish Life +12 (74), Prevent Monster Heal (117), 30% MF (80)
    let cta_stats = vec![
        (127, 0, 1),               // +1 to All Skills
        (93, 0, 40),               // +40% Increased Attack Speed
        (16, 0, 290),              // +290% Enhanced Defense / Armor
        make_oskill_entry(149, 3), // +3 to Battle Command (skill 149)
        make_oskill_entry(155, 6), // +6 to Battle Orders (skill 155)
        make_oskill_entry(154, 4), // +4 to Battle Cry (skill 154)
        (74, 0, 12),               // Replenish Life +12
        (117, 0, 1),               // Prevent Monster Heal
        (80, 0, 30),               // 30% Better Chance of Getting Magic Items
    ];

    let encoded = encode_standard_stat_snippet(&cta_stats);
    let decoded = parse_standard_stat_snippet(&encoded);

    assert_eq!(decoded.len(), cta_stats.len());
    for (actual, expected) in decoded.iter().zip(cta_stats.iter()) {
        assert_eq!(actual.stat_id, expected.0);
        assert_eq!(actual.param, expected.1);
        assert_eq!(actual.value, expected.2);
    }
}

#[test]
fn test_harness_authority_micro_snippet() {
    // Authority `권위` Alpha v105 runeword snippet (Ground Truth):
    // 10% Chance to cast Level 15 Chain of Poison on striking (198),
    // 2% Chance to cast Level 10 Mind Barrier when struck (201),
    // +2 to Class/All Skills (127),
    // +20% Faster Hit Recovery (99 - Shael),
    // +50% Enhanced Damage (17),
    // Requirements -15% (91 - Hel),
    // Fire Resist +30% (39 - Ral)
    let authority_stats = vec![
        make_proc_skill_entry(198, 120, 15, 10), // 10% Chance to cast Level 15 Chain of Poison on striking
        make_proc_skill_entry(201, 125, 10, 2),  // 2% Chance to cast Level 10 Mind Barrier when struck
        (127, 0, 2),                             // +2 to Skills
        (99, 0, 20),                             // +20% Faster Hit Recovery
        (17, 0, 50),                             // +50% Enhanced Damage
        (91, 0, 15),                             // Requirements -15%
        (39, 0, 30),                             // Fire Resist +30%
    ];

    let encoded = encode_standard_stat_snippet(&authority_stats);
    let decoded = parse_standard_stat_snippet(&encoded);

    assert_eq!(decoded.len(), authority_stats.len());
    for (actual, expected) in decoded.iter().zip(authority_stats.iter()) {
        assert_eq!(actual.stat_id, expected.0);
        assert_eq!(actual.param, expected.1);
        assert_eq!(actual.value, expected.2);
    }
}

#[test]
fn test_harness_harlequin_crest_micro_snippet() {
    // Harlequin Crest `할리퀸 관모 (샤코)` snippet:
    // +2 All Skills (127), +2 All Attributes (0/1/2/3), +1.5 Life/Lvl (216), +1.5 Mana/Lvl (217), Damage Reduced by 10% (36), 50% MF (80)
    let shako_stats = vec![
        (127, 0, 2),  // +2 to All Skills
        (0, 0, 2),    // +2 to Strength
        (1, 0, 2),    // +2 to Energy
        (2, 0, 2),    // +2 to Dexterity
        (3, 0, 2),    // +2 to Vitality
        (216, 0, 12), // +1.5 Life per Level (step 12 >> 3 = 1.5)
        (217, 0, 12), // +1.5 Mana per Level (step 12 >> 3 = 1.5)
        (36, 0, 10),  // Damage Reduced by 10%
        (80, 0, 50),  // 50% Better Chance of Getting Magic Items
    ];

    let encoded = encode_standard_stat_snippet(&shako_stats);
    let decoded = parse_standard_stat_snippet(&encoded);

    assert_eq!(decoded.len(), shako_stats.len());
    for (actual, expected) in decoded.iter().zip(shako_stats.iter()) {
        assert_eq!(actual.stat_id, expected.0);
        assert_eq!(actual.param, expected.1);
        assert_eq!(actual.value, expected.2);
    }
}

#[test]
fn test_harness_stone_of_jordan_micro_snippet() {
    // Stone of Jordan `요르단의 반지 (조던링)` snippet:
    // +1 All Skills (127), Increase Maximum Mana 25% (77), +20 Mana (9), Adds 1-12 Lightning Damage (50/51)
    let soj_stats = vec![
        (127, 0, 1),  // +1 to All Skills
        (77, 0, 25),  // Increase Maximum Mana 25%
        (9, 0, 20),   // +20 to Mana
        (50, 0, 1),   // Adds 1-12 Lightning Damage (Min)
        (51, 0, 12),  // Adds 1-12 Lightning Damage (Max)
    ];

    let encoded = encode_standard_stat_snippet(&soj_stats);
    let decoded = parse_standard_stat_snippet(&encoded);

    assert_eq!(decoded.len(), soj_stats.len());
    for (actual, expected) in decoded.iter().zip(soj_stats.iter()) {
        assert_eq!(actual.stat_id, expected.0);
        assert_eq!(actual.param, expected.1);
        assert_eq!(actual.value, expected.2);
    }
}

#[test]
fn test_harness_par_iter_batch_verification() {
    init_rayon_thread_pool();

    // Prepare 100 distinct micro-snippets across various stat categories
    let test_suite: Vec<Vec<(u32, u32, i32)>> = (0..100)
        .map(|i| {
            vec![
                (0, 0, (i % 30) + 1),                              // strength
                (2, 0, (i % 25) + 1),                              // dexterity
                (3, 0, (i % 40) + 1),                              // vitality
                (39, 0, ((i * 3) % 50) + 5),                       // fireresist
                (41, 0, ((i * 2) % 50) + 5),                       // lightresist
                (127, 0, (i % 3) + 1),                             // item_allskills
                (99, 0, ((i % 5) + 1) * 10),                       // item_fastergethitrate (FHR)
                make_proc_skill_entry(195, 42, 5, ((i % 10) + 1) * 5), // Proc Nova
                make_oskill_entry(54, (i % 3) + 1),                // oSkill Teleport
                make_charged_skill_entry(54, 1, 10, 20),           // Charged Teleport
            ]
        })
        .collect();

    // Validate in parallel using rayon par_iter
    test_suite.par_iter().for_each(|snippet_spec| {
        let encoded = encode_standard_stat_snippet(snippet_spec);
        let decoded = parse_standard_stat_snippet(&encoded);

        assert_eq!(decoded.len(), snippet_spec.len());
        for (actual, expected) in decoded.iter().zip(snippet_spec.iter()) {
            assert_eq!(actual.stat_id, expected.0);
            assert_eq!(actual.param, expected.1);
            assert_eq!(actual.value, expected.2);
        }
    });
}

/// Encodes multiple stat blocks into a single continuous unaligned bitstream.
pub fn encode_multi_block_stat_snippet(blocks: &[Vec<(u32, u32, i32)>]) -> Vec<u8> {
    let mut buffer = Vec::new();
    {
        let mut writer = BitWriter::endian(&mut buffer, LittleEndian);
        for entries in blocks {
            for &(stat_id, param, value) in entries {
                writer.write_var(9, stat_id).expect("bit write stat_id");
                let stat_def = STAT_COSTS.iter().find(|s| s.id == stat_id);
                if let Some(def) = stat_def {
                    if def.save_param_bits > 0 {
                        writer
                            .write_var(def.save_param_bits as u32, param)
                            .expect("bit write param");
                    }
                    let mut raw_val = value.wrapping_add(def.save_add) as u32;
                    if def.save_bits > 0 {
                        let mask = if def.save_bits >= 32 {
                            !0u32
                        } else {
                            (1u32 << def.save_bits) - 1
                        };
                        raw_val &= mask;
                        writer
                            .write_var(def.save_bits as u32, raw_val)
                            .expect("bit write value");
                    }
                } else {
                    writer.write_var(9, value as u32).expect("bit write fallback");
                }
            }
            // Block terminator (0x1FF)
            writer.write_var(9, 0x1FFu32).expect("bit write terminator");
        }
        writer.byte_align().expect("byte align");
    }
    buffer
}

#[test]
fn test_harness_multi_socket_runeword_blocks_decoding() {
    // Multi-block Runeword structure:
    // Block 1: Runeword Base Properties (Procs, Skills, ED) terminated by 0x1FF
    // Block 2: Socket 1 (Hel: Req -15%) terminated by 0x1FF
    // Block 3: Socket 2 (Shael: FHR +20%) terminated by 0x1FF
    // Block 4: Socket 3 (Ral: Fire Resist +30%) terminated by 0x1FF
    let base_block = vec![
        make_proc_skill_entry(198, 120, 15, 10), // 10% Chance to cast Level 15 Chain of Poison on striking
        make_proc_skill_entry(201, 125, 10, 2),  // 2% Chance to cast Level 10 Mind Barrier when struck
        (127, 0, 2),                             // +2 to Skills
        (17, 0, 50),                             // +50% Enhanced Damage
    ];
    let socket1_hel = vec![(91, 0, 15)];          // Requirements -15%
    let socket2_shael = vec![(99, 0, 20)];        // +20% Faster Hit Recovery
    let socket3_ral = vec![(39, 0, 30)];          // Fire Resist +30%

    let combined_bits = encode_multi_block_stat_snippet(&[
        base_block,
        socket1_hel,
        socket2_shael,
        socket3_ral,
    ]);

    // Decode sequentially over all blocks
    let mut reader = BitReader::endian(Cursor::new(&combined_bits), LittleEndian);
    let mut all_decoded = Vec::new();

    for _ in 0..4 {
        let block_props = read_standard_stat_block(&mut reader);
        all_decoded.extend(block_props);
    }

    assert_eq!(all_decoded.len(), 7);
    assert_eq!(all_decoded[0].stat_id, 198);
    assert_eq!(all_decoded[1].stat_id, 201);
    assert_eq!(all_decoded[2].stat_id, 127);
    assert_eq!(all_decoded[3].stat_id, 17);
    assert_eq!(all_decoded[4].stat_id, 91);
    assert_eq!(all_decoded[5].stat_id, 99);
    assert_eq!(all_decoded[6].stat_id, 39);
}

#[test]
fn test_harness_heart_of_the_oak_micro_snippet() {
    // Heart of the Oak `오크의 심장 (HOTO)` snippet:
    // +3 All Skills (127), +40% FCR (105), +75% Dmg to Demons (121), +100 AR vs Demons (122), 7% ML (60), +10 Dex (2), Replenish Life +20 (74), Max Mana 15% (77), All Res +40 (39/41/43/45)
    let hoto_stats = vec![
        (127, 0, 3),   // +3 to All Skills
        (105, 0, 40),  // +40% Faster Cast Rate
        (121, 0, 75),  // +75% Damage to Demons
        (122, 0, 100), // +100 Attack Rating against Demons
        (60, 0, 7),    // 7% Mana Stolen Per Hit
        (2, 0, 10),    // +10 to Dexterity
        (74, 0, 20),   // Replenish Life +20
        (77, 0, 15),   // Increase Maximum Mana 15%
        (39, 0, 40),   // Fire Resist +40%
        (41, 0, 40),   // Lightning Resist +40%
        (43, 0, 40),   // Cold Resist +40%
        (45, 0, 40),   // Poison Resist +40%
    ];

    let encoded = encode_standard_stat_snippet(&hoto_stats);
    let decoded = parse_standard_stat_snippet(&encoded);

    assert_eq!(decoded.len(), hoto_stats.len());
    for (actual, expected) in decoded.iter().zip(hoto_stats.iter()) {
        assert_eq!(actual.stat_id, expected.0);
        assert_eq!(actual.param, expected.1);
        assert_eq!(actual.value, expected.2);
    }
}

#[test]
fn test_harness_fortitude_micro_snippet() {
    // Fortitude `인내` snippet:
    // 20% Chance to cast Level 15 Chilling Armor when struck (201 Proc: Skill 60),
    // +25% FCR (105), +300% ED (17), +200% Enhanced Defense (16), +15 Defense (31),
    // +1.5 Life/Lvl (216), All Res +30 (39/41/43/45), 12% DTGM (78), Replenish Life +7 (74)
    let fortitude_stats = vec![
        make_proc_skill_entry(201, 60, 15, 20), // 20% Chance to cast Level 15 Chilling Armor when struck
        (105, 0, 25),                           // +25% Faster Cast Rate
        (17, 0, 300),                           // +300% Enhanced Damage
        (16, 0, 200),                           // +200% Enhanced Defense
        (31, 0, 15),                            // +15 Defense
        (216, 0, 12),                           // +1.5 Life per Level (step 12 >> 3 = 1.5)
        (39, 0, 30),                            // Fire Resist +30%
        (41, 0, 30),                            // Lightning Resist +30%
        (43, 0, 30),                            // Cold Resist +30%
        (45, 0, 30),                            // Poison Resist +30%
        (78, 0, 12),                            // 12% Damage Taken Goes to Mana
        (74, 0, 7),                             // Replenish Life +7
    ];

    let encoded = encode_standard_stat_snippet(&fortitude_stats);
    let decoded = parse_standard_stat_snippet(&encoded);

    assert_eq!(decoded.len(), fortitude_stats.len());
    for (actual, expected) in decoded.iter().zip(fortitude_stats.iter()) {
        assert_eq!(actual.stat_id, expected.0);
        assert_eq!(actual.param, expected.1);
        assert_eq!(actual.value, expected.2);
    }
}

#[test]
fn test_harness_spirit_micro_snippet() {
    // Spirit `영혼 (스피릿)` Shield snippet:
    // +2 All Skills (127), +35% FCR (105), +55% FHR (99), +250 Defense vs Missile (33),
    // +22 Vitality (3), +112 Mana (9), +8 Magic Absorb (148),
    // Cold Resist +35% (43), Lightning Resist +35% (41), Poison Resist +35% (45), Attacker Takes Damage of 14 (79)
    let spirit_stats = vec![
        (127, 0, 2),  // +2 to All Skills
        (105, 0, 35), // +35% Faster Cast Rate
        (99, 0, 55),  // +55% Faster Hit Recovery
        (33, 0, 250), // +250 Defense vs Missile
        (3, 0, 22),   // +22 to Vitality
        (9, 0, 112),  // +112 to Mana
        (148, 0, 8),  // +8 Magic Absorb
        (43, 0, 35),  // Cold Resist +35%
        (41, 0, 35),  // Lightning Resist +35%
        (45, 0, 35),  // Poison Resist +35%
        (79, 0, 14),  // Attacker Takes Damage of 14
    ];

    let encoded = encode_standard_stat_snippet(&spirit_stats);
    let decoded = parse_standard_stat_snippet(&encoded);

    assert_eq!(decoded.len(), spirit_stats.len());
    for (actual, expected) in decoded.iter().zip(spirit_stats.iter()) {
        assert_eq!(actual.stat_id, expected.0);
        assert_eq!(actual.param, expected.1);
        assert_eq!(actual.value, expected.2);
    }
}

#[test]
fn test_harness_maras_kaleidoscope_micro_snippet() {
    // Mara's Kaleidoscope `마라의 만화경` snippet:
    // +2 All Skills (127), +5 All Attributes (0/1/2/3), All Res +30 (39/41/43/45)
    let maras_stats = vec![
        (127, 0, 2), // +2 to All Skills
        (0, 0, 5),   // +5 to Strength
        (1, 0, 5),   // +5 to Energy
        (2, 0, 5),   // +5 to Dexterity
        (3, 0, 5),   // +5 to Vitality
        (39, 0, 30), // Fire Resist +30%
        (41, 0, 30), // Lightning Resist +30%
        (43, 0, 30), // Cold Resist +30%
        (45, 0, 30), // Poison Resist +30%
    ];

    let encoded = encode_standard_stat_snippet(&maras_stats);
    let decoded = parse_standard_stat_snippet(&encoded);

    assert_eq!(decoded.len(), maras_stats.len());
    for (actual, expected) in decoded.iter().zip(maras_stats.iter()) {
        assert_eq!(actual.stat_id, expected.0);
        assert_eq!(actual.param, expected.1);
        assert_eq!(actual.value, expected.2);
    }
}

#[test]
fn test_harness_arachnid_mesh_micro_snippet() {
    // Arachnid Mesh `거미줄 허리띠 (스웹)` snippet:
    // +1 All Skills (127), +20% FCR (105), Slows Target by 10% (150), +120% ED (16), Increase Max Mana 5% (77), Level 3 Venom Charges (204)
    let arach_stats = vec![
        (127, 0, 1),                           // +1 to All Skills
        (105, 0, 20),                          // +20% Faster Cast Rate
        (150, 0, 10),                          // Slows Target by 10%
        (16, 0, 120),                          // +120% Enhanced Defense
        (77, 0, 5),                            // Increase Maximum Mana 5%
        make_charged_skill_entry(92, 3, 11, 11), // Level 3 Venom (11/11 charges)
    ];

    let encoded = encode_standard_stat_snippet(&arach_stats);
    let decoded = parse_standard_stat_snippet(&encoded);

    assert_eq!(decoded.len(), arach_stats.len());
    for (actual, expected) in decoded.iter().zip(arach_stats.iter()) {
        assert_eq!(actual.stat_id, expected.0);
        assert_eq!(actual.param, expected.1);
        assert_eq!(actual.value, expected.2);
    }
}

#[test]
fn test_harness_war_traveler_micro_snippet() {
    // War Traveler `전쟁여행자 (배추)` snippet:
    // +25% FRW (96), Adds 15-25 Damage (21/22), +190% Enhanced Defense (16), +10 Strength (0), +10 Vitality (3),
    // 40% Slower Stamina Drain (76), Attacker Takes Damage of 10 (79), 50% MF (80)
    let wartrav_stats = vec![
        (96, 0, 25),  // +25% Faster Run/Walk
        (21, 0, 15),  // Adds 15 Min Damage
        (22, 0, 25),  // Adds 25 Max Damage
        (16, 0, 190), // +190% Enhanced Defense
        (0, 0, 10),   // +10 to Strength
        (3, 0, 10),   // +10 to Vitality
        (76, 0, 40),  // 40% Slower Stamina Drain
        (79, 0, 10),  // Attacker Takes Damage of 10
        (80, 0, 50),  // 50% Better Chance of Getting Magic Items
    ];

    let encoded = encode_standard_stat_snippet(&wartrav_stats);
    let decoded = parse_standard_stat_snippet(&encoded);

    assert_eq!(decoded.len(), wartrav_stats.len());
    for (actual, expected) in decoded.iter().zip(wartrav_stats.iter()) {
        assert_eq!(actual.stat_id, expected.0);
        assert_eq!(actual.param, expected.1);
        assert_eq!(actual.value, expected.2);
    }
}


