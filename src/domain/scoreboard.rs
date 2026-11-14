use std::{collections::HashMap, hash::Hash};

use crate::types::Score;

pub struct Scoreboard<T> {
    scores: HashMap<T, Score>,
}

impl<T: Eq + Hash + Clone> Default for Scoreboard<T> {
    fn default() -> Self {
        Self {
            scores: HashMap::new(),
        }
    }
}

impl<T: Eq + Hash + Clone> Scoreboard<T> {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn add_score(&mut self, option: T, points: Score) {
        *self.scores.entry(option).or_insert(Score::ZERO) += points;
    }
}

impl<T: Eq + Hash + Clone + Ord> Scoreboard<T> {
    pub fn get_winners(&self) -> (Score, Vec<T>) {
        let max_score = self.scores.values().copied().max().unwrap_or(Score::ZERO);
        let mut winners: Vec<_> = self
            .scores
            .iter()
            .filter(|(_, score)| **score == max_score)
            .map(|(option, _)| option.clone())
            .collect();

        winners.sort();
        (max_score, winners)
    }

    pub fn into_sorted_descending(self) -> Vec<(T, Score)> {
        let mut results: Vec<_> = self.scores.into_iter().collect();

        results.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.cmp(&b.0)));
        results
    }
}

#[cfg(test)]
mod tests {
    use rust_decimal::dec;

    use super::*;

    #[test]
    fn sorts_equal_scores_by_option() {
        let mut scoreboard = Scoreboard::new();
        let score = Score::new(dec!(1)).unwrap();
        scoreboard.add_score("B", score);
        scoreboard.add_score("A", score);

        assert_eq!(scoreboard.get_winners().1, vec!["A", "B"]);
        assert_eq!(
            scoreboard.into_sorted_descending(),
            vec![("A", score), ("B", score)]
        );
    }
}
