//! Keyboard input: USB HID first, PS/2 fallback (QEMU / Legacy)
use crate::interrupts;
use core::sync::atomic::{AtomicBool, Ordering};

static SHIFT_ACTIVE: AtomicBool = AtomicBool::new(false);
static CAPS_ACTIVE: AtomicBool = AtomicBool::new(false);

// ---------------------------------------------------------------------------
// Direction keys (Explorer / mouse simulation) — still from PS/2 extended
// ---------------------------------------------------------------------------

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
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

pub struct KeyboardDecoder {
    extended: bool,
}

impl KeyboardDecoder {
    pub const fn new() -> Self {
        Self { extended: false }
    }

    pub fn process(&mut self) -> Option<KeyEvent> {
        let sc = interrupts::peek_key()?;
        if sc == 0xE0 {
            let _ = interrupts::get_key();
            self.extended = true;
            return None;
        }
        if !self.extended {
            return None;
        }
        let sc = interrupts::get_key()?;
        self.extended = false;
        match sc {
            0x48 => Some(KeyEvent::UpPress),
            0x50 => Some(KeyEvent::DownPress),
            0x4B => Some(KeyEvent::LeftPress),
            0x4D => Some(KeyEvent::RightPress),
            0xC8 => Some(KeyEvent::UpRelease),
            0xD0 => Some(KeyEvent::DownRelease),
            0xCB => Some(KeyEvent::LeftRelease),
            0xCD => Some(KeyEvent::RightRelease),
            _ => None,
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
    pub const fn new() -> Self {
        Self {
            up: false,
            down: false,
            left: false,
            right: false,
        }
    }

    pub fn update(&mut self, ev: KeyEvent) {
        match ev {
            KeyEvent::UpPress => self.up = true,
            KeyEvent::DownPress => self.down = true,
            KeyEvent::LeftPress => self.left = true,
            KeyEvent::RightPress => self.right = true,
            KeyEvent::UpRelease => self.up = false,
            KeyEvent::DownRelease => self.down = false,
            KeyEvent::LeftRelease => self.left = false,
            KeyEvent::RightRelease => self.right = false,
        }
    }
}

// ---------------------------------------------------------------------------
// USB HID Usage ID → char
// ---------------------------------------------------------------------------

fn hid_unshifted(usage: u8) -> Option<char> {
    match usage {
        0x04 => Some('a'),
        0x05 => Some('b'),
        0x06 => Some('c'),
        0x07 => Some('d'),
        0x08 => Some('e'),
        0x09 => Some('f'),
        0x0A => Some('g'),
        0x0B => Some('h'),
        0x0C => Some('i'),
        0x0D => Some('j'),
        0x0E => Some('k'),
        0x0F => Some('l'),
        0x10 => Some('m'),
        0x11 => Some('n'),
        0x12 => Some('o'),
        0x13 => Some('p'),
        0x14 => Some('q'),
        0x15 => Some('r'),
        0x16 => Some('s'),
        0x17 => Some('t'),
        0x18 => Some('u'),
        0x19 => Some('v'),
        0x1A => Some('w'),
        0x1B => Some('x'),
        0x1C => Some('y'),
        0x1D => Some('z'),
        0x1E => Some('1'),
        0x1F => Some('2'),
        0x20 => Some('3'),
        0x21 => Some('4'),
        0x22 => Some('5'),
        0x23 => Some('6'),
        0x24 => Some('7'),
        0x25 => Some('8'),
        0x26 => Some('9'),
        0x27 => Some('0'),
        0x28 => Some('\n'),
        0x2A => Some('\u{8}'),
        0x2B => Some('\t'),
        0x2C => Some(' '),
        0x2D => Some('-'),
        0x2E => Some('='),
        0x2F => Some('['),
        0x30 => Some(']'),
        0x31 => Some('\\'),
        0x33 => Some(';'),
        0x34 => Some('\''),
        0x35 => Some('`'),
        0x36 => Some(','),
        0x37 => Some('.'),
        0x38 => Some('/'),
        _ => None,
    }
}

fn hid_shifted_symbol(usage: u8) -> Option<char> {
    match usage {
        0x1E => Some('!'),
        0x1F => Some('@'),
        0x20 => Some('#'),
        0x21 => Some('$'),
        0x22 => Some('%'),
        0x23 => Some('^'),
        0x24 => Some('&'),
        0x25 => Some('*'),
        0x26 => Some('('),
        0x27 => Some(')'),
        0x2D => Some('_'),
        0x2E => Some('+'),
        0x2F => Some('{'),
        0x30 => Some('}'),
        0x31 => Some('|'),
        0x33 => Some(':'),
        0x34 => Some('"'),
        0x35 => Some('~'),
        0x36 => Some('<'),
        0x37 => Some('>'),
        0x38 => Some('?'),
        _ => None,
    }
}

fn hid_usage_to_char(usage: u8, shift: bool, caps: bool) -> Option<char> {
    if usage == 0 || usage == 0x39 {
        return None; // empty or CapsLock handled elsewhere
    }

    let base = hid_unshifted(usage)?;

    if base.is_ascii_alphabetic() {
        let upper = shift ^ caps;
        return Some(if upper {
            base.to_ascii_uppercase()
        } else {
            base.to_ascii_lowercase()
        });
    }

    if shift {
        hid_shifted_symbol(usage).or(Some(base))
    } else {
        Some(base)
    }
}

/// 非阻塞：從 USB report 佇列取一筆並解成字元
fn read_char_usb() -> Option<char> {
    let report = interrupts::get_usb_report()?;
    // Boot keyboard: [modifier][reserved][key0..key5]
    let modifier = report[0];
    let shift = (modifier & 0x22) != 0; // bit1 L-Shift | bit5 R-Shift
    SHIFT_ACTIVE.store(shift, Ordering::Relaxed);

    let caps = CAPS_ACTIVE.load(Ordering::Relaxed);

    for i in 2..8 {
        let usage = report[i];
        if usage == 0 {
            continue;
        }
        if usage == 0x39 {
            CAPS_ACTIVE.store(!caps, Ordering::Relaxed);
            continue;
        }
        if let Some(ch) = hid_usage_to_char(usage, shift, CAPS_ACTIVE.load(Ordering::Relaxed)) {
            return Some(ch);
        }
    }
    None
}

// ---------------------------------------------------------------------------
// PS/2 Set1（QEMU / USB Legacy）
// ---------------------------------------------------------------------------

fn ps2_unshifted(sc: u8) -> Option<char> {
    match sc {
        0x02 => Some('1'),
        0x03 => Some('2'),
        0x04 => Some('3'),
        0x05 => Some('4'),
        0x06 => Some('5'),
        0x07 => Some('6'),
        0x08 => Some('7'),
        0x09 => Some('8'),
        0x0A => Some('9'),
        0x0B => Some('0'),
        0x0C => Some('-'),
        0x0D => Some('='),
        0x0E => Some('\u{8}'),
        0x0F => Some('\t'),
        0x10 => Some('q'),
        0x11 => Some('w'),
        0x12 => Some('e'),
        0x13 => Some('r'),
        0x14 => Some('t'),
        0x15 => Some('y'),
        0x16 => Some('u'),
        0x17 => Some('i'),
        0x18 => Some('o'),
        0x19 => Some('p'),
        0x1A => Some('['),
        0x1B => Some(']'),
        0x1C => Some('\n'),
        0x1E => Some('a'),
        0x1F => Some('s'),
        0x20 => Some('d'),
        0x21 => Some('f'),
        0x22 => Some('g'),
        0x23 => Some('h'),
        0x24 => Some('j'),
        0x25 => Some('k'),
        0x26 => Some('l'),
        0x27 => Some(';'),
        0x28 => Some('\''),
        0x29 => Some('`'),
        0x2C => Some('z'),
        0x2D => Some('x'),
        0x2E => Some('c'),
        0x2F => Some('v'),
        0x30 => Some('b'),
        0x31 => Some('n'),
        0x32 => Some('m'),
        0x33 => Some(','),
        0x34 => Some('.'),
        0x35 => Some('/'),
        0x39 => Some(' '),
        _ => None,
    }
}

fn ps2_shifted(sc: u8) -> Option<char> {
    match sc {
        0x02 => Some('!'),
        0x03 => Some('@'),
        0x04 => Some('#'),
        0x05 => Some('$'),
        0x06 => Some('%'),
        0x07 => Some('^'),
        0x08 => Some('&'),
        0x09 => Some('*'),
        0x0A => Some('('),
        0x0B => Some(')'),
        0x0C => Some('_'),
        0x0D => Some('+'),
        0x0E => Some('\u{8}'),
        0x0F => Some('\t'),
        0x10 => Some('Q'),
        0x11 => Some('W'),
        0x12 => Some('E'),
        0x13 => Some('R'),
        0x14 => Some('T'),
        0x15 => Some('Y'),
        0x16 => Some('U'),
        0x17 => Some('I'),
        0x18 => Some('O'),
        0x19 => Some('P'),
        0x1A => Some('{'),
        0x1B => Some('}'),
        0x1C => Some('\n'),
        0x1E => Some('A'),
        0x1F => Some('S'),
        0x20 => Some('D'),
        0x21 => Some('F'),
        0x22 => Some('G'),
        0x23 => Some('H'),
        0x24 => Some('J'),
        0x25 => Some('K'),
        0x26 => Some('L'),
        0x27 => Some(':'),
        0x28 => Some('"'),
        0x29 => Some('~'),
        0x2C => Some('Z'),
        0x2D => Some('X'),
        0x2E => Some('C'),
        0x2F => Some('V'),
        0x30 => Some('B'),
        0x31 => Some('N'),
        0x32 => Some('M'),
        0x33 => Some('<'),
        0x34 => Some('>'),
        0x35 => Some('?'),
        0x39 => Some(' '),
        _ => None,
    }
}

fn read_char_ps2() -> Option<char> {
    let sc = interrupts::get_key()?;

    if sc == 0xE0 {
        return None;
    }

    // release
    if sc & 0x80 != 0 {
        let make = sc & 0x7F;
        if make == 0x2A || make == 0x36 {
            SHIFT_ACTIVE.store(false, Ordering::Relaxed);
        }
        return None;
    }

    // make
    if sc == 0x2A || sc == 0x36 {
        SHIFT_ACTIVE.store(true, Ordering::Relaxed);
        return None;
    }
    if sc == 0x3A {
        CAPS_ACTIVE.store(!CAPS_ACTIVE.load(Ordering::Relaxed), Ordering::Relaxed);
        return None;
    }

    let shift = SHIFT_ACTIVE.load(Ordering::Relaxed);
    let caps = CAPS_ACTIVE.load(Ordering::Relaxed);

    if shift {
        if let Some(ch) = ps2_shifted(sc) {
            if ch.is_ascii_alphabetic() {
                // shifted map already upper; caps toggles back
                return Some(if caps {
                    ch.to_ascii_lowercase()
                } else {
                    ch
                });
            }
            return Some(ch);
        }
    }

    let ch = ps2_unshifted(sc)?;
    if ch.is_ascii_alphabetic() {
        let upper = shift ^ caps;
        return Some(if upper {
            ch.to_ascii_uppercase()
        } else {
            ch.to_ascii_lowercase()
        });
    }
    Some(ch)
}

// ---------------------------------------------------------------------------
// Public API（主迴圈繼續用這個）
// ---------------------------------------------------------------------------

/// 非阻塞：先 USB，再 PS/2
pub fn read_char() -> Option<char> {
    if let Some(c) = read_char_usb() {
        return Some(c);
    }
    read_char_ps2()
}

/// USB 驅動取得 report 後呼叫（也可用 interrupts::push_usb_report）
pub fn push_hid_report(report: [u8; 8]) {
    interrupts::push_usb_report(report);
}