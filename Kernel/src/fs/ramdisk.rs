use super::block::BlockDevice;

pub struct RamDisk<'a> {
    data: &'a mut [u8],
}

impl<'a> RamDisk<'a> {

    pub fn new(data: &'a mut [u8]) -> Self {
        Self {
            data,
        }
    }

}

impl<'a> BlockDevice for RamDisk<'a> {

    fn read_sector(
        &mut self,
        lba: u64,
        buffer: &mut [u8; 512],
    ) {

        let start = lba as usize * 512;
        let end = start + 512;

        buffer.copy_from_slice(
            &self.data[start..end]
        );

    }

    fn write_sector(
        &mut self,
        lba: u64,
        buffer: &[u8; 512],
    ) {

        let start = lba as usize * 512;
        let end = start + 512;

        self.data[start..end]
            .copy_from_slice(buffer);

    }

}