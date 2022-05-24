use std::io;

use crate::bitmap::Bitmap;
use crate::cluster::Cluster;
use crate::memory::Memory;
use crate::serde::{Deserialize, Serialize};

pub struct FileSystem<M: Memory> {
    bitmap: Bitmap,
    root_cluster: Cluster,
    memory: M,
}

impl<M: Memory> FileSystem<M> {
    const PREAMBLE_BLOCKS: usize = 32;

    pub fn new(memory: M) -> Self {
        let mut bitmap = Bitmap::new::<M>();
        for i in 0..Self::PREAMBLE_BLOCKS {
            bitmap.occupy(i);
        }
        Self {
            bitmap,
            root_cluster: Cluster::default(),
            memory,
        }
    }

    pub fn open(memory: M) -> io::Result<Self> {
        let mut bitmap = Bitmap::new::<M>();
        let mut root_cluster = Cluster::default();
        {
            let mut r = memory.reader();
            bitmap.deserialize(&mut r)?;
            root_cluster.deserialize(&mut r)?;
        }
        Ok(Self {
            bitmap,
            root_cluster,
            memory,
        })
    }

    pub fn write_into_cluster<'a>(
        &'a mut self,
        cluster: &'a mut Cluster,
    ) -> impl 'a + io::Write + io::Seek {
        cluster.writer(&mut self.bitmap, self.memory.writer())
    }

    pub fn write_into_root_cluster(&mut self) -> impl '_ + io::Write + io::Seek {
        self.root_cluster
            .writer(&mut self.bitmap, self.memory.writer())
    }

    pub fn read_from_cluster<'a>(&'a self, cluster: &'a Cluster) -> impl 'a + io::Read + io::Seek {
        cluster.reader(self.memory.reader())
    }

    pub fn read_from_root_cluster(&self) -> impl '_ + io::Read + io::Seek {
        self.root_cluster.reader(self.memory.reader())
    }
}

impl<M: Memory> Serialize for FileSystem<M> {
    fn serialize(&self, mut w: impl io::Write) -> io::Result<usize> {
        let mut bytes_written = self.bitmap.serialize(&mut w)?;
        bytes_written += self.root_cluster.serialize(w)?;
        Ok(bytes_written)
    }
}

impl<M: Memory> Deserialize for FileSystem<M> {
    fn deserialize(&mut self, mut r: impl io::Read) -> io::Result<usize> {
        let mut bytes_read = self.bitmap.deserialize(&mut r)?;
        bytes_read += self.root_cluster.deserialize(r)?;
        Ok(bytes_read)
    }
}

impl<M: Memory> Drop for FileSystem<M> {
    fn drop(&mut self) {
        let mut w = self.memory.writer();
        self.bitmap
            .serialize(&mut w)
            .expect("failed to write filesystem preamble");
        self.root_cluster
            .serialize(w)
            .expect("failed to write filesystem preamble");
    }
}

#[test]
fn test() {
    use crate::bitmap::BitState;
    use crate::block::Block;
    use crate::heap_memory::HeapMemory;
    use std::io::{Read, Write};

    const DATA_BLOCKS: usize = 128;

    let data: Vec<u8> = (0..Block::SIZE * DATA_BLOCKS)
        .map(|_| rand::random())
        .collect();

    let mut memory = HeapMemory::default();

    {
        let mut fs = FileSystem::new(&mut memory);

        fs.bitmap.occupy(42);
        fs.bitmap.occupy(39);
        fs.bitmap.occupy(58);

        {
            let mut writer = fs.write_into_root_cluster();
            writer.write_all(&data).unwrap();
        }

        {
            let mut reader = fs.read_from_root_cluster();
            let mut read_data = vec![];
            reader.read_to_end(&mut read_data).unwrap();
            assert_eq!(read_data, data);
        }

        assert_eq!(
            fs.bitmap
                .iter()
                .filter(|s| s == &BitState::Occupied)
                .count(),
            FileSystem::<HeapMemory>::PREAMBLE_BLOCKS + DATA_BLOCKS + 3
        );
    }

    {
        let fs = FileSystem::open(memory).unwrap();
        let mut reader = fs.read_from_root_cluster();

        let mut read_data = vec![];
        reader.read_to_end(&mut read_data).unwrap();
        assert_eq!(read_data, data);
    }
}
