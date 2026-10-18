use fatfs::{FileSystem, FormatVolumeOptions, FsOptions};
use gpt::disk::LogicalBlockSize;
use gpt::mbr::ProtectiveMBR;
use gpt::GptConfig;
use std::convert::TryFrom;
use std::env;
use std::error::Error;
use std::fs::{self, File, OpenOptions};
use std::io::{self, Read, Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};
use std::process::Command;

const TOTAL_IMAGE_SIZE: u64 = 512 * 1024 * 1024;
const SECTOR_SIZE: u64 = 512;
const ALIGNMENT_SECTORS: u64 = 2048;
const EFI_START_LBA: u64 = ALIGNMENT_SECTORS;

fn command_output(program: &str, args: &[&str]) -> Result<String, Box<dyn Error>> {
    let output = Command::new(program).args(args).output()?;

    if !output.status.success() {
        return Err(format!(
            "{} 執行失敗：{}",
            program,
            String::from_utf8_lossy(&output.stderr).trim()
        )
        .into());
    }

    Ok(String::from_utf8(output.stdout)?.trim().to_string())
}

fn validate_physical_device(device: &Path) -> Result<u64, Box<dyn Error>> {
    let device_str = device.to_str().ok_or("裝置路徑不是有效的 UTF-8")?;

    if !device.is_absolute() || !device.exists() {
        return Err("請指定有效的絕對裝置路徑，例如 /dev/sdb".into());
    }

    let canonical = fs::canonicalize(device)?;
    if canonical.parent() != Some(Path::new("/dev")) {
        return Err("裝置必須是 /dev 底下的直接裝置節點".into());
    }

    let device_type = command_output("lsblk", &["-dnro", "TYPE", device_str])?;

    if device_type != "disk" {
        return Err(format!("{} 不是整顆磁碟，拒絕寫入", device_str).into());
    }

    let mountpoints = command_output("lsblk", &["-nrpo", "MOUNTPOINTS", device_str])?;

    if mountpoints.lines().any(|line| !line.trim().is_empty()) {
        return Err("磁碟或其分區仍有掛載點，請先卸載所有分區後再執行".into());
    }

    let size_text = command_output("blockdev", &["--getsize64", device_str])?;
    let size_bytes: u64 = size_text.parse()?;

    if size_bytes < TOTAL_IMAGE_SIZE {
        return Err(format!(
            "磁碟容量不足：{} bytes，至少需要 {} bytes",
            size_bytes, TOTAL_IMAGE_SIZE
        )
        .into());
    }

    println!();
    println!("⚠️  即將清除這顆磁碟的原有分區表與資料！");
    println!("裝置：{}", device_str);
    println!("容量：{} bytes", size_bytes);
    println!("請確認裝置型號及備份均正確。");
    print!("若要繼續，請完整輸入 {}：", device_str);
    io::stdout().flush()?;

    let mut confirmation = String::new();
    io::stdin().read_line(&mut confirmation)?;

    if confirmation.trim() != device_str {
        return Err("確認不符，已取消部署".into());
    }

    Ok(size_bytes)
}

