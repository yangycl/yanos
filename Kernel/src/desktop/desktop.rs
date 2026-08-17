use crate::explorer::explorer;
use crate::explorer::explorer::Explorer;

use bootloader_api::info::FrameBuffer;
use crate::drivers::framebuffer as fb;
use crate::drivers::keyboard::KeyEvent;
use crate::drives::keyboard::read_char;

/// Simple desktop environment
pub struct Desktop {
    /// Position of the Explorer text
    explorer_x: usize,
    explorer_y: usize,
}

impl Desktop {
    pub const fn new() -> Self {
        Self {
            explorer_x: 40,
            explorer_y: 40,
        }
    }

    /// Handle keyboard events (currently does nothing)
    pub fn update(&mut self, _event: KeyEvent) {
        // TODO: implement later
    }

    /// Empty click event (can be connected to mouse or keyboard confirm later)
    pub fn on_click(&mut self, _x: i32, _y: i32) {
        if (x > 40 || x < 140) && (y > 40 || y < 140){
            let mut explorer = Explorer::new();
            
            while(read_char() != "q"){
                explorer = explorer::draw("/home/desktop/")
            }
        }  
    }

    /// Draw the entire desktop
    pub fn draw(&self, framebuffer: &mut FrameBuffer) {
        let width = framebuffer.info().width as usize;
        let height = framebuffer.info().height as usize;
        let buffer = framebuffer.buffer_mut();

        // Desktop background (light gray-white)
        fb::draw_rect(
            buffer,
            width,
            0,
            0,
            width,
            height,
            [240, 240, 245],
        );

        // Display Explorer as text
        fb::draw_string(
            buffer,
            width,
            self.explorer_x,
            self.explorer_y,
            "Explorer",
            [30, 30, 40],
        );
    }
}