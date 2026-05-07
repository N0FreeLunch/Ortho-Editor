// This software is licensed under the PolyForm Noncommercial License 1.0.0.
// Required Notice: Copyright 2026 N0FreeLunch (https://github.com/N0FreeLunch/d2r-core)

/// Alpha v105 Mercenary Payload structure.
///
/// Forensic evidence shows a 9-byte envelope composed of 'kf' and 'lf' markers
/// followed by specific bit-fields (level, xp, aura/skills).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MercenaryPayload {
    /// 3 bytes of raw data following the 'kf' marker.
    pub kf_raw: [u8; 3],
    /// 2 bytes of raw data following the 'lf' marker.
    pub lf_raw: [u8; 2],
}

impl MercenaryPayload {
    /// Creates a new payload from raw byte slices.
    /// Expects kf_data to be 3 bytes and lf_data to be 2 bytes.
    pub fn from_raw(kf_data: &[u8], lf_data: &[u8]) -> Self {
        let mut k = [0u8; 3];
        let mut l = [0u8; 2];
        let k_len = kf_data.len().min(3);
        let l_len = lf_data.len().min(2);
        k[..k_len].copy_from_slice(&kf_data[..k_len]);
        l[..l_len].copy_from_slice(&lf_data[..l_len]);
        Self {
            kf_raw: k,
            lf_raw: l,
        }
    }

    /// Returns the full 9-byte envelope (markers + data).
    pub fn to_envelope(&self) -> [u8; 9] {
        let mut out = [0u8; 9];
        out[0] = b'k';
        out[1] = b'f';
        out[2..5].copy_from_slice(&self.kf_raw);
        out[5] = b'l';
        out[6] = b'f';
        out[7..9].copy_from_slice(&self.lf_raw);
        out
    }
}
