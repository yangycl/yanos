use crate::drivers::framebuffer::{draw_string, draw_rect};
use crate::fs::fat32::Fat32;
use crate::fs::block::BlockDevice;
use bootloader_api::info::FrameBuffer;
use core::str;

// Simple line reader using scan codes. Ignores key releases (codes >= 0x80).
fn read_line(prompt_x: usize, prompt_y: usize, fb: &mut FrameBuffer) -> heapless::String<128> {
    use heapless::String;
    let mut buf: String<128> = String::new();
    let width = fb.info().width as usize;

    loop {
        // use keyboard.read_char() to get printable characters with shift/caps handling
        if let Some(c) = crate::drivers::keyboard::read_char() {
            // handle backspace
            if c == '\u{8}' || c == '\u{7f}' {
                if buf.len() > 0 {
                    buf.pop();
                    // erase char on screen
                    draw_string(fb.buffer_mut(), width, prompt_x + buf.len() * 8, prompt_y, " ", [255,255,255]);
                }
                continue;
            }
            // Enter or newline
            if c == '\n' {
                break;
            }

            if buf.push(c).is_ok() {
                let x = prompt_x + (buf.len()-1) * 8;
                let mut tmp = [0u8; 4];
                let s = c.encode_utf8(&mut tmp);
                draw_string(fb.buffer_mut(), width, x, prompt_y, s, [0,0,0]);
            }
        } else {
            x86_64::instructions::hlt();
        }
    }

    buf
}

fn resolve_parent_and_name<D: BlockDevice>(fs: &mut Fat32<D>, cwd: u32, path: &str) -> Option<(u32, [u8;11])> {
    if path.contains('/') {
        let absolute = path.starts_with('/');
        let trimmed = path.trim_end_matches('/');
        if trimmed.is_empty() { return None; }
        let idx = trimmed.rfind('/');
        let (parent_path, name) = match idx {
            Some(i) => (&trimmed[..i], &trimmed[i+1..]),
            None => ("", trimmed),
        };
        let base = if absolute { fs.root_cluster } else { cwd };
        if parent_path.is_empty() {
            return Some((base, crate::fs::directory::DirectoryEntry::format_short_name(name)));
        }
        let entry = fs.resolve_path(base, parent_path)?;
        let parent_cluster = entry.first_cluster?;
        Some((parent_cluster, crate::fs::directory::DirectoryEntry::format_short_name(name)))
    } else {
        Some((cwd, crate::fs::directory::DirectoryEntry::format_short_name(path)))
    }
}

fn execute_command_no_ui<D: BlockDevice>(fs: &mut Fat32<D>, s: &str, cwd: &mut u32) -> bool {
    use crate::fs::directory::DirectoryEntry;
    use crate::fs::file::File;

    if s.is_empty() {
        return false;
    }

    let mut parts = s.split_whitespace();
    if let Some(cmd) = parts.next() {
        match cmd {
            "help" | "clear" | "ls" | "cat" => {}
            "write" => {
                if let Some(name) = parts.next() {
                    let mut rest = heapless::String::<128>::new();
                    let mut first = true;
                    for p in parts {
                        if !first { rest.push(' ').ok(); }
                        rest.push_str(p).ok();
                        first = false;
                    }
                    use crate::fs::directory::DirectoryEntry as DirEntry;
                    let entry = DirEntry { name: DirEntry::format_short_name(name), attr: 0x20, first_cluster: None, file_size: 0 };
                    fs.create_entry(*cwd, &entry);
                    if let Some(mut file) = File::open(fs, *cwd, name) {
                        file.write(rest.as_bytes());
                    }
                }
            }
            "append" => {
                if let Some(name) = parts.next() {
                    let mut rest = heapless::String::<128>::new();
                    let mut first = true;
                    for p in parts {
                        if !first { rest.push(' ').ok(); }
                        rest.push_str(p).ok();
                        first = false;
                    }
                    if let Some(mut file) = File::open(fs, *cwd, name) {
                        file.append(rest.as_bytes());
                    }
                }
            }
            "mkdir" => {
                if let Some(name) = parts.next() {
                    if let Some((parent_cluster, short)) = resolve_parent_and_name(fs, *cwd, name) {
                        let _ = fs.create_directory(parent_cluster, short);
                    }
                }
            }
            "cd" => {
                if let Some(path) = parts.next() {
                    let absolute = path.starts_with('/');
                    let base = if absolute { fs.root_cluster } else { *cwd };
                    if let Some(entry) = fs.resolve_path(base, path) {
                        if entry.attr & 0x10 != 0 {
                            if let Some(cluster) = entry.first_cluster {
                                *cwd = cluster;
                            }
                        }
                    }
                }
            }
            "exit" => return true,
            _ => {}
        }
    }

    false
}

