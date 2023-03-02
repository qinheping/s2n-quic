use bitflags::bitflags;
use core::mem::size_of;

bitflags!(
    #[derive(Default)]
    #[repr(transparent)]
    pub struct SxdpFlags: u32 {
        const SHARED_UMEM = 1 << 0;
        const COPY = 1 << 1;
        const ZEROCOPY = 1 << 2;
        const USE_NEED_WAKEUP = 1 << 3;
    }
);

bitflags!(
    #[derive(Default)]
    #[repr(transparent)]
    pub struct UmemConfigFlags: u32 {
        const UNALIGNED_CHUNK_FLAG = 1 << 0;
    }
);

#[derive(Clone, Copy, Debug)]
#[repr(C)]
pub struct Address {
    pub family: u16,
    pub flags: u16,
    pub ifindex: u32,
    pub queue_id: u32,
    pub shared_umem_fd: u32,
}

impl Default for Address {
    fn default() -> Self {
        Self {
            family: libc::PF_XDP as _,
            flags: 0,
            ifindex: 0,
            queue_id: 0,
            shared_umem_fd: 0,
        }
    }
}

bitflags!(
    #[derive(Default)]
    #[repr(transparent)]
    pub struct RingFlags: u32 {
        const NEED_WAKEUP = 1 << 0;
    }
);

#[derive(Clone, Copy, Debug)]
#[repr(C)]
pub struct RingOffsetV1 {
    pub producer: u64,
    pub consumer: u64,
    pub desc: u64,
}

#[derive(Clone, Copy, Debug, Default)]
#[repr(C)]
pub struct RingOffsetV2 {
    pub producer: u64,
    pub consumer: u64,
    pub desc: u64,
    pub flags: u64,
}

impl RingOffsetV2 {
    #[inline]
    fn from_v1(&mut self, v1: RingOffsetV1) {
        self.producer = v1.producer;
        self.consumer = v1.consumer;
        self.desc = v1.desc;
        self.flags = v1.consumer + (size_of::<u32>() as u64);
    }
}

#[derive(Clone, Copy, Debug, Default)]
#[repr(C)]
pub struct MmapOffsets<O = RingOffsetV2> {
    pub rx: O,
    pub tx: O,
    pub fill: O,
    pub completion: O,
}

impl MmapOffsets {
    pub const RX_RING: usize = 0;
    pub const TX_RING: usize = 0x80000000;
    pub const FILL_RING: usize = 0x100000000;
    pub const COMPLETION_RING: usize = 0x180000000;

    #[inline]
    pub(crate) fn from_v1(&mut self) {
        // getsockopt on a kernel <= 5.3 has no flags fields.
        // Copy over the offsets to the correct places in the >=5.4 format
        // and put the flags where they would have been on that kernel.

        let v1 = unsafe { *(self as *const Self as *const MmapOffsets<RingOffsetV1>) };
        self.rx.from_v1(v1.rx);
        self.tx.from_v1(v1.tx);
        self.fill.from_v1(v1.fill);
        self.completion.from_v1(v1.completion);
    }
}

#[derive(Clone, Copy, Debug)]
#[repr(i32)]
pub enum SocketOptions {
    MmapOffsets = 1,
    RxRing = 2,
    TxRing = 3,
    UmemReg = 4,
    UmemFillRing = 5,
    UmemCompletionRing = 6,
    Statistics = 7,
    Options = 8,
}

#[derive(Clone, Copy, Debug)]
#[repr(C)]
pub struct UmemReg {
    pub addr: u64,
    pub len: u64,
    pub chunk_size: u32,
    pub headroom: u32,
    pub flags: u32,
}

#[derive(Clone, Copy, Debug, Default)]
#[repr(C)]
pub struct Statistics {
    pub rx_dropped: u64,
    pub rx_invalid_descriptions: u64,
    pub tx_invalid_descriptions: u32,
    pub rx_ring_full: u64,
    pub rx_fill_ring_empty_descriptions: u64,
    pub tx_ring_empty_descriptions: u64,
}

bitflags!(
    #[derive(Default)]
    #[repr(transparent)]
    pub struct XdpOptions: u32 {
        const ZEROCOPY = 1 << 0;
    }
);

#[derive(Clone, Copy, Debug, Default)]
#[repr(C)]
pub struct Description {
    pub address: u64,
    pub len: u32,
    pub options: u32,
}
