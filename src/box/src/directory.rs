use std::io;

use crate::cluster::{Cluster, ClusterReader, ClusterWriter};
use crate::file_system::FileSystem;
use crate::memory::{Memory, MemoryReader, MemoryWriter};
use crate::serde::{Deserialize, Serialize};

#[derive(Default, Debug)]
pub struct Directory {
    pub entries: Vec<Entry>,
}

impl Directory {
    pub fn add_file(&mut self, name: impl Into<String>) -> &mut Entry {
        self.entries.push(Entry {
            kind: EntryKind::File,
            name: name.into(),
            ..Default::default()
        });
        self.entries.last_mut().unwrap()
    }

    pub fn add_directory(&mut self, name: impl Into<String>) -> &mut Entry {
        self.entries.push(Entry {
            kind: EntryKind::Directory,
            name: name.into(),
            ..Default::default()
        });
        self.entries.last_mut().unwrap()
    }

    pub fn entry_with_name(&self, name: impl AsRef<str>) -> Option<&Entry> {
        let n = name.as_ref();
        self.entries.iter().find(|e| e.name == n)
    }

    pub fn entry_with_name_mut(&mut self, name: impl AsRef<str>) -> Option<&mut Entry> {
        let n = name.as_ref();
        self.entries.iter_mut().find(|e| e.name == n)
    }

    pub fn file_with_name_or_create_mut(
        &mut self,
        name: impl Into<String> + AsRef<str>,
    ) -> io::Result<&mut Entry> {
        let n = name.as_ref();

        let mut idx = None;

        for (i, e) in self.entries.iter_mut().enumerate() {
            if e.name == n {
                if e.kind == EntryKind::Directory {
                    return Err(io::Error::new(
                        io::ErrorKind::Other,
                        format!("name {} exists as a directory", name.as_ref()),
                    ));
                }
                idx = Some(i);
                break;
            }
        }

        match idx {
            None => Ok(self.add_file(name.into())),
            Some(idx) => Ok(self.entries.get_mut(idx).unwrap()),
        }
    }

    pub fn make_directory_recursive<P, S, M>(
        &mut self,
        fs: &mut FileSystem<M>,
        mut path: P,
    ) -> io::Result<()>
    where
        M: Memory,
        P: Iterator<Item = S>,
        S: Into<String> + AsRef<str>,
    {
        match path.next() {
            None => Ok(()),

            Some(segment) => match self.entry_with_name_mut(segment.as_ref()) {
                Some(Entry {
                    kind: EntryKind::File,
                    ..
                }) => Err(io::Error::new(
                    io::ErrorKind::InvalidInput,
                    format!("{} is not a directory", segment.into()),
                )),

                Some(e) => e
                    .read_from_file_system(fs)
                    .read_directory()?
                    .make_directory_recursive(fs, path),

                None => {
                    let mut new_dir = Directory::default();
                    new_dir.make_directory_recursive(fs, path)?;

                    let d = self.add_directory(segment);
                    d.write_to_file_system(fs).write_directory(&new_dir)?;
                    Ok(())
                }
            },
        }
    }
}

impl Serialize for Directory {
    fn serialize(&self, w: impl io::Write) -> io::Result<usize> {
        self.entries.serialize(w)
    }
}

impl Deserialize for Directory {
    fn deserialize(&mut self, r: impl io::Read) -> io::Result<usize> {
        self.entries.deserialize(r)
    }
}

#[derive(Default, Debug)]
pub struct Entry {
    pub kind: EntryKind,
    pub size: usize,
    pub name: String,
    pub cluster: Cluster,
}

impl Entry {
    pub fn new(name: impl Into<String>) -> Self {
        Entry {
            name: name.into(),
            ..Default::default()
        }
    }