fn execute_command<D: BlockDevice>(
    fs: &mut Fat32<D>,
    framebuffer: &mut FrameBuffer,
    s: &str,
    current_y: &mut usize,
    prompt_x: usize,
    width: usize,
    height: usize,
    cwd: &mut u32,
) -> bool {
    use crate::fs::directory::DirectoryEntry;
    use crate::fs::file::File;

    if s.is_empty() {
        return false;
    }

    fn resolve_parent_and_name<D: BlockDevice>(fs: &mut Fat32<D>, cwd: u32, path: &str) -> Option<(u32, [u8;11])> {
        // If path contains '/', split into parent path and name
        if path.contains('/') {
            let absolute = path.starts_with('/');
            // keep leading slash information but remove trailing slashes
            let trimmed = path.trim_end_matches('/');
            if trimmed.is_empty() { return None; }
            let idx = trimmed.rfind('/');
            let (parent_path, name) = match idx {
                Some(i) => (&trimmed[..i], &trimmed[i+1..]),
                None => ("", trimmed),
            };
            let base = if absolute { fs.root_cluster } else { cwd };
            if parent_path.is_empty() {
                return Some((base, crate::fs::directory::DirectoryEntry::format_short_name(name)));
            }
            let entry = fs.resolve_path(base, parent_path)?;
            let parent_cluster = entry.first_cluster?;
            return Some((parent_cluster, crate::fs::directory::DirectoryEntry::format_short_name(name)));
        } else {
            Some((cwd, crate::fs::directory::DirectoryEntry::format_short_name(path)))
        }
    }

    let mut parts = s.split_whitespace();
    if let Some(cmd) = parts.next() {
        match cmd {
            "help" => {
                draw_string(framebuffer.buffer_mut(), width, prompt_x, *current_y, "commands: ls cat write append mkdir cd clear exit help", [0,0,0]);
                *current_y += 16;
            }
            "clear" => {
                draw_rect(framebuffer.buffer_mut(), width, 0, 0, width, height, [255,255,255]);
                *current_y = 10;
            }
            "ls" => {
                let mut entries: [DirectoryEntry; 30] = [DirectoryEntry { name: [0u8;11], attr:0, first_cluster:None, file_size:0 }; 30];
                let count = fs.read_directory(*cwd, &mut entries);
                for i in 0..count {
                    let name = core::str::from_utf8(&entries[i].name).unwrap_or("BAD");
                    draw_string(framebuffer.buffer_mut(), width, prompt_x, *current_y, name, [0,0,0]);
                    *current_y += 16;
                    if *current_y + 16 >= height { break; }
                }
            }
            "cat" => {
                if let Some(name) = parts.next() {
                    // support path
                    let mut file_opt = None;
                    if name.contains('/') {
                        let absolute = name.starts_with('/');
                        let base = if absolute { fs.root_cluster } else { *cwd };
                        // File::open supports path components, so pass base and full name
                        file_opt = File::open(fs, base, name);
                    } else {
                        file_opt = File::open(fs, *cwd, name);
                    }
                    if let Some(mut file) = file_opt {
                        let mut buffer_read = [0u8;512];
                        loop {
                            let sz = file.read(&mut buffer_read);
                            if sz == 0 { break; }
                            let text = core::str::from_utf8(&buffer_read[..sz]).unwrap_or("READ_ERROR");
                            draw_string(framebuffer.buffer_mut(), width, prompt_x, *current_y, text, [0,0,0]);
                            *current_y += 16;
                            if *current_y + 16 >= height { break; }
                        }
                    } else {
                        draw_string(framebuffer.buffer_mut(), width, prompt_x, *current_y, "file not found", [255,0,0]);
                        *current_y += 16;
                    }
                }
            }
            "write" => {
                if let Some(name) = parts.next() {
                    let mut rest = heapless::String::<128>::new();
                    let mut first = true;
                    for p in parts {
                        if !first { rest.push(' ').ok(); }
                        rest.push_str(p).ok();
                        first = false;
                    }
                    use crate::fs::directory::DirectoryEntry as DirEntry;
                    let entry = DirEntry { name: DirEntry::format_short_name(name), attr: 0x20, first_cluster: None, file_size: 0 };
                    fs.create_entry(*cwd, &entry);
                    if let Some(mut file) = File::open(fs, *cwd, name) {
                        file.write(rest.as_bytes());
                        draw_string(framebuffer.buffer_mut(), width, prompt_x, *current_y, "written", [0,0,0]);
                        *current_y += 16;
                    }
                }
            }
            "append" => {
                if let Some(name) = parts.next() {
                    let mut rest = heapless::String::<128>::new();
                    let mut first = true;
                    for p in parts {
                        if !first { rest.push(' ').ok(); }
                        rest.push_str(p).ok();
                        first = false;
                    }
                    if let Some(mut file) = File::open(fs, *cwd, name) {
                        file.append(rest.as_bytes());
                        draw_string(framebuffer.buffer_mut(), width, prompt_x, *current_y, "appended", [0,0,0]);
                        *current_y += 16;
                    } else {
                        draw_string(framebuffer.buffer_mut(), width, prompt_x, *current_y, "file not found", [255,0,0]);
                        *current_y += 16;
                    }
                }
            }
            "mkdir" => {
                if let Some(name) = parts.next() {
                    if let Some((parent_cluster, short)) = resolve_parent_and_name(fs, *cwd, name) {
                        if let Some(cluster) = fs.create_directory(parent_cluster, short) {
                            let mut msg = heapless::String::<64>::new();
                            core::fmt::write(&mut msg, format_args!("created dir cluster {}", cluster)).ok();
                            draw_string(framebuffer.buffer_mut(), width, prompt_x, *current_y, &msg, [0,0,0]);
                            *current_y += 16;
                        } else {
                            draw_string(framebuffer.buffer_mut(), width, prompt_x, *current_y, "mkdir failed", [255,0,0]);
                            *current_y += 16;
                        }
                    } else {
                        draw_string(framebuffer.buffer_mut(), width, prompt_x, *current_y, "invalid path", [255,0,0]);
                        *current_y += 16;
                    }
                }
            }
            "exit" => {
                draw_rect(framebuffer.buffer_mut(), width, 0, 0, width, height, [255,255,255]);
                return true;
            }
            _ => {
                draw_string(framebuffer.buffer_mut(), width, prompt_x, *current_y, "unknown cmd", [255,0,0]);
                *current_y += 16;
            }
        }
    }

    false
}

