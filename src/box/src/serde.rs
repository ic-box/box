use std::io::{self, Read, Write};

pub trait Serialize {
    fn serialize(&self, w: impl Write) -> io::Result<usize>;
}

pub trait Deserialize {
    fn deserialize(&mut self, r: impl Read) -> io::Result<usize>;
}
