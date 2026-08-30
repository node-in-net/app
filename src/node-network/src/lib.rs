#[cfg(unix)]
mod sys_unix;

#[cfg(unix)]
pub use sys_unix::*;

#[cfg(windows)]
mod sys_windows;

#[cfg(windows)]
pub use sys_windows::*;
