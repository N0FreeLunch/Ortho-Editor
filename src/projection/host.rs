//! Tauri-shaped in-memory session store and newline-delimited JSON stdio bridge.
//!
//! Decouples host session orchestration (`load_fixture`, `load_save_bytes`,
//! `get_session`, `dispatch_intent`, `export_save_bytes`) from OS window toolkits
//! so both `src-tauri` commands and headless Elm <-> Rust scenario tests share
//! one deterministic contract.

use std::fs;
use std::io::{self, BufRead, Write};
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use super::{ActionRejection, EditorSession, ItemCardView, SessionEnvelope, UiIntent};

/// IPC command request handled by [`HostSessionStore`].
#[derive(Debug, Clone, Deserialize)]
#[serde(tag = "cmd", rename_all = "snake_case")]
pub enum HostRequest {
    GetSession,
    LoadFixturePath { path: String },
    LoadSaveHex { label: String, hex: String },
    DispatchIntent { intent: UiIntent },
    SeedTempItem { card: ItemCardView },
    ExportSaveHex,
}

/// IPC command response returned by [`HostSessionStore`].
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HostResponse {
    pub ok: bool,
    pub envelope: SessionEnvelope,
    pub export_hex: Option<String>,
    pub export_byte_len: Option<usize>,
    pub error: Option<ActionRejection>,
}

/// Stateful host store owning one active [`EditorSession`].
#[derive(Debug, Default)]
pub struct HostSessionStore {
    session: Option<EditorSession>,
}

impl HostSessionStore {
    pub fn new() -> Self {
        Self { session: None }
    }

    pub fn get_session(&self) -> SessionEnvelope {
        match &self.session {
            Some(sess) => SessionEnvelope {
                ok: true,
                session: Some(sess.project()),
                rejection: None,
            },
            None => no_session_envelope(),
        }
    }

    pub fn load_save_bytes(&mut self, bytes: &[u8], label: impl Into<String>) -> SessionEnvelope {
        match EditorSession::from_bytes(bytes, label) {
            Ok(sess) => {
                let view = sess.project();
                self.session = Some(sess);
                SessionEnvelope {
                    ok: true,
                    session: Some(view),
                    rejection: None,
                }
            }
            Err(err) => rejection_envelope(
                self.session.as_ref().map(EditorSession::project),
                super::RejectionCode::RejectionUnknown,
                "load_failed",
                err,
            ),
        }
    }

    pub fn load_save_hex(&mut self, hex_str: &str, label: impl Into<String>) -> SessionEnvelope {
        match hex::decode(hex_str.trim()) {
            Ok(bytes) => self.load_save_bytes(&bytes, label),
            Err(err) => rejection_envelope(
                self.session.as_ref().map(EditorSession::project),
                super::RejectionCode::RejectionUnknown,
                "invalid_hex",
                format!("Invalid hex save payload: {err}"),
            ),
        }
    }

    pub fn load_fixture_path(&mut self, raw_path: &str) -> SessionEnvelope {
        let resolved = resolve_fixture_path(raw_path);
        match fs::read(&resolved) {
            Ok(bytes) => {
                let label = resolved
                    .file_name()
                    .and_then(|s| s.to_str())
                    .unwrap_or(raw_path)
                    .to_string();
                self.load_save_bytes(&bytes, label)
            }
            Err(err) => rejection_envelope(
                self.session.as_ref().map(EditorSession::project),
                super::RejectionCode::RejectionUnknown,
                "io_error",
                format!("Failed to read `{}`: {err}", resolved.display()),
            ),
        }
    }

    pub fn dispatch_intent(&mut self, intent: UiIntent) -> SessionEnvelope {
        match &mut self.session {
            Some(sess) => sess.apply_intent(intent),
            None => no_session_envelope(),
        }
    }

    pub fn seed_temp_item(&mut self, card: ItemCardView) -> SessionEnvelope {
        match &mut self.session {
            Some(sess) => {
                sess.push_temp_item(card);
                SessionEnvelope {
                    ok: true,
                    session: Some(sess.project()),
                    rejection: None,
                }
            }
            None => no_session_envelope(),
        }
    }

    pub fn export_save_bytes(&self) -> Result<Vec<u8>, ActionRejection> {
        let Some(sess) = &self.session else {
            return Err(ActionRejection::new(
                super::RejectionCode::RejectionUnknown,
                "no_session_loaded",
                "Cannot export save before a session is loaded",
            ));
        };
        sess.to_save_bytes().map_err(|err| {
            ActionRejection::new(
                super::RejectionCode::RejectionUnknown,
                "export_failed",
                err,
            )
        })
    }

