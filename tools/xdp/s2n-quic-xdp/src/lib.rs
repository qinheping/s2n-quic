#![allow(dead_code)]

/// Calls the given libc function and wraps the result in an `io::Result`.
macro_rules! libc {
    ($fn: ident ( $($arg: expr),* $(,)* ) ) => {{
        let res = unsafe { libc::$fn($($arg, )*) };
        if res < 0 {
            Err(std::io::Error::last_os_error())
        } else {
            Ok(res)
        }
    }};
}

pub mod if_xdp;
pub mod mmap;
pub mod ring;
pub mod socket;
pub mod umem;
