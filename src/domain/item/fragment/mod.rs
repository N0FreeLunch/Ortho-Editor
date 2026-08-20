pub mod header;
pub mod option;
pub mod assembler;
#[cfg(test)]
pub mod tests;

pub use header::HeaderFragment;
pub use option::OptionFragment;
pub use assembler::ItemAssembler;

use bitstream_io::{BitRead, BitWrite};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum FragmentType {
    Header,
    Option,
    OpaquePayload,
    SocketTree,
    ExtendedList,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct FragmentContext {
    pub version: u8,
    pub is_alpha: bool,
    pub code: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FragmentError {
    Io(String),
    BitExhaustion,
    InvalidHeader(String),
    InvalidOption(String),
    UnknownStatId(u16),
}

impl std::fmt::Display for FragmentError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            FragmentError::Io(err) => write!(f, "IO error: {}", err),
            FragmentError::BitExhaustion => write!(f, "Bitstream exhausted unexpectedly"),
            FragmentError::InvalidHeader(err) => write!(f, "Invalid header: {}", err),
            FragmentError::InvalidOption(err) => write!(f, "Invalid option: {}", err),
            FragmentError::UnknownStatId(id) => write!(f, "Unknown stat id: {}", id),
        }
    }
}

impl std::error::Error for FragmentError {}

pub trait BitFragment: std::fmt::Debug + Send + Sync {
    fn fragment_type(&self) -> FragmentType;
    fn bit_len(&self) -> usize;
    fn encode_to_bits<W: BitWrite>(&self, writer: &mut W) -> Result<(), FragmentError>;
    fn decode_from_bits<R: BitRead>(
        reader: &mut R,
        ctx: &FragmentContext,
    ) -> Result<Self, FragmentError>
    where
        Self: Sized;
}
