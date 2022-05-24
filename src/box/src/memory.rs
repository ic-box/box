use std::io;

pub trait Memory {
    const PAGE_SIZE: usize;
    const MAX_PAGES: usize;
    const MAX_SIZE: usize = Self::PAGE_SIZE * Self::MAX_PAGES;

    fn page_count(&self) -> io::Result<usize>;
    fn grow(&mut self, num_pages: usize) -> io::Result<()>;

    fn read(&self, offset: usize, buf: &mut [u8]) -> io::Result<usize>;
    fn write(&mut self, offset: usize, buf: &[u8]) -> io::Result<usize>;

    fn len(&self) -> io::Result<usize> {
        Ok(self.page_count()? * Self::PAGE_SIZE)
    }

    fn reader(&self) -> MemoryReader<'_, Self>
    where
        Self: Sized,
    {
        MemoryReader {
            memory: self,
            offset: 0,
        }
    }

    fn writer(&mut self) -> MemoryWriter<'_, Self>
    where
        Self: Sized,
    {
        MemoryWriter {
            memory: self,
            offset: 0,
        }
    }
}

impl<'a, M: Memory> Memory for &'a mut M {
    const PAGE_SIZE: usize = M::PAGE_SIZE;
    const MAX_PAGES: usize = M::MAX_PAGES;

    fn page_count(&self) -> io::Result<usize> {
        M::page_count(self)
    }

    fn grow(&mut self, num_pages: usize) -> io::Result<()> {
        M::grow(self, num_pages)
    }

    fn read(&self, offset: usize, buf: &mut [u8]) -> io::Result<usize> {
        M::read(self, offset, buf)
    }

    fn write(&mut self, offset: usize, buf: &[u8]) -> io::Result<usize> {
        M::write(self, offset, buf)
    }
}

pub struct MemoryReader<'a, M: Sized> {
    pub memory: &'a M,
    offset: usize,
}

impl<'a, M> io::Seek for MemoryReader<'a, M>
where
    M: Memory,
{
    fn seek(&mut self, pos: io::SeekFrom) -> io::Result<u64> {
        let new_offset = match pos {
            io::SeekFrom::Start(offset) => offset,
            io::SeekFrom::Current(offset) => (self.offset as i64 + offset) as u64,
            io::SeekFrom::End(offset) => (self.memory.len()? as i64 + offset) as u64,
        };
        self.offset = new_offset as _;
        Ok(new_offset)
    }
}

impl<'a, M> io::Read for MemoryReader<'a, M>
where
    M: Memory,
{
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        let required_len = self.offset + buf.len();
        let current_len = self.memory.len()?;

        let read_buf = if required_len > current_len {
            let missing_len = required_len - current_len;
            if missing_len > buf.len() {
                return Ok(0);
            }
            let acceptable_len = buf.len() - missing_len;
            &mut buf[..acceptable_len]
        } else {
            buf
        };

        let read = self.memory.read(self.offset, read_buf)?;
        self.offset += read;
        Ok(read)
    }
}

pub struct MemoryWriter<'a, M: Sized> {
    pub memory: &'a mut M,
    offset: usize,
}

impl<'a, M> io::Seek for MemoryWriter<'a, M>
where
    M: Memory,
{
    fn seek(&mut self, pos: io::SeekFrom) -> io::Result<u64> {
        let new_offset = match pos {
            io::SeekFrom::Start(offset) => offset,
            io::SeekFrom::Current(offset) => (self.offset as i64 + offset) as u64,
            io::SeekFrom::End(offset) => (self.memory.len()? as i64 + offset) as u64,
        };
        self.offset = new_offset as _;
        Ok(new_offset)
    }
}

impl<'a, M> io::Write for MemoryWriter<'a, M>
where
    M: Memory,
{
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        let required_len = self.offset + buf.len();
        let current_len = self.memory.len()?;
        if required_len > current_len {
            let missing_len = required_len - current_len;
            let mut missing_pages = missing_len / M::PAGE_SIZE;
            if missing_len % M::PAGE_SIZE > 0 {
                missing_pages += 1;
            }
            self.memory.grow(missing_pages)?;
        }
        let written = self.memory.write(self.offset, buf)?;
        self.offset += written;
        Ok(written)
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

#[test]
fn io() {
    use std::io::{Read, Write, Seek};
    use super::heap_memory::HeapMemory;

    let mut memory = HeapMemory::default();

    {
        let mut w = memory.writer();
        w.seek(io::SeekFrom::Start((HeapMemory::PAGE_SIZE - 13) as _)).unwrap();
        w.write_all(b"Hello, World!").unwrap();
    }

    {
        let mut r = memory.reader();
        r.seek(io::SeekFrom::End(-6)).unwrap();
        let mut buf = [0u8; 6];
        r.read_exact(&mut buf).unwrap();
        assert_eq!(&buf, b"World!");
    }
}
