use usb_oxide::MscDevice;

use crate::dma::MyDma;
use crate::fs::block::BlockDevice;

/// xHCI Bulk-Only 隨身碟。`base` 是 512-byte 扇區編號，Fat32 看到的 sector 0 就是它。
pub struct UsbDisk {
    dev: MscDevice<MyDma>,
    base: u32,
    /// 裝置 READ CAPACITY 的邏輯區塊。512 或 4096。
    block_bytes: u32,
}


/// 標準 UEFI 規範下的 EFI 系統分割區 (ESP) GUID 陣列
/// 格式對應：c12a7328-f81f-11d2-ba4b-00a0c93ec93b
const EFI_SYSTEM_PART_GUID: [u8; 16] = [
    0x28, 0x73, 0x2A, 0xC1, // c12a7328 (Little-Endian)
    0x1F, 0xF8,             // f81f     (Little-Endian)
    0xD2, 0x11,             // 11d2     (Little-Endian)
    0xBA, 0x4B, 0x00, 0xA0, 0xC9, 0x3E, 0xC9, 0x3B // 剩餘為 Big-Endian
];


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
        let reported = cap.block_size();
        // Windows 這支是 LogicalBytesPerSector 512。容量欄位垃圾時仍用 512 試開機扇區。
        let block_bytes = if reported == 4096 { 4096 } else { 512 };
        let mut disk = Self {
            dev,
            base: 0,
            block_bytes,
        };
        let mut sector = [0u8; 512];
        if !disk.read_512(0, &mut sector) {
            return Err("READ");
        }
        if fat_boot(&sector) {
            return Ok(disk);
        }
        let mut bases = partition_bases(&sector);
        if let Some(lba) = disk.gpt_start() {
            bases[4] = lba;
        }
        let scale = block_bytes / 512;
        let mut tried = false;
        for dev_lba in bases {
            if dev_lba == 0 {
                continue;
            }
            let fat_lba = dev_lba.saturating_mul(scale);
            tried = true;
            if !disk.read_512(fat_lba, &mut sector) {
                continue;
            }
            if fat_boot(&sector) {
                disk.base = fat_lba;
                return Ok(disk);
            }
        }
        if !tried && fat_boot(&sector) {
            return Ok(disk);
        }
        Err("SIG")
    }

    pub fn base(&self) -> u32 {
        self.base
    }

    fn read_512(&mut self, fat_lba: u32, sector: &mut [u8; 512]) -> bool {
        let scale = self.block_bytes / 512;
        let dev_lba = fat_lba / scale;
        let off = ((fat_lba % scale) as usize) * 512;
        if self.block_bytes == 512 {
            let ok = matches!(self.dev.read_blocks(0, dev_lba, 1, sector), Ok(n) if n >= 512);
            return ok;
        }
        let mut block = [0u8; 4096];
        let ok = match self.dev.read_blocks(0, dev_lba, 1, &mut block) {
            Ok(n) if n >= off + 512 => {
                sector.copy_from_slice(&block[off..off + 512]);
                true
            }
            _ => false,
        };
        ok
    }


    fn gpt_start(&mut self) -> Option<u32> {
        let mut sector = [0u8; 512];
        let scale = self.block_bytes / 512;
        if !self.read_512(scale, &mut sector) || &sector[..8] != b"EFI PART" {
            return None;
        }
        let entry_lba = u64::from_le_bytes(sector[72..80].try_into().ok()?);
        let count = u32::from_le_bytes(sector[80..84].try_into().ok()?) as usize;
        let size = u32::from_le_bytes(sector[84..88].try_into().ok()?) as usize;
        if entry_lba == 0 || size < 48 || size > 512 || count == 0 {
            return None;
        }
        let mut raw = [0u8; 512];
        if !self.read_512(entry_lba as u32 * scale, &mut raw) {
            return None;
        }
        let n = count.min(512 / size);
        let scale = self.block_bytes / 512;
        for i in 0..n {
            let e = &raw[i * size..i * size + 48];
            if e[..16].iter().all(|b| *b == 0) {
                continue;
            }
            
            // 🌟 關鍵修正：如果偵測到目前這個分區是 EFI 系統引導區，立刻無情跳過！
            if e[..16] == EFI_SYSTEM_PART_GUID {
                continue;
            }

            // 確保它是合法的普通資料儲存區 (Microsoft Basic Data)
            if e[..16] != BASIC_DATA_GUID {
                continue;
            }

            let start = u64::from_le_bytes(e[32..40].try_into().ok()?);
            if start > 0 && start <= u32::MAX as u64 {
                let Some(fat_lba) = (start as u32).checked_mul(scale) else {
                    continue;
                };
                let mut boot = [0u8; 512];
                if self.read_512(fat_lba, &mut boot) && fat_boot(&boot) {
                    // 自動略過 EFI 區後，第一個撞到的就是合法的普通 FAT32 資料儲存區！
                    return Some(start as u32);
                }
            }
        }
        None
    }
}

