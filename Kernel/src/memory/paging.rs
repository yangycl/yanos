use x86_64::VirtAddr;


pub struct MemoryManager {

    pub physical_memory_offset: VirtAddr,

}


impl MemoryManager {


    pub fn new(offset:u64)->Self{

        Self{

            physical_memory_offset:
                VirtAddr::new(offset),

        }

    }



    pub fn physical_to_virtual(
        &self,
        physical:u64
    )->VirtAddr{


        self.physical_memory_offset
            + physical

    }

}