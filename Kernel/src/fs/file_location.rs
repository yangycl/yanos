#[derive(Debug, Clone, Copy)]
pub struct FileLocation {

    pub cluster: u32,
    
    pub offset: usize,
}

impl FileLocation {

    pub fn new(
        cluster: u32,
        offset: usize,
    ) -> Self {

        Self {
            cluster,
            offset,
        }
    }
}