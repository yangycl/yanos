#![no_std]
#![no_main]
#![allow(static_mut_refs)]

#![feature(abi_x86_interrupt)]

#![no_std]
#![no_main]

extern crate alloc;

mod arch;
mod drivers;

use arch::x86_64::{
    interrupts,
    pic,
};

use crate::fs::file::File;

use drivers::keyboard::{
    KeyboardDecoder,
    KeyEvent,
    KeyboardState,
};

mod fs;
mod yashell;

use drivers::framebuffer::{
    draw_rect,
    draw_cursor,
    draw_string,
    draw_char,
    draw_orange_screen,
};
use crate::usb::pci::find_xhci_bar0;


use crate::fs::directory::DirectoryEntry;
use crate::fs::file_location::FileLocation;

use linked_list_allocator::LockedHeap;

mod explorer;
mod exec;
mod desktop;
mod dma;
mod usb;
mod memory;

use bootloader_api::config::{BootloaderConfig, Mapping};
use bootloader_api::{entry_point, BootInfo};

pub static BOOTLOADER_CONFIG: BootloaderConfig = {
    let mut config = BootloaderConfig::new_default();
    config.mappings.physical_memory = Some(Mapping::Dynamic);
    config
};

entry_point!(kernel_main, config = &BOOTLOADER_CONFIG);

static mut FRAMEBUFFER:
    Option<*mut bootloader_api::info::FrameBuffer> = None;
static mut DEV_HOLD_LOG: bool = false;


struct Mouse {
    x: i32,
    y: i32,

    up:bool,
    down:bool,
    left:bool,
    right:bool,
}



const HEAP_SIZE: usize = 16 * 1024 * 1024; // 16MB heap, including a 1080p back buffer

#[repr(C, align(4096))]
struct HeapSpace([u8; HEAP_SIZE]);

static mut HEAP_SPACE: HeapSpace = HeapSpace([0; HEAP_SIZE]);

#[global_allocator]
static ALLOCATOR: LockedHeap = LockedHeap::empty();


