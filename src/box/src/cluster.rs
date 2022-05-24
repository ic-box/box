use std::io;
use std::ops::RangeInclusive;

use crate::bitmap::Bitmap;
use crate::block::Block;
use crate::serde::{Deserialize, Serialize};

#[derive(Default, Debug, PartialEq)]
pub struct Cluster {
    blocks: Vec<Block>,
}

impl Cluster {
    pub fn extend(&mut self, block: Block) {
        self.blocks.push(block);
    }

    pub fn blocks(&self) -> impl '_ + Iterator<Item = &Block> {
        self.blocks.iter()
    }

    pub fn reader<'a, R>(&'a self, reader: R) -> ClusterReader<'a, R> {
        ClusterReader {
            cluster: self,
            reader,
            cluster_block_index: 0,
            block_offset: 0,
        }
    }

    pub fn writer<'a, W>(&'a mut self, bitmap: &'a mut Bitmap, writer: W) -> ClusterWriter<'a, W> {
        ClusterWriter {
            cluster: self,
            bitmap,
            writer,
            cluster_block_index: 0,
            block_offset: 0,
        }
    }

    pub fn len(&self) -> usize {
        Block::SIZE * self.blocks.len()
    }
}

impl Serialize for Cluster {
    fn serialize(&self, mut w: impl io::Write) -> io::Result<usize> {
        let mut ranges: Vec<RangeInclusive<Block>> = vec![];

        for block in self.blocks.iter() {
            if let Some(last) = ranges.last_mut() {
                if *last.end() + 1 == *block {
                    *last = *last.start()..=*block;
                    continue;
                }
            }
            ranges.push(*block..=*block);
        }

        let mut bytes_written = 0;
        let mut write = |buf: u32| -> io::Result<()> {
            w.write_all(&buf.to_be_bytes())?;
            bytes_written += std::mem::size_of::<u32>();
            Ok(())
        };
        write(ranges.len() as _)?;

        for range in ranges {
            let start = range.start().index;
            let len = range.end().index - start + 1;
            if len == 1 {
                write(start as _)?;
            } else {
                write(start as u32 | (1 << 31))?;
                write(len as _)?;
            }
        }

        Ok(bytes_written)
    }
}

impl Deserialize for Cluster {
    fn deserialize(&mut self, mut r: impl io::Read) -> io::Result<usize> {
        let mut bytes_read = 0;
        let mut read = || -> io::Result<u32> {
            let mut buf = [0u8; 4];
            r.read_exact(&mut buf)?;
            bytes_read += 4;
            Ok(u32::from_be_bytes(buf))
        };

        let len = read()?;
        for _ in 0..len {
            let mut index = read()?;
            if (index & (1 << 31)) >> 31 == 1 {
                index &= !(1 << 31);
                let range_len = read()?;
                for i in index..index + range_len {
                    self.blocks.push(Block::at(i as _));
                }
            } else {
                self.blocks.push(Block::at(index as _));
            }
        }

        Ok(bytes_read)
    }
}

pub struct ClusterReader<'a, R> {
    cluster: &'a Cluster,
    reader: R,
    cluster_block_index: usize,
    block_offset: usize,
}

impl<'a, R> io::Read for ClusterReader<'a, R>
where
    R: io::Read + io::Seek,
{
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        if self.cluster_block_index >= self.cluster.blocks.len() {
            return Ok(0);
        }

        let block = &self.cluster.blocks[self.cluster_block_index];
        self.reader.seek(io::SeekFrom::Start(
            (block.index * Block::SIZE + self.block_offset) as _,
        ))?;

        let max_read = buf.len().min(Block::SIZE - self.block_offset);

        let read_bytes = self.reader.read(&mut buf[..max_read])?;

        self.block_offset += read_bytes;

        if self.block_offset >= Block::SIZE {
            self.cluster_block_index += 1;
            self.block_offset = 0;
        }

        Ok(read_bytes)
    }
}

impl<'a, R> io::Seek for ClusterReader<'a, R> {
    fn seek(&mut self, pos: io::SeekFrom) -> io::Result<u64> {
        let new_offset = match pos {
            io::SeekFrom::Start(offset) => offset,
            io::SeekFrom::Current(offset) => {
                ((self.cluster_block_index * Block::SIZE + self.block_offset) as i64 + offset)
                    as u64
            }
            io::SeekFrom::End(offset) => (self.cluster.len() as i64 + offset) as u64,
        };

        {
            let new_offset = new_offset as usize;
            self.cluster_block_index = new_offset / Block::SIZE;
            self.block_offset = new_offset % Block::SIZE;
        }

        Ok(new_offset)
    }
}

pub struct ClusterWriter<'a, W> {
    cluster: &'a mut Cluster,
    writer: W,
    bitmap: &'a mut Bitmap,
    cluster_block_index: usize,
    block_offset: usize,
}

