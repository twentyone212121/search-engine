use std::collections::{HashMap, HashSet};
use std::sync::atomic::{self, AtomicUsize};
use std::sync::RwLock;
use std::vec::Vec;

#[derive(Clone, Debug)]
pub struct Document {
    pub name: String,
    pub content: String,
}

#[derive(Clone, Debug)]
pub struct DocReference {
    pub doc_id: usize,
    pub positions: Vec<usize>,
}

pub struct InvertedIndex {
    index: RwLock<HashMap<String, Vec<DocReference>>>,
    next_doc_id: AtomicUsize,
}

impl InvertedIndex {
    pub fn new() -> Self {
        InvertedIndex {
            index: RwLock::new(HashMap::new()),
            next_doc_id: AtomicUsize::new(0),
        }
    }

    pub fn add_document(&self, document: Document) -> usize {
        let doc_id = self.next_doc_id.fetch_add(1, atomic::Ordering::Relaxed);

        let tokens = self.tokenize(&document.content);

        let mut index = self.index.write().unwrap();

        for (position, token) in tokens.into_iter().enumerate() {
            index
                .entry(token)
                .or_insert_with(Vec::new)
                .push(DocReference {
                    doc_id,
                    positions: vec![position],
                });
        }

        doc_id
    }

    pub fn search(&self, query: &str) -> Vec<(usize, Vec<DocReference>)> {
        let index = self.index.read().unwrap();

        let tokens = self.tokenize(query);

        let mut results: HashMap<usize, Vec<DocReference>> = HashMap::new();

        for token in tokens {
            if let Some(references) = index.get(&token) {
                let token_docs: HashSet<usize> = references
                    .iter()
                    .map(|r| r.doc_id)
                    .collect();

                if results.is_empty() {
                    for reference in references {
                        results.insert(reference.doc_id, vec![reference.clone()]);
                    }
                } else {
                    results = results
                        .into_iter()
                        .filter(|&(doc_id, _)| token_docs.contains(&doc_id))
                        .collect();

                    for (doc_id, doc_refs) in &mut results {
                        if let Some(new_refs) = references
                            .iter()
                            .filter(|r| r.doc_id == *doc_id)
                            .cloned()
                            .next() {
                            doc_refs.push(new_refs);
                        }
                    }
                }
            } else {
                return Vec::new();
            }
        }

        results
            .into_iter()
            .map(|(doc_id, references)| (doc_id, references))
            .collect()
    }

    fn tokenize(&self, text: &str) -> Vec<String> {
        text.to_lowercase()
            .split_whitespace()
            .map(|s| s.trim_matches(|c: char| !c.is_alphanumeric()).to_string())
            .filter(|s| !s.is_empty())
            .collect()
    }

    pub fn document_count(&self) -> usize {
        self.next_doc_id.load(atomic::Ordering::Relaxed)
    }

    pub fn term_count(&self) -> usize {
        self.index.read().unwrap().len()
    }
}
