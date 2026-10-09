use lazy_static::lazy_static;
use spin::Mutex;
use x86_64::structures::idt::{
    InterruptDescriptorTable,
    InterruptStackFrame,
    PageFaultErrorCode,
};

use crate::drivers::framebuffer::{draw_rect, draw_string};
use crate::fs::block::BlockDevice;
use crate::fs::directory::DirectoryEntry;
use crate::fs::fat32::Fat32;
use x86_64::VirtAddr;


lazy_static! {

    static ref IDT: InterruptDescriptorTable = {

    let mut idt =
            InterruptDescriptorTable::new();


        

        idt[32]
            .set_handler_fn(timer_handler);


        idt[33]
            .set_handler_fn(keyboard_handler);

        idt.double_fault.set_handler_fn(double_fault_handle);
        idt.page_fault.set_handler_fn(page_falut_handle);

        unsafe {
            idt[0x80].set_handler_addr(
                VirtAddr::new(draw_syscall_stub as *const () as u64)
            );
        }

        idt
    };
}

use core::ffi::CStr;

use core::arch::global_asm;

#[repr(C)]
pub struct SyscallRegs {
    pub r15: u64,
    pub r14: u64,
    pub r13: u64,
    pub r12: u64,
    pub r11: u64,
    pub r10: u64,
    pub r9: u64,
    pub r8: u64,
    pub rbp: u64,
    pub rdi: u64,
    pub rsi: u64,
    pub rdx: u64,
    pub rcx: u64,
    pub rbx: u64,
    pub rax: u64,
}

const MAX_SYSCALL_PATH: usize = 4096;
const MAX_DIRECTORY_ENTRIES: usize = 128;

#[derive(Clone, Copy)]
struct SyscallFilesystem {
    context: *mut (),
    root_cluster: u32,
    read_file: unsafe fn(*mut (), u32, &str, *mut u8, usize) -> usize,
    list_directory: unsafe fn(*mut (), u32, &str, *mut u8, usize) -> usize,
}

static mut SYSCALL_FILESYSTEM: Option<SyscallFilesystem> = None;

/// Register the mounted volume so int 0x80 handlers can use the existing FAT32 APIs.
pub fn register_syscall_filesystem<D: BlockDevice>(filesystem: &mut Fat32<D>) {
    unsafe {
        SYSCALL_FILESYSTEM = Some(SyscallFilesystem {
            context: filesystem as *mut Fat32<D> as *mut (),
            root_cluster: filesystem.root_cluster,
            read_file: syscall_read_file::<D>,
            list_directory: syscall_list_directory::<D>,
        });
    }
}

unsafe fn syscall_read_file<D: BlockDevice>(
    context: *mut (),
    root_cluster: u32,
    path: &str,
    destination: *mut u8,
    capacity: usize,
) -> usize {
    if destination.is_null() || capacity == 0 {
        return 0;
    }
    let filesystem = unsafe { &mut *context.cast::<Fat32<D>>() };
    let Some(mut file) = crate::fs::file::File::open(filesystem, root_cluster, path) else {
        return 0;
    };

    let mut copied = 0;
    let mut buffer = [0u8; 512];
    while copied < capacity {
        let count = file.read(&mut buffer).min(capacity - copied);
        if count == 0 {
            break;
        }
        unsafe {
            core::ptr::copy_nonoverlapping(buffer.as_ptr(), destination.add(copied), count);
        }
        copied += count;
    }
    copied
}

unsafe fn syscall_list_directory<D: BlockDevice>(
    context: *mut (),
    root_cluster: u32,
    path: &str,
    destination: *mut u8,
    capacity: usize,
) -> usize {
    if destination.is_null() || capacity < 32 {
        return 0;
    }
    let filesystem = unsafe { &mut *context.cast::<Fat32<D>>() };
    let directory_cluster = if path.is_empty() || path == "/" {
        root_cluster
    } else {
        let Some(entry) = filesystem.resolve_path(root_cluster, path) else {
            return 0;
        };
        if entry.attr & 0x10 == 0 {
            return 0;
        }
        let Some(cluster) = entry.first_cluster else {
            return 0;
        };
        cluster
    };

    let max_entries = (capacity / 32).min(MAX_DIRECTORY_ENTRIES);
    let mut entries = [DirectoryEntry::default(); MAX_DIRECTORY_ENTRIES];
    let count = filesystem.read_directory(directory_cluster, &mut entries[..max_entries]);
    for (index, entry) in entries[..count].iter().enumerate() {
        let raw = entry.to_bytes();
        unsafe {
            core::ptr::copy_nonoverlapping(raw.as_ptr(), destination.add(index * 32), 32);
        }
    }
    count
}

unsafe extern "C" {
    fn draw_syscall_stub();
}

