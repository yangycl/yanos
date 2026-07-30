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
    
}


