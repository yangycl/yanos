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

fn cfg_write_u32(bus: u8, dev: u8, func: u8, offset: u8, val: u32) {
    let addr = (1u32 << 31)
        | ((bus as u32) << 16)
        | ((dev as u32) << 11)
        | ((func as u32) << 8)
        | ((offset as u32) & 0xFC);
    unsafe {
        Port::new(0xCF8).write(addr);
        Port::new(0xCFC).write(val);
    }
}

fn cfg_write_u16(bus: u8, dev: u8, func: u8, offset: u8, val: u16) {
    let aligned = offset & 0xFC;
    let shift = (offset & 2) * 8;
    let mut cur = cfg_read_u32(bus, dev, func, aligned);
    cur &= !(0xFFFFu32 << shift);
    cur |= (val as u32) << shift;
    cfg_write_u32(bus, dev, func, aligned, cur);
}

/// 新筆電常有兩顆以上 xHCI。接收器插的 USB-A 不一定在第一顆上。
pub fn find_xhci_bars(out: &mut [usize]) -> usize {
    let mut n = 0;
    for bus in 0..=255u8 {
        for dev in 0..32u8 {
            for func in 0..8u8 {
                if cfg_read_u16(bus, dev, func, 0x00) == 0xFFFF {
                    continue;
                }
                let class = cfg_read_u8(bus, dev, func, 0x0B);
                let subclass = cfg_read_u8(bus, dev, func, 0x0A);
                let prog_if = cfg_read_u8(bus, dev, func, 0x09);
                if class == 0x0C && subclass == 0x03 && prog_if == 0x30 {
                    let cmd = cfg_read_u16(bus, dev, func, 0x04);
                    cfg_write_u16(bus, dev, func, 0x04, cmd | (1 << 1) | (1 << 2));
                    let bar0 = cfg_read_u32(bus, dev, func, 0x10);
                    let mut addr = (bar0 & !0xF) as u64;
                    if bar0 & 0x6 == 0x4 {
                        addr |= (cfg_read_u32(bus, dev, func, 0x14) as u64) << 32;
                    }
                    if n < out.len() {
                        out[n] = addr as usize;
                        n += 1;
                    }
                }
            }
        }
    }
    n
}

/// 回傳第一顆 xHCI 的 MMIO 物理位址 (BAR0)；找不到則 None
pub fn find_xhci_bar0() -> Option<usize> {
    let mut bars = [0usize; 1];
    if find_xhci_bars(&mut bars) == 0 {
        return None;
    }
    Some(bars[0])
}

