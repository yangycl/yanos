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

        // 已經讀完檔案
        if self.position >= self.entry.file_size {
            return 0;
        }


        let cluster =
            match self.location.cluster {

                0 => return 0,

                c => c,
            };


        self.fs.read_cluster(
            cluster,
            buffer,
        );


        let remaining =
            self.entry.file_size - self.position;


        let size =
            if remaining >= 512 {
                512
            } else {
                remaining as usize
            };


        self.position +=
            size as u32;


        let next =
            self.fs.read_fat_entry(cluster);


        if next < 0x0FFFFFF8 {

            self.location.cluster = next;
            self.location.offset = 0;

        } else {

            self.location.cluster = 0;
            self.location.offset = 0;

        }


        size
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

    pub fn append(
        &mut self,
        data: &[u8],
    ) -> Option<usize> {

        if data.is_empty() {
            return Some(0);
        }

        let mut cluster = match self.entry.first_cluster {
            Some(c) => c,
            None => {
                let c = self.fs.allocate_cluster()?;
                self.entry.first_cluster = Some(c);
                self.location.cluster = c;
                c
            }
        };

        let mut last_cluster = cluster;
        let mut offset_in_last = (self.entry.file_size % 512) as usize;

        if self.entry.file_size > 0 {
            loop {
                let next = self.fs.read_fat_entry(last_cluster);
                if next >= 0x0FFFFFF8 {
                    break;
                }
                last_cluster = next;
            }
            cluster = last_cluster;
        }

        let mut written = 0;
        let mut remaining = data.len();
        let mut source_offset = 0;

        if self.entry.file_size == 0 || offset_in_last != 0 {
            let mut buffer = [0u8; 512];
            if offset_in_last != 0 {
                self.fs.read_cluster(cluster, &mut buffer);
            }

            let write_count = if remaining < 512 - offset_in_last {
                remaining
            } else {
                512 - offset_in_last
            };

            buffer[offset_in_last..offset_in_last + write_count]
                .copy_from_slice(&data[source_offset..source_offset + write_count]);

            self.fs.write_cluster(cluster, &buffer);

            written += write_count;
            remaining -= write_count;
            source_offset += write_count;
        }

        while remaining > 0 {
            let next_cluster = self.fs.append_cluster(cluster)?;
            cluster = next_cluster;

            let mut buffer = [0u8; 512];
            let write_count = if remaining < 512 {
                remaining
            } else {
                512
            };

            buffer[..write_count]
                .copy_from_slice(&data[source_offset..source_offset + write_count]);

            self.fs.write_cluster(cluster, &buffer);

            written += write_count;
            remaining -= write_count;
            source_offset += write_count;
        }

        self.entry.file_size += written as u32;
        self.position = self.entry.file_size;
        self.location.cluster = cluster;
        self.location.offset = (self.entry.file_size % 512) as usize;

        Some(written)
    }

}