pub fn run_command<D: BlockDevice>(_fs: &mut Fat32<D>, command: &str) -> bool {
    let mut cwd = _fs.root_cluster;
    execute_command_no_ui(_fs, command, &mut cwd)
}

pub fn run<D: BlockDevice>(fs: &mut Fat32<D>, framebuffer: &mut FrameBuffer) {
    let width = framebuffer.info().width as usize;
    let height = framebuffer.info().height as usize;
    let buffer = framebuffer.buffer_mut();
    let mut cwd = fs.root_cluster;

    // Clear screen area for shell (simple white background)
    draw_rect(buffer, width, 0, 0, width, height, [255,255,255]);

    // Current printing Y position (start near top)
    let mut current_y: usize = 10;
    let prompt_x: usize = 10;
    let prompt = "yashell> ";
    let prompt_len = prompt.len();

    // draw initial prompt
    draw_string(buffer, width, prompt_x, current_y, prompt, [0,0,0]);

    loop {
        // read input after prompt
        let line = read_line(prompt_x + prompt_len * 8, current_y, framebuffer);
        let s: &str = line.as_str();

        // move to next line for output
        current_y += 16;
        if current_y + 16 >= height {
            // simple scroll: clear and reset to top
            draw_rect(framebuffer.buffer_mut(), width, 0, 0, width, height, [255,255,255]);
            current_y = 10;
        }

        if s == "" {
            // just redraw prompt
            draw_string(framebuffer.buffer_mut(), width, prompt_x, current_y, prompt, [0,0,0]);
            continue;
        }

        if execute_command(fs, framebuffer, s, &mut current_y, prompt_x, width, height, &mut cwd) {
            return;
        }

        // draw new prompt at current_y
        if current_y + 16 >= height {
            draw_rect(framebuffer.buffer_mut(), width, 0, 0, width, height, [255,255,255]);
            current_y = 10;
        }
        draw_string(framebuffer.buffer_mut(), width, prompt_x, current_y, prompt, [0,0,0]);
    }
}
