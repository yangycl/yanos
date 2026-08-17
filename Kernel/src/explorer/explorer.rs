use bootloader_api::info::FrameBuffer;

use crate::drivers::framebuffer as fb;
use crate::drivers::keyboard::KeyEvent;

use crate::fs::block::BlockDevice;
use crate::fs::directory::DirectoryEntry;
use crate::fs::fat32::Fat32;

pub struct Explorer {
    selected: usize,
    cluster: u32,
}


impl Explorer {

    pub const fn new(cluster: u32) -> Self {
        Self {
            selected: 0,
            cluster,
        }
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

        let buffer = framebuffer.buffer_mut();

        // ==========================
        // Background
        // ==========================

        fb::draw_rect(
            buffer,
            width,
            0,
            0,
            width,
            height,
            [35, 35, 35],
        );

        // ==========================
        // Title
        // ==========================

        fb::draw_rect(
            buffer,
            width,
            0,
            0,
            width,
            32,
            [45, 90, 255],
        );

        fb::draw_string(
            buffer,
            width,
            10,
            8,
            "YASYS Explorer",
            [255, 255, 255],
        );

        // ==========================
        // Path
        // ==========================

        fb::draw_rect(
            buffer,
            width,
            0,
            32,
            width,
            24,
            [55, 55, 55],
        );

        fb::draw_string(
            buffer,
            width,
            10,
            38,
            "Path: /",
            [255, 255, 255],
        );

        // ==========================
        // Header
        // ==========================

        fb::draw_rect(
            buffer,
            width,
            0,
            56,
            width,
            24,
            [70, 70, 70],
        );

        fb::draw_string(
            buffer,
            width,
            10,
            62,
            "Name",
            [255, 255, 255],
        );

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

        let mut y = 90usize;

        for (i, file) in names.iter().enumerate() {
            if y + 20 >= height.saturating_sub(24) {
                break;
            }

            if i == self.selected {
                fb::draw_rect(
                    buffer,
                    width,
                    0,
                    y - 2,
                    width,
                    18,
                    [70, 120, 255],
                );
            }

            fb::draw_string(
                buffer,
                width,
                10,
                y,
                file.as_str(),
                [255, 255, 255],
            );

            y += 20;
        }

        // ==========================
        // Empty directory
        // ==========================

        if names.is_empty() {
            fb::draw_string(
                buffer,
                width,
                10,
                90,
                "(empty)",
                [180, 180, 180],
            );
        }

        // ==========================
        // Status bar
        // ==========================

        fb::draw_rect(
            buffer,
            width,
            0,
            height.saturating_sub(24),
            width,
            24,
            [55, 55, 55],
        );

        fb::draw_string(
            buffer,
            width,
            10,
            height.saturating_sub(18),
            "Explorer",
            [255, 255, 255],
        );
    }
}