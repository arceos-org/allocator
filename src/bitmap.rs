//! Bitmap allocation in page-granularity.

use bitmap_allocator::BitAlloc;

use crate::{AllocError, AllocResult, BaseAllocator, PageAllocator};

cfg_if::cfg_if! {
    if #[cfg(feature = "page-alloc-1t")] {
        /// Support max 256M * PAGE_SIZE = 1TB memory (assume that PAGE_SIZE = 4KB).
        type BitAllocUsed = bitmap_allocator::BitAlloc256M;
    } else if #[cfg(feature = "page-alloc-64g")] {
        /// Support max 16M * PAGE_SIZE = 64GB memory (assume that PAGE_SIZE = 4KB).
        type BitAllocUsed = bitmap_allocator::BitAlloc16M;
    } else if #[cfg(feature = "page-alloc-4g")] {
        /// Support max 1M * PAGE_SIZE = 4GB memory (assume that PAGE_SIZE = 4KB).
        type BitAllocUsed = bitmap_allocator::BitAlloc1M;
    } else {// #[cfg(feature = "page-alloc-256m")]
        /// Support max 64K * PAGE_SIZE = 256MB memory (assume that PAGE_SIZE = 4KB).
        type BitAllocUsed = bitmap_allocator::BitAlloc64K;
    }
}

/// A page-granularity memory allocator based on the [bitmap_allocator].
///
/// It internally uses a bitmap, each bit indicates whether a page has been
/// allocated.
///
/// The `PAGE_SIZE` must be a power of two.
pub struct BitmapPageAllocator<const PAGE_SIZE: usize> {
    base: usize,
    total_pages: usize,
    used_pages: usize,
    inner: BitAllocUsed,
}

impl<const PAGE_SIZE: usize> BitmapPageAllocator<PAGE_SIZE> {
    /// Creates a new empty `BitmapPageAllocator`.
    pub const fn new() -> Self {
        Self {
            base: 0,
            total_pages: 0,
            used_pages: 0,
            inner: BitAllocUsed::DEFAULT,
        }
    }
}

impl<const PAGE_SIZE: usize> BaseAllocator for BitmapPageAllocator<PAGE_SIZE> {
    fn init(&mut self, start: usize, size: usize) {
        assert!(PAGE_SIZE.is_power_of_two());
        let end = super::align_down(start + size, PAGE_SIZE);
        let start = super::align_up(start, PAGE_SIZE);
        self.base = start;
        self.total_pages = (end - start) / PAGE_SIZE;
        self.inner.insert(0..self.total_pages);
    }

    fn add_memory(&mut self, _start: usize, _size: usize) -> AllocResult {
        Err(AllocError::NoMemory) // unsupported
    }
}

impl<const PAGE_SIZE: usize> PageAllocator for BitmapPageAllocator<PAGE_SIZE> {
    const PAGE_SIZE: usize = PAGE_SIZE;

    fn alloc_pages(&mut self, num_pages: usize, align_pow2: usize) -> AllocResult<usize> {
        if align_pow2 % PAGE_SIZE != 0 {
            return Err(AllocError::InvalidParam);
        }
        let align_pow2 = align_pow2 / PAGE_SIZE;
        if !align_pow2.is_power_of_two() {
            return Err(AllocError::InvalidParam);
        }
        let align_log2 = align_pow2.trailing_zeros() as usize;
        match num_pages.cmp(&1) {
            core::cmp::Ordering::Equal => self.inner.alloc().map(|idx| idx * PAGE_SIZE + self.base),
            core::cmp::Ordering::Greater => self
                .inner
                .alloc_contiguous(None, num_pages, align_log2)
                .map(|idx| idx * PAGE_SIZE + self.base),
            _ => return Err(AllocError::InvalidParam),
        }
        .ok_or(AllocError::NoMemory)
        .inspect(|_| self.used_pages += num_pages)
    }

    /// Allocate pages at a specific address.
    fn alloc_pages_at(
        &mut self,
        base: usize,
        num_pages: usize,
        align_pow2: usize,
    ) -> AllocResult<usize> {
        if align_pow2 % PAGE_SIZE != 0 {
            return Err(AllocError::InvalidParam);
        }

        // Check if the base address is aligned to the given alignment and the given PAGE_SIZE.
        if !crate::is_aligned(base, align_pow2) || !crate::is_aligned(base, Self::PAGE_SIZE) {
            return Err(AllocError::InvalidParam);
        }

        let align_pow2 = align_pow2 / PAGE_SIZE;
        if !align_pow2.is_power_of_two() {
            return Err(AllocError::InvalidParam);
        }
        let align_log2 = align_pow2.trailing_zeros() as usize;

        let idx = (base - self.base) / PAGE_SIZE;

        self.inner
            .alloc_contiguous(Some(idx), num_pages, align_log2)
            .map(|idx| idx * PAGE_SIZE + self.base)
            .ok_or(AllocError::NoMemory)
            .inspect(|_| self.used_pages += num_pages)
    }

    fn dealloc_pages(&mut self, pos: usize, num_pages: usize) {
        assert!(
            crate::is_aligned(pos, Self::PAGE_SIZE),
            "pos must be aligned to PAGE_SIZE"
        );

        if self
            .inner
            .dealloc_contiguous((pos - self.base) / PAGE_SIZE, num_pages)
        {
            self.used_pages -= num_pages;
        }
    }

    fn total_pages(&self) -> usize {
        self.total_pages
    }

    fn used_pages(&self) -> usize {
        self.used_pages
    }

    fn available_pages(&self) -> usize {
        self.total_pages - self.used_pages
    }
}

impl<const PAGE_SIZE: usize> BitmapPageAllocator<PAGE_SIZE> {}