fn kernel_main(
    boot_info: &'static mut BootInfo
) -> ! {

    pic::init();
    pic::unmask_keyboard_and_timer();
    interrupts::init_idt();
    drivers::keyboard::init_ps2();
    x86_64::instructions::interrupts::enable();

    unsafe {
        ALLOCATOR.lock().init(HEAP_SPACE.0.as_mut_ptr(), HEAP_SIZE);
    }

    memory::paging::init(boot_info);

    // framebuffer for panic
    if let Some(fb) = boot_info.framebuffer.as_mut() {
        unsafe {
            FRAMEBUFFER = Some(fb as *mut _);
        }
    }

    if let Some(framebuffer) = boot_info.framebuffer.as_mut() {
        let width = framebuffer.info().width as usize;
        let height = framebuffer.info().height as usize;
        let buffer = framebuffer.buffer_mut();
        draw_rect(buffer, width, 0, 0, width, height, [255,255,255]);
        draw_string(buffer, width, 10, 10, "YANOS BOOT", [0, 0, 0]);
        draw_string(buffer, width, 10, 30, "PCI SCAN...", [0, 0, 0]);
        let xhci_bar = crate::usb::pci::find_xhci_bar0();
        let width = framebuffer.info().width as usize;
        let buffer = framebuffer.buffer_mut();
        match xhci_bar {
            Some(_) => draw_string(buffer, width, 10, 50, "XHCI OK", [0, 0, 0]),
            None => draw_string(buffer, width, 10, 50, "NO XHCI", [0, 0, 0]),
        }
        draw_string(buffer, width, 10, 70, "USB INIT...", [0, 0, 0]);
        let usb_ok = crate::usb::init();
        let width = framebuffer.info().width as usize;
        let buffer = framebuffer.buffer_mut();
        draw_string(buffer, width, 10, 90, crate::usb::status(), [0, 0, 0]);
        for (i, line) in crate::usb::log_lines().iter().enumerate() {
            draw_string(buffer, width, 10, 110 + i * 10, line, [0, 0, 0]);
        }
        let _ = usb_ok;
        // PIT 預設約 18.2Hz。91 tick 大約 5 秒，然後清掉開機 LOG。
        // 桌面還沒蓋上去時按 d，log 留在畫面上。再按一次才 draw。
        let start = crate::arch::x86_64::interrupts::TIMER_TICKS
            .load(core::sync::atomic::Ordering::Relaxed);
        let mut spins = 0u32;
        let mut hold_log = false;
        while crate::arch::x86_64::interrupts::TIMER_TICKS
            .load(core::sync::atomic::Ordering::Relaxed)
            .wrapping_sub(start)
            < 91
            && spins < 80_000_000
        {
            crate::usb::poll();
            if matches!(drivers::keyboard::read_char(), Some('d') | Some('D')) {
                hold_log = true;
                let width = framebuffer.info().width as usize;
                let buffer = framebuffer.buffer_mut();
                draw_string(buffer, width, 10, 10, "DEV LOG", [180, 0, 0]);
            }
            x86_64::instructions::hlt();
            spins += 1;
        }
        if !hold_log {
            desktop::desktop::Desktop::new().draw(framebuffer);
        }
        unsafe {
            DEV_HOLD_LOG = hold_log;
        }
    }
    let mut mouse = Mouse{
        x : 600,
        y : 300,
        up:false,
        down:false,
        left:false,
        right:false,
    };
    // 上面 framebuffer 區塊已經 init 過。再叫一次會 HCRST，把剛找到的鍵盤清掉。
    let mut keyboard = KeyboardDecoder::new();
    let mut keyboard_state = KeyboardState::new();
    use crate::fs::block::BlockDevice;
    use crate::fs::fat32::Fat32;
    use crate::fs::ramdisk::RamDisk;
    use crate::fs::usbdisk::UsbDisk;
    let mut memory = [0u8; 512 * 10];
    memory[11..13].copy_from_slice(&512u16.to_le_bytes());
    memory[13] = 1;
    memory[14..16].copy_from_slice(&1u16.to_le_bytes());
    memory[16] = 1;
    memory[36..40].copy_from_slice(&1u32.to_le_bytes());
    memory[44..48].copy_from_slice(&2u32.to_le_bytes());
    memory[510] = 0x55;
    memory[511] = 0xAA;
    let fat = &mut memory[512..1024];
    fat[0..4].copy_from_slice(&0x0FFFFFF8u32.to_le_bytes());
    fat[4..8].copy_from_slice(&0x0FFFFFFFu32.to_le_bytes());
    fat[8..12].copy_from_slice(&3u32.to_le_bytes());
    fat[12..16].copy_from_slice(&0x0FFFFFFFu32.to_le_bytes());
    // 根目錄在 cluster 2（sector 2）。要是 32-byte 目錄項，不能只塞三個字母。
    let ent = &mut memory[1024..1056];
    ent[0..11].copy_from_slice(b"README  TXT");
    ent[11] = 0x20;
    ent[26..28].copy_from_slice(&3u16.to_le_bytes());
    ent[28..32].copy_from_slice(&5u32.to_le_bytes());
    memory[1536..1541].copy_from_slice(b"hello");
    enum BootDisk<'a> {
        Ram(RamDisk<'a>),
        Usb(UsbDisk),
    }
    impl BlockDevice for BootDisk<'_> {
        fn read_sector(&mut self, lba: u64, buffer: &mut [u8; 512]) {
            match self {
                BootDisk::Ram(disk) => disk.read_sector(lba, buffer),
                BootDisk::Usb(disk) => disk.read_sector(lba, buffer),
            }
        }
        fn write_sector(&mut self, lba: u64, buffer: &[u8; 512]) {
            match self {
                BootDisk::Ram(disk) => disk.write_sector(lba, buffer),
                BootDisk::Usb(disk) => disk.write_sector(lba, buffer),
            }
        }
    }
    let mut volume = "RAM";
    let mut fat32 = if let Some(msc) = crate::usb::take_stick() {
        match UsbDisk::open(msc) {
            Some(disk) => {
                volume = "USB";
                Fat32::mount(BootDisk::Usb(disk))
            }
            None => Fat32::mount(BootDisk::Ram(RamDisk::new(&mut memory))),
        }
    } else {
        Fat32::mount(BootDisk::Ram(RamDisk::new(&mut memory)))
    };
    let mut is_explorer_running = false;
    let mut explorer = explorer::explorer::Explorer::new(fat32.root_cluster);
    let desktop = desktop::desktop::Desktop::new();
    let mut back_buffer = alloc::vec::Vec::new();
    if let Some(framebuffer) = boot_info.framebuffer.as_ref() {
        back_buffer.extend_from_slice(framebuffer.buffer());
    }
    let mut prev_left = false;
    let mut hold_log = unsafe { DEV_HOLD_LOG };
    let mut redraw = !hold_log;
    loop {
        for _ in 0..8 {
            crate::usb::poll();
        }
        while let Some(event) = keyboard.process() {
            keyboard_state.update(event);
            if is_explorer_running {
                explorer.update(event);
                if matches!(event, KeyEvent::UpPress | KeyEvent::DownPress) {
                    redraw = true;
                }
            }
        }
        let (dx, dy, buttons) = crate::usb::take_mouse();
        if dx != 0 || dy != 0 {
            redraw = true;
        }
        mouse.x += dx;
        mouse.y += dy;
        let left = buttons & 1 != 0;
        if left && !prev_left && desktop.hit_explorer(mouse.x, mouse.y) {
            let cluster = fat32
                .resolve_path(fat32.root_cluster, desktop::desktop::DESKTOP_PATH)
                .and_then(|entry| entry.first_cluster)
                .unwrap_or(fat32.root_cluster);
            explorer = explorer::explorer::Explorer::with_volume(cluster, volume);
            is_explorer_running = true;
            redraw = true;
        }
        prev_left = left;
        while let Some(ch) = drivers::keyboard::read_char() {
            match ch {
                'q' => {
                    if is_explorer_running {
                        is_explorer_running = false;
                        redraw = true;
                    }
                }
                'r' if is_explorer_running => {
                    if let Some(entry) = explorer.selected_entry(&mut fat32) {
                        let _ = exec::load_and_run(&mut fat32, entry);
                    }
                    redraw = true;
                }
                'd' | 'D' if hold_log => {
                    hold_log = false;
                    redraw = true;
                }
                'e' | 'E' => {
                    let cluster = fat32
                        .resolve_path(fat32.root_cluster, desktop::desktop::DESKTOP_PATH)
                        .and_then(|entry| entry.first_cluster)
                        .unwrap_or(fat32.root_cluster);
                    explorer = explorer::explorer::Explorer::with_volume(cluster, volume);
                    is_explorer_running = true;
                    redraw = true;
                }
                _ => {}
            }
        }
        if let Some(framebuffer) = boot_info.framebuffer.as_mut() {
            let width = framebuffer.info().width as i32;
            let height = framebuffer.info().height as i32;
            if mouse.x < 0 {
                mouse.x = 0;
            }
            if mouse.y < 0 {
                mouse.y = 0;
            }
            if mouse.x >= width {
                mouse.x = width.saturating_sub(1);
            }
            if mouse.y >= height {
                mouse.y = height.saturating_sub(1);
            }
            if redraw && !hold_log {
                let width = framebuffer.info().width as usize;
                let height = framebuffer.info().height as usize;
                desktop.draw_to_buffer(&mut back_buffer, width, height);
                if is_explorer_running {
                    let win_w = 420.min(width.saturating_sub(80));
                    let win_h = 360.min(height.saturating_sub(80));
                    explorer.draw_window_to_buffer(
                        &mut back_buffer,
                        &mut fat32,
                        width,
                        40,
                        80,
                        win_w,
                        win_h,
                    );
                }
                draw_cursor(&mut back_buffer, width, mouse.x, mouse.y);
                drivers::framebuffer::present_changed_pixels(
                    framebuffer.buffer_mut(),
                    &back_buffer,
                    width,
                    height,
                );
                redraw = false;
            }
        }
        x86_64::instructions::hlt();
    }
}