global_asm!(
    r#"
    .global draw_syscall_stub
draw_syscall_stub:
    push rax
    push rbx
    push rcx
    push rdx
    push rsi
    push rdi
    push rbp
    push r8
    push r9
    push r10
    push r11
    push r12
    push r13
    push r14
    push r15

    mov rdi, rsp
    mov rbx, rsp
    and rsp, -16
    sub rsp, 16
    mov [rsp], rbx
    call syscall_dispatch
    mov rsp, [rsp]

    pop r15
    pop r14
    pop r13
    pop r12
    pop r11
    pop r10
    pop r9
    pop r8
    pop rbp
    pop rdi
    pop rsi
    pop rdx
    pop rcx
    pop rbx
    pop rax
    iretq
"#
);

       
#[unsafe(no_mangle)]
pub extern "C" fn syscall_dispatch(regs: *mut SyscallRegs) {
    let regs = unsafe { &mut *regs };

    match regs.rax {
        1 => {
            let Some(framebuffer_ptr) = (unsafe { crate::FRAMEBUFFER }) else {
                return;
            };
            let fb = unsafe { &mut *framebuffer_ptr };
            let width = fb.info().width as usize;
            let buffer = fb.buffer_mut();

            draw_rect(
                buffer,
                width,
                regs.rbx as usize,
                regs.rcx as usize,
                regs.r8 as usize,
                regs.r9 as usize,
                [regs.r10 as u8, regs.r11 as u8, regs.r12 as u8],
            );
        }
        2 => {
            let Some(framebuffer_ptr) = (unsafe { crate::FRAMEBUFFER }) else {
                return;
            };
            let fb = unsafe { &mut *framebuffer_ptr };
            let width = fb.info().width as usize;
            let buffer = fb.buffer_mut();

            let cstr = unsafe { CStr::from_ptr(regs.rbx as *const i8) };
            let string = cstr.to_str().unwrap_or("[invalid utf-8]");

            draw_string(
                buffer,
                width,
                regs.rcx as usize,
                regs.rdx as usize,
                string,
                [regs.r8 as u8, regs.r9 as u8, regs.r10 as u8],
            );
        }

        // keyboard
        3 => {
            regs.rax = get_key().unwrap_or(0) as u64;
        }
        // rbx=path, rcx=path length, rdx=destination, rsi=destination capacity.
        4 => {
            regs.rax = dispatch_filesystem_syscall(regs, false) as u64;
        }
        // Writes 32-byte FAT directory entries to the supplied destination.
        // Returns the number of entries written.
        5 => {
            regs.rax = dispatch_filesystem_syscall(regs, true) as u64;
        }
        _ => {}
    }
}

fn dispatch_filesystem_syscall(regs: &SyscallRegs, list_directory: bool) -> usize {
    let path_len = regs.rcx as usize;
    let path_ptr = regs.rbx as *const u8;
    let destination = regs.rdx as *mut u8;
    let capacity = regs.rsi as usize;
    if path_len > MAX_SYSCALL_PATH
        || (path_len != 0 && path_ptr.is_null())
        || destination.is_null()
    {
        return 0;
    }
    let path_bytes = if path_len == 0 {
        &[][..]
    } else {
        unsafe { core::slice::from_raw_parts(path_ptr, path_len) }
    };
    let Ok(path) = core::str::from_utf8(path_bytes) else {
        return 0;
    };
    let filesystem = unsafe { SYSCALL_FILESYSTEM };
    let Some(filesystem) = filesystem else {
        return 0;
    };
    unsafe {
        let operation = if list_directory {
            filesystem.list_directory
        } else {
            filesystem.read_file
        };
        operation(
            filesystem.context,
            filesystem.root_cluster,
            path,
            destination,
            capacity,
        )
    }
}

pub fn init_idt() {

    IDT.load();

}


const KEYBOARD_QUEUE_SIZE: usize = 128;


struct KeyboardQueue {

    buffer: [u8; KEYBOARD_QUEUE_SIZE],

    read: usize,

    write: usize,

}


impl KeyboardQueue {

    const fn new() -> Self {

        Self {
            buffer: [0; KEYBOARD_QUEUE_SIZE],
            read: 0,
            write: 0,
        }

    }


    fn push(&mut self, value: u8) {

        let next =
            (self.write + 1)
            % KEYBOARD_QUEUE_SIZE;


        if next != self.read {

            self.buffer[self.write] = value;
            self.write = next;

        }

    }


    fn pop(&mut self) -> Option<u8> {

        if self.read == self.write {
            return None;
        }


        let value =
            self.buffer[self.read];


        self.read =
            (self.read + 1)
            % KEYBOARD_QUEUE_SIZE;


        Some(value)

    }
}

static mut LAST_KEY: u8 = 0;

    static KEYBOARD_QUEUE: Mutex<KeyboardQueue> =
        Mutex::new(
            KeyboardQueue::new()
        );


