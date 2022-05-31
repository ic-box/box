use std::cell::RefCell;
use std::io::{self, Read, Write};

use ic_cdk_macros::{init, post_upgrade, pre_upgrade, query, update};

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

#[query]
fn list(path: Vec<String>) -> Vec<String> {
    FILE_SYSTEM
        .with(|fs| {
            let fs = fs.borrow();
            fs.with_directory(path, |dir| {
                Ok(dir.entries.iter().map(|e| &e.name).cloned().collect())
            })
        })
        .unwrap()
}

#[query]
fn read(mut path: Vec<String>) -> Vec<u8> {
    FILE_SYSTEM
        .with(|fs| {
            let name = match path.pop() {
                Some(file) => file,
                None => panic!("invalid file name"),
            };
            let fs = fs.borrow();
            fs.with_directory(path, |dir| {
                let entry = dir
                    .entry_with_name(name)
                    .ok_or::<io::Error>(io::ErrorKind::NotFound.into())?;
                let mut result = vec![];
                entry.read_from_file_system(&fs).read_to_end(&mut result)?;
                Ok(result)
            })
        })
        .unwrap()
}

#[update]
fn write(mut path: Vec<String>, data: Vec<u8>) {
    FILE_SYSTEM.with(|fs| {
        let name = match path.pop() {
            Some(file) => file,
            None => panic!("invalid file name"),
        };
        let mut fs = fs.borrow_mut();
        fs.with_directory_mut(path, |dir, fs| {
            let entry = dir.file_with_name_or_create_mut(name)?;
            entry.write_to_file_system(fs).write_all(data.as_slice())?;
            Ok(())
        })
        .unwrap();
    })
}

#[allow(non_snake_case)]
#[update]
fn makeDirectory(path: Vec<String>) {
    FILE_SYSTEM.with(|fs| {
        let mut fs = fs.borrow_mut();
        fs.make_directory_recursive(path).unwrap();
    })
}
