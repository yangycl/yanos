use core::sync::atomic::{AtomicU64, Ordering};

use bootloader_api::info::{MemoryRegionKind, BootInfo};
use x86_64::registers::control::Cr3;
use x86_64::VirtAddr;

const PRESENT: u64 = 1 << 0;
const WRITABLE: u64 = 1 << 1;
const PCD: u64 = 1 << 4;
const ADDR_MASK: u64 = 0x000F_FFFF_FFFF_F000;
const PAGE: u64 = 4096;

/// 給 xHCI MMIO 用的虛擬洞，不跟 kernel / framebuffer 重疊。
const MMIO_VIRT_BASE: u64 = 0xFFFF_E000_0000_0000;
const MMIO_WINDOW: u64 = 64 * 1024 * 1024;

static PHYS_OFFSET: AtomicU64 = AtomicU64::new(0);
static KERNEL_IMAGE_OFFSET: AtomicU64 = AtomicU64::new(0);
static NEXT_TABLE_PHYS: AtomicU64 = AtomicU64::new(0);
static TABLE_PHYS_END: AtomicU64 = AtomicU64::new(0);
static NEXT_MMIO_VIRT: AtomicU64 = AtomicU64::new(MMIO_VIRT_BASE);

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

/// 在現有 CR3 頁表上加映射。回傳這次用的虛擬基底。
pub fn map_mmio(phys: u64, size: usize) -> Option<u64> {
    let pages = (size as u64).div_ceil(PAGE) as usize;
    if pages == 0 {
        return None;
    }
    let virt = NEXT_MMIO_VIRT.fetch_add(pages as u64 * PAGE, Ordering::Relaxed);
    if virt < MMIO_VIRT_BASE || virt + pages as u64 * PAGE > MMIO_VIRT_BASE + MMIO_WINDOW {
        return None;
    }
    let bar = phys & ADDR_MASK;
    map_pages(virt, bar, pages)?;
    Some(virt)
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
                    (phys & ADDR_MASK) | PRESENT | WRITABLE | PCD,
                );
            }
            x86_64::instructions::tlb::flush(VirtAddr::new(virt));
        }
    }
    Some(())
}

pub fn virt_to_phys(virt: u64) -> u64 {
    if (MMIO_VIRT_BASE..MMIO_VIRT_BASE + MMIO_WINDOW).contains(&virt) {
        return walk(virt).unwrap_or(virt);
    }
    let slide = KERNEL_IMAGE_OFFSET.load(Ordering::Relaxed);
    virt.wrapping_sub(slide)
}

fn walk(virt: u64) -> Option<u64> {
    let mut table_phys = Cr3::read().0.start_address().as_u64();
    for level in 0..4 {
        let table = (table_phys + phys_offset()) as *mut u64;
        let entry = unsafe { core::ptr::read_volatile(table.add(index(virt, level))) };
        if entry & PRESENT == 0 {
            return None;
        }
        if level < 3 {
            table_phys = entry & ADDR_MASK;
        } else {
            return Some((entry & ADDR_MASK) | (virt & 0xFFF));
        }
    }
    None
}
