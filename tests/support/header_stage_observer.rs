use bitstream_io::{BitRead, BitReader, LittleEndian};
use d2r_core::data::bit_cursor::BitCursor;
use d2r_core::domain::header::entity::IN_NESTED_RECOVERY;
use d2r_core::domain::inventory::InventoryGrid;
use d2r_core::domain::payload::HuffmanTree;
use d2r_core::item::Item;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::BTreeSet;
use std::fs::{self, OpenOptions};
use std::io::{BufReader, BufWriter, Cursor, Read, Write};
use std::path::{Path, PathBuf};

const PROFILE: &str = "legacy-alpha-v105-v1";

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Request {
    schema_version: u32,
    profile: String,
    registry_label: String,
    output_directory: PathBuf,
    items_per_fixture: usize,
    fixtures: Vec<Fixture>,
    bindings: Vec<Binding>,
    compare_directory: Option<PathBuf>,
    baseline_format: Option<String>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Fixture {
    id: String,
    label: String,
    path: PathBuf,
    historical_expected: Option<u64>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Binding {
    label: String,
    role: String,
    path: PathBuf,
}

#[derive(Debug, Serialize)]
pub struct Failure {
    code: &'static str,
    message: String,
    hint: &'static str,
}

fn fail(code: &'static str, message: impl Into<String>) -> Failure {
    Failure {
        code,
        message: message.into(),
        hint: "Inspect the request, first differing artifact and declared bindings; preserve old evidence. Use a fresh output directory and an exact ignored radar test in its own process.",
    }
}

type Result<T> = std::result::Result<T, Failure>;

fn io<T>(result: std::io::Result<T>, operation: &str) -> Result<T> {
    result.map_err(|error| fail("io", format!("{operation}: {error}")))
}

fn shape(request: &Request) -> Result<()> {
    if request.schema_version != 1 || request.profile != PROFILE {
        return Err(fail(
            "schema",
            "Unsupported request schema or recipe profile",
        ));
    }
    if request.registry_label.trim().is_empty()
        || !(1..=2).contains(&request.items_per_fixture)
        || request.fixtures.is_empty()
        || request.fixtures.len() > 8
    {
        return Err(fail(
            "scope",
            "Require a registry label, 1..=8 ordered fixtures and 1..=2 items per fixture",
        ));
    }
    if !request.output_directory.is_absolute()
        || request
            .compare_directory
            .as_ref()
            .is_some_and(|path| !path.is_absolute())
    {
        return Err(fail("path", "Artifact directories must be absolute"));
    }
    match (
        &request.compare_directory,
        request.baseline_format.as_deref(),
    ) {
        (None, None) | (Some(_), Some("legacy-stage-v1" | "header-stage-v1")) => {}
        _ => {
            return Err(fail(
                "comparison",
                "Comparison needs a directory and an explicit compatible baseline format",
            ))
        }
    }
    let mut ids = BTreeSet::new();
    let mut labels = BTreeSet::new();
    for fixture in &request.fixtures {
        let safe_label = !fixture.label.is_empty()
            && fixture
                .label
                .bytes()
                .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_'));
        if fixture.id.is_empty()
            || !ids.insert(&fixture.id)
            || !safe_label
            || !labels.insert(&fixture.label)
            || !fixture.path.is_absolute()
        {
            return Err(fail(
                "fixture",
                "Fixture IDs/labels must be unique, labels filename-safe, and paths absolute",
            ));
        }
    }
    labels.clear();
    for binding in &request.bindings {
        if binding.label.is_empty()
            || !labels.insert(&binding.label)
            || !matches!(binding.role.as_str(), "source" | "registry")
            || !binding.path.is_absolute()
        {
            return Err(fail(
                "binding",
                "Bindings need unique labels, source/registry roles and absolute file paths",
            ));
        }
    }
    if !request
        .bindings
        .iter()
        .any(|binding| binding.role == "source")
        || !request
            .bindings
            .iter()
            .any(|binding| binding.role == "registry")
    {
        return Err(fail(
            "binding",
            "Declare both source and registry bindings; completeness remains caller-reviewed",
        ));
    }
    Ok(())
}

fn digest(bytes: &[u8]) -> u64 {
    bytes.iter().fold(0xcbf29ce484222325, |hash, byte| {
        (hash ^ u64::from(*byte)).wrapping_mul(0x100000001b3)
    })
}

fn stamp(path: &Path) -> Result<Value> {
    let bytes = io(
        fs::read(path),
        &format!("read declared file {}", path.display()),
    )?;
    Ok(json!({"path": path, "bytes": bytes.len(), "fnv1a64": digest(&bytes)}))
}

fn binding_stamps(request: &Request) -> Result<Vec<Value>> {
    request
        .bindings
        .iter()
        .map(|binding| {
            Ok(json!({"label": binding.label, "role": binding.role, "file": stamp(&binding.path)?}))
        })
        .collect()
}

fn input_stamps(request: &Request) -> Result<Vec<Value>> {
    request
        .fixtures
        .iter()
        .map(|fixture| {
            Ok(json!({"id": fixture.id, "label": fixture.label, "file": stamp(&fixture.path)?}))
        })
        .collect()
}

fn new_file(path: &Path) -> Result<fs::File> {
    io(
        OpenOptions::new().write(true).create_new(true).open(path),
        "create new artifact",
    )
}

fn save(path: &Path, bytes: &[u8]) -> Result<()> {
    io(new_file(path)?.write_all(bytes), "write new artifact")
}

fn save_json(path: &Path, value: &Value) -> Result<()> {
    let bytes =
        serde_json::to_vec_pretty(value).map_err(|error| fail("json", error.to_string()))?;
    save(path, &bytes)
}

fn snapshot<R: BitRead>(cursor: &BitCursor<R>) -> Value {
    json!({"pos":cursor.pos(),"base_pos":cursor.base_pos,"limit":cursor.limit(),"trace":cursor.trace_enabled,"alpha_quality":format!("{:?}",cursor.alpha_quality),"contexts":cursor.context_stack(),"active":cursor.active_segments(),"segments":cursor.segments(),"bits":cursor.recorded_bits().iter().map(|bit|(bit.offset,bit.bit)).collect::<Vec<_>>(),"events":cursor.operation_events(),"nested_tls":IN_NESTED_RECOVERY.with(|value|value.get())})
}

struct RecoveryGuard(bool);

impl RecoveryGuard {
    fn enter(value: bool) -> Self {
        Self(IN_NESTED_RECOVERY.with(|state| state.replace(value)))
    }
}

impl Drop for RecoveryGuard {
    fn drop(&mut self) {
        IN_NESTED_RECOVERY.with(|state| state.set(self.0));
    }
}

struct Case<'a> {
    start: u64,
    limit: u64,
    alpha: bool,
    idx: usize,
    forced: Option<bool>,
    hint: Option<&'a str>,
    context: Option<u64>,
    trace: bool,
    nested: bool,
    tls: bool,
}

fn run(bytes: &[u8], case: Case<'_>, huffman: &HuffmanTree) -> Result<Value> {
    let mut reader = BitReader::endian(Cursor::new(bytes), LittleEndian);
    let start = u32::try_from(case.start)
        .map_err(|_| fail("range", "Start exceeds BitReader skip width"))?;
    io(reader.skip(start), "seek diagnostic cursor")?;
    let mut cursor = BitCursor::new(reader);
    cursor.set_pos(case.start);
    cursor.set_limit(case.limit);
    cursor.set_trace(case.trace);
    if case.nested {
        cursor.push_context("nested");
    }
    let _guard = RecoveryGuard::enter(case.tls);
    let result = Item::read_header_stage(
        &mut cursor,
        huffman,
        case.context.map(|start| (bytes, start)),
        case.alpha,
        case.idx,
        case.forced,
        case.hint,
    );
    let value = match result {
        Ok(stage) => {
            json!({"ok":true,"header":format!("{:?}",stage.header),"gap":stage.alpha_header_gap,"gap_bits":stage.alpha_header_gap_bits,"peek":stage.peek,"stored_in":stage.d2r_v3_stored_in})
        }
        Err(error) => json!({"ok":false,"error":format!("{error:?}")}),
    };
    let state = snapshot(&cursor);
    let next = cursor.read_bits::<u8>(3);
    let after = snapshot(&cursor);
    Ok(json!({"result":value,"state":state,"next":format!("{next:?}"),"after":after}))
}

fn row(writer: &mut impl Write, value: &Value) -> Result<()> {
    serde_json::to_writer(&mut *writer, value).map_err(|error| fail("json", error.to_string()))?;
    io(writer.write_all(b"\n"), "finish observation row")
}

fn payload_names(request: &Request) -> Vec<String> {
    let mut names = vec!["stage.jsonl".to_string(), "report.json".to_string()];
    for fixture in &request.fixtures {
        for suffix in ["signature.bin", "headers.txt", "serialized.bin"] {
            names.push(format!("{}-{suffix}", fixture.label));
        }
    }
    names
}

fn fill_block(reader: &mut impl Read, buffer: &mut [u8]) -> std::io::Result<usize> {
    let mut filled = 0;
    while filled < buffer.len() {
        match reader.read(&mut buffer[filled..]) {
            Ok(0) => break,
            Ok(count) => filled += count,
            Err(error) if error.kind() == std::io::ErrorKind::Interrupted => continue,
            Err(error) => return Err(error),
        }
    }
    Ok(filled)
}

fn first_difference(left: &mut impl Read, right: &mut impl Read) -> std::io::Result<Option<u64>> {
    let mut offset = 0u64;
    let mut a = [0u8; 65536];
    let mut b = [0u8; 65536];
    loop {
        let n = fill_block(left, &mut a)?;
        let m = fill_block(right, &mut b)?;
        let common = n.min(m);
        if let Some(index) = a[..common]
            .iter()
            .zip(&b[..common])
            .position(|(a, b)| a != b)
        {
            return Ok(Some(offset + index as u64));
        }
        if n != m {
            return Ok(Some(offset + common as u64));
        }
        if n == 0 {
            return Ok(None);
        }
        offset += n as u64;
    }
}

fn compare(request: &Request) -> Result<Value> {
    let Some(baseline) = &request.compare_directory else {
        return Ok(json!({"status":"not_requested","baseline_adopted":false}));
    };
    for name in payload_names(request) {
        let mut left = BufReader::new(io(
            fs::File::open(baseline.join(&name)),
            "open baseline payload",
        )?);
        let mut right = BufReader::new(io(
            fs::File::open(request.output_directory.join(&name)),
            "open candidate payload",
        )?);
        if let Some(offset) = io(
            first_difference(&mut left, &mut right),
            "compare whole payload bytes",
        )? {
            return Ok(
                json!({"status":"different","first_file":name,"first_artifact_byte":offset,"baseline_adopted":false,"hint":"Inspect this artifact near its first differing byte; this is not a save-file bit offset. Preserve both runs and review input/source/mode bindings."}),
            );
        }
    }
    Ok(
        json!({"status":"same","compared_files":payload_names(request).len(),"baseline_adopted":false,"binding_scope":"Payload equality only; input/source/environment/mode compatibility requires reviewed external receipts, including legacy manifests."}),
    )
}

#[derive(Serialize)]
pub struct Summary {
    pub status: String,
    pub matrix: usize,
    pub natural_input_controls: usize,
    pub successes: usize,
    pub errors: usize,
    pub comparison: Value,
}

pub fn observe_file(path: &Path) -> Result<Summary> {
    if !path.is_absolute() {
        return Err(fail("path", "Request path must be absolute"));
    }
    let request: Request = serde_json::from_slice(&io(fs::read(path), "read request")?)
        .map_err(|error| fail("schema", error.to_string()))?;
    shape(&request)?;
    if request.output_directory.exists() {
        return Err(fail(
            "exists",
            "Output directory already exists; no artifact was overwritten",
        ));
    }
    if !request.output_directory.parent().is_some_and(Path::is_dir) {
        return Err(fail("path", "Output parent must already exist"));
    }
    let registry_root = std::env::var_os("D2R_DATA_PATH")
        .map(PathBuf::from)
        .filter(|path| path.is_absolute() && path.is_dir())
        .ok_or_else(|| {
            fail(
                "environment",
                "D2R_DATA_PATH must identify an absolute readable registry root",
            )
        })?;
    if IN_NESTED_RECOVERY.with(|state| state.get()) {
        return Err(fail(
            "state",
            "Run the radar in an isolated process with default nested-recovery TLS",
        ));
    }
    let before_bindings = binding_stamps(&request)?;
    let before_inputs = input_stamps(&request)?;
    if let Some(baseline) = &request.compare_directory {
        for name in payload_names(&request) {
            let path = baseline.join(name);
            if !path.is_file() {
                return Err(fail(
                    "baseline",
                    format!("Baseline required payload missing: {}", path.display()),
                ));
            }
        }
        let report: Value = serde_json::from_slice(&io(
            fs::read(baseline.join("report.json")),
            "read baseline report",
        )?)
        .map_err(|error| fail("baseline", error.to_string()))?;
        let anchors = request.fixtures.len() * request.items_per_fixture;
        let ids = report["full_snapshots"].as_array().map(|rows| {
            rows.iter()
                .map(|row| row["fixture"].as_str())
                .collect::<Vec<_>>()
        });
        if report["matrix"] != anchors * 4320
            || report["natural_input_controls"] != anchors
            || report["tls_restored"] != true
            || ids
                != Some(
                    request
                        .fixtures
                        .iter()
                        .map(|fixture| Some(fixture.id.as_str()))
                        .collect(),
                )
        {
            return Err(fail(
                "baseline",
                "Baseline recipe, ordered fixture IDs or completion fields are incompatible",
            ));
        }
        if request.baseline_format.as_deref() == Some("header-stage-v1") {
            let execution: Value = serde_json::from_slice(&io(
                fs::read(baseline.join("execution.json")),
                "read baseline execution",
            )?)
            .map_err(|error| fail("baseline", error.to_string()))?;
            if execution["schema_version"] != 1
                || execution["profile"] != PROFILE
                || execution["complete"] != true
                || execution["items_per_fixture"] != request.items_per_fixture
            {
                return Err(fail(
                    "baseline",
                    "Promoted baseline execution receipt is incomplete or incompatible",
                ));
            }
        }
    }
    io(
        fs::create_dir(&request.output_directory),
        "create fresh output directory",
    )?;
    let mut rows = BufWriter::new(new_file(&request.output_directory.join("stage.jsonl"))?);
    let huffman = HuffmanTree::new();
    let mut matrix = 0;
    let mut successes = 0;
    let mut natural = 0;
    let mut full = Vec::new();
    for fixture in &request.fixtures {
        let bytes = io(fs::read(&fixture.path), "read fixture")?;
        let items = Item::read_player_items(&bytes, &huffman, true)
            .map_err(|error| fail("parse", error.to_string()))?;
        if items.len() < request.items_per_fixture {
            return Err(fail("empty", "Fixture has fewer item anchors than requested; zero/partial matrices are not successful"));
        }
        let mut signature = Vec::new();
        for item in &items {
            signature.extend_from_slice(
                format!("{:?}|{:?}|{}\n", item.header, item.range, item.code).as_bytes(),
            );
        }
        let headers = signature.clone();
        let inventory = format!(
            "{:?}",
            InventoryGrid::validate_all_containers_integrity(&items)
        );
        signature.extend_from_slice(inventory.as_bytes());
        let serialized = Item::serialize_section(&items, &huffman, true)
            .map_err(|error| fail("serialize", error.to_string()))?;
        signature.extend_from_slice(&serialized);
        save(
            &request
                .output_directory
                .join(format!("{}-signature.bin", fixture.label)),
            &signature,
        )?;
        save(
            &request
                .output_directory
                .join(format!("{}-headers.txt", fixture.label)),
            &headers,
        )?;
        save(
            &request
                .output_directory
                .join(format!("{}-serialized.bin", fixture.label)),
            &serialized,
        )?;
        full.push(json!({"fixture":fixture.id,"count":items.len(),"signature":digest(&signature),"historical_expected":fixture.historical_expected,"historical_match":fixture.historical_expected.map(|expected|digest(&signature)==expected),"inventory":inventory}));
        for (original_index, item) in items.iter().take(request.items_per_fixture).enumerate() {
            let start = item.range.start;
            let end = bytes.len() as u64 * 8;
            if start > u64::from(u32::MAX) || start > end {
                return Err(fail("range", "Unsupported fixture item anchor"));
            }
            let control = run(
                &bytes,
                Case {
                    start,
                    limit: end,
                    alpha: true,
                    idx: original_index,
                    forced: None,
                    hint: None,
                    context: Some(start),
                    trace: false,
                    nested: false,
                    tls: false,
                },
                &huffman,
            )?;
            row(
                &mut rows,
                &json!({"kind":"natural-input-control","fixture":fixture.id,"index":original_index,"observation":control}),
            )?;
            natural += 1;
            // This ordered replay is one process/state contract, not a corpus scan.
            for alpha in [false, true] {
                for idx in [0, 7] {
                    for forced in [None, Some(false), Some(true)] {
                        for hint in [
                            None,
                            Some(""),
                            Some(item.code.as_str()),
                            Some("jav"),
                            Some("bad code"),
                        ] {
                            for context in [None, Some(start), Some(start + 1)] {
                                for limit in [start + 16, start + 48, end] {
                                    for trace in [false, true] {
                                        for nested in [false, true] {
                                            for tls in [false, true] {
                                                let observation = run(
                                                    &bytes,
                                                    Case {
                                                        start,
                                                        limit,
                                                        alpha,
                                                        idx,
                                                        forced,
                                                        hint,
                                                        context,
                                                        trace,
                                                        nested,
                                                        tls,
                                                    },
                                                    &huffman,
                                                )?;
                                                if observation["result"]["ok"] == true {
                                                    successes += 1;
                                                }
                                                row(
                                                    &mut rows,
                                                    &json!({"kind":"diagnostic-matrix","fixture":fixture.id,"original_index":original_index,"alpha":alpha,"idx":idx,"forced":forced,"hint":hint,"context":context,"limit":limit,"trace":trace,"nested":nested,"tls":tls,"observation":observation}),
                                                )?;
                                                matrix += 1;
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
    }
    io(rows.flush(), "flush full observation rows")?;
    if natural != request.fixtures.len() * request.items_per_fixture
        || matrix != natural * 4320
        || IN_NESTED_RECOVERY.with(|state| state.get())
    {
        return Err(fail(
            "coverage",
            "Incomplete recipe or nested TLS restoration failure",
        ));
    }
    if before_bindings != binding_stamps(&request)? || before_inputs != input_stamps(&request)? {
        return Err(fail(
            "binding",
            "Declared source/registry/input fingerprints changed during observation",
        ));
    }
    save_json(
        &request.output_directory.join("report.json"),
        &json!({"matrix":matrix,"natural_input_controls":natural,"successes":successes,"errors":matrix-successes,"full_snapshots":full,"tls_restored":true}),
    )?;
    let comparison = compare(&request)?;
    let status = comparison["status"]
        .as_str()
        .filter(|value| *value != "not_requested")
        .unwrap_or("observed")
        .to_string();
    save_json(
        &request.output_directory.join("comparison.json"),
        &comparison,
    )?;
    save_json(
        &request.output_directory.join("execution.json"),
        &json!({"schema_version":1,"profile":request.profile,"registry_label":request.registry_label,"registry_root":registry_root,"items_per_fixture":request.items_per_fixture,"inputs":before_inputs,"bindings":before_bindings,"fingerprint":"FNV-1a64 comparison fingerprint, not a cryptographic or semantic authority","complete":true,"status":status,"baseline_format":request.baseline_format,"baseline_adopted":false,"limits":"Caller reviews complete source/input/environment/mode closure. Diagnostic cases are not valid gameplay/fidelity denominators. Payload equality never adopts a baseline."}),
    )?;
    Ok(Summary {
        status,
        matrix,
        natural_input_controls: natural,
        successes,
        errors: matrix - successes,
        comparison,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[d2r_macros::hypothesis_test]
    fn first_difference_preserves_prefix_length_and_large_stream_offsets() {
        struct Short<'a>(&'a [u8]);
        impl Read for Short<'_> {
            fn read(&mut self, buffer: &mut [u8]) -> std::io::Result<usize> {
                let count = self.0.len().min(buffer.len()).min(17);
                buffer[..count].copy_from_slice(&self.0[..count]);
                self.0 = &self.0[count..];
                Ok(count)
            }
        }
        let data = vec![42u8; 131073];
        assert_eq!(
            first_difference(&mut Cursor::new(&data), &mut Cursor::new(&data)).unwrap(),
            None
        );
        assert_eq!(
            first_difference(&mut Cursor::new(&data), &mut Short(&data)).unwrap(),
            None
        );
        let mut changed = data.clone();
        changed[65536] = 43;
        assert_eq!(
            first_difference(&mut Cursor::new(&data), &mut Cursor::new(&changed)).unwrap(),
            Some(65536)
        );
        assert_eq!(
            first_difference(&mut Cursor::new(&data), &mut Cursor::new(&data[..65535])).unwrap(),
            Some(65535)
        );
        assert_eq!(
            first_difference(&mut Cursor::new([]), &mut Cursor::new([1])).unwrap(),
            Some(0)
        );
    }

    #[d2r_macros::hypothesis_test]
    fn empty_unknown_and_ambiguous_requests_do_not_pass_shape_validation() {
        let mut value = json!({"schema_version":1,"profile":PROFILE,"registry_label":"production","output_directory":std::env::current_dir().unwrap().join("new-output"),"items_per_fixture":2,"fixtures":[],"bindings":[]});
        assert_eq!(
            shape(&serde_json::from_value(value.clone()).unwrap())
                .unwrap_err()
                .code,
            "scope"
        );
        value["unexpected"] = json!(true);
        assert!(serde_json::from_value::<Request>(value).is_err());
    }

    #[d2r_macros::hypothesis_test]
    fn diagnostic_tls_scope_restores_the_previous_value_on_unwind() {
        let previous = IN_NESTED_RECOVERY.with(|state| state.get());
        let outcome = std::panic::catch_unwind(|| {
            let _guard = RecoveryGuard::enter(!previous);
            assert_eq!(IN_NESTED_RECOVERY.with(|state| state.get()), !previous);
            panic!("intentional observer scope unwind");
        });
        assert!(outcome.is_err());
        assert_eq!(IN_NESTED_RECOVERY.with(|state| state.get()), previous);
    }
}
