use std::fs::{File, OpenOptions};
use std::io::{Read, Write, Seek, SeekFrom};
use std::path::Path;
use std::process::Command;
use std::env;

const TOTAL_IMAGE_SIZE: u64 = 512 * 1024 * 1024; // 總大小 512 MiB

fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("====================================================");
    println!("   🛡️  Yanos 獨立型專業硬碟發行版部署工具 (v2.5)     ");
    println!("====================================================");

    let args: Vec<String> = env::args().collect();
    if args.len() < 2 {
        println!("❌ 錯誤: 請指定目標隨身碟裝置或映像檔路徑！");
        println!("   範例: cargo run -p installer -- os/nova_os.img");
        std::process::exit(1);
    }
    let target_dev = &args;
    println!("🎯 部署目標設備: {}", target_dev);

    let kernel_elf = Path::new("target/x86_64-unknown-none/debug/nova_kernel");
    if !kernel_elf.exists() {
        panic!("❌ 錯誤: 找不到 Yanos 核心編譯產物，請先執行 `make rebuild`！");
    }

    // 1. 先讓 bootloader 生成基礎的核心引導分區包（VBR）
    println!("[1/5] 正在編譯核心 UEFI 系統分區 VBR 數據...");
    let temp_part_path = "os/nova_boot_part.img";
    let disk_builder = bootloader::DiskImageBuilder::new(kernel_elf.to_path_buf());
    disk_builder.create_uefi_image(Path::new(temp_part_path))?; // 取得純 FAT 分區包

    let part1_size_bytes = File::open(temp_part_path)?.metadata()?.len();
    let part1_sectors = ((part1_size_bytes + 511) / 512) as u64;

    // 2. 建立全新的空白外殼檔案並擴展容量
    println!("[2/5] 正在開闢 {}MB 稀疏物理硬碟外殼...", TOTAL_IMAGE_SIZE / 1024 / 1024);
    {
        let final_file = OpenOptions::new().read(true).write(true).create(true).truncate(true).open(target_dev)?;
        final_file.set_len(TOTAL_IMAGE_SIZE)?; 
    }

    // 3. 【神級戰術】：利用 Linux 原生 sfdisk 自動一鍵生成標準 GPT 雙分區
    println!("[3/5] 正在呼叫 Linux sfdisk 工具全新規劃 GPT 雙分區...");
    // 腳本意圖：分區1 大小為核心所需磁區、類型為 EFI (C12A7328-F81F-11D2-BA4B-00A0C93EC93B)
    //          分區2 佔用賸餘全體空間、類型為微軟基本資料 (EBD0A0A2-B9E5-4433-87C0-68B6B72699C7)
    let sfdisk_script = format!(
        "label: gpt\n\
         start=2048, size={}, type=C12A7328-F81F-11D2-BA4B-00A0C93EC93B, name=\"Yanos_Boot\"\n\
         start={}, type=EBD0A0A2-B9E5-4433-87C0-68B6B72699C7, name=\"Yanos_Storage\"\n",
        part1_sectors,
        ((2048 + part1_sectors + 2047) / 2048) * 2048 // 儲存區自動對齊 1MB 邊界
    );

    let mut child = Command::new("sudo")
        .args(["sfdisk", "--force", target_dev])
        .stdin(std::process::Stdio::piped())
        .spawn()?;
    
    if let Some(mut stdin) = child.stdin.take() {
        stdin.write_all(sfdisk_script.as_bytes())?;
    }
    let status = child.wait()?;
    if !status.success() {
        panic!("❌ sfdisk 原生分割失敗！");
    }

    // 4. 【二進位直接灌入】：從 LBA 0 讀回精確的分割元數據，並把引導包灌入 Partition 1
    // 標準對齊下，分區1 的物理起始點就是 LBA 2048
    let part1_start = 2048u64;
    println!("[4/5] 正在將 UEFI 核心資料直接灌入分區 1 物理扇區 (LBA 2048)...");
    let mut boot_buffer = Vec::new();
    File::open(temp_part_path)?.read_to_end(&mut boot_buffer)?;
    
    let mut final_file = OpenOptions::new().read(true).write(true).open(target_dev)?;
    final_file.seek(SeekFrom::Start(part1_start * 512))?; 
    final_file.write_all(&boot_buffer)?;
    let _ = std::fs::remove_file(temp_part_path); // 清除臨時文件

    // 5. 格式化第二分區並建立 /System/EXPLORER.YEX
    // 根據對齊動態解出第二分區起始點
    let part2_start = ((part1_start + part1_sectors + 2047) / 2048) * 2048;
    let total_sectors = TOTAL_IMAGE_SIZE / 512;
    let last_usable_lba = total_sectors - 34;
    let part2_size_bytes = (last_usable_lba - part2_start + 1) * 512;
    let part2_offset_bytes = part2_start * 512;

    println!("[5/5] 正在將第二分區初始化為 FAT32 並建立 /System/ 目錄...");
    {
        let mut part2_handle = PartitionSlice::new(
            OpenOptions::new().read(true).write(true).open(target_dev)?,
            part2_offset_bytes,
            part2_size_bytes
        );
        fatfs::format_volume(&mut part2_handle, fatfs::FormatVolumeOptions::new())?;
    }

    {
        let part2_disk = PartitionSlice::new(
            OpenOptions::new().read(true).write(true).open(target_dev)?,
            part2_offset_bytes,
            part2_size_bytes
        );
        let fs = fatfs::FileSystem::new(part2_disk, fatfs::FsOptions::new())?;
        let root_dir = fs.root_dir();
        let system_dir = root_dir.create_dir("System")?;

        deploy_system_file(&system_dir, "apps/explorer/build/EXPLORER.YEX", "EXPLORER.YEX")?;
    }

    println!("\n🎉 [成功] 專業解耦版隨身碟系統部署完全成功！ (v2.5)");
    println!("====================================================");
    Ok(())
}

