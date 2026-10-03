use pic8259::ChainedPics;
use spin::Mutex;


pub const PIC1_OFFSET: u8 = 32;
pub const PIC2_OFFSET: u8 = 40;


pub static PICS: Mutex<ChainedPics> =
    Mutex::new(
        unsafe {
            ChainedPics::new(
                PIC1_OFFSET,
                PIC2_OFFSET
            )
        }
    );


pub fn init() {
    unsafe {
        PICS.lock().initialize();
    }
}
/// IRQ0 timer、IRQ1 keyboard。initialize() 之後全部是罩住的。
pub fn unmask_keyboard_and_timer() {
    unsafe {
        PICS.lock().write_masks(0xFC, 0xFF);
    }
}
