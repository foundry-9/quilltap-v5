//! Output + process-exit plumbing. v4's CLI calls `process.exit()` freely;
//! Rust's `std::process::exit` skips destructors, so every exit funnels
//! through [`exit`] which flushes both streams first. `console.log` /
//! `console.error` equivalents append the newline exactly as Node does.

use std::io::Write;

pub fn exit(code: i32) -> ! {
    let _ = std::io::stdout().flush();
    let _ = std::io::stderr().flush();
    std::process::exit(code);
}

/// `console.log(s)`.
pub fn log(s: &str) {
    let mut out = std::io::stdout();
    let _ = out.write_all(s.as_bytes());
    let _ = out.write_all(b"\n");
}

/// `console.error(s)`.
pub fn elog(s: &str) {
    let mut err = std::io::stderr();
    let _ = err.write_all(s.as_bytes());
    let _ = err.write_all(b"\n");
}

/// `process.stdout.write(bytes)`.
pub fn write_stdout(bytes: &[u8]) {
    let mut out = std::io::stdout();
    let _ = out.write_all(bytes);
}

/// `process.stderr.write(s)`.
pub fn write_stderr(s: &str) {
    let mut err = std::io::stderr();
    let _ = err.write_all(s.as_bytes());
}

/// `process.stdout.isTTY`.
pub fn stdout_is_tty() -> bool {
    // SAFETY: isatty is a pure query on the fd.
    unsafe { libc::isatty(1) == 1 }
}

/// `process.stdin.isTTY`.
pub fn stdin_is_tty() -> bool {
    // SAFETY: isatty is a pure query on the fd.
    unsafe { libc::isatty(0) == 1 }
}
