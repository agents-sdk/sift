//! BM25 相关性打分。
//! 用途：压缩时决定哪些条目值得保留（pin）——与当前 query 越相关越优先。

use std::collections::HashMap;

/// 相关性打分器。
pub trait RelevanceScorer: Send + Sync {
    fn name(&self) -> &'static str;
    /// 给候选文档打分，分数越高越相关。
    fn score(&self, query: &str, document: &str) -> f64;
}

const K1: f64 = 1.2;
const B: f64 = 0.75;

/// 经典 BM25 打分器（语料统计在每次调用时按候选集现算，无全局索引）。
#[derive(Default)]
pub struct Bm25Scorer;

impl Bm25Scorer {
    pub fn new() -> Self {
        Self
    }
}

/// 一次查询上下文：预分词 query + 候选集文档频率。
struct Bm25Context {
    query_terms: Vec<String>,
    df: HashMap<String, usize>,
    avgdl: f64,
    n: usize,
}

fn tokenize(text: &str) -> Vec<String> {
    text.to_lowercase()
        .split(|c: char| !c.is_alphanumeric())
        .filter(|t| !t.is_empty())
        .map(|t| t.to_string())
        .collect()
}

impl Bm25Context {
    /// 从语料（全部候选文档）构建统计。
    fn build(query: &str, corpus: &[&str]) -> Self {
        let n = corpus.len().max(1);
        let mut df: HashMap<String, usize> = HashMap::new();
        let mut total_len = 0usize;
        for doc in corpus {
            let terms = tokenize(doc);
            total_len += terms.len();
            let mut seen = std::collections::HashSet::new();
            for t in terms {
                if seen.insert(t.clone()) {
                    *df.entry(t).or_default() += 1;
                }
            }
        }
        Self {
            query_terms: tokenize(query),
            df,
            avgdl: total_len as f64 / n as f64,
            n,
        }
    }

    fn score(&self, doc: &str) -> f64 {
        let terms = tokenize(doc);
        let dl = terms.len() as f64;
        let mut tf: HashMap<&str, usize> = HashMap::new();
        for t in &terms {
            *tf.entry(t.as_str()).or_default() += 1;
        }
        let mut score = 0.0;
        for q in &self.query_terms {
            let Some(&f) = tf.get(q.as_str()) else {
                continue;
            };
            let df = (*self.df.get(q).unwrap_or(&0)) as f64;
            let idf = (((self.n as f64 - df + 0.5) / (df + 0.5)) + 1.0).ln();
            let f = f as f64;
            let norm = f * (K1 + 1.0) / (f + K1 * (1.0 - B + B * dl / self.avgdl.max(1.0)));
            score += idf * norm;
        }
        score
    }
}

impl RelevanceScorer for Bm25Scorer {
    fn name(&self) -> &'static str {
        "bm25"
    }

    /// 单文档打分：无候选集时退化为词频余弦近似（df 全为 1）。
    fn score(&self, query: &str, document: &str) -> f64 {
        Bm25Context::build(query, &[document]).score(document)
    }
}

/// 给候选集按相关性排序，返回按分数降序的下标。
/// SmartCrusher planning 用它决定 pin 哪些条目。
pub fn rank_by_relevance(scorer: &dyn RelevanceScorer, query: &str, candidates: &[String]) -> Vec<usize> {
    let mut indexed: Vec<(usize, f64)> = candidates
        .iter()
        .enumerate()
        .map(|(i, c)| (i, scorer.score(query, c)))
        .collect();
    indexed.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
    indexed.into_iter().map(|(i, _)| i).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn relevant_doc_scores_higher() {
        let s = Bm25Scorer::new();
        let q = "rust cargo build error";
        let hit = s.score(q, "cargo build failed with rust compiler error E0308");
        let miss = s.score(q, "the weather is nice today for a walk");
        assert!(hit > miss);
    }

    #[test]
    fn ranking_orders_by_score() {
        let cands = vec![
            "unrelated text about cooking".to_string(),
            "error in cargo build".to_string(),
            "another cooking recipe".to_string(),
        ];
        let order = rank_by_relevance(&Bm25Scorer::new(), "cargo build error", &cands);
        assert_eq!(order[0], 1);
    }

    #[test]
    fn empty_query_is_neutral() {
        let s = Bm25Scorer::new();
        assert_eq!(s.score("", "anything"), 0.0);
    }
}
