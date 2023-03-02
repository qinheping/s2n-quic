use crate::{
    if_xdp::{MmapOffsets, RingFlags, RingOffsetV2},
    mmap::Mmap,
    socket::SocketExt,
};
use core::{
    ffi::c_void,
    mem::size_of,
    num::Wrapping,
    ptr::NonNull,
    sync::atomic::{AtomicU32, Ordering},
};
use std::io;

#[derive(Debug)]
struct Cursors {
    cached_producer: Wrapping<u32>,
    cached_consumer: Wrapping<u32>,
    size: u32,
    mask: u32,
    producer: NonNull<AtomicU32>,
    consumer: NonNull<AtomicU32>,
    flags: NonNull<RingFlags>,
    desc: NonNull<c_void>,
}

impl Cursors {
    #[inline]
    unsafe fn new(area: &Mmap, offsets: &RingOffsetV2, size: u32) -> Self {
        let mask = size - 1;

        let producer = area.addr().as_ptr().add(offsets.producer as _);
        let producer = NonNull::new_unchecked(producer as _);
        let consumer = area.addr().as_ptr().add(offsets.consumer as _);
        let consumer = NonNull::new_unchecked(consumer as _);

        let flags = area.addr().as_ptr().add(offsets.flags as _);
        let flags = NonNull::new_unchecked(flags as *mut RingFlags);

        let desc = area.addr().as_ptr().add(offsets.desc as _);
        let desc = NonNull::new_unchecked(desc);

        Self {
            cached_consumer: Wrapping(0),
            cached_producer: Wrapping(0),
            size,
            mask,
            producer,
            consumer,
            flags,
            desc,
        }
    }

    #[inline]
    fn producer(&self) -> &AtomicU32 {
        unsafe { &*self.producer.as_ptr() }
    }

    #[inline]
    fn consumer(&self) -> &AtomicU32 {
        unsafe { &*self.consumer.as_ptr() }
    }

    #[inline]
    fn acquire_producer(&mut self, watermark: u32) -> u32 {
        let free = self.cached_producer();

        // if we have enough space, then return the cached value
        if free >= watermark {
            return free;
        }

        self.cached_consumer.0 = self.consumer().load(Ordering::Acquire);

        // increment the consumer cursor by the total size to avoid doing an addition inside
        // `cached_producer`
        self.cached_consumer += self.size;

        self.cached_producer()
    }

    #[inline]
    fn cached_producer(&self) -> u32 {
        (self.cached_consumer - self.cached_producer).0
    }

    #[inline]
    fn release_producer(&mut self, len: u32) {
        if cfg!(debug_assertions) {
            let max_len = self.acquire_producer(len);
            assert!(
                max_len >= len,
                "available: {}, requested: {}, {self:?}",
                max_len,
                len
            );
        }
        self.cached_producer += len;
        self.producer().fetch_add(len, Ordering::Release);
    }

    #[inline]
    fn acquire_consumer(&mut self, watermark: u32) -> u32 {
        let filled = self.cached_consumer();

        if filled >= watermark {
            return filled;
        }

        self.cached_producer.0 = self.producer().load(Ordering::Acquire);

        self.cached_consumer()
    }

    #[inline]
    fn cached_consumer(&mut self) -> u32 {
        (self.cached_producer - self.cached_consumer).0
    }

    #[inline]
    fn release_consumer(&mut self, len: u32) {
        if cfg!(debug_assertions) {
            let max_len = self.acquire_consumer(len);
            assert!(
                max_len >= len,
                "available: {}, requested: {}, {self:?}",
                max_len,
                len
            );
        }
        self.cached_consumer += len;
        self.consumer().fetch_add(len, Ordering::Release);
    }

    #[inline]
    fn needs_wakeup(&self) -> bool {
        self.flags().contains(RingFlags::NEED_WAKEUP)
    }

    #[inline]
    fn flags(&self) -> &RingFlags {
        unsafe { &*self.flags.as_ptr() }
    }
}

#[derive(Debug)]
struct Ring {
    cursors: Cursors,
    area: Mmap,
}

unsafe impl Send for Ring {}
unsafe impl Sync for Ring {}

pub struct Rings {
    pub tx: Tx,
    pub rx: Rx,
    pub fill: Fill,
    pub completion: Completion,
}

pub struct Tx(Ring);

impl Tx {
    pub(crate) fn new<S: SocketExt>(s: &S, offsets: &MmapOffsets, size: u32) -> io::Result<Self> {
        s.set_tx_ring_size(size)?;

        let len = offsets.tx.desc as usize + size as usize * size_of::<u64>();
        let offset = MmapOffsets::TX_RING;

        let area = Mmap::new(len, offset, s.as_raw_fd())?;

        let cursors = unsafe { Cursors::new(&area, &offsets.tx, size) };

        Ok(Self(Ring { cursors, area }))
    }
}

pub struct Rx(Ring);

