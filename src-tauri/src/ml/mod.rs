pub mod detector;
pub mod classifier;
pub mod embedding;

use crate::types::Post;

pub struct MLPipeline {
    detector: detector::SyntheticDetector,
    classifier: classifier::BaitClassifier,
    embedder: embedding::EmbeddingEngine,
}

impl MLPipeline {
    pub fn new() -> Self {
        Self {
            detector: detector::SyntheticDetector::new(),
            classifier: classifier::BaitClassifier::new(),
            embedder: embedding::EmbeddingEngine::new(),
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

    pub fn generate_embedding(&self, text: &str) -> Vec<f32> {
        self.embedder.embed(text)
    }

    pub fn batch_filter(&self, posts: &[Post]) -> Vec<(Post, PostFilterResult)> {
        posts.iter()
            .map(|p| {
                let result = self.filter_post(p);
                (p.clone(), result)
            })
            .collect()
    }

    pub fn batch_embed_and_filter(&self, posts: &[Post]) -> Vec<(Post, PostFilterResult, Option<Vec<f32>>)> {
        let mut results = Vec::new();
        for post in posts {
            let filter = self.filter_post(post);
            let embedding = if !filter.should_filter {
                Some(self.embedder.embed(&post.content))
            } else {
                None
            };
            results.push((post.clone(), filter, embedding));
        }
        results
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
