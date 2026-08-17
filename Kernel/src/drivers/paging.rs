use x86_64::structures::paging::{
    FrameAllocator, Mapper, OffsetPageTable, Page, PageTable, PageTableFlags, PhysFrame,
    Size4KiB,
};
use x86_64::VirtAddr;
use x86_64::PhysAddr;

pub struct BootFrameAllocator;

unsafe impl FrameAllocator<Size4KiB> for BootFrameAllocator {
    fn allocate_frame(&mut self) -> Option<PhysFrame<Size4KiB>> {
        None
    }
}

pub unsafe fn init_page_table(
    level_4_table: &'static mut PageTable,
    physical_memory_offset: u64,
) -> OffsetPageTable<'static> {
    let phys = PhysAddr::new(level_4_table as *mut PageTable as u64);

    let virt = VirtAddr::new(
        phys.as_u64() + physical_memory_offset
    );

    OffsetPageTable::new(level_4_table, virt)
}
