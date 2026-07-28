pub struct BaitClassifier {
    subjective_indicators: Vec<&'static str>,
    emotional_triggers: Vec<&'static str>,
    numeric_deception: Vec<&'static str>,
    factual_indicators: Vec<&'static str>,
}

impl BaitClassifier {
    pub fn new() -> Self {
        Self {
            subjective_indicators: vec![
                "unbelievable", "shocking", "you won't believe", "mind-blowing",
                "incredible", "devastating", "heartbreaking", "must see",
                "viral", "everyone is talking", "woke", "destroyed", "exposed",
                "betrayal", "nightmare", "miracle", "disaster", "epic",
                "worst", "best ever", "never seen before", "changed forever",
                "speechless", "priceless", "absolutely", "completely",
                "totally", "literally", "omg", "wow", "cannot believe",
                "this is why", "the real reason", "what happens next",
                "you need to see", "don't miss", "life-changing",
                "game-changer", "mind-blowing", "jaw-dropping",
                "this changes everything", "we need to talk about",
                "the truth about", "what they don't want you to know",
            ],
            emotional_triggers: vec![
                "angry", "furious", "outraged", "disgusting", "revolting",
                "terrifying", "frightening", "scared", "afraid", "panic",
                "desperate", "hopeless", "devastated", "heartbreaking",
                "tragic", "horrific", "appalling", "shameful", "disgrace",
                "pathetic", "ridiculous", "absurd", "preposterous",
                "hilarious", "hysterical", "thrilled", "ecstatic", "overjoyed",
            ],
            numeric_deception: vec![
                "you won't believe what happened next", "number will shock you",
                "doctors hate this", "what happened next", "single trick",
                "one weird trick", "you'll be amazed", "secret they don't want",
                "they don't want you to know", "what the media won't tell you",
            ],
            factual_indicators: vec![
                "according to", "source", "reported by", "citing",
                "published in", "research from", "study by", "data from",
                "statistics show", "survey found", "analysis reveals",
                "according to a report", "peer-reviewed", "journal",
                "university of", "institute for", "world health",
                "government data", "official figures",
            ],
        }
    }

    pub fn score(&self, text: &str) -> f32 {
        let subjectivity = self.subjectivity_score(text);
        let emotional_charge = self.emotional_charge(text);
        let factual_specificity = self.factual_specificity(text);
        let deception_score = self.deception_patterns(text);
        let lexical_density = self.lexical_density(text);
        let length_penalty = self.length_penalty(text);

        let raw = subjectivity * 0.25
            + emotional_charge * 0.20
            + (1.0 - factual_specificity) * 0.20
            + deception_score * 0.15
            + (1.0 - lexical_density) * 0.10
            + length_penalty * 0.10;

        raw.clamp(0.0, 1.0)
    }

    fn subjectivity_score(&self, text: &str) -> f32 {
        let lower = text.to_lowercase();
        let matches = self.subjective_indicators.iter()
            .filter(|w| lower.contains(*w))
            .count();
        (matches as f32 / 8.0).clamp(0.0, 1.0)
    }

    fn emotional_charge(&self, text: &str) -> f32 {
        let lower = text.to_lowercase();
        let matches = self.emotional_triggers.iter()
            .filter(|w| lower.contains(*w))
            .count();
        (matches as f32 / 6.0).clamp(0.0, 1.0)
    }

    fn factual_specificity(&self, text: &str) -> f32 {
        let has_numbers = text.chars().any(|c| c.is_ascii_digit());
        let has_quotes = text.contains('"') || text.contains('"') || text.contains('"');
        let has_url = text.contains("http") || text.contains("www.");
        let has_percent = text.contains('%');
        let has_dates = text.contains("202") || text.contains("202");

        let indicators = self.factual_indicators.iter()
            .filter(|w| text.to_lowercase().contains(*w))
            .count();

        let structural = [has_numbers, has_quotes, has_url, has_percent, has_dates];
        let struct_score = structural.iter().filter(|&&s| s).count() as f32 / structural.len() as f32;

        (struct_score * 0.5) + ((indicators as f32 / 5.0).clamp(0.0, 1.0) * 0.5)
    }

    fn deception_patterns(&self, text: &str) -> f32 {
        let lower = text.to_lowercase();
        let matches = self.numeric_deception.iter()
            .filter(|w| lower.contains(*w))
            .count();

        let has_excessive_punctuation = text.chars().filter(|&c| c == '!' || c == '?').count() > 3;
        let has_allcaps = text.chars().filter(|c| c.is_uppercase()).count() as f32 > text.len() as f32 * 0.3;

        let mut score = (matches as f32 / 3.0).clamp(0.0, 1.0);
        if has_excessive_punctuation { score = (score + 0.2).min(1.0); }
        if has_allcaps { score = (score + 0.3).min(1.0); }

        score
    }

    fn lexical_density(&self, text: &str) -> f32 {
        let words: Vec<&str> = text.split_whitespace().collect();
        if words.len() < 3 {
            return 0.5;
        }

        let stop_words = [
            "a", "an", "the", "is", "are", "was", "were", "be", "been",
            "being", "have", "has", "had", "do", "does", "did", "will", "would",
            "could", "should", "may", "might", "shall", "can", "need", "dare",
            "ought", "used", "to", "of", "in", "for", "on", "with", "at", "by",
            "from", "as", "into", "through", "during", "before", "after", "above",
            "below", "between", "out", "off", "over", "under", "again", "further",
            "then", "once", "here", "there", "when", "where", "why", "how", "all",
            "each", "every", "both", "few", "more", "most", "other", "some", "such",
            "no", "nor", "not", "only", "own", "same", "so", "than", "too", "very",
            "just", "because", "but", "and", "or", "if", "while", "although",
            "however", "therefore", "thus", "also", "well", "really", "actually",
            "indeed", "still", "yet", "already", "even", "ever", "never", "now",
        ];

        let content_words: Vec<&&str> = words.iter().filter(|w| !stop_words.contains(*w)).collect();
        content_words.len() as f32 / words.len() as f32
    }

    fn length_penalty(&self, text: &str) -> f32 {
        let words: Vec<&str> = text.split_whitespace().collect();
        let len = words.len();
        if len < 5 {
            0.0
        } else if len < 15 {
            0.2
        } else if len < 30 {
            0.1
        } else {
            0.0
        }
    }
}
