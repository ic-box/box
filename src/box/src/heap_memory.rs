use std::fmt;
use std::io;

use crate::block::Block;
use crate::memory::Memory;

const HEAP_PAGE_SIZE: usize = 1024;

#[derive(Default)]
pub struct HeapMemory {
    pages: Vec<[u8; HEAP_PAGE_SIZE]>,
}

impl HeapMemory {
    pub fn iter(&self) -> impl '_ + Iterator<Item = &u8> {
        self.pages.iter().flat_map(|page| page.iter())
    }
}

impl fmt::Debug for HeapMemory {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "Heap {{")?;
        for (i, byte) in self.iter().enumerate() {
            if i % HEAP_PAGE_SIZE == 0 {
                write!(f, "\nPage {}:", i / HEAP_PAGE_SIZE)?;
            }
            if i % Block::SIZE == 0 {
                write!(f, "\n  Block {}:", i / Block::SIZE)?;
            }
            if i % 64 == 0 {
                write!(f, "\n   ")?;
            }
            if i % 4 == 0 {
                write!(f, " ")?;
            }
            if i % 2 == 0 {
                write!(f, " ")?;
            }
            write!(f, "{:02x}", byte)?;
        }
        write!(f, "\n}}")?;
        Ok(())
    }
}

impl Memory for HeapMemory {
    const PAGE_SIZE: usize = HEAP_PAGE_SIZE;
    const MAX_PAGES: usize = 256;

    fn page_count(&self) -> io::Result<usize> {
        Ok(self.pages.len())
    }

    fn grow(&mut self, num_pages: usize) -> io::Result<()> {
        self.pages.extend(&vec![[0u8; HEAP_PAGE_SIZE]; num_pages]);
        Ok(())
    }

    fn read(&self, offset: usize, buf: &mut [u8]) -> io::Result<usize> {
        let page_index = offset / HEAP_PAGE_SIZE;
        let page_offset = offset % HEAP_PAGE_SIZE;

        if page_index >= self.pages.len() {
            return Ok(0);
        }

        let page = &self.pages[page_index];
        let data_to_read = &page[page_offset..];

        let len_to_read = data_to_read.len().min(buf.len());
        for i in 0..len_to_read {
            buf[i] = data_to_read[i];
        }

        Ok(len_to_read)
    }

    fn write(&mut self, offset: usize, buf: &[u8]) -> io::Result<usize> {
        let page_index = offset / HEAP_PAGE_SIZE;
        let page_offset = offset % HEAP_PAGE_SIZE;

        if page_index >= self.pages.len() {
            return Ok(0);
        }

        let page = &mut self.pages[page_index];
        let data_to_write = &mut page[page_offset..];

        let len_to_write = data_to_write.len().min(buf.len());
        for i in 0..len_to_write {
            data_to_write[i] = buf[i];
        }

        Ok(len_to_write)
    }
}
