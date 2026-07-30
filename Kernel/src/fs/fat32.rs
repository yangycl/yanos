use core::result;

use super::block::BlockDevice;
use crate::fs::directory::DirectoryEntry;


pub struct Fat32<D: BlockDevice> {

    device: D,

    pub bytes_per_sector: u16,
    pub sectors_per_cluster: u8,

    pub reserved_sectors: u16,
    pub fat_count: u8,

    pub fat_size: u32,

    pub root_cluster: u32,

    pub fat_start: u32,
    pub data_start: u32,
    

}



impl<D: BlockDevice> Fat32<D> {

    fn cluster_to_sector(
        &self,
        cluster: u32
    ) -> u32 {

        self.data_start
            +
        (cluster - 2)
            *
        self.sectors_per_cluster as u32

    }


    pub fn mount(
        mut device: D
    ) -> Self {


        let mut sector = [0u8;512];



        device.read_sector(
            0,
            &mut sector
        );


        // Boot signature
        if sector[510] != 0x55 ||
        sector[511] != 0xAA {

            panic!("Not FAT boot sector");

        }


        // BPB

        let bytes_per_sector =
            u16::from_le_bytes([
                sector[11],
                sector[12],
            ]);


        let sectors_per_cluster =
            sector[13];


        let reserved_sectors =
            u16::from_le_bytes([
                sector[14],
                sector[15],
            ]);


        let fat_count =
            sector[16];


        let fat_size =
            u32::from_le_bytes([
                sector[36],
                sector[37],
                sector[38],
                sector[39],
            ]);


        let root_cluster =
            u32::from_le_bytes([
                sector[44],
                sector[45],
                sector[46],
                sector[47],
            ]);



        let fat_start =
            reserved_sectors as u32;


        let data_start =
            fat_start
            + (fat_count as u32 * fat_size);



        Self {

            device,

            bytes_per_sector,

            sectors_per_cluster,

            reserved_sectors,

            fat_count,

            fat_size,

            root_cluster,

            fat_start,

            data_start,

        }

    }


    pub fn read_fat_entry(
        &mut self,
        cluster: u32,
    ) -> u32 {

        // FAT32 每個 entry 4 bytes
        let offset =
            cluster * 4;


        // FAT 表中的第幾個 sector
        let sector =
            self.fat_start
            + offset / self.bytes_per_sector as u32;


        // sector 裡面的 byte 位置
        let index =
            offset % self.bytes_per_sector as u32;


        let mut buffer = [0u8;512];


        // 讀 FAT 所在的 sector
        self.device.read_sector(
            sector as u64,
            &mut buffer
        );


        // 取出 FAT entry 的 4 bytes
        let value =
            u32::from_le_bytes([
                buffer[index as usize],
                buffer[index as usize + 1],
                buffer[index as usize + 2],
                buffer[index as usize + 3],
            ]);


        // FAT32 只使用低 28 bits
        value & 0x0FFFFFFF
    }

    pub fn read_cluster(
        &mut self,
        cluster: u32,
        buffer: &mut [u8;512],
    ) {

        let sector =
            self.data_start
            + ((cluster - 2)
            * self.sectors_per_cluster as u32);


        self.device.read_sector(
            sector as u64,
            buffer
        );
    }


    pub fn write_cluster(
        &mut self,
        cluster: u32,
        buffer: &[u8;512],
    ) {

        let sector =
            self.data_start
            + ((cluster - 2)
            * self.sectors_per_cluster as u32);


        self.device.write_sector(
            sector as u64,
            buffer
        );
    }
    pub fn write_fat_entry(
        &mut self,
        cluster: u32,
        value: u32,
    ) {

        // FAT32 一個 entry = 4 bytes
        let offset =
            cluster * 4;


        // FAT 裡的 sector
        let sector =
            self.fat_start
            + offset / self.bytes_per_sector as u32;


        // sector 內 offset
        let index =
            offset % self.bytes_per_sector as u32;


        let mut buffer = [0u8;512];


        // 讀整個 FAT sector
        self.device.read_sector(
            sector as u64,
            &mut buffer
        );


        // FAT32 只用低 28 bits
        let value =
            value & 0x0FFFFFFF;


        // 寫入 4 bytes
        buffer[index as usize..index as usize + 4]
            .copy_from_slice(
                &value.to_le_bytes()
            );


        // 寫回 FAT sector
        self.device.write_sector(
            sector as u64,
            &buffer
        );
    }

    pub fn find_free_cluster(
        &mut self
    ) -> Option<u32> {

        // FAT32 cluster 通常從 2 開始
        let mut cluster = 2;


        loop {

            let value =
                self.read_fat_entry(cluster);


            // 0 代表沒有使用
            if value == 0 {

                return Some(cluster);

            }


            cluster += 1;


            // 防止無限掃描
            // 之後可以改成根據容量計算
            if cluster > 0x0FFFFFFF {

                break;
            }
        }


        None
    }

    pub fn allocate_cluster(
        &mut self
    ) -> Option<u32> {

        let cluster =
            self.find_free_cluster()?;


        // 標記這個 cluster 為檔案最後一塊
        self.write_fat_entry(
            cluster,
            0x0FFFFFFF
        );


        Some(cluster)
    }

    pub fn append_cluster(
        &mut self,
        last_cluster: u32,
    ) -> Option<u32> {

        // 找新的空 cluster
        let new_cluster =
            self.allocate_cluster()?;


        // 原本最後一塊指向新的一塊
        self.write_fat_entry(
            last_cluster,
            new_cluster
        );


        Some(new_cluster)
    }

    pub fn extend_file(
        &mut self,
        start_cluster: u32,
        count: usize,
    ) -> Option<u32> {

        let mut last =
            start_cluster;


        for _ in 0..count {

            last =
                self.append_cluster(last)?;
        }


        Some(last)
    }
}