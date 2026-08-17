use lazy_static::lazy_static;
use spin::Mutex;

use x86_64::structures::idt::{
    InterruptDescriptorTable,
    InterruptStackFrame,
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