    pub fn read_from_file_system<'a, M: Memory>(
        &'a self,
        fs: &'a FileSystem<M>,
    ) -> EntryReader<'a, ClusterReader<'a, MemoryReader<'a, M>>> {
        self.reader(fs.read_from_cluster(&self.cluster))
    }

    pub fn reader<R>(&self, reader: R) -> EntryReader<R> {
        EntryReader {
            entry: self,
            reader,
            offset: 0,
        }
    }

    pub fn write_to_file_system<'a, M: Memory>(
        &'a mut self,
        fs: &'a mut FileSystem<M>,
    ) -> EntryWriter<'a, ClusterWriter<'a, MemoryWriter<'a, M>>> {
        let writer = fs.write_into_cluster(&mut self.cluster);
        EntryWriter {
            entry_size: &mut self.size,
            writer,
            offset: 0,
        }
    }

    pub fn writer<W>(&mut self, writer: W) -> EntryWriter<W> {
        EntryWriter {
            entry_size: &mut self.size,
            writer,
            offset: 0,
        }
    }
}

impl Serialize for Entry {
    fn serialize(&self, mut w: impl io::Write) -> io::Result<usize> {
        Ok(self.kind.serialize(&mut w)?
            + self.name.as_str().serialize(&mut w)?
            + self.size.serialize(&mut w)?
            + self.cluster.serialize(w)?)
    }
}

impl Deserialize for Entry {
    fn deserialize(&mut self, mut r: impl io::Read) -> io::Result<usize> {
        Ok(self.kind.deserialize(&mut r)?
            + self.name.deserialize(&mut r)?
            + self.size.deserialize(&mut r)?
            + self.cluster.deserialize(r)?)
    }
}

#[derive(Debug, PartialEq)]
pub enum EntryKind {
    File,
    Directory,
}

impl Default for EntryKind {
    fn default() -> Self {
        EntryKind::File
    }
}

impl Serialize for EntryKind {
    fn serialize(&self, mut w: impl io::Write) -> io::Result<usize> {
        match self {
            EntryKind::File => w.write_all(&[1u8])?,
            EntryKind::Directory => w.write_all(&[2u8])?,
        }
        Ok(1)
    }
}

impl Deserialize for EntryKind {
    fn deserialize(&mut self, mut r: impl io::Read) -> io::Result<usize> {
        let mut code = [0u8; 1];
        r.read_exact(&mut code)?;
        let kind = match code[0] {
            1 => EntryKind::File,
            2 => EntryKind::Directory,
            _ => return Err(io::ErrorKind::InvalidInput.into()),
        };
        *self = kind;
        Ok(1)
    }
}

pub struct EntryReader<'a, R> {
    entry: &'a Entry,
    reader: R,
    offset: usize,
}

impl<'a, R> EntryReader<'a, R>
where
    R: io::Read,
{
    pub fn read_directory(&mut self) -> io::Result<Directory> {
        Directory::deserialize_into_default(self)
    }
}

impl<'a, R: io::Read> io::Read for EntryReader<'a, R> {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        let read_len = buf.len().min(self.entry.size - self.offset);
        if read_len == 0 {
            return Ok(0);
        }

        let read_bytes = self.reader.read(&mut buf[..read_len])?;
        self.offset += read_bytes;
        Ok(read_bytes)
    }
}

impl<'a, R: io::Seek> io::Seek for EntryReader<'a, R> {
    fn seek(&mut self, pos: io::SeekFrom) -> io::Result<u64> {
        let new_offset = self.reader.seek(pos)?;
        self.offset = new_offset as _;
        Ok(new_offset)
    }
}

pub struct EntryWriter<'a, W> {
    entry_size: &'a mut usize,
    writer: W,
    offset: usize,
}

impl<'a, W> EntryWriter<'a, W>
where
    W: io::Write,
{
    pub fn write_directory(&mut self, directory: &Directory) -> io::Result<usize> {
        directory.serialize(self)
    }
}

impl<'a, W: io::Write> io::Write for EntryWriter<'a, W> {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        let written_bytes = self.writer.write(&buf)?;
        self.offset += written_bytes;
        *self.entry_size = (*self.entry_size).max(self.offset);
        Ok(written_bytes)
    }

    fn flush(&mut self) -> io::Result<()> {
        self.writer.flush()
    }
}

impl<'a, W: io::Seek> io::Seek for EntryWriter<'a, W> {
    fn seek(&mut self, pos: io::SeekFrom) -> io::Result<u64> {
        let new_offset = self.writer.seek(pos)?;
        self.offset = new_offset as _;
        Ok(new_offset)
    }
}
