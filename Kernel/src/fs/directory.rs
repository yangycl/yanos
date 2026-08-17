use super::fat32::Fat32;
pub use super::dir_entry::DirectoryEntry;
use crate::fs::block::BlockDevice;
use crate::fs::file_location::FileLocation;

impl <D: BlockDevice> Fat32 <D>{


    pub fn serialize_dir_entry(
        &self,
        entry: &DirectoryEntry,
    ) -> [u8; 32] {

        let mut data = [0u8; 32];


        // 8.3 檔名
        data[0..11]
            .copy_from_slice(&entry.name);


        // attribute
        data[11] =
            entry.attr;


        // cluster
        if let Some(cluster) = entry.first_cluster {

            let high =
                ((cluster >> 16) as u16)
                .to_le_bytes();

            let low =
                (cluster as u16)
                .to_le_bytes();


            data[20..22]
                .copy_from_slice(&high);

            data[26..28]
                .copy_from_slice(&low);
        }


        // file size
        data[28..32]
            .copy_from_slice(
                &entry.file_size.to_le_bytes()
            );


        data
    }

    pub fn create_entry(
        &mut self,
        directory_cluster: u32,
        entry: &DirectoryEntry,
    ) -> Option<u32> {

        // 自動配置第一個 cluster
        let first_cluster =
            self.allocate_cluster()?;


        // 讀整個目錄
        let mut buffer = [0u8; 512];

        self.read_cluster(
            directory_cluster,
            &mut buffer,
        );


        // 找空的 Directory Entry
        let mut offset = 0;

        while offset < 512 {

            // 0x00 = 後面都沒有 Entry
            // 0xE5 = 已刪除 Entry
            if buffer[offset] == 0x00 ||
            buffer[offset] == 0xE5 {

                let raw =
                    self.serialize_dir_entry(
                        &DirectoryEntry {
                            first_cluster: Some(first_cluster),
                            name: entry.name,
                            attr: entry.attr,
                            file_size: entry.file_size,
                        }
                    );


                buffer[offset..offset + 32]
                    .copy_from_slice(&raw);


                self.write_cluster(
                    directory_cluster,
                    &buffer,
                );


                return Some(first_cluster);
            }            offset += 32;
        }

        None
    }

    pub fn read_directory(
        &mut self,
        start_cluster: u32,
        output: &mut [DirectoryEntry],
    ) -> usize {

        let mut count = 0;

        let mut cluster =
            start_cluster;

        loop {

            let mut buffer =
                [0u8; 512];

            self.read_cluster(
                cluster,
                &mut buffer,
            );

            for offset in
                (0..512).step_by(32)
            {

                // 後面沒有 Entry
                if buffer[offset] == 0x00 {

                    return count;

                }

                // 已刪除
                if buffer[offset] == 0xE5 {

                    continue;

                }

                if count == output.len() {

                    return count;

                }

                let raw:
                    &[u8;32] =
                    buffer[offset
                        ..offset + 32]
                        .try_into()
                        .unwrap();

                output[count] =
                    self.read_dir_struct(raw);

                count += 1;

            }

            let next =
                self.read_fat_entry(
                    cluster
                );

            // FAT32 End Of Chain
            if next >= 0x0FFFFFF8 {

                break;

            }

            cluster = next;

        }

        count

    }

    pub fn find_entry ( &mut self, directory_cluster: u32, name: &[u8;11]) -> Option<DirectoryEntry> {
        let mut entries = [DirectoryEntry {
            name: [0u8; 11],
            attr: 0,
            first_cluster: None,
            file_size: 0,
        }; 30]; // 假設最多有 30 個目錄項

        let count = self.read_directory(directory_cluster, &mut entries);

        for i in 0..count {
            if &entries[i].name == name {
                return Some(entries[i].clone());
            }
        }

        None
    }
    pub fn read_dir_struct(
        &self,
        data: &[u8; 32],
    ) -> DirectoryEntry {

        let high =
            u16::from_le_bytes([
                data[20],
                data[21],
            ]) as u32;

        let low =
            u16::from_le_bytes([
                data[26],
                data[27],
            ]) as u32;

        let cluster =
            (high << 16) | low;

        DirectoryEntry {

            name: data[0..11]
                .try_into()
                .unwrap(),

            attr: data[11],

            first_cluster:
                if cluster == 0 {
                    None
                } else {
                    Some(cluster)
                },

            file_size:
                u32::from_le_bytes([
                    data[28],
                    data[29],
                    data[30],
                    data[31],
                ]),

        }

    }

