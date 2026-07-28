pub struct EmbeddingEngine {
    fallback: FallbackEmbedder,
}

impl EmbeddingEngine {
    pub fn new() -> Self {
        log::info!("Using fallback embedder (no ONNX Runtime loaded)");
        Self {
            fallback: FallbackEmbedder::new(384),
        }
    }

    pub fn embed(&self, text: &str) -> Vec<f32> {
        self.fallback.embed(text)
    }

    pub fn embed_batch(&self, texts: &[String]) -> Vec<Vec<f32>> {
        texts.iter().map(|t| self.fallback.embed(t)).collect()
    }

    pub fn fallback_dimension(&self) -> usize {
        self.fallback.dimension
    }
}

impl Default for EmbeddingEngine {
    fn default() -> Self {
        Self::new()
    }
}

pub struct FallbackEmbedder {
    pub dimension: usize,
}

impl FallbackEmbedder {
    pub fn new(dimension: usize) -> Self {
        Self { dimension }
    }

    pub fn embed(&self, text: &str) -> Vec<f32> {
        let cleaned = text.trim().to_lowercase();
        if cleaned.is_empty() {
            return vec![0.0_f32; self.dimension];
        }

        let mut vec = vec![0.0f32; self.dimension];

        for ch in cleaned.chars() {
            let bucket = (ch as usize) % self.dimension;
            vec[bucket] += 1.0;
        }

        let chars: Vec<char> = cleaned.chars().collect();
        for pair in chars.windows(2) {
            let hash = (pair[0] as usize).wrapping_mul(31).wrapping_add(pair[1] as usize) % self.dimension;
            vec[hash] += 2.0;
        }

        let mag: f32 = vec.iter().map(|x| x * x).sum::<f32>().sqrt();
        if mag > 0.0 {
            for x in &mut vec {
                *x /= mag;
            }
        }

        vec
    }
}
