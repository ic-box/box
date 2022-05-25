use std::io::{self, Read, Write};
use std::mem::size_of;

pub trait Serialize {
    fn serialize(&self, w: impl Write) -> io::Result<usize>;
}

pub trait Deserialize {
    fn deserialize(&mut self, r: impl Read) -> io::Result<usize>;

    fn deserialize_into_default(r: impl Read) -> io::Result<Self>
    where
        Self: Default,
    {
        let mut this = Self::default();
        this.deserialize(r)?;
        Ok(this)
    }
}

impl Serialize for u8 {
    fn serialize(&self, mut w: impl Write) -> io::Result<usize> {
        w.write_all(&[*self])?;
        Ok(1)
    }
}

impl Deserialize for u8 {
    fn deserialize(&mut self, mut r: impl Read) -> io::Result<usize> {
        let mut d = [0u8];
        r.read_exact(&mut d)?;
        *self = d[0];
        Ok(1)
    }
}

impl Serialize for u64 {
    fn serialize(&self, mut w: impl Write) -> io::Result<usize> {
        w.write_all(&self.to_be_bytes())?;
        Ok(size_of::<u64>())
    }
}

impl Deserialize for u64 {
    fn deserialize(&mut self, mut r: impl Read) -> io::Result<usize> {
        let mut d = [0u8; size_of::<u64>()];
        r.read_exact(&mut d)?;
        *self = u64::from_be_bytes(d);
        Ok(size_of::<u64>())
    }
}

impl Serialize for usize {
    fn serialize(&self, w: impl Write) -> io::Result<usize> {
        (*self as u64).serialize(w)
    }
}

impl Deserialize for usize {
    fn deserialize(&mut self, r: impl Read) -> io::Result<usize> {
        let mut fixed_size = *self as u64;
        let n = fixed_size.deserialize(r)?;
        *self = fixed_size as _;
        Ok(n)
    }
}

impl<T: Serialize> Serialize for Vec<T> {
    fn serialize(&self, mut w: impl Write) -> io::Result<usize> {
        let mut data_bytes_written = self.len().serialize(&mut w)?;
        for t in self.iter() {
            data_bytes_written += t.serialize(&mut w)?;
        }
        Ok(data_bytes_written)
    }
}

impl<T: Deserialize + Default> Deserialize for Vec<T> {
    fn deserialize(&mut self, mut r: impl Read) -> io::Result<usize> {
        let mut len_bytes = [0u8; size_of::<u64>()];
        r.read_exact(&mut len_bytes)?;
        let len = u64::from_be_bytes(len_bytes) as usize;
        let mut data_bytes_read = 0;
        for _ in 0..len {
            let mut t = T::default();
            data_bytes_read += t.deserialize(&mut r)?;
            self.push(t);
        }
        Ok(size_of::<u64>() + data_bytes_read)
    }
}

impl<'a> Serialize for &'a [u8] {
    fn serialize(&self, mut w: impl Write) -> io::Result<usize> {
        w.write_all(self)?;
        Ok(self.len())
    }
}

impl<'a> Deserialize for &'a mut [u8] {
    fn deserialize(&mut self, mut r: impl Read) -> io::Result<usize> {
        r.read_exact(self)?;
        Ok(self.len())
    }
}

impl<'a> Serialize for &'a str {
    fn serialize(&self, mut w: impl Write) -> io::Result<usize> {
        Ok(self.len().serialize(&mut w)? + self.as_bytes().serialize(&mut w)?)
    }
}

impl Deserialize for String {
    fn deserialize(&mut self, mut r: impl Read) -> io::Result<usize> {
        let mut len = 0usize;
        let n = len.deserialize(&mut r)?;
        let mut bytes = vec![0u8; len];
        r.read_exact(&mut bytes)?;
        *self = String::from_utf8_lossy(&bytes).to_string();
        Ok(n + len)
    }
}

#[test]
fn serde() {
    let mut buf = vec![];

    let string = "This is a message".to_string();
    string.as_str().serialize(&mut buf).unwrap();
    let mut actual = String::new();
    actual.deserialize(&*buf).unwrap();
    assert_eq!(string, actual);
}
