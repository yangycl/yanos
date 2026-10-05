use usb_oxide::MscDevice;

use crate::dma::MyDma;
use crate::fs::block::BlockDevice;

/// xHCI Bulk-Only 隨身碟。`base` 是分割區起始 LBA，Fat32 看到的 sector 0 就是它。
pub struct UsbDisk {
    dev: MscDevice<MyDma>,
    base: u32,
}

impl UsbDisk {
    /// `Err` 是為什麼沒能當成 FAT 開：`TUR` `CAP` `BSZ` `READ` `SIG`。
    pub fn open(mut dev: MscDevice<MyDma>) -> Result<Self, &'static str> {
        // 剛 reset 完常是 unit attention。先轉起來，再多試幾次。
        let start = [0x1B, 0, 0, 0, 1, 0];
        let _ = dev.scsi_command(0, &start, None, false);
        let mut ready = false;
        for _ in 0..32 {
            if dev.test_unit_ready(0).unwrap_or(false) {
                ready = true;
                break;
            }
            let _ = dev.request_sense(0);
            for _ in 0..200_000 {
                core::hint::spin_loop();
            }
        }
        if !ready {
            return Err("TUR");
        }
        let cap = dev.read_capacity(0).map_err(|_| "CAP")?;
        if cap.block_size() != 512 {
            return Err("BSZ");
        }
        let mut sector = [0u8; 512];
        if !read_lba(&mut dev, 0, &mut sector) {
            return Err("READ");
        }
        if fat_boot(&sector) {
            return Ok(Self { dev, base: 0 });
        }
        let mut bases = partition_bases(&sector);
        if let Some(lba) = gpt_start(&mut dev) {
            bases[4] = lba;
        }
        for base in bases {
            if base == 0 {
                continue;
            }
            if !read_lba(&mut dev, base, &mut sector) {
                return Err("READ");
            }
            if fat_boot(&sector) {
                return Ok(Self { dev, base });
            }
        }
        Err("SIG")
    }

    pub fn base(&self) -> u32 {
        self.base
    }
}

fn read_lba(dev: &mut MscDevice<MyDma>, lba: u32, sector: &mut [u8; 512]) -> bool {
    match dev.read_blocks(0, lba, 1, sector) {
        Ok(n) => n >= 512,
        Err(_) => false,
    }
}

fn fat_boot(sector: &[u8; 512]) -> bool {
    sector[510] == 0x55
        && sector[511] == 0xAA
        && (sector[0] == 0xEB || sector[0] == 0xE9)
}

fn partition_bases(sector: &[u8; 512]) -> [u32; 5] {
    let mut out = [0u32; 5];
    let mut n = 0;
    for i in 0..4 {
        let part = &sector[0x1BE + i * 16..0x1BE + (i + 1) * 16];
        if part[0] != 0x80 && part[0] != 0x00 {
            continue;
        }
        if part[4] == 0 {
            continue;
        }
        let lba = u32::from_le_bytes([part[8], part[9], part[10], part[11]]);
        if lba == 0 {
            continue;
        }
        out[n] = lba;
        n += 1;
    }
    out
}

fn gpt_start(dev: &mut MscDevice<MyDma>) -> Option<u32> {
    let mut sector = [0u8; 512];
    if !read_lba(dev, 1, &mut sector) || &sector[..8] != b"EFI PART" {
        return None;
    }
    let entry_lba = u64::from_le_bytes(sector[72..80].try_into().ok()?);
    let count = u32::from_le_bytes(sector[80..84].try_into().ok()?) as usize;
    let size = u32::from_le_bytes(sector[84..88].try_into().ok()?) as usize;
    if entry_lba == 0 || size < 48 || size > 512 || count == 0 {
        return None;
    }
    let mut raw = [0u8; 512];
    if !read_lba(dev, entry_lba as u32, &mut raw) {
        return None;
    }
    let n = count.min(512 / size);
    for i in 0..n {
        let e = &raw[i * size..i * size + 48];
        if e[..16].iter().all(|b| *b == 0) {
            continue;
        }
        let start = u64::from_le_bytes(e[32..40].try_into().ok()?);
        if start > 0 && start <= u32::MAX as u64 {
            return Some(start as u32);
        }
    }
    None
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
