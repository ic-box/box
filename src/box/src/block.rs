use std::ops::Add;

#[derive(Clone, Copy, PartialEq, Debug, PartialOrd)]
pub struct Block {
    pub index: usize,
}

impl Block {
    pub const SIZE: usize = 512;

    pub fn at(index: usize) -> Self {
        Block { index }
    }
}

impl Add<usize> for Block {
    type Output = Block;

    fn add(self, rhs: usize) -> Self::Output {
        Block {
            index: self.index + rhs,
        }
    }
}
