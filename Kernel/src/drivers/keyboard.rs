use crate::interrupts;


pub static SCAN_CODE_TABLE: [Option<char>; 110] = [
    None, // 0x00
    None, // 0x01 ESC
    Some('1'), // 0x02
    Some('2'), // 0x03
    Some('3'), // 0x04
    Some('4'), // 0x05
    Some('5'), // 0x06
    Some('6'), // 0x07
    Some('7'), // 0x08
    Some('8'), // 0x09
    Some('9'), // 0x0A
    Some('0'), // 0x0B
    None,
    None,
    Some('\t'), // 0x0F

    None, // 0x10
    None, // 0x11
    Some('q'), // 0x12
    Some('w'), // 0x13
    Some('e'), // 0x14
    Some('r'), // 0x15
    Some('t'), // 0x16
    Some('y'), // 0x17
    Some('u'), // 0x18
    Some('i'), // 0x19
    Some('o'), // 0x1A
    Some('p'), // 0x1B

    Some('\n'), // 0x1C Enter

    None,
    Some('a'), // 0x1E
    Some('s'), // 0x1F
    Some('d'), // 0x20
    Some('f'), // 0x21
    Some('g'), // 0x22
    Some('h'), // 0x23
    Some('j'), // 0x24
    Some('k'), // 0x25
    Some('l'), // 0x26

    None,
    None,

    Some('z'), // 0x2C
    Some('x'), // 0x2D
    Some('c'), // 0x2E
    Some('v'), // 0x2F
    Some('b'), // 0x30
    Some('n'), // 0x31
    Some('m'), // 0x32

    None,
    None,

    Some(' '), // 0x39 Space

    // 後面未用填 None
    None,None,None,None,None,None,None,None,
    None,None,None,None,None,None,None,None,
    None,None,None,None,None,None,None,None,
    None,None,None,None,None,None,None,None,
    None,None,None,None,None,None,None,
    None,None,None,None,None,None,None,
    None,None,None,None,None,None,None,
    None,None,None,None,None,None,None,
];


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

    pub const fn new() -> Self {

        Self {
            extended:false,
        }

    }

    pub fn process(
        &mut self
    ) -> Option<KeyEvent> {

        loop {

            let scan_code =
                interrupts::get_key()?;


            if scan_code == 0xE0 {

                self.extended = true;

                continue;
            }


            if self.extended {

                self.extended = false;

                return match scan_code {

                    0x48 => Some(KeyEvent::UpPress),
                    0x50 => Some(KeyEvent::DownPress),
                    0x4B => Some(KeyEvent::LeftPress),
                    0x4D => Some(KeyEvent::RightPress),

                    0xC8 => Some(KeyEvent::UpRelease),
                    0xD0 => Some(KeyEvent::DownRelease),
                    0xCB => Some(KeyEvent::LeftRelease),
                    0xCD => Some(KeyEvent::RightRelease),

                    _ => None,
                };
            }

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
            up:false,
            down:false,
            left:false,
            right:false,
        }
    }


    pub fn update(&mut self, key: KeyEvent) {

        match key {

            KeyEvent::UpPress => {
                self.up = true;
            }

            KeyEvent::UpRelease => {
                self.up = false;
            }


            KeyEvent::DownPress => {
                self.down = true;
            }

            KeyEvent::DownRelease => {
                self.down = false;
            }


            KeyEvent::LeftPress => {
                self.left = true;
            }

            KeyEvent::LeftRelease => {
                self.left = false;
            }


            KeyEvent::RightPress => {
                self.right = true;
            }

            KeyEvent::RightRelease => {
                self.right = false;
            }

        }

    }
}