fn main() -> Result<(), Box<dyn Error>> {
    println!("====================================================");
    println!("   Yanos GPT / UEFI 安裝工具 v2.3");
    println!("====================================================");

    let args: Vec<String> = env::args().collect();

    let (is_device, target_arg) = match args.get(1).map(String::as_str) {
        Some("--image") => (
            false,
            args.get(2)
                .ok_or("用法：installer --image os/nova_os.img")?,
        ),
        Some("--device") => (
            true,
            args.get(2).ok_or("用法：installer --device /dev/sdX")?,
        ),
        _ => {
            return Err("請指定 --image os/nova_os.img 或 --device /dev/sdX".into());
        }
    };

    let target = Path::new(target_arg);

    if is_device {
        validate_physical_device(target)?;
    } else if target.exists() && !target.is_file() {
        return Err("映像檔模式的目標必須是一般檔案".into());
    }

    println!(
        "部署模式：{}",
        if is_device {
            "實體磁碟"
        } else {
            "映像檔"
        }
    );
    println!("目標：{}", target.display());

    let kernel = Path::new("target/x86_64-unknown-none/debug/nova_kernel");
    if !kernel.is_file() {
        return Err("找不到核心，請先執行 make rebuild".into());
    }

    let explorer = Path::new("tools/explorer/build/EXPLORER.YEX");
    let temp_part = PathBuf::from("os/nova_boot_part.img");

    if let Some(parent) = temp_part.parent() {
        fs::create_dir_all(parent)?;
    }

    println!("[1/5] 建立 UEFI FAT 開機分區...");

    let builder = bootloader::DiskImageBuilder::new(kernel.to_path_buf());
    builder.create_uefi_fat_partition(&temp_part)?;

    let boot_size = fs::metadata(&temp_part)?.len();
    let boot_sectors = boot_size.div_ceil(SECTOR_SIZE);

    if boot_size == 0 {
        return Err("UEFI 分區映像是空的".into());
    }

    println!("[2/5] 準備目標磁碟...");

    if !is_device {
        if let Some(parent) = target.parent() {
            if !parent.as_os_str().is_empty() {
                fs::create_dir_all(parent)?;
            }
        }

        let image = OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(true)
            .open(target)?;

        image.set_len(TOTAL_IMAGE_SIZE)?;
        image.sync_all()?;
    }

    let disk_size = if is_device {
        command_output(
            "blockdev",
            &["--getsize64", target.to_str().ok_or("無效路徑")?],
        )?
        .parse::<u64>()?
    } else {
        TOTAL_IMAGE_SIZE
    };

    if disk_size % SECTOR_SIZE != 0 {
        return Err("磁碟大小不是 512 bytes 的整數倍".into());
    }

    println!("[3/5] 建立 GPT 雙分區...");

    {
        let mut raw = OpenOptions::new().read(true).write(true).open(target)?;
        let lb_count = u32::try_from(disk_size / SECTOR_SIZE - 1).unwrap_or(0xFFFF_FFFF);
        ProtectiveMBR::with_lb_size(lb_count).overwrite_lba0(&mut raw)?;
        raw.sync_all()?;
    }

    let mut disk = GptConfig::new()
        .writable(true)
        .logical_block_size(LogicalBlockSize::Lb512)
        .create(target)?;

    let first_usable = disk.header().first_usable;
    let last_usable = disk.header().last_usable;

    let efi_end = EFI_START_LBA
        .checked_add(boot_sectors - 1)
        .ok_or("EFI 分區位置溢位")?;

    let data_start = efi_end
        .checked_add(ALIGNMENT_SECTORS - 1)
        .ok_or("資料分區起始位置計算溢位")?
        .checked_div(ALIGNMENT_SECTORS)
        .ok_or("對齊計算錯誤")?
        .checked_mul(ALIGNMENT_SECTORS)
        .ok_or("資料分區對齊計算溢位")?;

    if EFI_START_LBA < first_usable || efi_end > last_usable {
        return Err("EFI 分區超出 GPT 可用範圍".into());
    }

    if data_start > last_usable {
        return Err("磁碟容量不足，無法建立資料分區".into());
    }

    let data_sectors = last_usable - data_start + 1;

    disk.add_partition_at(
        "Yanos_Boot",
        1,
        EFI_START_LBA,
        boot_sectors,
        gpt::partition_types::EFI,
        0,
    )?;

    disk.add_partition_at(
        "Yanos_Storage",
        2,
        data_start,
        data_sectors,
        gpt::partition_types::BASIC,
        0,
    )?;

    let mut device = disk.write()?;
    device.sync_all()?;

    println!("EFI：LBA {} - {}", EFI_START_LBA, efi_end);
    println!("資料：LBA {} - {}", data_start, last_usable);

    println!("[4/5] 寫入 UEFI 開機資料...");

    {
        let mut src = File::open(&temp_part)?;
        let mut dst = OpenOptions::new().read(true).write(true).open(target)?;

        dst.seek(SeekFrom::Start(EFI_START_LBA * SECTOR_SIZE))?;
        io::copy(&mut src, &mut dst)?;
        dst.sync_all()?;
    }

    fs::remove_file(&temp_part)?;

    println!("[5/5] 格式化資料分區並部署 Explorer...");

    let data_offset = data_start * SECTOR_SIZE;
    let data_size = data_sectors * SECTOR_SIZE;

    {
        let slice = PartitionSlice::new(
            OpenOptions::new().read(true).write(true).open(target)?,
            data_offset,
            data_size,
        );

        fatfs::format_volume(slice, FormatVolumeOptions::new())?;
    }

    {
        let slice = PartitionSlice::new(
            OpenOptions::new().read(true).write(true).open(target)?,
            data_offset,
            data_size,
        );

        let filesystem = FileSystem::new(slice, FsOptions::new())?;
        let root = filesystem.root_dir();
        let system_dir = root.create_dir("System")?;

        if explorer.is_file() {
            let mut src = File::open(explorer)?;
            let mut dest = system_dir.create_file("EXPLORER.YEX")?;

            io::copy(&mut src, &mut dest)?;
            dest.flush()?;

            println!("已部署 /System/EXPLORER.YEX");
        } else {
            println!("警告：找不到 {}，略過 Explorer", explorer.display());
        }
    }

    if is_device {
        let _ = Command::new("partprobe").arg(target).status();
        if let Some(s) = target.to_str() {
            let _ = Command::new("blockdev").args(["--rereadpt", s]).status();
        }
    }

    println!();
    println!("🎉 Yanos 部署完成！");
    println!("====================================================");

    Ok(())
}

struct PartitionSlice {
    file: File,
    offset: u64,
    size: u64,
    cursor: u64,
}

impl PartitionSlice {
    fn new(file: File, offset: u64, size: u64) -> Self {
        Self {
            file,
            offset,
            size,
            cursor: 0,
        }
    }
}

impl Read for PartitionSlice {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        if self.cursor >= self.size {
            return Ok(0);
        }

        let count = buf.len().min((self.size - self.cursor) as usize);
        self.file.seek(SeekFrom::Start(self.offset + self.cursor))?;

        let n = self.file.read(&mut buf[..count])?;
        self.cursor += n as u64;
        Ok(n)
    }
}

impl Write for PartitionSlice {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        if self.cursor >= self.size {
            return Err(io::Error::new(io::ErrorKind::WriteZero, "Partition Full"));
        }

        let count = buf.len().min((self.size - self.cursor) as usize);
        self.file.seek(SeekFrom::Start(self.offset + self.cursor))?;

        let n = self.file.write(&buf[..count])?;
        self.cursor += n as u64;
        Ok(n)
    }

    fn flush(&mut self) -> io::Result<()> {
        self.file.flush()
    }
}

impl Seek for PartitionSlice {
    fn seek(&mut self, pos: SeekFrom) -> io::Result<u64> {
        let next = match pos {
            SeekFrom::Start(n) => n as i128,
            SeekFrom::Current(n) => self.cursor as i128 + n as i128,
            SeekFrom::End(n) => self.size as i128 + n as i128,
        };

        if next < 0 || next > self.size as i128 {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "Seek outside partition",
            ));
        }

        self.cursor = next as u64;
        Ok(self.cursor)
    }
}
