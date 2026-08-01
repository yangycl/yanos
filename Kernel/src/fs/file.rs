use crate::fs;
use crate::fs::block::BlockDevice;
use crate::fs::dir_entry::DirectoryEntry;
use crate::fs::fat32::Fat32;


pub struct File<'a, D: BlockDevice> {

    pub fs:
        &'a mut Fat32<D>,

    pub entry:
        DirectoryEntry,

    pub position:
        u32,
}



impl<'a, D: BlockDevice> File<'a, D> {



    pub fn read(
        &mut self,
        buffer: &mut [u8;512],
    )
    -> usize {


        let cluster =
            match self.entry.first_cluster {

                Some(c) => c,

                None => {
                    return 0;
                }

            };


        let mut current_cluster =
            cluster;


        // 目前在第幾個 cluster
        let mut skip =
            self.position / 512;


        // 沿 FAT chain 找位置
        while skip > 0 {


            let next =
                self.fs.read_fat_entry(
                    current_cluster
                );


            // EOF
            if next >= 0x0FFFFFF8 {

                return 0;

            }


            current_cluster =
                next;


            skip -= 1;

        }



        // 讀 cluster
        self.fs.read_cluster(
            current_cluster,
            buffer,
        );


        self.position += 512;


        512
    }

    pub fn new(
        fs: &'a mut Fat32<D>,
        entry: DirectoryEntry,
    ) -> Self {

        Self {
            fs,
            entry,
            position: 0,
        }
    }


    pub fn size(&self) -> u32 {
        self.entry.file_size
    }


    pub fn position(&self) -> u32 {
        self.position
    }

    pub fn write(
        &mut self,
        data: &[u8],
    ) -> Option<usize> {


        if data.len() > 512 {

            return None;

        }


        let cluster =
            match self.entry.first_cluster {


                Some(c) => c,


                None => {

                    let c =
                        self.fs.allocate_cluster()?;


                    self.entry.first_cluster =
                        Some(c);


                    c

                }

            };


        let mut buffer =
            [0u8;512];


        buffer[..data.len()]
            .copy_from_slice(data);


        self.fs.write_cluster(
            cluster,
            &buffer,
        );


        self.entry.file_size =
            data.len() as u32;


        self.position +=
            data.len() as u32;


        Some(
            data.len()
        )
    }

    pub fn open (fs: &'a mut Fat32<D>, root_cluster: u32, name: &str) -> Option<Self> {

        let entry =
            fs.resolve_path(
                root_cluster,
                name,
            )?;
        return Some(
            Self {
                fs,
                entry,
                position: 0,
            }
        );
        
    }

}


