use std::io;

pub use crate::wire::BitEmitter;

pub(crate) fn d2r_v3_write_checked(
    emitter: &mut BitEmitter,
    value: u64,
    width: u32,
    field: &str,
) -> io::Result<()> {
    if width > 32 || value >= (1u64 << width) {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            format!("D2R v3 {field} value {value} does not fit {width} bits"),
        ));
    }
    emitter.write_bits(value as u32, width)
}
