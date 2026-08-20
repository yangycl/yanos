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

use bootloader_api::{entry_point, BootInfo};


entry_point!(kernel_main);

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
    interrupts::init_idt();
    x86_64::instructions::interrupts::enable();

    unsafe {
        ALLOCATOR.lock().init(HEAP_SPACE.0.as_mut_ptr(), HEAP_SIZE);
    }

    // 保存 framebuffer（panic 用）
    if let Some(fb) = boot_info.framebuffer.as_mut() {
        unsafe {
            FRAMEBUFFER = Some(fb as *mut _);
        }
    }

        
    //白色長方形桌面
    if let Some(framebuffer) = 
    boot_info.framebuffer.as_mut() {

        let width =
            framebuffer.info().width as usize;

        let height =
            framebuffer.info().height as usize;

        let buffer = framebuffer.buffer_mut();

        //白長方形
        draw_rect(
            buffer,
            width,
            0,
            0,
            width,
            height,
            [255,255,255],
        );


        draw_string(buffer, width, 10, 10, "YANOS BOOT", [255, 255, 255]);
        draw_string(buffer, width, 10, 30, "PCI SCAN...", [255, 255, 255]);

        let xhci_bar = crate::drivers::pci::find_xhci_bar0();

        let width = framebuffer.info().width as usize;
        let buffer = framebuffer.buffer_mut();
        match xhci_bar {
            Some(_) => draw_string(buffer, width, 10, 50, "XHCI OK", [255, 255, 255]),
            None => draw_string(buffer, width, 10, 50, "NO XHCI", [255, 255, 255]),
        }
        draw_string(buffer, width, 10, 70, "USB INIT...", [255, 255, 255]);

        let usb_ok = crate::drivers::usb_host::init(); // 只留一次

        let width = framebuffer.info().width as usize;
        let buffer = framebuffer.buffer_mut();
        if usb_ok {
            draw_string(buffer, width, 10, 90, "HID KBD OK", [255, 255, 255]);
        } else {
            draw_string(buffer, width, 10, 90, "NO HID", [255, 255, 255]);
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

    //鍵盤

    let usb_ok = crate::drivers::usb_host::init();
    // 可畫 "HID KBD OK" / "NO HID"

    let mut keyboard = KeyboardDecoder::new();

    let mut keyboard_state =
        KeyboardState::new();


    use crate::fs::fat32::Fat32;


    use crate::fs::ramdisk::RamDisk;

    let mut memory = [0u8; 512 * 10];


    // ==========================
    // 1. 建立假 Boot Sector
    // ==========================

    // bytes per sector = 512
    memory[11..13]
        .copy_from_slice(
            &512u16.to_le_bytes()
        );


    // sectors per cluster = 1
    memory[13] = 1;


    // reserved sectors = 1
    memory[14..16]
        .copy_from_slice(
            &1u16.to_le_bytes()
        );


    // FAT 數量 = 1
    memory[16] = 1;


    // FAT size = 1 sector
    memory[36..40]
        .copy_from_slice(
            &1u32.to_le_bytes()
        );


    // root cluster = 2
    memory[44..48]
        .copy_from_slice(
            &2u32.to_le_bytes()
        );


    // FAT32 signature
    memory[510] = 0x55;
    memory[511] = 0xAA;



    // ==========================
    // 2. 建立 FAT 表
    // sector 1
    // ==========================

    let fat =
        &mut memory[512..1024];


    // FAT[0] 保留
    fat[0..4]
        .copy_from_slice(
            &0x0FFFFFF8u32.to_le_bytes()
        );


    // FAT[1] 保留
    fat[4..8]
        .copy_from_slice(
            &0x0FFFFFFFu32.to_le_bytes()
        );


    // FAT[2] = 3
    // cluster 2 下一個是 cluster 3
    fat[8..12]
        .copy_from_slice(
            &3u32.to_le_bytes()
        );


    // FAT[3] = EOF
    // cluster 3 是最後一塊
    fat[12..16]
        .copy_from_slice(
            &0x0FFFFFFFu32.to_le_bytes()
        );



    // ==========================
    // 3. 建立資料區
    // cluster 2
    // sector 2
    // ==========================

    memory[1024] = b'A';
    memory[1025] = b'B';
    memory[1026] = b'C';



    // ==========================
    // cluster 3
    // sector 3
    // ==========================

    memory[1536] = b'D';
    memory[1537] = b'E';



    // ==========================
    // 建立 RamDisk
    // ==========================

    let disk =
        RamDisk::new(
            &mut memory
        );

    let mut fat32 =
        Fat32::mount(disk);
        
    let entry =
        DirectoryEntry {

            name:
                *b"TEST    TXT",

            attr:
                0x20,

            first_cluster:
                None,

            file_size:
                0,
        };


    let mut file =
        File::new(
            &mut fat32,
            entry,
        );


    file.write(
        b"HELLO",
    )
    .unwrap();


    file.position = 0;


    let mut buffer =
        [0u8;512];


    file.read(
        &mut buffer,
    );


    let text =
        core::str::from_utf8(
            &buffer[..5]
        )
        .unwrap_or(
            "READ ERROR"
        );






    let entry =
        DirectoryEntry {
            name: *b"TEST    TXT",
            attr: 0x20,
            first_cluster: None,
            file_size: 0,
        };


    let mut file =
        File::new(
            &mut fat32,
            entry,
        );

    //explorer
    let mut is_explorer_running = false;
    let mut explorer =
        explorer::explorer::Explorer::new(
            fat32.root_cluster,
        );

    // Fake USB HID: modifier=0, key 'e' = usage 0x08
    // crate::interrupts::push_usb_report([0, 0, 0x08, 0, 0, 0, 0, 0]);
    loop {

        
        crate::drivers::usb_host::poll();

        // 1. 單一事件處理入口
        while let Some(event) = keyboard.process() {
            keyboard_state.update(event);
            if is_explorer_running {
                explorer.update(event);
            }
        }

        // 2. 按鍵切換邏輯
        if let Some(ch) = drivers::keyboard::read_char() {
            match ch {
                'q' => { 
                    is_explorer_running = false;
                    if let Some(framebuffer) = 
                        boot_info.framebuffer.as_mut()  {
                        let width =
                            framebuffer.info().width as usize;

                        let height =
                            framebuffer.info().height as usize;

                        let buffer = framebuffer.buffer_mut();

                        draw_rect(buffer, width, 0, 0, width, height, [255, 255, 255]);

                    }
                }
                'e' => { is_explorer_running = true; }
                _ => {}
            }
        }

        // 3. 畫面繪製分流
        if let Some(framebuffer) = boot_info.framebuffer.as_mut() {
            if is_explorer_running {
                // 只有在切換到 explorer 時才畫 explorer 介面
                explorer.draw(framebuffer, &mut fat32);
            } else {
                // 未啟動 explorer 時，繪製預設桌面與文字
                let width = framebuffer.info().width as usize;
                let height = framebuffer.info().height as usize;
                let buffer = framebuffer.buffer_mut();

                // 畫滑鼠游標
                draw_cursor(buffer, width, mouse.x, mouse.y);
            }
        }

        x86_64::instructions::hlt();
    }
}

use core::panic::PanicInfo;



//崩潰橘屏
#[panic_handler]
fn panic(info: &PanicInfo) -> ! {

    unsafe {

        if let Some(ptr) = FRAMEBUFFER {

            let framebuffer =
                &mut *ptr;
            
            let width = framebuffer.info().width as usize;

            draw_orange_screen(framebuffer);
            // Only show location if available, otherwise a generic message
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

