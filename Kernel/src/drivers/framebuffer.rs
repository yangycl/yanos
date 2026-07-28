use crate::drivers::framebuffer::wordbitmap::FONT_LOWER;

mod wordbitmap;


fn draw_pixel(
    buffer: &mut [u8],
    width: usize,
    x: usize,
    y: usize,
    color: [u8; 3],
) {
    let index = (y * width + x) * 3;

    if index + 2 < buffer.len() {
        buffer[index] = color[0];     // B
        buffer[index + 1] = color[1]; // G
        buffer[index + 2] = color[2]; // R
    }
}


pub fn draw_cursor(
    buffer: &mut [u8],
    width: usize,
    x: i32,
    y: i32,
) {

    let color = [30, 30, 30]; // Dark gray

    let shape = [
        (0,0),
        (0,1),
        (0,2),
        (1,1),
        (2,1),
        (-1,1),
        (-2,1),
        (0,-1),
        (0,-2),
    ];


    for (dx, dy) in shape {

        let px = x + dx;
        let py = y + dy;


        if px >= 0 && py >= 0 {

            draw_pixel(
                buffer,
                width,
                px as usize,
                py as usize,
                color
            );

        }
    }
}

pub fn draw_rect(
    buffer: &mut [u8],
    width: usize,
    x: usize,
    y: usize,
    w: usize,
    h: usize,
    color: [u8; 3],
) {
    for yy in y..y+h {
        for xx in x..x+w {

            let index = (yy * width + xx) * 3;

            buffer[index] = color[0];
            buffer[index + 1] = color[1];
            buffer[index + 2] = color[2];
        }
    }
}

pub fn draw_char(
    buffer: &mut [u8],
    width: usize,
    x: usize,
    y: usize,
    c: char,
    color: [u8; 3],
)   {
    match c {

        'A'..='Z' => {

            let index =
                (c as u8 - b'A') as usize;

            let bitmap = wordbitmap::FONT_UPPER[index];
            for (row, bits) in bitmap.iter().enumerate() {

                for col in 0..8 {

                    if (bits >> (7 - col)) & 1 == 1 {

                        draw_pixel(
                            buffer,
                            width,
                            x + col,
                            y + row,
                            color,
                        );

                    }

                }

            }

        }


        'a'..='z' => {

            let index =
                (c as u8 - b'a') as usize;

            let bitmap = wordbitmap::FONT_LOWER[index];
            for (row, bits) in bitmap.iter().enumerate() {

                for col in 0..8 {

                    if (bits >> (7 - col)) & 1 == 1 {

                        draw_pixel(
                            buffer,
                            width,
                            x + col,
                            y + row,
                            color,
                        );

                    }

                }

            }
       }


        '0'..='9' => {

            let index =
                (c as u8 - b'0') as usize;

            let bitmap =   wordbitmap::FONT_NUMBER[index];
            for (row, bits) in bitmap.iter().enumerate() {

                for col in 0..8 {

                    if (bits >> (7 - col)) & 1 == 1 {

                        draw_pixel(
                            buffer,
                            width,
                            x + col,
                            y + row,
                            color,
                        );

                    }

                }

            }        }


        _ => return,

    }
}

pub fn draw_string(
    buffer: &mut [u8],
    width: usize,
    x: usize,
    y: usize,
    text: &str,
    color: [u8;3],
) {

    let mut cursor_x = x;
    let mut cursor_y = y;


    for c in text.chars() {

        // 換行
        if c == '\n' {

            cursor_x = x;
            cursor_y += 8;

            continue;
        }


        draw_char(
            buffer,
            width,
            cursor_x,
            cursor_y,
            c,
            color,
        );


        cursor_x += 8;
    }
}

pub fn draw_orange_screen(
    framebuffer: &mut bootloader_api::info::FrameBuffer,
) {
    let width = framebuffer.info().width as usize;
    let height = framebuffer.info().height as usize;
    let buffer = framebuffer.buffer_mut();

    for y in 0..height {
        for x in 0..width {
            let index = (y * width + x) * 3;

            if index + 2 < buffer.len() {
                // BGR
                buffer[index] = 0;
                buffer[index + 1] = 140;
                buffer[index + 2] = 255;
            }
        }
    }
}
