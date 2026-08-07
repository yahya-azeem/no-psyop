pub mod detector;
pub mod classifier;

use crate::types::Post;

pub struct MLPipeline {
    detector: detector::SyntheticDetector,
    classifier: classifier::BaitClassifier,
}

impl MLPipeline {
    pub fn new() -> Self {
        Self {
            detector: detector::SyntheticDetector::new(),
            classifier: classifier::BaitClassifier::new(),
        }
    }

    pub fn filter_post(&self, post: &Post) -> PostFilterResult {
        let is_synthetic = self.detector.classify(&post.content);
        let bait_score = self.classifier.score(&post.content);
        let should_filter = is_synthetic || bait_score > 0.8;

        PostFilterResult {
            is_synthetic,
            bait_score,
            should_filter,
        }
    }
}

#[derive(Clone, serde::Serialize)]
pub struct PostFilterResult {
    pub is_synthetic: bool,
    pub bait_score: f32,
    pub should_filter: bool,
}

impl PostFilterResult {
    pub fn is_quality(&self) -> bool {
        !self.should_filter
    }
}
