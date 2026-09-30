use lazy_static::lazy_static;
use spin::Mutex;

use x86_64::structures::idt::{
    InterruptDescriptorTable,
    InterruptStackFrame,
    PageFaultErrorCode,
};

use crate::pic::PICS;


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


        idt
    };
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

        crate::pic::PICS
            .lock()
            .notify_end_of_interrupt(33);

    }

}

extern "x86-interrupt" fn timer_handler(
    _stack_frame: InterruptStackFrame
) {

    unsafe {
        crate::pic::PICS
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
