#![no_std]
#![no_main]
#![allow(static_mut_refs)]

#![feature(abi_x86_interrupt)]

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

use drivers::framebuffer::{
    draw_rect,
    draw_cursor,
    draw_string,
    draw_char,
    draw_orange_screen,
};

use crate::fs::directory::DirectoryEntry;

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

fn fs_test(fs: &mut Fat32)-> Result<(), Error>{
    let mut file =
        File::open(
            &mut fs,
            root,
            "TEST.TXT",
        )?;


    file.write(
        b"hello"
    );


    let mut buf =
        [0u8;512];


    file.position = 0;


    file.read(
        &mut buf
    );
}

fn kernel_main(
    boot_info: &'static mut BootInfo
) -> ! {

    pic::init();

    interrupts::init_idt();

    x86_64::instructions::interrupts::enable();

    if let Some(framebuffer) =
        boot_info.framebuffer.as_mut()
    {
        unsafe {
            FRAMEBUFFER =
                Some(framebuffer as *mut _);
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
    }
    let mut mouse = Mouse{
        x : 600,
        y : 300,
        up:false,
        down:false,
        left:false,
        right:false,
    };

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


    let cluster =
        file.write(
            b"HELLO"
        )
        .unwrap();

    let entry =
        fat32.find_entry(
            fat32.root_cluster,
            b"TEST    TXT",
        )
        .unwrap();


    let mut file =
        File::new(
            &mut fat32,
            entry,
        );


    let mut buffer =
        [0u8;512];


    let size =
        file.read(
            &mut buffer
        );



    if let Some(framebuffer) =
        boot_info.framebuffer.as_mut()
    {

        let width =
            framebuffer.info().width as usize;


        let text =
            core::str::from_utf8(
                &buffer[..size]
            )
            .unwrap_or("READ ERROR");


        draw_string(
            framebuffer.buffer_mut(),
            width,
            10,
            50,
            text,
            [0,0,0],
        );

    }




    loop {


        //滑鼠
        while let Some(event) =
            keyboard.process()
        {

            keyboard_state.update(event);

        }

        let oldx = mouse.x;
        let oldy = mouse.y;

        if keyboard_state.up {
            mouse.y -=5;
        }


        if keyboard_state.down {
            mouse.y += 5;
        }


        if keyboard_state.left {
            mouse.x -= 5;
        }


        if keyboard_state.right {
            mouse.x += 5;
        }

        let moved =
            oldx != mouse.x ||
            oldy != mouse.y;

        // 畫面

        if moved && let Some(framebuffer) = boot_info.framebuffer.as_mut()//避免原地閃
        {

            let width =
                framebuffer.info().width as usize;


            let height =
                framebuffer.info().height as usize;


            let buffer =
                framebuffer.buffer_mut();


            draw_rect(
                buffer,
                width,
                (oldx - 2) as usize,
                (oldy - 2) as usize,
                5,
                5,
                [255,255,255],
            );

            draw_cursor(
                buffer,
                width,
                mouse.x,
                mouse.y,
            );
            if fs_test(&mut fat32).is_err() {
                draw_string(
                    buffer,
                    width,
                    10,
                    10,
                    "FAT32 TEST FAILED",
                    [255,0,0],
                );
            }else {
                draw_string(
                    buffer,
                    width,
                    10,
                    10,
                    "FAT32 TEST PASSED",
                    [0,255,0],
                );
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
            draw_string(framebuffer.buffer_mut(), width, 10, 10, info.message().as_str().unwrap_or("UNKNOWN ERROR"), [255, 0, 0]);

        }

    }


    loop {
        x86_64::instructions::hlt();
    }
}
