use std::collections::HashMap;

pub struct VectorIndex {
    dimension: usize,
    storage: HashMap<String, Vec<f32>>,
}

impl VectorIndex {
    pub fn new(dimension: usize) -> Self {
        Self {
            dimension,
            storage: HashMap::new(),
        }
    }

    pub fn insert(&mut self, id: &str, vector: Vec<f32>) {
        if vector.len() == self.dimension {
            self.storage.insert(id.to_string(), vector);
        }
    }

    pub fn get(&self, id: &str) -> Option<&[f32]> {
        self.storage.get(id).map(|v| v.as_slice())
    }

    pub fn len(&self) -> usize {
        self.storage.len()
    }

    pub fn remove(&mut self, id: &str) {
        self.storage.remove(id);
    }

    pub fn clear(&mut self) {
        self.storage.clear();
    }

    pub fn search(&self, query: &[f32], top_k: usize) -> Vec<String> {
        let mut scored: Vec<(f32, &String)> = self
            .storage
            .iter()
            .map(|(id, vec)| {
                let sim = cosine_similarity(query, vec);
                (sim, id)
            })
            .collect();

        scored.sort_by(|a, b| {
            b.0.partial_cmp(&a.0).unwrap_or(std::cmp::Ordering::Equal)
        });

        scored.into_iter().take(top_k).map(|(_, id)| id.clone()).collect()
    }

    pub fn search_with_scores(&self, query: &[f32], top_k: usize) -> Vec<(String, f32)> {
        let mut scored: Vec<(f32, &String)> = self
            .storage
            .iter()
            .map(|(id, vec)| (cosine_similarity(query, vec), id))
            .collect();

        scored.sort_by(|a, b| b.0.partial_cmp(&a.0).unwrap_or(std::cmp::Ordering::Equal));

        scored.into_iter().take(top_k).map(|(s, id)| (id.clone(), s)).collect()
    }

    pub fn dimension(&self) -> usize {
        self.dimension
    }
}

pub fn cosine_similarity(a: &[f32], b: &[f32]) -> f32 {
    if a.len() != b.len() || a.is_empty() {
        return 0.0;
    }

    let dot: f32 = a.iter().zip(b.iter()).map(|(x, y)| x * y).sum();
    let mag_a: f32 = a.iter().map(|x| x * x).sum::<f32>().sqrt();
    let mag_b: f32 = b.iter().map(|x| x * x).sum::<f32>().sqrt();

    if mag_a == 0.0 || mag_b == 0.0 {
        0.0
    } else {
        (dot / (mag_a * mag_b)).clamp(-1.0, 1.0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_vec(v: &[f32], dim: usize) -> Vec<f32> {
        let mut result = vec![0.0; dim];
        for (i, &val) in v.iter().enumerate() {
            if i < dim {
                result[i] = val;
            }
        }
        let mag: f32 = result.iter().map(|x| x * x).sum::<f32>().sqrt();
        if mag > 0.0 {
            for x in &mut result {
                *x /= mag;
            }
        }
        result
    }

    #[test]
    fn test_insert_and_search() {
        let mut idx = VectorIndex::new(4);

        idx.insert("a", make_vec(&[1.0, 0.0, 0.0, 0.0], 4));
        idx.insert("b", make_vec(&[0.0, 1.0, 0.0, 0.0], 4));
        idx.insert("c", make_vec(&[0.5, 0.5, 0.0, 0.0], 4));

        let query = make_vec(&[1.0, 0.0, 0.0, 0.0], 4);
        let results = idx.search(&query, 3);

        assert_eq!(results[0], "a");
        assert_eq!(results[1], "c");
        assert_eq!(results.len(), 3);
    }

    #[test]
    fn test_cosine_similarity_identical() {
        let a = make_vec(&[1.0, 2.0, 3.0], 3);
        let b = make_vec(&[1.0, 2.0, 3.0], 3);
        let sim = cosine_similarity(&a, &b);
        assert!((sim - 1.0).abs() < 1e-6);
    }

    #[test]
    fn test_cosine_similarity_orthogonal() {
        let a = make_vec(&[1.0, 0.0], 2);
        let b = make_vec(&[0.0, 1.0], 2);
        let sim = cosine_similarity(&a, &b);
        assert!((sim - 0.0).abs() < 1e-6);
    }

    #[test]
    fn test_empty_index() {
        let idx = VectorIndex::new(4);
        let query = make_vec(&[1.0, 0.0, 0.0, 0.0], 4);
        let results = idx.search(&query, 10);
        assert!(results.is_empty());
    }

    #[test]
    fn test_remove() {
        let mut idx = VectorIndex::new(2);
        idx.insert("x", make_vec(&[1.0, 0.0], 2));
        idx.insert("y", make_vec(&[0.0, 1.0], 2));
        assert_eq!(idx.len(), 2);

        idx.remove("x");
        assert_eq!(idx.len(), 1);

        let query = make_vec(&[1.0, 0.0], 2);
        let results = idx.search(&query, 5);
        assert_eq!(results, vec!["y"]);
    }
}
