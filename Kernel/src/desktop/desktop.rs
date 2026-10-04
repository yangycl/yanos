use bootloader_api::info::FrameBuffer;

use crate::drivers::framebuffer as fb;
use crate::drivers::keyboard::KeyEvent;

/// 桌面就是一個目錄。現在是 `/home/desktop`，之後只換使用者名稱。
pub const DESKTOP_PATH: &str = "/home/desktop";

pub struct Desktop {
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

    pub fn update(&mut self, _event: KeyEvent) {}

    /// Explorer 圖示的範圍。主迴圈有滑鼠座標時再呼叫。
    pub fn hit_explorer(&self, x: i32, y: i32) -> bool {
        x > self.explorer_x as i32
            && x < self.explorer_x as i32 + 100
            && y > self.explorer_y as i32
            && y < self.explorer_y as i32 + 40
    }

    pub fn draw(&self, framebuffer: &mut FrameBuffer) {
        let width = framebuffer.info().width as usize;
        let height = framebuffer.info().height as usize;
        self.draw_to_buffer(framebuffer.buffer_mut(), width, height);
    }

    pub fn draw_to_buffer(&self, buffer: &mut [u8], width: usize, height: usize) {

        fb::draw_rect(buffer, width, 0, 0, width, height, [240, 240, 245]);

        let start_x = width / 2 - 100;
        let start_y = height / 2 - 20;
        fb::draw_string(buffer, width, start_x, start_y, "yanos", [200, 80, 0]);

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
