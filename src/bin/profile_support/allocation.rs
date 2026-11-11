//! Safe access to the optional diagnostic allocator. Ordinary builds take no snapshots.
use serde::Serialize;

#[cfg(feature = "allocation-profile")]
use stats_alloc::{INSTRUMENTED_SYSTEM, Region, StatsAlloc};
#[cfg(feature = "allocation-profile")]
use std::alloc::System;

#[cfg(feature = "allocation-profile")]
#[global_allocator]
static GLOBAL: &StatsAlloc<System> = &INSTRUMENTED_SYSTEM;

#[derive(Clone, Copy, Serialize)]
pub struct Counts {
    allocations: usize,
    deallocations: usize,
    reallocations: usize,
    /// The pinned allocator includes positive realloc growth here.
    bytes_allocated: usize,
    /// The pinned allocator includes realloc shrinkage here.
    bytes_deallocated: usize,
    /// Signed resize delta, already accounted for in the two byte totals.
    bytes_reallocated: isize,
}

pub struct Window {
    #[cfg(feature = "allocation-profile")]
    region: Region<'static, System>,
}

impl Window {
    pub fn new() -> Self {
        Self {
            #[cfg(feature = "allocation-profile")]
            region: Region::new(GLOBAL),
        }
    }

    pub fn finish(self) -> Option<Counts> {
        #[cfg(feature = "allocation-profile")]
        {
            let stats = self.region.change();
            Some(Counts {
                allocations: stats.allocations,
                deallocations: stats.deallocations,
                reallocations: stats.reallocations,
                bytes_allocated: stats.bytes_allocated,
                bytes_deallocated: stats.bytes_deallocated,
                bytes_reallocated: stats.bytes_reallocated,
            })
        }
        #[cfg(not(feature = "allocation-profile"))]
        None
    }
}

pub fn measure<T>(work: impl FnOnce() -> T) -> (T, Option<Counts>) {
    let window = Window::new();
    let result = work();
    (result, window.finish())
}
