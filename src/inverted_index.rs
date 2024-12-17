use std::collections::{HashMap, HashSet};
use std::sync::atomic::{self, AtomicUsize};
use std::sync::RwLock;
use std::vec::Vec;
use serde::{Serialize, Deserialize};

#[derive(Clone, Debug)]
pub struct Document {
    pub name: String,
    pub content: String,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq, Hash)]
pub struct DocReference {
    pub doc_id: usize,
    pub matches: usize,
}

pub struct InvertedIndex {
    index: RwLock<HashMap<String, Vec<DocReference>>>,
    next_doc_id: AtomicUsize,
    documents: RwLock<HashMap<usize, Document>>,
}

impl InvertedIndex {
    pub fn new() -> Self {
        InvertedIndex {
            index: RwLock::new(HashMap::new()),
            next_doc_id: AtomicUsize::new(0),
            documents: RwLock::new(HashMap::new()),
        }
    }

    pub fn add_document(&self, document: Document) -> usize {
        let doc_id = self.next_doc_id.fetch_add(1, atomic::Ordering::Relaxed);

        let tokens = self.tokenize(&document.content);
        let mut token_counts: HashMap<String, usize> = HashMap::new();
        for token in tokens {
            *token_counts.entry(token).or_default() += 1;
        }

        {
            let mut index = self.index.write().unwrap();

            for (token, matches) in token_counts.into_iter() {
                index
                    .entry(token)
                    .or_insert_with(Vec::new)
                    .push(DocReference { doc_id, matches });
            }
        }

        self.documents.write().unwrap().insert(doc_id, document);

        doc_id
    }

    pub fn search(&self, query: &str) -> Vec<DocReference> {
        let index = self.index.read().unwrap();

        let tokens = self.tokenize(query);

        let mut results: HashSet<DocReference> = HashSet::new();

        for token in tokens {
            if let Some(references) = index.get(&token) {
                if results.is_empty() {
                    results = HashSet::from_iter(references.iter().cloned());
                } else {
                    let references: HashSet<DocReference> =
                        HashSet::from_iter(references.iter().cloned());
                    results.retain(|doc_ref| references.contains(doc_ref));
                }
            } else {
                return Vec::new();
            }
        }

        results.into_iter().collect()
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

    pub fn get_document(&self, doc_id: usize) -> Option<Document> {
        self.documents
            .read()
            .unwrap()
            .get(&doc_id)
            .map(|doc_ref| doc_ref.clone())
    }
}
