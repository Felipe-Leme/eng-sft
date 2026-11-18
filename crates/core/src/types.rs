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

#[cfg(test)]
mod tests {
    use rust_decimal::dec;

    use super::*;

    #[test]
    fn validates_score_and_percentage_boundaries() {
        assert!(Score::new(dec!(0.0)).is_ok());
        assert!(Score::new(dec!(-0.01)).is_err());

        assert!(Percentage::new(dec!(0.0)).is_ok());
        assert!(Percentage::new(dec!(100.0)).is_ok());
        assert!(Percentage::new(dec!(-0.01)).is_err());
        assert!(Percentage::new(dec!(100.01)).is_err());

        let score = Score::try_from(dec!(1.25)).unwrap();
        assert_eq!(score.inner(), dec!(1.25));
        assert_eq!((score + Score::ONE).inner(), dec!(2.25));

        let mut accumulated = score;
        accumulated += Score::ONE;
        assert_eq!(accumulated.inner(), dec!(2.25));
        let score_decimal: Decimal = Score::ONE.into();
        assert_eq!(score_decimal, Decimal::ONE);

        let percentage = Percentage::try_from(dec!(25)).unwrap();
        assert_eq!(percentage.inner(), dec!(25));
        let percentage_decimal: Decimal = percentage.into();
        assert_eq!(percentage_decimal, dec!(25));
        let percentage_score: Score = percentage.into();
        assert_eq!(percentage_score.inner(), dec!(25));
    }
}