    pub fn create_directory(
        &mut self,
        parent_cluster: u32,
        name: [u8;11],
    ) -> Option<u32> {

        // 1. 配置新的 cluster 給資料夾
        let new_cluster =
            self.allocate_cluster()?;


        // 2. 清空新的資料夾 cluster

        let empty =
            [0u8;512];

        self.write_cluster(
            new_cluster,
            &empty,
        );


        // 3. 建立 "." entry

        let dot = DirectoryEntry {

            name: [
                b'.', b' ', b' ', b' ',
                b' ', b' ', b' ', b' ',
                b' ', b' ', b' ',
            ],

            attr: 0x10, // directory

            first_cluster:
                Some(new_cluster),

            file_size: 0,

        };


        // 4. 建立 ".." entry

        let dot_dot = DirectoryEntry {

            name: [
                b'.', b'.', b' ', b' ',
                b' ', b' ', b' ', b' ',
                b' ', b' ', b' ',
            ],

            attr: 0x10,

            first_cluster:
                Some(parent_cluster),

            file_size: 0,

        };


        // 5. 寫入新的 directory cluster

        let mut buffer =
            [0u8;512];


        self.read_cluster(
            new_cluster,
            &mut buffer,
        );


        // 第一個 entry "."
        buffer[0..32]
            .copy_from_slice(
                &self.serialize_dir_entry(&dot)
            );


        // 第二個 entry ".."
        buffer[32..64]
            .copy_from_slice(
                &self.serialize_dir_entry(&dot_dot)
            );


        self.write_cluster(
            new_cluster,
            &buffer,
        );


        // 6. 在父目錄建立 entry

        let entry = DirectoryEntry {

            name,

            attr: 0x10,

            first_cluster:
                Some(new_cluster),

            file_size: 0,

        };


        self.create_entry(
            parent_cluster,
            &entry,
        )?;


        Some(new_cluster)

    }

    pub fn resolve_path(
        &mut self,
        root_cluster: u32,
        path: &str,
    ) -> Option<DirectoryEntry> {

        let mut current_cluster = root_cluster;

        let mut components = 
            path.split('/').filter(|x| !x.is_empty()).peekable();


        while let Some(name) = components.next() {

            let short_name =
                DirectoryEntry::format_short_name(name);


            let entry =
                self.find_entry(
                    current_cluster,
                    &short_name,
                )?;


            // 還有下一層
            if components.peek().is_some() {

                current_cluster =
                    entry.first_cluster?;

            } 
            // 最後一層
            else {

                return Some(entry);

            }
        }


        None
    }
    pub fn find_entry_with_location(
        &mut self,
        directory_cluster: u32,
        name: &[u8; 11],
    ) -> Option<(DirectoryEntry, FileLocation)> {

        let mut entries = [DirectoryEntry {
            name: [0u8; 11],
            attr: 0,
            first_cluster: None,
            file_size: 0,
        }; 30];

        let count = self.read_directory(directory_cluster, &mut entries);

        for i in 0..count {
            if &entries[i].name == name {

                let location = FileLocation::new(
                    directory_cluster,
                    i * 32,
                );

                return Some((
                    entries[i].clone(),
                    location,
                ));
            }
        }

        None
    }
    pub fn write_dir_entry(
        &mut self,
        location: FileLocation,
        entry: &DirectoryEntry,
    ) -> Option<()> {

        let mut buffer = [0u8; 512];

        self.read_cluster(
            location.cluster,
            &mut buffer,
        );

        // Serialize the directory entry into the buffer
        let entry_bytes = entry.to_bytes();
        buffer[location.offset..location.offset + 32].copy_from_slice(&entry_bytes);

        // Write the modified buffer back to the cluster
        self.write_cluster(
            location.cluster,
            &buffer,
        );

        Some(())
    }
}