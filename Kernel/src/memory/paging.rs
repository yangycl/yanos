use core::sync::atomic::{AtomicU64, Ordering};

use bootloader_api::info::{MemoryRegionKind, BootInfo};
use x86_64::registers::control::Cr3;
use x86_64::VirtAddr;

const PRESENT: u64 = 1 << 0;
const WRITABLE: u64 = 1 << 1;
const PCD: u64 = 1 << 4;
const PWT: u64 = 1 << 3;
const PS: u64 = 1 << 7;
const ADDR_MASK: u64 = 0x000F_FFFF_FFFF_F000;
const PAGE: u64 = 4096;

static PHYS_OFFSET: AtomicU64 = AtomicU64::new(0);
static KERNEL_IMAGE_OFFSET: AtomicU64 = AtomicU64::new(0);
static KERNEL_PHYS: AtomicU64 = AtomicU64::new(0);
static NEXT_TABLE_PHYS: AtomicU64 = AtomicU64::new(0);
static TABLE_PHYS_END: AtomicU64 = AtomicU64::new(0);
static NEXT_MMIO_VIRT: AtomicU64 = AtomicU64::new(0);
static MMIO_VIRT_LIMIT: AtomicU64 = AtomicU64::new(0);

pub struct MemoryManager {
    pub physical_memory_offset: VirtAddr,
}

impl MemoryManager {
    pub fn new(offset: u64) -> Self {
        Self {
            physical_memory_offset: VirtAddr::new(offset),
        }
    }

    pub fn physical_to_virtual(&self, physical: u64) -> VirtAddr {
        self.physical_memory_offset + physical
    }
}

pub fn init(boot_info: &BootInfo) {
    let offset = boot_info
        .physical_memory_offset
        .into_option()
        .expect("physical_memory_offset missing; enable Mapping::Dynamic");
    PHYS_OFFSET.store(offset, Ordering::Relaxed);
    KERNEL_IMAGE_OFFSET.store(boot_info.kernel_image_offset, Ordering::Relaxed);
    KERNEL_PHYS.store(boot_info.kernel_addr, Ordering::Relaxed);

    let mut best_start = 0u64;
    let mut best_end = 0u64;
    for region in boot_info.memory_regions.iter() {
        if region.kind != MemoryRegionKind::Usable {
            continue;
        }
        let start = (region.start + PAGE - 1) & !(PAGE - 1);
        let end = region.end & !(PAGE - 1);
        if end > start && end - start > best_end - best_start {
            best_start = start;
            best_end = end;
        }
    }
    // 從這塊 usable RAM 的尾端切頁表頁，避開開頭。
    let reserve = 256 * PAGE;
    let alloc_end = best_end;
    let alloc_start = best_start.max(alloc_end.saturating_sub(reserve));
    NEXT_TABLE_PHYS.store(alloc_start, Ordering::Relaxed);
    TABLE_PHYS_END.store(alloc_end, Ordering::Relaxed);
}

fn phys_offset() -> u64 {
    PHYS_OFFSET.load(Ordering::Relaxed)
}

fn alloc_table_page() -> Option<u64> {
    let end = TABLE_PHYS_END.load(Ordering::Relaxed);
    loop {
        let cur = NEXT_TABLE_PHYS.load(Ordering::Relaxed);
        let next = cur.checked_add(PAGE)?;
        if next > end {
            return None;
        }
        if NEXT_TABLE_PHYS
            .compare_exchange(cur, next, Ordering::SeqCst, Ordering::Relaxed)
            .is_ok()
        {
            let virt = (cur + phys_offset()) as *mut u8;
            unsafe {
                core::ptr::write_bytes(virt, 0, PAGE as usize);
            }
            return Some(cur);
        }
    }
}

fn index(virt: u64, level: u8) -> usize {
    let shift = 39 - level * 9;
    ((virt >> shift) & 0x1FF) as usize
}

/// 真機的 xHCI BAR 不能走 bootloader physmap：那張表是 write-back。
/// 實機對 MMIO 做 WB 讀寫會被晶片組當成非法存取，直接重開。
/// 這裡另開一個空的高半部 PML4，leaf 設 PCD|PWT（uncacheable）。
pub fn map_mmio(phys: u64, size: usize) -> Option<u64> {
    if size == 0 || phys_offset() == 0 {
        return None;
    }
    map_mmio_window(phys, size)
}