    pub fn export_save_hex(&self) -> Result<(String, usize), ActionRejection> {
        let bytes = self.export_save_bytes()?;
        let len = bytes.len();
        Ok((hex::encode(&bytes), len))
    }

    pub fn handle_request(&mut self, req: HostRequest) -> HostResponse {
        match req {
            HostRequest::GetSession => {
                let env = self.get_session();
                response_from_envelope(env)
            }
            HostRequest::LoadFixturePath { path } => {
                let env = self.load_fixture_path(&path);
                response_from_envelope(env)
            }
            HostRequest::LoadSaveHex { label, hex } => {
                let env = self.load_save_hex(&hex, label);
                response_from_envelope(env)
            }
            HostRequest::DispatchIntent { intent } => {
                let env = self.dispatch_intent(intent);
                response_from_envelope(env)
            }
            HostRequest::SeedTempItem { card } => {
                let env = self.seed_temp_item(card);
                response_from_envelope(env)
            }
            HostRequest::ExportSaveHex => {
                let env = self.get_session();
                match self.export_save_hex() {
                    Ok((hex_out, len)) => HostResponse {
                        ok: true,
                        envelope: env,
                        export_hex: Some(hex_out),
                        export_byte_len: Some(len),
                        error: None,
                    },
                    Err(rej) => HostResponse {
                        ok: false,
                        envelope: rejection_envelope(
                            env.session,
                            rej.code,
                            rej.raw_code.clone(),
                            rej.message.clone(),
                        ),
                        export_hex: None,
                        export_byte_len: None,
                        error: Some(rej),
                    },
                }
            }
        }
    }

    pub fn handle_json_line(&mut self, line: &str) -> HostResponse {
        match serde_json::from_str::<HostRequest>(line) {
            Ok(req) => self.handle_request(req),
            Err(err) => {
                let rej = ActionRejection::new(
                    super::RejectionCode::RejectionUnknown,
                    "invalid_host_request",
                    format!("Invalid HostRequest JSON: {err}"),
                );
                HostResponse {
                    ok: false,
                    envelope: rejection_envelope(
                        self.session.as_ref().map(EditorSession::project),
                        rej.code,
                        rej.raw_code.clone(),
                        rej.message.clone(),
                    ),
                    export_hex: None,
                    export_byte_len: None,
                    error: Some(rej),
                }
            }
        }
    }
}

/// Runs a newline-delimited JSON stdio loop for headless integration and Tauri IPC parity.
pub fn run_stdio_loop() -> io::Result<()> {
    let stdin = io::stdin();
    let stdout = io::stdout();
    let mut out = io::BufWriter::new(stdout.lock());
    let mut store = HostSessionStore::new();

    for line_res in stdin.lock().lines() {
        let line = line_res?;
        if line.trim().is_empty() {
            continue;
        }
        let resp = store.handle_json_line(&line);
        let serialized = serde_json::to_string(&resp)
            .map_err(|err| io::Error::new(io::ErrorKind::InvalidData, err))?;
        writeln!(out, "{serialized}")?;
        out.flush()?;
    }
    Ok(())
}

fn resolve_fixture_path(raw: &str) -> PathBuf {
    let candidate = Path::new(raw);
    if candidate.is_absolute() || candidate.exists() {
        return candidate.to_path_buf();
    }
    let manifest_rel = Path::new(env!("CARGO_MANIFEST_DIR")).join(candidate);
    if manifest_rel.exists() {
        return manifest_rel;
    }
    candidate.to_path_buf()
}

fn no_session_envelope() -> SessionEnvelope {
    SessionEnvelope {
        ok: false,
        session: None,
        rejection: Some(ActionRejection::new(
            super::RejectionCode::RejectionUnknown,
            "no_session_loaded",
            "No save session is currently loaded",
        )),
    }
}

fn rejection_envelope(
    session: Option<super::SessionView>,
    code: super::RejectionCode,
    raw_code: impl Into<String>,
    message: impl Into<String>,
) -> SessionEnvelope {
    SessionEnvelope {
        ok: false,
        session,
        rejection: Some(ActionRejection::new(
            code,
            raw_code,
            message,
        )),
    }
}

fn response_from_envelope(envelope: SessionEnvelope) -> HostResponse {
    HostResponse {
        ok: envelope.ok,
        error: envelope.rejection.clone(),
        envelope,
        export_hex: None,
        export_byte_len: None,
    }
}
