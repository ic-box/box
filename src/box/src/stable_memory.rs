use std::io;

use ic_cdk::api::stable;

use crate::memory::Memory;

pub struct StableMemory;

impl Memory for StableMemory {
    const PAGE_SIZE: usize = 65536;
    const MAX_PAGES: usize = 65536;

    #[cfg(target_pointer_width = "32")]
    fn page_count(&self) -> io::Result<usize> {
        Ok(stable::stable_size() as _)
    }

    #[cfg(target_pointer_width = "64")]
    fn page_count(&self) -> io::Result<usize> {
        Ok(stable::stable64_size() as _)
    }

    #[cfg(target_pointer_width = "32")]
    fn grow(&mut self, num_pages: usize) -> io::Result<()> {
        stable::stable_grow(num_pages as _)
            .map_err(|_| io::Error::new(io::ErrorKind::Other, "out of memory"))?;
        Ok(())
    }

    #[cfg(target_pointer_width = "64")]
    fn grow(&mut self, num_pages: usize) -> io::Result<()> {
        stable::stable64_grow(num_pages as _)
            .map_err(|_| io::Error::new(io::ErrorKind::Other, "out of memory"))?;
        Ok(())
    }

    #[cfg(target_pointer_width = "32")]
    fn read(&self, offset: usize, buf: &mut [u8]) -> io::Result<usize> {
        stable::stable_read(offset as _, buf);
        Ok(buf.len())
    }

    #[cfg(target_pointer_width = "64")]
    fn read(&self, offset: usize, buf: &mut [u8]) -> io::Result<usize> {
        stable::stable64_read(offset as _, buf);
        Ok(buf.len())
    }

    #[cfg(target_pointer_width = "32")]
    fn write(&mut self, offset: usize, buf: &[u8]) -> io::Result<usize> {
        stable::stable_write(offset as _, buf);
        Ok(buf.len())
    }

    #[cfg(target_pointer_width = "64")]
    fn write(&mut self, offset: usize, buf: &[u8]) -> io::Result<usize> {
        stable::stable64_write(offset as _, buf);
        Ok(buf.len())
    }
}
