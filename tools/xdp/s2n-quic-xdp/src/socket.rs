use crate::if_xdp::{Address, MmapOffsets, RingOffsetV1, SocketOptions, Statistics, UmemReg};
use core::mem::size_of;
use libc::{AF_XDP, SOCK_RAW, SOL_XDP};
use std::{
    io,
    os::fd::{AsRawFd, RawFd},
};

#[derive(Clone, Copy)]
pub struct Config {
    rx_size: u32,
    tx_size: u32,
    bpf_flags: u32,
    xdp_flags: u32,
    bind_flags: u16,
}

#[repr(transparent)]
struct Fd(RawFd);

impl Fd {
    #[inline]
    pub fn new() -> io::Result<Self> {
        let fd = libc!(socket(AF_XDP, SOCK_RAW, 0))?;
        Ok(Self(fd))
    }
}

impl AsRawFd for Fd {
    #[inline]
    fn as_raw_fd(&self) -> RawFd {
        self.0
    }
}

impl Drop for Fd {
    fn drop(&mut self) {
        let _ = libc!(close(self.0));
    }
}

impl SocketExt for Fd {}

pub trait SocketExt: AsRawFd {
    /// Returns all of the offsets for each of the queues in the socket
    #[inline]
    fn offsets(&self) -> io::Result<MmapOffsets> {
        let mut offsets = MmapOffsets::default();

        let optlen = self.xdp_option(SocketOptions::MmapOffsets, &mut offsets)?;

        // if the written size matched the V2 size, then return
        if optlen == size_of::<MmapOffsets>() {
            return Ok(offsets);
        }

        // adapt older versions of the kernel from v1 to v2
        if optlen == size_of::<MmapOffsets<RingOffsetV1>>() {
            offsets.from_v1();
            return Ok(offsets);
        }

        // an invalid size was returned
        Err(io::Error::new(
            io::ErrorKind::Other,
            format!("invalid mmap offset size: {optlen}"),
        ))
    }

    #[inline]
    fn statistics(&self) -> io::Result<Statistics> {
        let mut stats = Statistics::default();
        self.xdp_option(SocketOptions::Statistics, &mut stats)?;
        Ok(stats)
    }

    #[inline]
    fn xdp_option<T: Sized>(&self, opt: SocketOptions, value: &mut T) -> io::Result<usize> {
        let mut optlen = size_of::<T>() as libc::socklen_t;

        libc!(getsockopt(
            self.as_raw_fd(),
            SOL_XDP,
            opt as _,
            value as *mut _ as _,
            &mut optlen,
        ))?;

        Ok(optlen as usize)
    }

    #[inline]
    fn set_fill_ring_size(&self, len: u32) -> io::Result<()> {
        self.set_xdp_option(SocketOptions::UmemFillRing, &len)
    }

    #[inline]
    fn set_completion_ring_size(&self, len: u32) -> io::Result<()> {
        self.set_xdp_option(SocketOptions::UmemCompletionRing, &len)
    }

    #[inline]
    fn set_umem_reg(&self, reg: &UmemReg) -> io::Result<()> {
        self.set_xdp_option(SocketOptions::UmemReg, reg)
    }

    #[inline]
    fn set_rx_ring_size(&self, len: u32) -> io::Result<()> {
        self.set_xdp_option(SocketOptions::RxRing, &len)
    }

    #[inline]
    fn set_tx_ring_size(&self, len: u32) -> io::Result<()> {
        self.set_xdp_option(SocketOptions::TxRing, &len)
    }

    #[inline]
    fn bind(&self, addr: &mut Address) -> io::Result<()> {
        libc!(bind(
            self.as_raw_fd(),
            addr as *mut _ as _,
            size_of::<Address>() as _
        ))?;
        Ok(())
    }

    #[inline]
    fn set_xdp_option<T: Sized>(&self, opt: SocketOptions, value: &T) -> io::Result<()> {
        libc!(setsockopt(
            self.as_raw_fd(),
            SOL_XDP,
            opt as _,
            value as *const _ as _,
            size_of::<T>() as _
        ))?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn create_test() -> io::Result<()> {
        if let Ok(fd) = Fd::new() {
            dbg!(fd.offsets()?);
            dbg!(fd.statistics()?);

            panic!();
        }

        Ok(())
    }
}