fn map_mmio_window(phys: u64, size: usize) -> Option<u64> {
    let pages = (size as u64).div_ceil(PAGE) as usize;
    if pages == 0 {
        return None;
    }
    let base = mmio_window_base()?;
    let virt = NEXT_MMIO_VIRT.fetch_add(pages as u64 * PAGE, Ordering::Relaxed);
    let end = virt.checked_add(pages as u64 * PAGE)?;
    if virt < base || end > MMIO_VIRT_LIMIT.load(Ordering::Relaxed) {
        return None;
    }
    map_pages(virt, phys & ADDR_MASK, pages)?;
    Some(virt + (phys & 0xFFF))
}

/// 找一個空的高半部 PML4，避免蓋掉 bootloader 已占用的 slot。
fn mmio_window_base() -> Option<u64> {
    let existing = NEXT_MMIO_VIRT.load(Ordering::Relaxed);
    if existing != 0 {
        return Some(existing & !((1 << 39) - 1));
    }
    let pml4_phys = Cr3::read().0.start_address().as_u64();
    let pml4 = (pml4_phys + phys_offset()) as *const u64;
    for idx in 256..511u64 {
        let entry = unsafe { core::ptr::read_volatile(pml4.add(idx as usize)) };
        if entry & PRESENT != 0 {
            continue;
        }
        let base = (0xFFFFu64 << 48) | (idx << 39);
        NEXT_MMIO_VIRT.store(base, Ordering::Relaxed);
        MMIO_VIRT_LIMIT.store(base + (1 << 39), Ordering::Relaxed);
        return Some(base);
    }
    None
}

fn map_pages(virt: u64, phys: u64, pages: usize) -> Option<()> {
    for i in 0..pages {
        map_page(virt + i as u64 * PAGE, phys + i as u64 * PAGE)?;
    }
    Some(())
}

fn map_page(virt: u64, phys: u64) -> Option<()> {
    let mut table_phys = Cr3::read().0.start_address().as_u64();
    for level in 0..4 {
        let table = (table_phys + phys_offset()) as *mut u64;
        let idx = index(virt, level);
        let entry = unsafe { core::ptr::read_volatile(table.add(idx)) };
        if level < 3 {
            table_phys = if entry & PRESENT != 0 {
                if entry & (1 << 7) != 0 {
                    return None;
                }
                entry & ADDR_MASK
            } else {
                let page_phys = alloc_table_page()?;
                unsafe {
                    core::ptr::write_volatile(table.add(idx), page_phys | PRESENT | WRITABLE);
                }
                page_phys
            };
        } else {
            unsafe {
                core::ptr::write_volatile(
                    table.add(idx),
                    (phys & ADDR_MASK) | PRESENT | WRITABLE | PCD | PWT,
                );
            }
            x86_64::instructions::tlb::flush(VirtAddr::new(virt));
        }
    }
    Some(())
}

pub fn virt_to_phys(virt: u64) -> u64 {
    if let Some(phys) = walk(virt) {
        return phys;
    }
    // kernel_image_offset 是映像的虛擬基底，不是 slide。
    // phys = kernel_addr + (va - kernel_image_offset)
    let image = KERNEL_IMAGE_OFFSET.load(Ordering::Relaxed);
    let phys_base = KERNEL_PHYS.load(Ordering::Relaxed);
    virt.wrapping_sub(image).wrapping_add(phys_base)
}

fn walk(virt: u64) -> Option<u64> {
    let mut table_phys = Cr3::read().0.start_address().as_u64();
    for level in 0..4 {
        let table = (table_phys + phys_offset()) as *mut u64;
        let entry = unsafe { core::ptr::read_volatile(table.add(index(virt, level))) };
        if entry & PRESENT == 0 {
            return None;
        }
        // bootloader 的 kernel / physmap 常用 2MB、1GB 大頁。
        if level == 1 && entry & PS != 0 {
            let base = entry & 0x000F_FFFF_C000_0000;
            return Some(base | (virt & 0x3FFF_FFFF));
        }
        if level == 2 && entry & PS != 0 {
            let base = entry & 0x000F_FFFF_FFE0_0000;
            return Some(base | (virt & 0x1F_FFFF));
        }
        if level < 3 {
            if entry & PS != 0 {
                return None;
            }
            table_phys = entry & ADDR_MASK;
        } else {
            return Some((entry & ADDR_MASK) | (virt & 0xFFF));
        }
    }
    None
}
