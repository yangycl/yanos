use crate::drivers::framebuffer::wordbitmap::FONT_LOWER;

mod wordbitmap;

/// Copy only pixels whose rendered BGRA bytes differ from the visible framebuffer.
pub fn present_changed_pixels(
    framebuffer: &mut [u8],
    rendered: &[u8],
    width: usize,
    height: usize,
) {
    for pixel in 0..width.saturating_mul(height) {
        let start = pixel.saturating_mul(4);
        let end = start.saturating_add(4);
        if end > framebuffer.len() || end > rendered.len() {
            break;
        }
        if framebuffer[start..end] != rendered[start..end] {
            framebuffer[start..end].copy_from_slice(&rendered[start..end]);
        }
    }
}


    fn draw_pixel(
        buffer: &mut [u8],
        width: usize,
        x: usize,
        y: usize,
        color: [u8; 3],
    ) {
        let index = (y * width + x) * 4;

        if index + 3 < buffer.len() {
            buffer[index] = color[2];         // B
            buffer[index + 1] = color[1];     // G
            buffer[index + 2] = color[0];     // R
            buffer[index + 3] = 0;             // padding
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
        for yy in y..y + h {
            for xx in x..x + w {
                draw_pixel(
                    buffer,
                    width,
                    xx,
                    yy,
                    color,
                );
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


            '!' => {
                for row in 0..6 {
                    draw_pixel(buffer, width, x + 3, y + row, color);
                    draw_pixel(buffer, width, x + 4, y + row, color);
                }
                draw_pixel(buffer, width, x + 3, y + 7, color);
                draw_pixel(buffer, width, x + 4, y + 7, color);
            }


            '_' => {
                for col in 0..8 {
                    draw_pixel(buffer, width, x + col, y + 7, color);
                }
            }


        _ => return,

    }
}

pub fn draw_string(
    buffer: &mut [u8],
    width: usize,
    x: usize,
    y: usize,
    s: &str,
    color: [u8; 3],
) {

    let mut current_x = x; // 補上這一行
    let mut current_y = y; // 補上這一行

    for c in s.chars() {

        // 換行
        if c == '\n' {

            current_x = x;
            current_y += 8;

            continue;
        }

        if c == ' ' {
            current_x += 8;
            continue;
        }

        draw_char(
            buffer,
            width,
            current_x,
            current_y,
            c,
            color,
        );


        current_x += 8;
    }
}

pub fn draw_orange_screen(
    framebuffer: &mut bootloader_api::info::FrameBuffer,
) {
    let info = framebuffer.info();
    let buffer = framebuffer.buffer_mut();

    for y in 0..info.height {
        for x in 0..info.width {
            let index = (y * info.stride + x) * info.bytes_per_pixel;
            if index + info.bytes_per_pixel > buffer.len() {
                continue;
            }

            let pixel_format = info.pixel_format;
            match pixel_format {
                bootloader_api::info::PixelFormat::Rgb => {
                    buffer[index] = 255;
                    buffer[index + 1] = 140;
                    buffer[index + 2] = 0;
                }
                bootloader_api::info::PixelFormat::Bgr => {
                    buffer[index] = 0;
                    buffer[index + 1] = 140;
                    buffer[index + 2] = 255;
                }
                bootloader_api::info::PixelFormat::U8 => {
                    buffer[index] = 160;
                }
                bootloader_api::info::PixelFormat::Unknown {
                    red_position,
                    green_position,
                    blue_position,
                } => {
                    let pixel = 255u32.checked_shl(red_position as u32).unwrap_or(0)
                        | 140u32.checked_shl(green_position as u32).unwrap_or(0)
                        | 0u32.checked_shl(blue_position as u32).unwrap_or(0);
                    for byte in 0..info.bytes_per_pixel.min(4) {
                        buffer[index + byte] = (pixel >> (byte * 8)) as u8;
                    }
                }
                _ => {}
            }
        }
    }
}
