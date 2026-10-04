use usb_oxide::MscDevice;

use crate::dma::MyDma;
use crate::fs::block::BlockDevice;

/// xHCI Bulk-Only 隨身碟。`base` 是分割區起始 LBA，Fat32 看到的 sector 0 就是它。
pub struct UsbDisk {
    dev: MscDevice<MyDma>,
    base: u32,
}

impl UsbDisk {
    pub fn open(mut dev: MscDevice<MyDma>) -> Option<Self> {
        let mut ready = false;
        for _ in 0..8 {
            if dev.test_unit_ready(0).unwrap_or(false) {
                ready = true;
                break;
            }
            let _ = dev.request_sense(0);
        }
        if !ready {
            return None;
        }
        let cap = dev.read_capacity(0).ok()?;
        if cap.block_size() != 512 {
            return None;
        }
        let mut sector = [0u8; 512];
        dev.read_blocks(0, 0, 1, &mut sector).ok()?;
        let base = partition_base(&sector);
        if base != 0 {
            dev.read_blocks(0, base, 1, &mut sector).ok()?;
        }
        if sector[510] != 0x55 || sector[511] != 0xAA {
            return None;
        }
        Some(Self { dev, base })
    }

    pub fn base(&self) -> u32 {
        self.base
    }
}

fn partition_base(sector: &[u8; 512]) -> u32 {
    // 本身就是開機扇區，不是 MBR。
    if sector[0] == 0xEB || sector[0] == 0xE9 {
        return 0;
    }
    let mut fallback = 0u32;
    for i in 0..4 {
        let part = &sector[0x1BE + i * 16..0x1BE + (i + 1) * 16];
        if part[0] != 0x80 && part[0] != 0x00 {
            continue;
        }
        let kind = part[4];
        if kind == 0 {
            continue;
        }
        let lba = u32::from_le_bytes([part[8], part[9], part[10], part[11]]);
        if lba == 0 {
            continue;
        }
        // 0x0B/0x0C FAT32，0x0E/0x06 FAT16，0x07 NTFS/exFAT 也先交給簽名判斷。
        if matches!(kind, 0x01 | 0x04 | 0x06 | 0x0B | 0x0C | 0x0E | 0x07) {
            return lba;
        }
        if fallback == 0 {
            fallback = lba;
        }
    }
    fallback
}

impl BlockDevice for UsbDisk {
    fn read_sector(&mut self, lba: u64, buffer: &mut [u8; 512]) {
        let at = self.base as u64 + lba;
        let _ = self.dev.read_blocks(0, at as u32, 1, buffer);
    }

    fn write_sector(&mut self, lba: u64, buffer: &[u8; 512]) {
        let at = self.base as u64 + lba;
        let mut tmp = *buffer;
        let _ = self.dev.write_blocks(0, at as u32, 1, &mut tmp);
    }
}
