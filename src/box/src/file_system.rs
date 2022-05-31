use std::fmt;
use std::io;

use crate::bitmap::Bitmap;
use crate::block::Block;
use crate::cluster::{Cluster, ClusterReader, ClusterWriter};
use crate::directory::{Directory, Entry, EntryKind};
use crate::memory::{Memory, MemoryReader, MemoryWriter};
use crate::serde::{Deserialize, Serialize};

pub struct FileSystem<M: Memory> {
    bitmap: Bitmap,
    root_cluster: Cluster,
    memory: M,
}

impl<M: Memory> FileSystem<M> {
    fn preamble_blocks() -> usize {
        Bitmap::len_for_memory_impl::<M>() / Block::SIZE + 8
    }

    pub fn allocate(memory: M) -> Self {
        Self {
            bitmap: Bitmap::new::<M>(),
            root_cluster: Cluster::default(),
            memory,
        }
    }

    pub fn new(memory: M) -> io::Result<Self> {
        let mut fs = Self::allocate(memory);
        fs.init()?;
        Ok(fs)
    }

    pub fn open(memory: M) -> io::Result<Self> {
        let mut fs = Self::allocate(memory);
        fs.restore()?;
        Ok(fs)
    }

    pub fn init(&mut self) -> io::Result<()> {
        for i in 0..Self::preamble_blocks() {
            self.bitmap.occupy(i);
        }

        Directory::default().serialize(
            self.root_cluster
                .writer(&mut self.bitmap, self.memory.writer()),
        )?;

        Ok(())
    }

    pub fn restore(&mut self) -> io::Result<()> {
        let mut r = self.memory.reader();
        self.bitmap.deserialize(&mut r)?;
        self.root_cluster.deserialize(r)?;
        Ok(())
    }

    pub fn persist(&mut self) -> io::Result<()> {
        let mut w = self.memory.writer();
        self.bitmap.serialize(&mut w)?;
        self.root_cluster.serialize(w)?;
        Ok(())
    }

    pub fn with_root_directory<R>(
        &self,
        f: impl FnOnce(&Directory) -> io::Result<R>,
    ) -> io::Result<R> {
        let dir = self.read_root_directory()?;
        f(&dir)
    }

    pub fn with_directory<R>(
        &self,
        path: impl IntoIterator<Item = impl AsRef<str>>,
        f: impl FnOnce(&Directory) -> io::Result<R>,
    ) -> io::Result<R> {
        let mut dir = self.read_root_directory()?;
        for segment in path {
            dir = match dir.entry_with_name(&segment) {
                None => return Err(io::ErrorKind::NotFound.into()),
                Some(entry) => entry.read_from_file_system(&self).read_directory()?,
            };
        }
        f(&dir)
    }

    pub fn with_root_directory_mut<R>(
        &mut self,
        f: impl FnOnce(&mut Directory, &mut Self) -> io::Result<R>,
    ) -> io::Result<R> {
        let mut dir = self.read_root_directory()?;
        let r = f(&mut dir, self);
        self.write_root_directory(&dir)?;
        r
    }

    pub fn with_directory_mut<R>(
        &mut self,
        path: impl IntoIterator<Item = impl AsRef<str>>,
        f: impl FnOnce(&mut Directory, &mut Self) -> io::Result<R>,
    ) -> io::Result<R> {
        self.with_root_directory_mut(|root, fs| {
            fs.with_directory_mut_rec(root, path.into_iter(), f)
        })
    }

    fn with_directory_mut_rec<R>(
        &mut self,
        dir: &mut Directory,
        mut path: impl Iterator<Item = impl AsRef<str>>,
        f: impl FnOnce(&mut Directory, &mut Self) -> io::Result<R>,
    ) -> io::Result<R> {
        match path.next() {
            Some(segment) => match dir.entry_with_name_mut(&segment) {
                None => Err(io::ErrorKind::NotFound.into()),
                Some(Entry {
                    kind: EntryKind::File,
                    ..
                }) => Err(io::ErrorKind::Other.into()),
                Some(
                    entry @ Entry {
                        kind: EntryKind::Directory,
                        ..
                    },
                ) => {
                    let mut subdir = entry.read_from_file_system(&self).read_directory()?;
                    let r = self.with_directory_mut_rec(&mut subdir, path, f)?;
                    entry.write_to_file_system(self).write_directory(&subdir)?;
                    Ok(r)
                }
            },
            None => f(dir, self),
        }
    }