impl<'a, W> io::Write for ClusterWriter<'a, W>
where
    W: io::Write + io::Seek,
{
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        while self.cluster_block_index >= self.cluster.blocks.len() {
            let block = self
                .bitmap
                .occupy_next()
                .map(Block::at)
                .ok_or_else(|| io::ErrorKind::OutOfMemory)?;
            self.cluster.extend(block);
        }

        let block = &self.cluster.blocks[self.cluster_block_index];
        self.writer.seek(io::SeekFrom::Start(
            (block.index * Block::SIZE + self.block_offset) as _,
        ))?;

        let max_write = buf.len().min(Block::SIZE - self.block_offset);

        let written_bytes = self.writer.write(&buf[..max_write])?;

        self.block_offset += written_bytes;

        if self.block_offset >= Block::SIZE {
            self.cluster_block_index += 1;
            self.block_offset = 0;
        }

        Ok(written_bytes)
    }

    fn flush(&mut self) -> io::Result<()> {
        self.writer.flush()
    }
}

impl<'a, W> io::Seek for ClusterWriter<'a, W> {
    fn seek(&mut self, pos: io::SeekFrom) -> io::Result<u64> {
        let new_offset = match pos {
            io::SeekFrom::Start(offset) => offset,
            io::SeekFrom::Current(offset) => {
                ((self.cluster_block_index * Block::SIZE + self.block_offset) as i64 + offset)
                    as u64
            }
            io::SeekFrom::End(offset) => (self.cluster.len() as i64 + offset) as u64,
        };

        {
            let new_offset = new_offset as usize;
            self.cluster_block_index = new_offset / Block::SIZE;
            self.block_offset = new_offset % Block::SIZE;
        }

        Ok(new_offset)
    }
}

#[test]
fn reader() {
    use crate::heap_memory::HeapMemory;
    use crate::memory::Memory;
    use std::io::{Read, Seek, Write};

    let mut heap = HeapMemory::default();

    {
        let mut w = heap.writer();
        w.write(b"FIRST BLOCK START").unwrap();
        w.seek(io::SeekFrom::Start((Block::SIZE * 2) as u64))
            .unwrap();
        w.write(b"THIRD BLOCK START").unwrap();
    }

    let mut cluster = Cluster::default();
    cluster.extend(Block::at(2));
    cluster.extend(Block::at(0));

    let mut data = [0u8; Block::SIZE * 2];

    let mut r = cluster.reader(heap.reader());
    r.read_exact(&mut data).unwrap();

    assert_eq!(&data[..17], b"THIRD BLOCK START");
    assert_eq!(&data[Block::SIZE..Block::SIZE + 17], b"FIRST BLOCK START");
}

#[test]
fn writer() {
    use crate::bitmap::BitState;
    use crate::heap_memory::HeapMemory;
    use crate::memory::Memory;
    use std::io::{Read, Seek, Write};

    let mut heap = HeapMemory::default();
    let mut bitmap = Bitmap::new::<HeapMemory>();
    let mut cluster = Cluster::default();

    {
        bitmap.occupy(0);
        cluster.extend(Block::at(0));

        bitmap.occupy(2);
        cluster.extend(Block::at(2));

        let mut writer = cluster.writer(&mut bitmap, heap.writer());
        writer
            .seek(io::SeekFrom::Start((Block::SIZE * 2 - 1) as _))
            .unwrap();

        // This will write "H" at the end of the second block with index 2
        // and then add a block at index 1 and write "ello World!" there.
        writer.write_all(b"Hello World!").unwrap();
    }

    assert_eq!(bitmap[1], BitState::Occupied);
    assert_eq!(
        cluster.blocks,
        vec![Block::at(0), Block::at(2), Block::at(1)]
    );

    let mut reader = heap.reader();
    reader
        .seek(io::SeekFrom::Start((Block::SIZE * 3 - 1) as _))
        .unwrap();

    let mut last_char = [0u8; 1];
    reader.read_exact(&mut last_char).unwrap();

    assert_eq!(&last_char, b"H");

    reader.seek(io::SeekFrom::Start(Block::SIZE as _)).unwrap();

    let mut first_chars = [0u8; b"ello World!".len()];
    reader.read_exact(&mut first_chars).unwrap();

    assert_eq!(&first_chars, b"ello World!");
}

#[test]
fn serde() {
    let mut cluster = Cluster::default();
    // Range 1   (1 -> 2)
    cluster.extend(Block::at(1));
    cluster.extend(Block::at(2));
    cluster.extend(Block::at(3));
    // Range 2   (5)
    cluster.extend(Block::at(5));

    let mut data = vec![];
    cluster.serialize(&mut data).unwrap();

    #[rustfmt::skip]
    assert_eq!(&data, &vec![
        //                                                 👇 marks number of ranges (2)
        0b0000_0000, 0b0000_0000, 0b0000_0000, 0b0000_000__10__,

        // Range 1
        // 👇 marks range > 1 indices                         👇 marks start index (1)
        0b__1__000_0000, 0b0000_0000, 0b0000_0000, 0b0000_000__1__,
        //                                                    👇 marks length of range (3)
        0b____0000_0000, 0b0000_0000, 0b0000_0000, 0b0000_00__11__,

        // Range 2
        // 👇 marks single index range                       👇 marks index (5)
        0b__0__000_0000, 0b0000_0000, 0b0000_0000, 0b0000_0__101__,
    ]);

    let mut cluster2 = Cluster::default();
    cluster2.deserialize(&*data).unwrap();
    assert_eq!(cluster, cluster2);
}
