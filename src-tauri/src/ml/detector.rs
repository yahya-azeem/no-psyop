use std::collections::HashMap;

pub struct SyntheticDetector {
    ngram_model: NGramModel,
}

impl SyntheticDetector {
    pub fn new() -> Self {
        Self {
            ngram_model: NGramModel::new(4),
        }
    }

    pub fn classify(&self, text: &str) -> bool {
        let score = self.score(text);
        score > 0.7
    }

    pub fn score(&self, text: &str) -> f32 {
        let ngram_anomaly = self.ngram_model.anomaly_score(text);
        let entropy_score = self.entropy_score(text);
        let repetition_score = self.repetition_penalty(text);
        let burstiness_score = self.burstiness_score(text);
        let formality_score = self.formality_score(text);

        let raw = ngram_anomaly * 0.35
            + (1.0 - entropy_score) * 0.25
            + repetition_score * 0.20
            + burstiness_score * 0.10
            + formality_score * 0.10;

        raw.clamp(0.0, 1.0)
    }

    fn entropy_score(&self, text: &str) -> f32 {
        if text.is_empty() {
            return 0.0;
        }

        let chars: Vec<char> = text.chars().collect();
        let len = chars.len() as f64;
        let mut counts: HashMap<char, usize> = HashMap::new();
        for c in &chars {
            *counts.entry(*c).or_insert(0) += 1;
        }

        let entropy: f64 = -counts.values().fold(0.0, |acc, &count| {
            let p = count as f64 / len;
            acc + if p > 0.0 { p * p.log2() } else { 0.0 }
        });

        let max_entropy = (counts.len() as f64).log2();
        if max_entropy > 0.0 {
            (entropy / max_entropy) as f32
        } else {
            1.0
        }
    }

    fn repetition_penalty(&self, text: &str) -> f32 {
        let words: Vec<&str> = text.split_whitespace().map(|w| w.trim_matches(|c: char| !c.is_alphanumeric())).filter(|w| !w.is_empty()).collect();

        if words.len() < 5 {
            return 0.0;
        }

        let unique: std::collections::HashSet<&str> = words.iter().copied().collect();
        1.0 - (unique.len() as f32 / words.len() as f32)
    }

    fn burstiness_score(&self, text: &str) -> f32 {
        let sentences: Vec<&str> = text.split(|c: char| c == '.' || c == '!' || c == '?')
            .map(|s| s.trim())
            .filter(|s| !s.is_empty())
            .collect();

        if sentences.len() < 3 {
            return 0.0;
        }

        let lens: Vec<usize> = sentences.iter().map(|s| s.split_whitespace().count()).collect();
        let mean = lens.iter().sum::<usize>() as f32 / lens.len() as f32;
        if mean == 0.0 {
            return 0.0;
        }

        let variance: f32 = lens.iter().map(|&l| (l as f32 - mean).powi(2)).sum::<f32>() / lens.len() as f32;
        let cv = variance.sqrt() / mean;

        (1.0 - cv).clamp(0.0, 1.0)
    }

    fn formality_score(&self, text: &str) -> f32 {
        let formal_markers = [
            "therefore", "thus", "consequently", "furthermore", "moreover",
            "nevertheless", "nonetheless", "notwithstanding", "accordingly",
            "subsequently", "in addition", "in conclusion", "as a result",
            "it is important to", "it should be noted", "it is worth noting",
            "research shows", "studies indicate", "data suggests",
            "consequently", "significantly", "notably", "particularly",
        ];

        let lower = text.to_lowercase();
        let matches = formal_markers.iter().filter(|m| lower.contains(*m)).count();
        (matches as f32 / 10.0).clamp(0.0, 1.0)
    }
}

struct NGramModel {
    n: usize,
}

impl NGramModel {
    fn new(n: usize) -> Self {
        Self { n }
    }

    fn anomaly_score(&self, text: &str) -> f32 {
        let chars: Vec<char> = text.chars().collect();
        if chars.len() < self.n {
            return 0.0;
        }

        let mut ngram_counts: HashMap<Vec<char>, usize> = HashMap::new();
        let mut total = 0;

        for window in chars.windows(self.n) {
            *ngram_counts.entry(window.to_vec()).or_insert(0) += 1;
            total += 1;
        }

        if total == 0 {
            return 0.0;
        }

        let unique = ngram_counts.len();
        let expected_unique = (total as f64).min(26usize.pow(self.n as u32) as f64);

        let ratio = unique as f64 / expected_unique;
        let llm_ratio = unique as f64 / total as f64;

        if llm_ratio < 0.3 && ratio < 0.5 {
            0.9
        } else if llm_ratio < 0.5 && ratio < 0.7 {
            0.6
        } else {
            (1.0 - llm_ratio as f32).clamp(0.0, 0.5)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_empty_text_not_synthetic() {
        let d = SyntheticDetector::new();
        assert!(!d.classify(""));
        assert!(d.score("") < 0.5);
    }

    #[test]
    fn test_short_text_not_synthetic() {
        let d = SyntheticDetector::new();
        assert!(!d.classify("hello world"));
    }

    #[test]
    fn test_natural_text_low_score() {
        let d = SyntheticDetector::new();
        let text = "hey guys just got back from the store and they were totally out of milk can you believe it";
        assert!(d.score(text) < 0.7);
    }

    #[test]
    fn test_repetitive_text_high_penalty() {
        let d = SyntheticDetector::new();
        let text = "great post thanks for sharing great post thanks for sharing great post thanks for sharing great post thanks for sharing";
        let score = d.score(text);
        assert!(score > 0.3, "repetition should increase score, got {}", score);
    }

    #[test]
    fn test_formal_markers_increase_score() {
        let d = SyntheticDetector::new();
        let text = "It should be noted that research shows data suggests therefore consequently furthermore this is clearly a significant finding that notably demonstrates the aforementioned phenomenon.";
        assert!(d.formality_score(text) > 0.3);
    }

    #[test]
    fn test_entropy_bounds() {
        let d = SyntheticDetector::new();
        let balanced = "ab";
        let skewed = "aaaaaaaab";
        let high_e = d.entropy_score(balanced);
        let low_e = d.entropy_score(skewed);
        assert!(high_e > low_e, "balanced text should have higher entropy, got {} <= {}", high_e, low_e);
        assert!(high_e <= 1.0);
        assert!(low_e >= 0.0);
    }
}
