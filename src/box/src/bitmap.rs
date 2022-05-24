use std::io::{self, Read, Write};

use crate::block::Block;
use crate::memory::Memory;
use crate::serde::{Deserialize, Serialize};

#[derive(Clone)]
pub struct Bitmap {
    map: Vec<u8>,
}

impl Bitmap {
    pub fn new<M: Memory>() -> Self {
        Self {
            map: vec![0u8; Self::len_for_memory_impl::<M>()],
        }
    }

    fn len_for_memory_impl<M: Memory>() -> usize {
        M::MAX_SIZE / Block::SIZE / 8
    }

    pub fn occupy(&mut self, index: usize) {
        let byte_offset = index / 8;
        let bit_offset = index % 8;

        assert!(byte_offset < self.len());

        self.map[byte_offset] |= 1 << bit_offset;
    }

    pub fn free(&mut self, index: usize) {
        let byte_offset = index / 8;
        let bit_offset = index % 8;

        assert!(byte_offset < self.len());

        self.map[byte_offset] &= !(1 << bit_offset);
    }

    pub fn len(&self) -> usize {
        self.map.len()
    }

    pub fn iter(&self) -> impl '_ + Iterator<Item = BitState> {
        BitStateIterator {
            map: self,
            byte_offset: 0,
            bit_offset: 0,
        }
    }

    pub fn occupy_next(&mut self) -> Option<usize> {
        let mut result = None;
        for (i, state) in self.iter().enumerate() {
            if let BitState::Free = state {
                result = Some(i);
                break;
            }
        }
        if let Some(i) = &result {
            self.occupy(*i);
        }
        result
    }
}

impl Serialize for Bitmap {
    fn serialize(&self, mut writer: impl Write) -> io::Result<usize> {
        writer.write_all(&self.map)?;
        Ok(self.map.len())
    }
}

impl Deserialize for Bitmap {
    fn deserialize(&mut self, mut r: impl Read) -> io::Result<usize> {
        r.read_exact(&mut self.map)?;
        Ok(self.map.len())
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum BitState {
    Occupied,
    Free,
}

const OCCUPIED: BitState = BitState::Occupied;
const FREE: BitState = BitState::Free;

impl std::ops::Index<usize> for Bitmap {
    type Output = BitState;

    fn index(&self, index: usize) -> &Self::Output {
        let byte_offset = index / 8;
        let bit_offset = index % 8;

        assert!(byte_offset < self.map.len());

        match (self.map[byte_offset] >> bit_offset) & 1 {
            1 => &OCCUPIED,
            0 => &FREE,
            _ => unreachable!(),
        }
    }
}

struct BitStateIterator<'a> {
    map: &'a Bitmap,
    byte_offset: usize,
    bit_offset: usize,
}

impl<'a> Iterator for BitStateIterator<'a> {
    type Item = BitState;

    fn next(&mut self) -> Option<Self::Item> {
        if self.byte_offset >= self.map.len() {
            return None;
        }

        let state = match (self.map.map[self.byte_offset] >> self.bit_offset) & 1 {
            1 => BitState::Occupied,
            0 => BitState::Free,
            _ => unreachable!(),
        };

        self.bit_offset += 1;
        if self.bit_offset >= 8 {
            self.bit_offset = 0;
            self.byte_offset += 1;
        }

        Some(state)
    }
}

#[test]
fn bitmap() {
    use crate::heap_memory::HeapMemory;

    let mut bitmap: Bitmap = Bitmap::new::<HeapMemory>();

    assert_eq!(bitmap[7], BitState::Free);

    bitmap.occupy(7);

    assert_eq!(bitmap[7], BitState::Occupied);

    let slots = Bitmap::len_for_memory_impl::<HeapMemory>();

    assert_eq!(bitmap[slots - 1], BitState::Free);
    assert_eq!(bitmap[0], BitState::Free);

    bitmap.occupy(slots - 1);
    bitmap.occupy(0);

    assert_eq!(bitmap[slots - 1], BitState::Occupied);
    assert_eq!(bitmap[0], BitState::Occupied);

    bitmap.free(slots - 1);
    assert_eq!(bitmap[slots - 1], BitState::Free);
}
