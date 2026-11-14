use std::ops::{Add, AddAssign};

use rust_decimal::Decimal;

use crate::error::{PercentageError, ScoreError};

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct Score(Decimal);

impl Score {
    pub const ZERO: Self = Self(Decimal::ZERO);
    pub const ONE: Self = Self(Decimal::ONE);

    fn valid(value: Decimal) -> bool {
        value >= Decimal::ZERO
    }

    pub fn new(value: Decimal) -> Result<Self, ScoreError> {
        if Self::valid(value) {
            return Ok(Self(value));
        }
        Err(ScoreError)
    }

    pub fn inner(self) -> Decimal {
        self.0
    }
}

impl AddAssign for Score {
    fn add_assign(&mut self, rhs: Self) {
        self.0 += rhs.0
    }
}

impl Add for Score {
    type Output = Self;

    fn add(self, rhs: Self) -> Self::Output {
        Self(self.0 + rhs.0)
    }
}

impl TryFrom<Decimal> for Score {
    type Error = ScoreError;

    fn try_from(value: Decimal) -> Result<Self, Self::Error> {
        Self::new(value)
    }
}

impl From<Score> for Decimal {
    fn from(value: Score) -> Self {
        value.0
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct Percentage(Decimal);

impl Percentage {
    pub const ZERO: Self = Self(Decimal::ZERO);
    pub const ONE_HUNDRED: Self = Self(Decimal::ONE_HUNDRED);

    fn valid(value: Decimal) -> bool {
        value >= Decimal::ZERO && value <= Decimal::ONE_HUNDRED
    }

    pub fn new(value: Decimal) -> Result<Self, PercentageError> {
        if Self::valid(value) {
            return Ok(Self(value));
        }
        Err(PercentageError)
    }

    pub fn inner(self) -> Decimal {
        self.0
    }
}

impl TryFrom<Decimal> for Percentage {
    type Error = PercentageError;

    fn try_from(value: Decimal) -> Result<Self, Self::Error> {
        Self::new(value)
    }
}

impl From<Percentage> for Decimal {
    fn from(value: Percentage) -> Self {
        value.0
    }
}

impl From<Percentage> for Score {
    fn from(value: Percentage) -> Self {
        Self(value.0)
    }
}
