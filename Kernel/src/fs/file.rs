use crate::fs;
use crate::fs::block::BlockDevice;
use crate::fs::dir_entry::DirectoryEntry;
use crate::fs::fat32::Fat32;
use crate::fs::file_location::FileLocation;

pub struct File<'a, D: BlockDevice> {

    pub fs:
        &'a mut Fat32<D>,

    pub entry:
        DirectoryEntry,

    pub position:
        u32,
    
    pub location:
        fs::file_location::FileLocation,
}



impl<'a, D: BlockDevice> File<'a, D> {



    pub fn read(
        &mut self,
        buffer: &mut [u8;512],
    ) -> usize {

        let cluster =
            match self.location.cluster {

                0 => return 0,

                c => c,
            };


        self.fs.read_cluster(
            cluster,
            buffer,
        );


        self.position += 512;


        let next =
            self.fs.read_fat_entry(cluster);


        if next < 0x0FFFFFF8 {

            self.location.cluster = next;
            self.location.offset = 0;

        } else {

            self.location.offset = 512;

        }


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
            location:  FileLocation::new(entry.first_cluster.unwrap_or(0), 0),
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
                location:  FileLocation::new(entry.first_cluster.unwrap_or(0), 0),
            }
        );
        
    }

}


