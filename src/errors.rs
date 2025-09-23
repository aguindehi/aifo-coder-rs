use std::io;

/// Map an io::Error to a process exit code, preserving current behavior:
/// - 127 for NotFound (command not found)
/// - 1 for all other errors
pub fn exit_code_for_io_error(e: &io::Error) -> u8 {
    if e.kind() == io::ErrorKind::NotFound {
        127
    } else {
        1
    }
}
