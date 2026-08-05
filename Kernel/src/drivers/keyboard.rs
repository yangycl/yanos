use crate::interrupts;
use core::option::Option;

// Provide a more complete scancode -> char mapping (PS/2 set 1), with shift and caps handling.
// This function-style API is intended for use by userland code like yashell via keyboard::read_char().

static mut SHIFT_ACTIVE: bool = false;
static mut CAPS_ACTIVE: bool = false;

fn map_unshifted(sc: u8) -> Option<char> {
    match sc {
        0x02 => Some('1'), 0x03 => Some('2'), 0x04 => Some('3'), 0x05 => Some('4'), 0x06 => Some('5'),
        0x07 => Some('6'), 0x08 => Some('7'), 0x09 => Some('8'), 0x0A => Some('9'), 0x0B => Some('0'),
        0x0C => Some('-'), 0x0D => Some('='), 0x0E => Some('\u{8}'), 0x0F => Some('\t'),
        0x10 => Some('q'), 0x11 => Some('w'), 0x12 => Some('e'), 0x13 => Some('r'), 0x14 => Some('t'),
        0x15 => Some('y'), 0x16 => Some('u'), 0x17 => Some('i'), 0x18 => Some('o'), 0x19 => Some('p'),
        0x1A => Some('['), 0x1B => Some(']'), 0x1C => Some('\n'),
        0x1E => Some('a'), 0x1F => Some('s'), 0x20 => Some('d'), 0x21 => Some('f'), 0x22 => Some('g'),
        0x23 => Some('h'), 0x24 => Some('j'), 0x25 => Some('k'), 0x26 => Some('l'),
        0x27 => Some(';'), 0x28 => Some('\''), 0x29 => Some('`'),
        0x2C => Some('z'), 0x2D => Some('x'), 0x2E => Some('c'), 0x2F => Some('v'), 0x30 => Some('b'),
        0x31 => Some('n'), 0x32 => Some('m'), 0x33 => Some(','), 0x34 => Some('.'), 0x35 => Some('/'),
        0x39 => Some(' '),
        _ => None,
    }
}

fn map_shifted(sc: u8) -> Option<char> {
    match sc {
        0x02 => Some('!'), 0x03 => Some('@'), 0x04 => Some('#'), 0x05 => Some('$'), 0x06 => Some('%'),
        0x07 => Some('^'), 0x08 => Some('&'), 0x09 => Some('*'), 0x0A => Some('('), 0x0B => Some(')'),
        0x0C => Some('_'), 0x0D => Some('+'), 0x0E => Some('\u{8}'), 0x0F => Some('\t'),
        0x10 => Some('Q'), 0x11 => Some('W'), 0x12 => Some('E'), 0x13 => Some('R'), 0x14 => Some('T'),
        0x15 => Some('Y'), 0x16 => Some('U'), 0x17 => Some('I'), 0x18 => Some('O'), 0x19 => Some('P'),
        0x1A => Some('{'), 0x1B => Some('}'), 0x1C => Some('\n'),
        0x1E => Some('A'), 0x1F => Some('S'), 0x20 => Some('D'), 0x21 => Some('F'), 0x22 => Some('G'),
        0x23 => Some('H'), 0x24 => Some('J'), 0x25 => Some('K'), 0x26 => Some('L'),
        0x27 => Some(':'), 0x28 => Some('"'), 0x29 => Some('~'),
        0x2C => Some('Z'), 0x2D => Some('X'), 0x2E => Some('C'), 0x2F => Some('V'), 0x30 => Some('B'),
        0x31 => Some('N'), 0x32 => Some('M'), 0x33 => Some('<'), 0x34 => Some('>'), 0x35 => Some('?'),
        0x39 => Some(' '),
        _ => None,
    }
}

// Read next printable character, handling shift and capslock state.
pub fn read_char() -> Option<char> {
    loop {
        let sc = interrupts::get_key()?;

        // ignore extended prefix for now
        if sc == 0xE0 { continue; }

        // key release
        if sc & 0x80 != 0 {
            let make = sc & 0x7F;
            unsafe {
                if make == 0x2A || make == 0x36 {
                    SHIFT_ACTIVE = false;
                }
            }
            continue;
        }

        // key make
        unsafe {
            if sc == 0x2A || sc == 0x36 {
                SHIFT_ACTIVE = true;
                continue;
            }
            if sc == 0x3A {
                CAPS_ACTIVE = !CAPS_ACTIVE;
                continue;
            }
        }

        // translate
        let ch = unsafe {
            // If letter, handle caps/shift: prefer shifted mapping when SHIFT active; for letters, shifted mapping gives uppercase
            if SHIFT_ACTIVE {
                map_shifted(sc)
            } else if CAPS_ACTIVE {
                // when caps active, for letters use shifted mapping, otherwise unshifted
                match map_unshifted(sc) {
                    Some(c) if c.is_ascii_alphabetic() => map_shifted(sc),
                    _ => map_unshifted(sc),
                }
            } else {
                map_unshifted(sc)
            }
        };

        if ch.is_some() {
            return ch;
        }

        // ignore other scancodes
    }
}


pub struct KeyboardDecoder {
    extended: bool,
}

pub enum KeyEvent {
    UpPress,
    DownPress,
    LeftPress,
    RightPress,
    UpRelease,
    DownRelease,
    LeftRelease,
    RightRelease,
}

impl KeyboardDecoder {
    pub const fn new() -> Self { Self { extended:false } }

    pub fn process(&mut self) -> Option<KeyEvent> {

        // 先看第一個 byte
        let first = interrupts::peek_key()?;


        // 不是 E0 擴展鍵，不碰它
        // 留給 read_char()
        if first != 0xE0 {
            return None;
        }


        // 移除 E0
        interrupts::get_key();


        // 取得第二個 byte
        let scan_code =
            interrupts::get_key()?;


        match scan_code {

            0x48 =>
                Some(KeyEvent::UpPress),

            0x50 =>
                Some(KeyEvent::DownPress),

            0x4B =>
                Some(KeyEvent::LeftPress),

            0x4D =>
                Some(KeyEvent::RightPress),


            0xC8 =>
                Some(KeyEvent::UpRelease),

            0xD0 =>
                Some(KeyEvent::DownRelease),

            0xCB =>
                Some(KeyEvent::LeftRelease),

            0xCD =>
                Some(KeyEvent::RightRelease),


            _ =>
                None,
        }
    }
}


pub struct KeyboardState {
    pub up: bool,
    pub down: bool,
    pub left: bool,
    pub right: bool,
}

impl KeyboardState {
    pub const fn new() -> Self { Self { up:false, down:false, left:false, right:false } }
    pub fn update(&mut self, key: KeyEvent) {
        match key {
            KeyEvent::UpPress => { self.up = true; }
            KeyEvent::UpRelease => { self.up = false; }
            KeyEvent::DownPress => { self.down = true; }
            KeyEvent::DownRelease => { self.down = false; }
            KeyEvent::LeftPress => { self.left = true; }
            KeyEvent::LeftRelease => { self.left = false; }
            KeyEvent::RightPress => { self.right = true; }
            KeyEvent::RightRelease => { self.right = false; }
        }
    }
}
