use std::cell::RefCell;
use std::io::{self, Read, Seek, Write};

use ic_cdk::export::candid::types::Serializer;
use ic_cdk::export::candid::{CandidType, Deserialize};
use ic_cdk::export::serde::Deserializer;
use ic_cdk_macros::{init, post_upgrade, pre_upgrade, query, update};
use percent_encoding::{percent_decode, utf8_percent_encode, CONTROLS};

use crate::directory;
use crate::file_system::FileSystem;
use crate::stable_memory::StableMemory;

thread_local! {
    static FILE_SYSTEM: RefCell<FileSystem<StableMemory>> =
        RefCell::new(FileSystem::allocate(StableMemory));
}

#[init]
fn init() {
    FILE_SYSTEM.with(|fs| fs.borrow_mut().init()).unwrap()
}

#[pre_upgrade]
fn pre_upgrade() {
    FILE_SYSTEM.with(|fs| fs.borrow_mut().persist()).unwrap()
}

#[post_upgrade]
fn post_upgrade() {
    FILE_SYSTEM.with(|fs| fs.borrow_mut().restore()).unwrap()
}

#[query(name = "openDirectory")]
fn open_directory(path: Path) -> Directory {
    FILE_SYSTEM
        .with(|fs| {
            let fs = fs.borrow();
            fs.with_directory(path, |dir| Ok(Directory::from(dir)))
        })
        .unwrap()
}

#[query(name = "openFile")]
fn open_file(path: Path) -> File {
    FILE_SYSTEM
        .with(|fs| {
            let fs = fs.borrow();
            fs.with_file(path, |file| Ok(File::from(file)))
        })
        .unwrap()
}

#[query(name = "readFile")]
fn read_file(path: Path, start: Option<i64>, end: Option<i64>) -> Vec<u8> {
    FILE_SYSTEM
        .with(|fs| {
            let fs = fs.borrow();
            fs.with_file(path, |file| {
                let size = file.size as i64;

                let mut start = start.unwrap_or_default();
                let mut end = end.unwrap_or(file.size as i64);

                if start < 0 {
                    start = size + start;
                }
                if end < 0 {
                    end = size + end;
                }

                if start > end {
                    return Err(io::ErrorKind::InvalidInput.into());
                }

                let len = end - start;

                let mut data = vec![0u8; len as usize];

                let mut r = file.read_from_file_system(&fs);

                if start > 0 {
                    r.seek(io::SeekFrom::Start(start as u64))?;
                }

                r.read_exact(&mut data)?;
                Ok(data)
            })
        })
        .unwrap()
}

#[update(name = "createDirectory")]
fn create_directory(path: Path) -> Directory {
    FILE_SYSTEM
        .with(|fs| -> io::Result<Directory> {
            let mut fs = fs.borrow_mut();
            fs.make_directory_recursive(path)?;
            Ok(Directory { entries: vec![] })
        })
        .unwrap()
}

#[update(name = "createFile")]
fn create_file(mut path: Path, content_type: String) -> File {
    let filename = path.pop().expect("path cannot be empty");

    FILE_SYSTEM
        .with(|fs| {
            let mut fs = fs.borrow_mut();
            fs.with_directory_mut(path, |dir, _| {
                dir.add_file(filename, content_type.clone());
                Ok(File { size: 0, content_type })
            })
        })
        .unwrap()
}

#[update(name = "writeFile")]
fn write_file(path: Path, data: Vec<u8>, offset: Option<i64>) {
    FILE_SYSTEM
        .with(|fs| {
            let mut fs = fs.borrow_mut();
            fs.with_file_mut(path, |file, fs| {
                let mut w = file.write_to_file_system(fs);
                if let Some(offset) = offset {
                    w.seek(io::SeekFrom::Start(offset as u64))?;
                }
                w.write_all(&data)?;
                Ok(())
            })
        })
        .unwrap()
}

#[derive(CandidType, Deserialize)]
struct Directory {
    pub entries: Vec<Entry>,
}

impl<'a> From<&'a directory::Directory> for Directory {
    fn from(dir: &'a directory::Directory) -> Self {
        Directory {
            entries: dir.entries.iter().map(Entry::from).collect(),
        }
    }
}

#[derive(CandidType, Deserialize)]
struct Entry {
    pub name: String,
    pub kind: EntryKind,
}

impl<'a> From<&'a directory::Entry> for Entry {
    fn from(e: &'a directory::Entry) -> Self {
        Entry {
            name: e.name.clone(),
            kind: match e.kind {
                crate::directory::EntryKind::Directory => EntryKind::Directory,
                crate::directory::EntryKind::File => EntryKind::File(e.into()),
            },
        }
    }
}

#[derive(CandidType, Deserialize)]
struct File {
    size: u64,
    #[serde(rename = "contentType")]
    content_type: String,
}

impl<'a> From<&'a directory::Entry> for File {
    fn from(entry: &'a directory::Entry) -> Self {
        Self {
            size: entry.size as u64,
            content_type: entry.content_type.clone(),
        }
    }
}

#[derive(CandidType, Deserialize)]
enum EntryKind {
    Directory,
    File(File),
}

struct Path {
    segments: Vec<String>,
}

impl Path {
    pub fn pop(&mut self) -> Option<String> {
        self.segments.pop()
    }

    pub fn len(&self) -> usize {
        self.segments.len()
    }
}

impl Into<Vec<String>> for Path {
    fn into(self) -> Vec<String> {
        self.segments
    }
}

impl IntoIterator for Path {
    type Item = String;

    type IntoIter = std::vec::IntoIter<String>;

    fn into_iter(self) -> Self::IntoIter {
        self.segments.into_iter()
    }
}

impl<'a> Deserialize<'a> for Path {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'a>,
    {
        let full: String = Deserialize::deserialize(deserializer)?;

        let segments = full
            .split('/')
            .filter(|s| !s.is_empty())
            .map(|s| percent_decode(s.as_bytes()).decode_utf8_lossy().to_string())
            .collect();

        Ok(Self { segments })
    }
}

const CHARS: percent_encoding::AsciiSet = CONTROLS.add(b'/').add(b'#').add(b'?');

impl CandidType for Path {
    fn _ty() -> candid::types::Type {
        candid::types::Type::Text
    }

    fn idl_serialize<S>(&self, serializer: S) -> Result<(), S::Error>
    where
        S: Serializer,
    {
        self.segments
            .iter()
            .map(|s| utf8_percent_encode(s.as_str(), &CHARS).to_string())
            .collect::<Vec<_>>()
            .join("/")
            .idl_serialize(serializer)
    }
}
