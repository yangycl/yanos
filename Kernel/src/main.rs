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
use crate::drivers::pci::find_xhci_bar0;


use crate::fs::directory::DirectoryEntry;
use crate::fs::file_location::FileLocation;

use linked_list_allocator::LockedHeap;

mod explorer;
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


struct Mouse {
    x: i32,
    y: i32,

    up:bool,
    down:bool,
    left:bool,
    right:bool,
}



const HEAP_SIZE: usize = 1024 * 1024; // 1MB Heap

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
        let xhci_bar = crate::drivers::pci::find_xhci_bar0();
        let width = framebuffer.info().width as usize;
        let buffer = framebuffer.buffer_mut();
        match xhci_bar {
            Some(_) => draw_string(buffer, width, 10, 50, "XHCI OK", [0, 0, 0]),
            None => draw_string(buffer, width, 10, 50, "NO XHCI", [0, 0, 0]),
        }
        draw_string(buffer, width, 10, 70, "USB INIT...", [0, 0, 0]);
        let usb_ok = crate::drivers::usb_host::init();
        let width = framebuffer.info().width as usize;
        let buffer = framebuffer.buffer_mut();
        draw_string(buffer, width, 10, 90, crate::drivers::usb_host::status(), [0, 0, 0]);
        for (i, line) in crate::drivers::usb_host::log_lines().iter().enumerate() {
            draw_string(buffer, width, 10, 110 + i * 10, line, [0, 0, 0]);
        }
        let _ = usb_ok;
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
    use crate::fs::fat32::Fat32;
    use crate::fs::ramdisk::RamDisk;
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
    memory[1024] = b'A';
    memory[1025] = b'B';
    memory[1026] = b'C';
    memory[1536] = b'D';
    memory[1537] = b'E';
    let disk = RamDisk::new(&mut memory);
    let mut fat32 = Fat32::mount(disk);
    let entry = DirectoryEntry {
        name: *b"TEST    TXT",
        attr: 0x20,
        first_cluster: None,
        file_size: 0,
    };
    let mut file = File::new(&mut fat32, entry);
    file.write(b"HELLO").unwrap();
    file.position = 0;
    let mut buffer = [0u8;512];
    file.read(&mut buffer);
    let text = core::str::from_utf8(&buffer[..5]).unwrap_or("READ ERROR");
    let entry = DirectoryEntry {
        name: *b"TEST    TXT",
        attr: 0x20,
        first_cluster: None,
        file_size: 0,
    };
    let mut file = File::new(&mut fat32, entry);
    let mut is_explorer_running = false;
    let mut explorer = explorer::explorer::Explorer::new(fat32.root_cluster);
    loop {
        for _ in 0..8 {
            crate::drivers::usb_host::poll();
        }
        while let Some(event) = keyboard.process() {
            keyboard_state.update(event);
            if is_explorer_running {
                explorer.update(event);
            }
        }
        if let Some(ch) = drivers::keyboard::read_char() {
            if let Some(framebuffer) = boot_info.framebuffer.as_mut() {
                let width = framebuffer.info().width as usize;
                let buffer = framebuffer.buffer_mut();
                let shown = [ch as u8];
                let label = core::str::from_utf8(&shown).unwrap_or("?");
                draw_string(buffer, width, 10, 110, "KEY", [0, 0, 0]);
                draw_string(buffer, width, 50, 110, label, [0, 0, 0]);
            }
            match ch {
                'q' => {
                    is_explorer_running = false;
                    if let Some(framebuffer) = boot_info.framebuffer.as_mut() {
                        let width = framebuffer.info().width as usize;
                        let height = framebuffer.info().height as usize;
                        let buffer = framebuffer.buffer_mut();
                        draw_rect(buffer, width, 0, 0, width, height, [255, 255, 255]);
                    }
                }
                'e' => { is_explorer_running = true; }
                _ => {}
            }
        }
        if let Some(framebuffer) = boot_info.framebuffer.as_mut() {
            if is_explorer_running {
                explorer.draw(framebuffer, &mut fat32);
            } else {
                let width = framebuffer.info().width as usize;
                let buffer = framebuffer.buffer_mut();
                draw_cursor(buffer, width, mouse.x, mouse.y);
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