extern "x86-interrupt"
fn keyboard_handler(
    _stack_frame: InterruptStackFrame
) {

    let mut port =
        x86_64::instructions::port::Port::new(0x60);


    let scan_code: u8 =
        unsafe {
            port.read()
        };



    KEYBOARD_QUEUE
        .lock()
        .push(scan_code);



    unsafe {

        crate::arch::x86_64::pic::PICS
            .lock()
            .notify_end_of_interrupt(33);

    }

}

use core::sync::atomic::{AtomicU64, Ordering};

pub static TIMER_TICKS: AtomicU64 = AtomicU64::new(0);

extern "x86-interrupt" fn timer_handler(
    _stack_frame: InterruptStackFrame
) {
    TIMER_TICKS.fetch_add(1, Ordering::Relaxed);
    unsafe {
        crate::arch::x86_64::pic::PICS
            .lock()
            .notify_end_of_interrupt(32);
    }
}

pub fn get_key() -> Option<u8>
{
    KEYBOARD_QUEUE
        .lock()
        .pop()
}

pub fn peek_key() -> Option<u8>
{
    let queue = KEYBOARD_QUEUE.lock();

    if queue.read == queue.write {
        return None;
    }

    Some(queue.buffer[queue.read])
}


const USB_REPORT_QUEUE_SIZE: usize = 16;

struct UsbReportQueue {
    buffer: [[u8; 8]; USB_REPORT_QUEUE_SIZE],
    read: usize,
    write: usize,
}

impl UsbReportQueue {
    const fn new() -> Self {
        Self {
            buffer: [[0; 8]; USB_REPORT_QUEUE_SIZE],
            read: 0,
            write: 0,
        }
    }

    fn push(&mut self, report: [u8; 8]) {
        let next = (self.write + 1) % USB_REPORT_QUEUE_SIZE;
        if next != self.read {
            self.buffer[self.write] = report;
            self.write = next;
        }
    }

    fn pop(&mut self) -> Option<[u8; 8]> {
        if self.read == self.write {
            return None;
        }
        let v = self.buffer[self.read];
        self.read = (self.read + 1) % USB_REPORT_QUEUE_SIZE;
        Some(v)
    }
}

static USB_REPORT_QUEUE: Mutex<UsbReportQueue> =
    Mutex::new(UsbReportQueue::new());

/// USB 驅動把 Boot Protocol 8-byte report 推進來
pub fn push_usb_report(report: [u8; 8]) {
    USB_REPORT_QUEUE.lock().push(report);
}

/// keyboard.rs 使用
pub fn peek_usb_report() -> Option<[u8; 8]> {
    let queue = USB_REPORT_QUEUE.lock();
    if queue.read == queue.write {
        return None;
    }
    Some(queue.buffer[queue.read])
}

pub fn get_usb_report() -> Option<[u8; 8]> {
    USB_REPORT_QUEUE.lock().pop()
}

extern "x86-interrupt" fn page_falut_handle(
    _stack_frame: InterruptStackFrame,
    _error_code: PageFaultErrorCode,
) {
    unsafe {
        if let Some(framebuffer_ptr) = crate::FRAMEBUFFER {
            let framebuffer = &mut *framebuffer_ptr;
            crate::drivers::framebuffer::draw_orange_screen(framebuffer);
            let width = framebuffer.info().width as usize;

            let fault_addr = x86_64::registers::control::Cr2::read()
                .unwrap_or(x86_64::VirtAddr::new(0));
            let msg = alloc::format!("PF at 0x{:x}", fault_addr.as_u64());
            crate::drivers::framebuffer::draw_string(
                framebuffer.buffer_mut(),
                width,
                10,
                10,
                &msg,
                [255, 0, 0],
            );

            let rip = _stack_frame.instruction_pointer.as_u64();
            let rip_msg = alloc::format!("RIP 0x{:x}", rip);
            crate::drivers::framebuffer::draw_string(
                framebuffer.buffer_mut(),
                width,
                10,
                30,
                &rip_msg,
                [255, 0, 0],
            );
        }
    }

    loop {
        x86_64::instructions::hlt();
    }
}

extern "x86-interrupt" fn double_fault_handle(
    _stack_frame: InterruptStackFrame,
    _error_code: u64,
) -> ! {
    unsafe {
        if let Some(framebuffer_ptr) = crate::FRAMEBUFFER {
            let framebuffer = &mut *framebuffer_ptr;
            crate::drivers::framebuffer::draw_orange_screen(framebuffer);
            let width = framebuffer.info().width as usize;

            let rip = _stack_frame.instruction_pointer.as_u64();
            let msg = alloc::format!("DF RIP 0x{:x}", rip);
            crate::drivers::framebuffer::draw_string(
                framebuffer.buffer_mut(),
                width,
                10,
                10,
                &msg,
                [255, 0, 0],
            );

            let err = alloc::format!("err=0x{:x}", _error_code);
            crate::drivers::framebuffer::draw_string(
                framebuffer.buffer_mut(),
                width,
                10,
                30,
                &err,
                [255, 0, 0],
            );
        }
    }

    loop {
        x86_64::instructions::hlt();
    }
}
