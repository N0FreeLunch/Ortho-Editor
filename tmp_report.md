# Symmetry Forensic Report

## Summary

- **Total Files Processed:** 11
- **Failed/Mismatch Files:** 4
- **Total Mismatch Rows:** 2

## File Integrity Summary

| File | Integrity | Symmetry | Baseline | Shadow | Bits | Note |
| :--- | :--- | :--- | :--- | :--- | :--- | :--- |
| amazon_10_scrolls.d2s | ✅ OK | ❌ FAIL | ✅ OK | ✅ OK | 0/0 |  |
| amazon_authority_runeword.d2s | ✅ OK | ❌ FAIL | ✅ OK | ✅ OK | 0/0 |  |
| amazon_empty.d2s | ✅ OK | ✅ OK | ✅ OK | ✅ OK | 0/0 |  |
| amazon_initial.d2s | ❌ FAIL | ❌ FAIL | ✅ OK | ✅ OK | 0/0 |  |
| amazon_lvl2_progression_complex.d2s | ❌ FAIL | ❌ FAIL | ✅ OK | ✅ OK | 0/0 |  |
| amazon_moved_diff_basis.d2s | ✅ OK | ✅ OK | ✅ OK | ✅ OK | 0/0 |  |
| amazon_moved_manual.d2s | ✅ OK | ✅ OK | ✅ OK | ✅ OK | 0/0 |  |
| amazon_v105_act2_start.d2s | ✅ OK | ✅ OK | ✅ OK | ✅ OK | 0/0 |  |
| amazon_v105_andariel_killed_no_talk.d2s | ✅ OK | ✅ OK | ✅ OK | ✅ OK | 0/0 |  |
| amazon_v105_re_probe_zigzag_all_diff.d2s | ✅ OK | ✅ OK | ✅ OK | ✅ OK | 0/0 |  |
| TESTAMAZON.d2s | ✅ OK | ✅ OK | ✅ OK | ✅ OK | 0/0 |  |

## Top Mismatch Segments

| Segment | Count |
| :--- | :--- |
| Unknown | 2 |

## Mismatch Types

| Type | Count |
| :--- | :--- |
| Content | 2 |

## Detailed Mismatches

| File | Item Label | Code | Segment | Offset | Type |
| :--- | :--- | :--- | :--- | :--- | :--- |
| amazon_10_scrolls.d2s | Item 15 | `` | Unknown | 53 | Content |
| amazon_authority_runeword.d2s | Item 4 | `y99x` | Unknown | 55 | Content |

## Actionable Clues

1. **Content Mismatches** in specific segments (e.g., `Stats`) usually indicate a field size or mapping error.
2. **Length Mismatches** often point to missing or extra bits in the bitstream serialization.
3. **ChildCount Mismatches** suggest issues with socketed items or nested structures.
