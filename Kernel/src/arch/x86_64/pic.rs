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