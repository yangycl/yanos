use bootloader_api::info::FrameBuffer;

use crate::drivers::framebuffer as fb;
use crate::drivers::keyboard::KeyEvent;

use crate::fs::block::BlockDevice;
use crate::fs::directory::DirectoryEntry;
use crate::fs::fat32::Fat32;

pub struct Explorer {
    selected: usize,
    cluster: u32,
    volume: &'static str,
}


impl Explorer {

    pub const fn new(cluster: u32) -> Self {
        Self::with_volume(cluster, "RAM")
    }

    pub const fn with_volume(cluster: u32, volume: &'static str) -> Self {
        Self {
            selected: 0,
            cluster,
            volume,
        }
    }


    pub fn selected_entry<D: BlockDevice>(
        &mut self,
        fat32: &mut Fat32<D>,
    ) -> Option<DirectoryEntry> {
        let mut entries = [DirectoryEntry {
            name: [0u8; 11],
            attr: 0,
            first_cluster: None,
            file_size: 0,
        }; 32];
        let count = fat32.read_directory(self.cluster, &mut entries);
        let mut seen = 0usize;
        for entry in entries.iter().take(count) {
            if entry.name[0] == 0x00 || entry.name[0] == 0xE5 || entry.attr == 0x0F {
                continue;
            }
            if seen == self.selected {
                return Some(*entry);
            }
            seen += 1;
        }
        None
    }

    pub fn update(
        &mut self,
        event: KeyEvent,
    ) {

        match event {

            KeyEvent::UpPress => {

                if self.selected > 0 {
                    self.selected -= 1;
                }

            }


            KeyEvent::DownPress => {

                if self.selected < 5 {
                    self.selected += 1;
                }

            }


            _ => {}

        }

    }



    pub fn draw<D: BlockDevice>(
        &self,
        framebuffer: &mut FrameBuffer,
        fat32: &mut Fat32<D>,
    ) {
        let width = framebuffer.info().width as usize;
        let height = framebuffer.info().height as usize;
        self.draw_window(framebuffer, fat32, 0, 0, width, height);
    }

    /// 只畫一塊視窗，不蓋掉桌面。
    pub fn draw_window<D: BlockDevice>(
        &self,
        framebuffer: &mut FrameBuffer,
        fat32: &mut Fat32<D>,
        x0: usize,
        y0: usize,
        win_w: usize,
        win_h: usize,
    ) {
        let stride = framebuffer.info().width as usize;
        self.draw_window_to_buffer(
            framebuffer.buffer_mut(),
            fat32,
            stride,
            x0,
            y0,
            win_w,
            win_h,
        );
    }

    pub fn draw_window_to_buffer<D: BlockDevice>(
        &self,
        buffer: &mut [u8],
        fat32: &mut Fat32<D>,
        stride: usize,
        x0: usize,
        y0: usize,
        win_w: usize,
        win_h: usize,
    ) {

        fb::draw_rect(buffer, stride, x0, y0, win_w, win_h, [35, 35, 35]);
        fb::draw_rect(buffer, stride, x0, y0, win_w, 32, [45, 90, 255]);
        fb::draw_string(buffer, stride, x0 + 10, y0 + 8, "YASYS Explorer", [255, 255, 255]);
        fb::draw_rect(buffer, stride, x0, y0 + 32, win_w, 24, [55, 55, 55]);
        let path = match self.volume {
            "USB" => "Path: USB:/",
            "TUR" => "Path: USB TUR",
            "CAP" => "Path: USB CAP",
            "BSZ" => "Path: USB BSZ",
            "READ" => "Path: USB READ",
            "SIG" => "Path: USB SIG",
            _ => "Path: RAM:/",
        };
        fb::draw_string(buffer, stride, x0 + 10, y0 + 38, path, [255, 255, 255]);
        if self.volume == "BSZ" {
            let n = crate::fs::usbdisk::last_block_size();
            let size = alloc::format!("{n}");
            fb::draw_string(buffer, stride, x0 + 150, y0 + 38, &size, [255, 220, 80]);
        }
        fb::draw_rect(buffer, stride, x0, y0 + 56, win_w, 24, [70, 70, 70]);
        fb::draw_string(buffer, stride, x0 + 10, y0 + 62, "Name", [255, 255, 255]);

        // ==========================
        // Read directory
        // ==========================

        let mut entries = [DirectoryEntry {
            name: [0u8; 11],
            attr: 0,
            first_cluster: None,
            file_size: 0,
        }; 32];

        let count = fat32.read_directory(
            self.cluster,
            &mut entries,
        );

        // ==========================
        // Convert FAT 8.3 names
        // ==========================

        let mut names =
            heapless::Vec::<heapless::String<13>, 32>::new();

        for entry in entries.iter().take(count) {
            if entry.name[0] == 0x00 {
                continue;
            }

            if entry.name[0] == 0xE5 {
                continue;
            }

            if entry.attr == 0x0F {
                continue;
            }

            let mut s =
                heapless::String::<13>::new();

            // ==========================
            // 檔名
            // ==========================

            for &b in entry.name[0..8].iter() {
                if b != b' ' {
                    let _ = s.push(b as char);
                }
            }

            // ==========================
            // 副檔名
            // ==========================

            let ext = &entry.name[8..11];

            if ext.iter().any(|&c| c != b' ') {
                let _ = s.push('.');

                for &b in ext.iter() {
                    if b != b' ' {
                        let _ = s.push(b as char);
                    }
                }
            }

            let _ = names.push(s);
        }

        // ==========================
        // Draw file list
        // ==========================

        let mut y = y0 + 90;

        for (i, file) in names.iter().enumerate() {
            if y + 20 >= y0 + win_h.saturating_sub(24) {
                break;
            }

            if i == self.selected {
                fb::draw_rect(buffer, stride, x0, y - 2, win_w, 18, [70, 120, 255]);
            }

            fb::draw_string(buffer, stride, x0 + 10, y, file.as_str(), [255, 255, 255]);

            y += 20;
        }

        if names.is_empty() {
            fb::draw_string(buffer, stride, x0 + 10, y0 + 90, "(empty)", [180, 180, 180]);
        }

        fb::draw_rect(
            buffer,
            stride,
            x0,
            y0 + win_h.saturating_sub(24),
            win_w,
            24,
            [55, 55, 55],
        );
        fb::draw_string(
            buffer,
            stride,
            x0 + 10,
            y0 + win_h.saturating_sub(18),
            "Explorer",
            [255, 255, 255],
        );
    }
}