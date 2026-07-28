pub struct DirectoryEntry {

    // 8+3 格式
    pub name: [u8;11],

    // 檔案屬性
    pub attr: u8,

    // 檔案大小
    pub file_size: u32,
}