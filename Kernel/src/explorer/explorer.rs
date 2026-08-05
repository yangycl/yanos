use bootloader_api::info::FrameBuffer;

use crate::drivers::framebuffer as fb;
use crate::drivers::keyboard::KeyEvent;


pub struct Explorer {

    selected: usize,

}


impl Explorer {

    pub const fn new() -> Self {

        Self {
            selected: 0,
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



    pub fn draw(
        &self,
        framebuffer: &mut FrameBuffer,
    ) {

        let width =
            framebuffer.info().width as usize;

        let height =
            framebuffer.info().height as usize;


        let buffer =
            framebuffer.buffer_mut();



        // Background

        fb::draw_rect(
            buffer,
            width,
            0,
            0,
            width,
            height,
            [35,35,35],
        );



        // Title

        fb::draw_rect(
            buffer,
            width,
            0,
            0,
            width,
            32,
            [45,90,255],
        );


        fb::draw_string(
            buffer,
            width,
            10,
            8,
            "YASYS Explorer",
            [255,255,255],
        );



        // Path

        fb::draw_rect(
            buffer,
            width,
            0,
            32,
            width,
            24,
            [55,55,55],
        );


        fb::draw_string(
            buffer,
            width,
            10,
            38,
            "Path: /",
            [255,255,255],
        );



        // Header

        fb::draw_rect(
            buffer,
            width,
            0,
            56,
            width,
            24,
            [70,70,70],
        );


        fb::draw_string(
            buffer,
            width,
            10,
            62,
            "Name",
            [255,255,255],
        );



        let files = [

            "[DIR] System",
            "[DIR] Users",
            "[DIR] Boot",
            "kernel.bin",
            "readme.txt",
            "config.sys",

        ];



        let mut y = 90;



        for (i,file) in files.iter().enumerate() {


            if i == self.selected {

                fb::draw_rect(
                    buffer,
                    width,
                    0,
                    y - 2,
                    width,
                    18,
                    [70,120,255],
                );

            }



            fb::draw_string(
                buffer,
                width,
                10,
                y,
                file,
                [255,255,255],
            );


            y += 20;

        }



        // Status

        fb::draw_rect(
            buffer,
            width,
            0,
            height - 24,
            width,
            24,
            [55,55,55],
        );


        fb::draw_string(
            buffer,
            width,
            10,
            height - 18,
            "6 Items",
            [255,255,255],
        );

    }

}