fn fat_boot(sector: &[u8; 512]) -> bool {
    let bytes_per_sector = u16::from_le_bytes([sector[11], sector[12]]);
    let sectors_per_cluster = sector[13];
    let reserved_sectors = u16::from_le_bytes([sector[14], sector[15]]);
    let fat_count = sector[16];
    let root_entry_count = u16::from_le_bytes([sector[17], sector[18]]);
    let fat_size_16 = u16::from_le_bytes([sector[22], sector[23]]);
    let total_sectors_16 = u16::from_le_bytes([sector[19], sector[20]]);
    let total_sectors_32 = u32::from_le_bytes([sector[32], sector[33], sector[34], sector[35]]);
    let fat_size_32 = u32::from_le_bytes([sector[36], sector[37], sector[38], sector[39]]);
    let root_cluster = u32::from_le_bytes([sector[44], sector[45], sector[46], sector[47]])
        & 0x0FFF_FFFF;

    sector[510] == 0x55
        && sector[511] == 0xAA
        && (sector[0] == 0xEB || sector[0] == 0xE9)
        && bytes_per_sector == 512
        && sectors_per_cluster.is_power_of_two()
        && sectors_per_cluster <= 128
        && reserved_sectors > 0
        && (1..=2).contains(&fat_count)
        && root_entry_count == 0
        && fat_size_16 == 0
        && (total_sectors_16 > 0 || total_sectors_32 > 0)
        && fat_size_32 > 0
        && (2..0x0FFF_FFF8).contains(&root_cluster)
}

const BASIC_DATA_GUID: [u8; 16] = [
    0xA2, 0xA0, 0xD0, 0xEB, 0xE5, 0xB9, 0x33, 0x44, 0x87, 0xC0, 0x68, 0xB6, 0xB7, 0x26, 0x99,
    0xC7,
];

fn partition_bases(sector: &[u8; 512]) -> [u32; 5] {
    let mut out = [0u32; 5];
    let mut n = 0;
    for i in 0..4 {
        let part = &sector[0x1BE + i * 16..0x1BE + (i + 1) * 16];
        let typ = part[4];
        if part[0] != 0x80 && part[0] != 0x00 {
            continue;
        }
        if typ == 0 || !is_fat_partition_type(typ) {
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

fn is_fat_partition_type(typ: u8) -> bool {
    matches!(
        typ,
        0x01 | 0x04 | 0x06 | 0x07 | 0x0B | 0x0C | 0x0E | 0x0F | 0x11 | 0x14 | 0x16 | 0x1B
            | 0x1C | 0x1E | 0x27 | 0x29 | 0x39 | 0x3C
    )
}

impl BlockDevice for UsbDisk {
    fn read_sector(&mut self, lba: u64, buffer: &mut [u8; 512]) {
        let at = self.base.saturating_add(lba as u32);
        let _ = self.read_512(at, buffer);
    }

    fn write_sector(&mut self, lba: u64, buffer: &[u8; 512]) {
        if self.block_bytes != 512 {
            return;
        }
        let at = self.base as u64 + lba;
        let mut tmp = *buffer;
        let _ = self.dev.write_blocks(0, at as u32, 1, &mut tmp);
    }
}
