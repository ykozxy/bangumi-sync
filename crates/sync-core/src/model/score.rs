#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Score(u8);

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ScoreError {
    OutOfRange { value: u16 },
}

impl Score {
    pub const fn from_hundred_point(value: u16) -> Result<Self, ScoreError> {
        if value <= 100 {
            Ok(Self(value as u8))
        } else {
            Err(ScoreError::OutOfRange { value })
        }
    }

    pub const fn as_hundred_point(self) -> u8 {
        self.0
    }
}
