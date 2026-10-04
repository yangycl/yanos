use crate::fs::block::BlockDevice;
use crate::fs::directory::DirectoryEntry;
use crate::fs::fat32::Fat32;

const MAX_IMAGE: usize = 64 * 1024;
static mut IMAGE: [u8; MAX_IMAGE] = [0; MAX_IMAGE];

/// Load a `YEXE` file and call its first instruction.
///
/// Layout: `b"YEXE"`, little-endian `u32` `.text` length, then the assembled
/// code. The kernel enters at the first code byte and only accepts `text_len`
/// bytes after the header. The entry is `extern "C" fn()` and runs in kernel mode.
pub fn load_and_run<D: BlockDevice>(
    fs: &mut Fat32<D>,
    entry: DirectoryEntry,
) -> Result<(), ()> {
    if entry.attr & 0x10 != 0 {
        return Err(());
    }
    let n = read_file(fs, &entry, unsafe { &mut IMAGE })?;
    let image = unsafe { &IMAGE[..n] };
    if n < 8 || &image[..4] != b"YEXE" {
        return Err(());
    }
    let text_len = u32::from_le_bytes([image[4], image[5], image[6], image[7]]) as usize;
    if text_len == 0 || 8 + text_len > n {
        return Err(());
    }
    let code = unsafe { IMAGE.as_ptr().add(8) };
    let start: extern "C" fn() = unsafe { core::mem::transmute(code) };
    start();
    Ok(())
}

fn read_file<D: BlockDevice>(
    fs: &mut Fat32<D>,
    entry: &DirectoryEntry,
    dest: &mut [u8],
) -> Result<usize, ()> {
    let mut cluster = entry.first_cluster.ok_or(())?;
    let mut left = entry.file_size as usize;
    let mut off = 0usize;
    let mut sector = [0u8; 512];
    while left > 0 && cluster >= 2 && cluster < 0x0FFF_FFF8 {
        fs.read_cluster(cluster, &mut sector);
        let n = left.min(512).min(dest.len() - off);
        dest[off..off + n].copy_from_slice(&sector[..n]);
        off += n;
        left -= n;
        if off == dest.len() {
            break;
        }
        cluster = fs.read_fat_entry(cluster);
    }
    if off < 8 {
        return Err(());
    }
    Ok(off)
}