impl Rx {
    pub(crate) fn new<S: SocketExt>(s: &S, offsets: &MmapOffsets, size: u32) -> io::Result<Self> {
        s.set_rx_ring_size(size)?;

        let len = offsets.rx.desc as usize + size as usize * size_of::<u64>();
        let offset = MmapOffsets::RX_RING;

        let area = Mmap::new(len, offset, s.as_raw_fd())?;

        let cursors = unsafe { Cursors::new(&area, &offsets.rx, size) };

        Ok(Self(Ring { cursors, area }))
    }
}

pub struct Fill(Ring);

impl Fill {
    pub(crate) fn new<S: SocketExt>(s: &S, offsets: &MmapOffsets, size: u32) -> io::Result<Self> {
        s.set_fill_ring_size(size)?;

        let len = offsets.fill.desc as usize + size as usize * size_of::<u64>();
        let offset = MmapOffsets::FILL_RING;

        let area = Mmap::new(len, offset, s.as_raw_fd())?;

        let cursors = unsafe { Cursors::new(&area, &offsets.fill, size) };

        Ok(Self(Ring { cursors, area }))
    }
}

pub struct Completion(Ring);

impl Completion {
    pub(crate) fn new<S: SocketExt>(s: &S, offsets: &MmapOffsets, size: u32) -> io::Result<Self> {
        s.set_completion_ring_size(size)?;

        let len = offsets.completion.desc as usize + size as usize * size_of::<u64>();
        let offset = MmapOffsets::COMPLETION_RING;

        let area = Mmap::new(len, offset, s.as_raw_fd())?;

        let cursors = unsafe { Cursors::new(&area, &offsets.completion, size) };

        Ok(Self(Ring { cursors, area }))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use bolero::{check, generator::*};
    use core::{cell::UnsafeCell, num::NonZeroU16};

    #[derive(Clone, Copy, Debug, TypeGenerator)]
    enum Op {
        ConsumerAcquire(u16),
        ConsumerRelease(u16),
        ProducerAcquire(u16),
        ProducerRelease(u16),
    }

    #[derive(Clone, Debug, Default)]
    struct Oracle {
        size: u32,
        producer: u32,
        consumer: u32,
    }

    impl Oracle {
        fn acquire_consumer(&self, actual: u32, count: u16) {
            let expected = self.consumer;
            assert!(
                actual <= expected,
                "\n{self:#?}\nactual: {actual}\ncount: {count}"
            );
            assert!(actual <= self.size);
        }

        fn release_consumer(&mut self, count: u16) -> u32 {
            let count = self.consumer.min(count as u32);

            self.consumer -= count;
            self.producer += count;

            self.invariants();
            count
        }

        fn acquire_producer(&self, actual: u32, count: u16) {
            let expected = self.producer;
            assert!(
                actual <= expected,
                "\n{self:#?}\nactual: {actual}\nexpected: {expected}\ncount: {count}"
            );
            assert!(actual <= self.size);
        }

        fn release_producer(&mut self, count: u16) -> u32 {
            let count = self.producer.min(count as u32);

            self.producer -= count;
            self.consumer += count;

            self.invariants();
            count
        }

        fn invariants(&self) {
            assert_eq!(self.size, self.producer + self.consumer);
        }
    }

    #[test]
    fn cursor_test() {
        check!()
            .with_type::<(NonZeroU16, Vec<Op>)>()
            .for_each(|(size, ops)| {
                let size = size.get() as u32;
                let producer_v = UnsafeCell::new(AtomicU32::new(0));
                let consumer_v = UnsafeCell::new(AtomicU32::new(0));

                let producer_v = producer_v.get();
                let consumer_v = consumer_v.get();

                let mut oracle = Oracle {
                    size,
                    producer: size,
                    ..Default::default()
                };

                let mut producer = Cursors {
                    cached_consumer: Wrapping(0),
                    cached_producer: Wrapping(0),
                    size,
                    producer: NonNull::new(producer_v).unwrap(),
                    consumer: NonNull::new(consumer_v).unwrap(),
                    desc: NonNull::dangling(),
                    flags: NonNull::dangling(),
                    mask: 0,
                };
                let mut consumer = Cursors {
                    cached_consumer: Wrapping(0),
                    cached_producer: Wrapping(0),
                    size,
                    producer: NonNull::new(producer_v).unwrap(),
                    consumer: NonNull::new(consumer_v).unwrap(),
                    desc: NonNull::dangling(),
                    flags: NonNull::dangling(),
                    mask: 0,
                };

                for op in ops.iter().copied() {
                    match op {
                        Op::ConsumerAcquire(count) => {
                            let actual = consumer.acquire_consumer(count as _);
                            oracle.acquire_consumer(actual, count);
                        }
                        Op::ConsumerRelease(count) => {
                            let oracle_count = oracle.release_consumer(count);
                            consumer.release_consumer(oracle_count);
                        }
                        Op::ProducerAcquire(count) => {
                            let actual = producer.acquire_producer(count as _);
                            oracle.acquire_producer(actual, count);
                        }
                        Op::ProducerRelease(count) => {
                            let oracle_count = oracle.release_producer(count);
                            producer.release_producer(oracle_count);
                        }
                    }
                }
            });
    }
}