    pub fn write_into_cluster<'a>(
        &'a mut self,
        cluster: &'a mut Cluster,
    ) -> ClusterWriter<'a, MemoryWriter<'a, M>> {
        cluster.writer(&mut self.bitmap, self.memory.writer())
    }

    pub fn write_into_root_cluster(&mut self) -> ClusterWriter<MemoryWriter<M>> {
        self.root_cluster
            .writer(&mut self.bitmap, self.memory.writer())
    }

    pub fn read_from_cluster<'a>(&'a self, cluster: &'a Cluster) -> ClusterReader<MemoryReader<M>> {
        cluster.reader(self.memory.reader())
    }

    pub fn read_from_root_cluster(&self) -> ClusterReader<MemoryReader<M>> {
        self.root_cluster.reader(self.memory.reader())
    }

    pub fn read_root_directory(&self) -> io::Result<Directory> {
        let r = self.read_from_root_cluster();
        Directory::deserialize_into_default(r)
    }

    pub fn write_root_directory(&mut self, directory: &Directory) -> io::Result<()> {
        directory.serialize(self.write_into_root_cluster())?;
        Ok(())
    }

    pub fn make_directory_recursive<P, S>(&mut self, path: P) -> io::Result<()>
    where
        P: IntoIterator<Item = S>,
        S: Into<String> + AsRef<str>,
    {
        self.with_root_directory_mut(|root, fs| root.make_directory_recursive(fs, path.into_iter()))
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
        self.persist().expect("failed to write filesystem preamble");
    }
}

impl<M: Memory> fmt::Display for FileSystem<M> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "/")?;

        let mut dirs = vec![self.read_root_directory().or(Err(fmt::Error))?];
        while dirs.len() > 0 {
            let l = dirs.len() - 1;
            let dir = dirs.last_mut().unwrap();
            if dir.entries.is_empty() {
                dirs.pop().unwrap();
                continue;
            }

            write!(f, "\n{:>width$}", "| ", width = l * 4 + 2)?;

            match &dir.entries.remove(0) {
                Entry {
                    kind: EntryKind::File,
                    name,
                    ..
                } => {
                    write!(f, "{}", name)?;
                }

                inner_dir @ Entry {
                    kind: EntryKind::Directory,
                    name,
                    ..
                } => {
                    write!(f, "{}/", &name)?;
                    drop(dir);

                    dirs.push(
                        inner_dir
                            .read_from_file_system(self)
                            .read_directory()
                            .or(Err(fmt::Error))?,
                    );
                }
            }
        }
        Ok(())
    }
}

#[test]
fn test() {
    use crate::bitmap::BitState;
    use crate::heap_memory::HeapMemory;
    use std::io::{Read, Write};

    const DATA_BLOCKS: usize = 128;

    let data: Vec<u8> = (0..Block::SIZE * DATA_BLOCKS)
        .map(|_| rand::random())
        .collect();

    let mut memory = HeapMemory::default();

    {
        let mut fs = FileSystem::new(&mut memory).unwrap();

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
            FileSystem::<HeapMemory>::preamble_blocks() + DATA_BLOCKS + 3
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

#[test]
fn a_file() {
    use crate::directory::EntryKind;
    use crate::heap_memory::HeapMemory;
    use std::io::{Read, Write};

    let mut mem = HeapMemory::default();

    {
        let mut fs = FileSystem::new(&mut mem).unwrap();

        fs.with_root_directory_mut(|root, fs| {
            root.add_file("my-file.txt")
                .write_to_file_system(fs)
                .write_all(b"Hello World")
        })
        .unwrap();
    }

    {
        let fs = FileSystem::open(&mut mem).unwrap();

        fs.with_root_directory(|root| {
            let entry = &root.entries[0];
            assert_eq!(entry.kind, EntryKind::File);

            let mut r = entry.read_from_file_system(&fs);
            let mut result = [0u8; 5];
            r.read_exact(&mut result)?;

            assert_eq!(&result, b"Hello");
            Ok(())
        })
        .unwrap();
    }
}

#[test]
fn a_nested_dir() {
    use crate::heap_memory::HeapMemory;
    use std::io::{Read, Write};

    let mut mem = HeapMemory::default();

    {
        let mut fs = FileSystem::new(&mut mem).unwrap();

        fs.with_root_directory_mut(|root, fs| {
            let mut dir = Directory::default();
            dir.add_file("my_file.txt")
                .write_to_file_system(fs)
                .write_all(b"Hello, World!")?;

            root.add_directory("my_dir")
                .write_to_file_system(fs)
                .write_directory(&dir)?;
            Ok(())
        })
        .unwrap();
    }

    {
        let fs = FileSystem::open(&mut mem).unwrap();

        fs.with_root_directory(|root| {
            let dir_entry = &root.entries[0];
            assert_eq!(&dir_entry.name, "my_dir");

            let file_entry = &dir_entry
                .read_from_file_system(&fs)
                .read_directory()?
                .entries[0];
            assert_eq!(&file_entry.name, "my_file.txt");

            let mut result = String::new();
            file_entry
                .read_from_file_system(&fs)
                .read_to_string(&mut result)?;
            assert_eq!(&result, "Hello, World!");
            Ok(())
        })
        .unwrap();
    }
}

#[test]
fn make_dir_recursive() {
    use crate::heap_memory::HeapMemory;

    let mut fs = FileSystem::new(HeapMemory::default()).unwrap();

    let path = vec!["one", "two", "three"];
    fs.make_directory_recursive(path).unwrap();

    assert_eq!(
        format!("{}", fs),
        "/
| one/
    | two/
        | three/"
    )
}
