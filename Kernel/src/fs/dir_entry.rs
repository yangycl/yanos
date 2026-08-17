use super::fat32::Fat32;


#[derive(Clone, Copy, Default)]
pub struct DirectoryEntry {

    // 8+3 格式
    pub name: [u8;11],

    // 檔案屬性
    pub attr: u8,

    pub first_cluster: Option<u32>,

    // 檔案大小
    pub file_size: u32,
}

impl DirectoryEntry {

    pub fn format_short_name(
        name: &str,
    ) -> [u8; 11] {

        let mut short = [b' '; 11];

        let bytes = name.as_bytes();

        let mut i = 0;
        let mut j = 0;

        // 檔名 (最多 8 字元)
        while i < bytes.len() && bytes[i] != b'.' && j < 8 {

            short[j] = bytes[i].to_ascii_uppercase();

            i += 1;
            j += 1;

        }

        // 跳過 '.'
        if i < bytes.len() && bytes[i] == b'.' {

            i += 1;

        }

        // 副檔名 (最多 3 字元)
        j = 8;

        while i < bytes.len() && j < 11 {

            short[j] = bytes[i].to_ascii_uppercase();

            i += 1;
            j += 1;

        }

        short

    }   


    pub fn to_bytes(
        &self,
    ) -> [u8; 32] {

        let mut bytes = [0u8; 32];

        bytes[0..11]
            .copy_from_slice(&self.name);

        bytes[11] = self.attr;

        let cluster =
            self.first_cluster.unwrap_or(0);

        bytes[20..22]
            .copy_from_slice(
                &((cluster >> 16) as u16).to_le_bytes()
            );

        bytes[26..28]
            .copy_from_slice(
                &(cluster as u16).to_le_bytes()
            );

        bytes[28..32]
            .copy_from_slice(
                &self.file_size.to_le_bytes()
            );

        bytes
    }
       
}


