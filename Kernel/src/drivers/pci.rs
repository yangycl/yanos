// Kernel/src/drivers/pci.rs

use x86_64::instructions::port::Port;

fn cfg_read_u32(bus: u8, dev: u8, func: u8, offset: u8) -> u32 {
    let addr = (1u32 << 31)
        | ((bus as u32) << 16)
        | ((dev as u32) << 11)
        | ((func as u32) << 8)
        | ((offset as u32) & 0xFC);
    unsafe {
        Port::new(0xCF8).write(addr);
        Port::<u32>::new(0xCFC).read()
    }
}

fn cfg_read_u8(bus: u8, dev: u8, func: u8, offset: u8) -> u8 {
    let v = cfg_read_u32(bus, dev, func, offset & 0xFC);
    ((v >> ((offset & 3) * 8)) & 0xFF) as u8
}

fn cfg_read_u16(bus: u8, dev: u8, func: u8, offset: u8) -> u16 {
    let v = cfg_read_u32(bus, dev, func, offset & 0xFC);
    ((v >> ((offset & 2) * 8)) & 0xFFFF) as u16
}

/// 回傳 xHCI 的 MMIO 物理位址 (BAR0)；找不到則 None
pub fn find_xhci_bar0() -> Option<usize> {
    for bus in 0..=255u8 {
        for dev in 0..32u8 {
            for func in 0..8u8 {
                if cfg_read_u16(bus, dev, func, 0x00) == 0xFFFF {
                    continue;
                }
                let class = cfg_read_u8(bus, dev, func, 0x0B);
                let subclass = cfg_read_u8(bus, dev, func, 0x0A);
                let prog_if = cfg_read_u8(bus, dev, func, 0x09);
                // Class 0x0C = Serial Bus, Subclass 0x03 = USB, Prog IF 0x30 = xHCI
                if class == 0x0C && subclass == 0x03 && prog_if == 0x30 {
                    let bar = cfg_read_u32(bus, dev, func, 0x10) & !0xF;
                    return Some(bar as usize);
                }
            }
        }
    }
    None
}