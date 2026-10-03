use core::sync::atomic::{AtomicUsize, Ordering};
use usb_oxide::Dma;

pub struct MyDma {
    base: usize,
    size: usize,
    offset: AtomicUsize,
}

impl MyDma {
    pub const fn new(base: usize, size: usize) -> Self {
        Self {
            base,
            size,
            offset: AtomicUsize::new(0),
        }
    }
}

impl Dma for MyDma {
    unsafe fn alloc(&self, size: usize, align: usize) -> Option<usize> {
        let align = align.max(1);
        loop {
            let cur = self.offset.load(Ordering::Relaxed);
            let aligned = (cur + align - 1) & !(align - 1);
            let next = aligned.checked_add(size)?;
            if next > self.size {
                return None;
            }
            if self
                .offset
                .compare_exchange(cur, next, Ordering::SeqCst, Ordering::Relaxed)
                .is_ok()
            {
                return Some(self.base + aligned);
            }
        }
    }

    unsafe fn free(&self, _addr: usize, _size: usize, _align: usize) {}

    unsafe fn map_mmio(&self, phys: usize, size: usize) -> Option<usize> {
        crate::memory::paging::map_mmio(phys as u64, size).map(|v| v as usize)
    }

    unsafe fn unmap_mmio(&self, _virt: usize, _size: usize) {}

    fn virt_to_phys(&self, va: usize) -> usize {
        crate::memory::paging::virt_to_phys(va as u64) as usize
    }

    fn page_size(&self) -> usize {
        4096
    }
}

/// 1MB DMA 池。虛擬位址在 kernel image 裡，實體位址靠 virt_to_phys。
#[repr(align(4096))]
pub struct DmaPool(pub [u8; 1024 * 1024]);

pub static mut DMA_POOL: DmaPool = DmaPool([0; 1024 * 1024]);