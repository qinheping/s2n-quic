use core::{ffi::c_void, ptr::NonNull};
use std::{io, os::fd::RawFd};

#[derive(Debug)]
pub struct Mmap {
    addr: NonNull<c_void>,
    len: usize,
}

impl Mmap {
    #[inline]
    pub fn new(len: usize, offset: usize, fd: RawFd) -> io::Result<Self> {
        unsafe {
            let addr = libc::mmap(
                core::ptr::null_mut(),
                len as _,
                libc::PROT_READ | libc::PROT_WRITE,
                libc::MAP_SHARED | libc::MAP_POPULATE,
                fd,
                offset as _,
            );

            if addr == libc::MAP_FAILED {
                return Err(io::Error::last_os_error());
            }

            let addr = NonNull::new(addr).ok_or_else(|| {
                io::Error::new(io::ErrorKind::Other, "mmap returned null pointer")
            })?;

            Ok(Self { addr, len })
        }
    }

    pub fn addr(&self) -> NonNull<c_void> {
        self.addr
    }
}

impl Mmap {
    fn drop(&mut self) {
        let _ = libc!(munmap(self.addr.as_mut(), self.len as _));
    }
}