use core::panic::PanicInfo;

#[panic_handler]
fn panic(info: &PanicInfo) -> ! {
    unsafe {
        if let Some(ptr) = FRAMEBUFFER {
            let framebuffer = &mut *ptr;
            let width = framebuffer.info().width as usize;
            draw_orange_screen(framebuffer);
            if let Some(loc) = info.location() {
                let file = loc.file();
                let line = loc.line();
                let mut buf = [0u8; 96];
                let mut pos = 0usize;
                for &b in b"panic at " { if pos < buf.len() { buf[pos] = b; pos += 1; } }
                for &b in file.as_bytes() { if pos < buf.len() { buf[pos] = b; pos += 1; } }
                if pos < buf.len() { buf[pos] = b':'; pos += 1; }
                let mut tmp = [0u8; 12];
                let mut n = 0usize;
                let mut v = line;
                if v == 0 { tmp[n] = b'0'; n += 1; }
                while v > 0 && n < tmp.len() {
                    tmp[n] = b'0' + (v % 10) as u8;
                    v /= 10;
                    n += 1;
                }
                for i in 0..n { if pos < buf.len() { buf[pos] = tmp[n - 1 - i]; pos += 1; } }
                let loc_str = core::str::from_utf8(&buf[..pos]).unwrap_or("panic at <unknown>");
                draw_string(framebuffer.buffer_mut(), width, 10, 10, loc_str, [255,0,0]);
            } else {
                draw_string(framebuffer.buffer_mut(), width, 10, 10, "panic (no location)", [255,0,0]);
            }
        }
    }
    loop {
        x86_64::instructions::hlt();
    }
}
