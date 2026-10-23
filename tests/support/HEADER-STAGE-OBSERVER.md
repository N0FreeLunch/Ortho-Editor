# Header-stage observation radar

The owning [parity test](../item_header_stage_parity.rs) includes an ignored, explicitly requested observer for the legacy Alpha Header path. It preserves complete Header results/errors, cursor and segment state, the following three-bit read, nested-recovery TLS, and full-save consumer snapshots. It observes existing parser behavior; it does not repair parsing or update frozen expectations.

## Run

Run from the Core root resolved through `.env`. Resolve all input, binding and artifact paths before creating a request; JSON does not expand environment variables. Set `D2R_DATA_PATH` to the intended existing registry root. Each registry mode needs a fresh process and a separate new output directory. Keep fixture order and execution environment consistent with the baseline.

```powershell
$env:D2R_HEADER_STAGE_REQUEST = (Resolve-Path '<request.json>').Path
cargo test --test item_header_stage_parity export_header_stage_observations -- --exact --ignored --test-threads=1 --nocapture
```

For repeated runs, obtain the current test executable from `cargo test --test item_header_stage_parity --no-run --message-format=json` and invoke it with the same exact test arguments. This avoids reconstructing temporary Rust sources or manually linking the Core library. A caller may choose a test-worker stack budget with `RUST_MIN_STACK`; record that execution setting. No throughput or stack-independence claim follows from output equality.

Without `--ignored`, this radar remains ignored. The ordinary owning target still contains its independent parity and frozen snapshot tests. The small `header_stage_observer::tests::` contract tests do not parse save files.

## Request version 1

Paths below are schematic absolute-path placeholders. Replace them with `.env`-resolved paths before running. Supply a reviewed dependency closure, not just the minimal example bindings.

```json
{
  "schema_version": 1,
  "profile": "legacy-alpha-v105-v1",
  "registry_label": "production",
  "output_directory": "<absolute-new-output-directory>",
  "items_per_fixture": 2,
  "fixtures": [
    {
      "id": "stable-fixture-id",
      "label": "safe-file-label",
      "path": "<absolute-save-path>",
      "historical_expected": null
    }
  ],
  "bindings": [
    {"label": "reviewed-parser-source", "role": "source", "path": "<absolute-source-file>"},
    {"label": "runtime-registry", "role": "registry", "path": "<absolute-data-root>/constants/alpha_v105_forensics.json"}
  ],
  "compare_directory": null,
  "baseline_format": null
}
```

The closed schema rejects unknown fields. It accepts 1–8 ordered fixtures and 1–2 item anchors per fixture, rejecting missing anchors rather than completing an empty or partial matrix. Fixture IDs and filename-safe labels are unique. Labels do not change parser behavior. Source and registry bindings identify files; the caller reviews completeness, environment and mode compatibility. `registry_label` is a caller label, not automatic proof of the selected registry contents.

For comparison, set both `compare_directory` and `baseline_format`: `legacy-stage-v1` imports the earlier temporary observer payload shape and requires external historical bindings; `header-stage-v1` also requires the promoted baseline's completed execution receipt. The recipe and ordered fixture IDs must match. Observation-only status is `observed`; it is never reported as `same` without a comparison.

## Artifacts and failure behavior

The output directory must not exist and its parent must already exist. Input files and existing outputs are never overwritten or deleted. The producer writes:

- `stage.jsonl`: every ordered control and diagnostic case, with complete before/after state.
- `<label>-headers.txt`, `<label>-serialized.bin`, `<label>-signature.bin`: full existing consumer outputs.
- `report.json`: diagnostic counts, full snapshot observations and historical signature-match fields.
- `comparison.json`: `not_requested`, `same`, or `different`, with only the first differing artifact filename/byte offset when different.
- `execution.json`: profile, actual registry root, declared file fingerprints, input facts and completion/status. This marker is written only after the complete observation and comparison steps.

Comparison reads every payload byte in bounded blocks; it does not compare only a digest or a chosen JSON projection. An artifact byte offset is not an offset in the save file. The exact radar test fails on `different`; a completed observation with a failed comparison is not a successful comparison. Missing/incompatible baselines and malformed requests fail before observation. Other execution failures can leave partial artifacts without a completed execution marker; preserve them for diagnosis and choose a fresh directory for a new run.

Declared file/input fingerprints are captured before and after observation. They use FNV-1a64 as comparison metadata, not cryptographic or semantic authority. Strong receipt integrity, full dependency/environment compatibility, source-change review and baseline decisions remain the caller's responsibility through the existing verification control plane. Payload equality never adopts or replaces a baseline. Extra execution/comparison receipts are metadata, separate from the legacy payload inventory.

## Recipe and limits

The versioned recipe retains its original fixture and case order inside one process, one Huffman tree and all alpha/index/forced-compact/hint/context/limit/trace/nested/TLS combinations. Independent registry processes may run concurrently under the caller's CPU budget; the within-process sequence is not a parallel corpus scan. The temporary nested-recovery setting is restored on both normal completion and unwind.

Diagnostic errors are expected observations for deliberately constrained inputs. Control inputs and diagnostic rows are not a valid gameplay, ownership or fidelity denominator. Full snapshots retain the existing parser and serializer cache/raw-replay behavior; they do not prove cache-free semantic reconstruction. This radar does not resolve a pre-existing frozen snapshot failure, establish new format truth, verify other dialects, or prove Kani properties, performance, a stable ABI or runtime WASM behavior. Detailed traces live in files only after this explicit radar request.