fn deploy_system_file(dir: &fatfs::Dir<PartitionSlice>, src_path: &str, dest_name: &str) -> Result<(), Box<dyn std::error::Error>> {
    if !Path::new(src_path).exists() { return Ok(()); }
    let mut src = File::open(src_path)?;
    let mut buffer = Vec::new();
    src.read_to_end(&mut buffer)?;
    let mut dest = dir.create_file(dest_name)?;
    dest.write_all(&buffer)?;
    println!("  -> 成功部署: /System/{}", dest_name);
    Ok(())
}

struct PartitionSlice {
    file: File,
    offset: u64,
    size: u64,
    cursor: u64,
}
impl PartitionSlice {
    fn new(file: File, offset: u64, size: u64) -> Self { Self { file, offset, size, cursor: 0 } }
}
impl std::io::Read for PartitionSlice {
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        let max_read = (self.size - self.cursor) as usize;
        let to_read = buf.len().min(max_read);
        if to_read == 0 { return Ok(0); }
        self.file.seek(SeekFrom::Start(self.offset + self.cursor))?;
        let bytes_read = self.file.read(&mut buf[..to_read])?;
        self.cursor += bytes_read as u64;
        Ok(bytes_read)
    }
}
impl std::io::Write for PartitionSlice {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        let max_write = (self.size - self.cursor) as usize;
        let to_write = buf.len().min(max_write);
        if to_write == 0 { return Err(std::io::Error::new(std::io::ErrorKind::WriteZero, "Partition Full")); }
        self.file.seek(SeekFrom::Start(self.offset + self.cursor))?;
        let bytes_written = self.file.write(&buf[..to_write])?;
        self.cursor += bytes_written as u64;
        Ok(bytes_written)
    }
    fn flush(&mut self) -> std::io::Result<()> { self.file.flush() }
}
impl std::io::Seek for PartitionSlice {
    fn seek(&mut self, pos: SeekFrom) -> std::io::Result<u64> {
        let new_cursor = match pos {
            SeekFrom::Start(offset) => offset as i64,
            SeekFrom::Current(offset) => self.cursor as i64 + offset,
            SeekFrom::End(offset) => self.size as i64 + offset,
        };
        if new_cursor < 0 || new_cursor > self.size as i64 {
            return Err(std::io::Error::new(std::io::ErrorKind::InvalidInput, "Invalid Seek"));
        }
        self.cursor = new_cursor as u64;
        Ok(self.cursor)
    }
}
