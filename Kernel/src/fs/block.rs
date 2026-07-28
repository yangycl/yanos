pub trait BlockDevice {

    fn read_sector(
        &mut self,
        lba: u64,
        buffer: &mut [u8;512],
    );


    fn write_sector(
        &mut self,
        lba: u64,
        buffer: &[u8;512],
    );

}