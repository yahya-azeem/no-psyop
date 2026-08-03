pub mod vector;

use crate::ml::embedding::EmbeddingEngine;
use crate::types::{Platform, Post, UserIntent};

pub struct SemanticSearch {
    vector_index: vector::VectorIndex,
    embedder: EmbeddingEngine,
}

impl SemanticSearch {
    pub fn new() -> Self {
        Self {
            vector_index: vector::VectorIndex::new(384),
            embedder: EmbeddingEngine::new(),
        }
    }

    pub fn index_post(&mut self, post: &Post) {
        if let Some(embedding) = &post.vector_embedding {
            self.vector_index.insert(&post.id, embedding.clone());
        }
    }

    pub fn index_posts(&mut self, posts: &[Post]) {
        for post in posts {
            self.index_post(post);
        }
    }

    pub fn index_text(&mut self, id: &str, text: &str) -> Vec<f32> {
        let embedding = self.embedder.embed(text);
        self.vector_index.insert(id, embedding.clone());
        embedding
    }

    pub fn search(&self, intent: &UserIntent, top_k: usize) -> Vec<String> {
        if let Some(query_vec) = &intent.vector {
            self.vector_index.search(query_vec, top_k)
        } else {
            let query_embedding = self.embedder.embed(&intent.query);
            self.vector_index.search(&query_embedding, top_k)
        }
    }

    pub fn search_text(&self, query: &str, platform: Option<&Platform>, top_k: usize) -> Vec<String> {
        let query_vec = self.embedder.embed(query);
        let results = self.vector_index.search(&query_vec, top_k);

        if platform.is_none() {
            return results;
        }

        results
    }

    pub fn semantic_rank(&self, query: &str, candidates: &[Post]) -> Vec<(Post, f32)> {
        let query_vec = self.embedder.embed(query);
        let mut scored: Vec<(Post, f32)> = candidates
            .iter()
            .map(|p| {
                let post_vec = p.vector_embedding.as_ref()
                    .cloned()
                    .unwrap_or_else(|| self.embedder.embed(&p.content));
                let sim = vector::cosine_similarity(&query_vec, &post_vec);
                (p.clone(), sim)
            })
            .collect();

        scored.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
        scored
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::Platform;

    #[test]
    fn test_index_and_search() {
        let mut search = SemanticSearch::new();

        let results = search.search_text("hello world", None, 5);
        assert!(results.is_empty());

        search.index_text("post1", "dallas food");
        search.index_text("post2", "xxxx");

        let results = search.search_text("food in dallas", None, 5);
        assert!(!results.is_empty());
        assert_eq!(results[0], "post1");
    }

    #[test]
    fn test_semantic_rank() {
        let mut search = SemanticSearch::new();

        let post1 = Post {
            id: "p1".into(),
            platform: Platform::Instagram,
            author_id: "a1".into(),
            author_username: "user1".into(),
            content: "dallas food".into(),
            media_urls: vec![],
            poster_url: None,
            liker_ids: vec![],
            commenter_ids: vec![],
            timestamp: 1000,
            is_video: false,
            author_is_mutual: None,
            author_is_close_friend: None,
            engagement_score: None,
            is_synthetic: None,
            vector_embedding: Some(search.index_text("p1", "dallas food")),
        };

        let post2 = Post {
            id: "p2".into(),
            platform: Platform::Twitter,
            author_id: "a2".into(),
            author_username: "user2".into(),
            content: "xxxx".into(),
            media_urls: vec![],
            poster_url: None,
            liker_ids: vec![],
            commenter_ids: vec![],
            timestamp: 1001,
            is_video: false,
            author_is_mutual: None,
            author_is_close_friend: None,
            engagement_score: None,
            is_synthetic: None,
            vector_embedding: Some(search.index_text("p2", "xxxx")),
        };

        let ranked = search.semantic_rank("food in dallas", &[post1, post2]);
        assert_eq!(ranked[0].0.id, "p1");
        assert!(ranked[0].1 > ranked[1].1);
    }
}
