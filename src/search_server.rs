use std::{fs, thread};
use std::io::{self, prelude::*, BufReader};
use std::collections::HashMap;
use std::net::{IpAddr, TcpListener, TcpStream};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, SystemTime};
use serde::{Serialize, Deserialize};
use serde_json;

use crate::inverted_index::{Document, DocReference, InvertedIndex};
use crate::thread_pool::ThreadPool;

pub struct SearchServer {
    listener: TcpListener,
    pool: ThreadPool,
    index: Arc<InvertedIndex>,
    corpus_dir: PathBuf,
}

impl SearchServer {
    pub fn new(ip: IpAddr, port: u16, corpus_dir: PathBuf, thread_num: usize) -> io::Result<Self> {
        let listener =
            TcpListener::bind((ip, port)).expect("Failed to bind to specified address and port");

        let pool = ThreadPool::new(thread_num);
        let index = Arc::new(InvertedIndex::new());

        let mut server = Self {
            listener,
            pool,
            index,
            corpus_dir,
        };

        server.index_initial_corpus()?;

        server.setup_directory_watcher();

        Ok(server)
    }

    fn index_initial_corpus(&mut self) -> io::Result<()> {
        let corpus = txt_files_in_dir(&self.corpus_dir)?;

        for path in corpus.clone() {
            let index = Arc::clone(&self.index);
            self.pool.execute(move || {
                if let Err(e) = add_file_to_index(path.as_path(), &index) {
                    eprintln!("Error processing file {}: {}", path.to_string_lossy(), e);
                } else {
                    println!("Indexed file: {}", path.to_string_lossy());
                }
            });
        }

        self.pool.join();

        println!(
            "Indexing complete. Total documents: {}",
            self.index.document_count()
        );
        println!("Unique terms: {}", self.index.term_count());

        Ok(())
    }

    fn setup_directory_watcher(&self) {
        let corpus_dir = self.corpus_dir.clone();
        let index = Arc::clone(&self.index);

        self.pool.execute(move || {
            let corpus = txt_files_in_dir(&corpus_dir).unwrap_or_default();

            watch_directory(
                &corpus_dir,
                corpus,
                Duration::from_secs(1),
                |path| {
                    println!("New file {} detected", path.to_string_lossy());
                    if let Err(e) = add_file_to_index(path, &index) {
                        eprintln!(
                            "Error processing new file {}: {}",
                            path.to_string_lossy(),
                            e
                        );
                    } else {
                        println!("Indexed file: {}", path.to_string_lossy());
                    }
                },
                |path| {
                    println!("Modified file {} detected", path.to_string_lossy());
                    if let Err(e) = add_file_to_index(path, &index) {
                        eprintln!(
                            "Error processing modified file {}: {}",
                            path.to_string_lossy(),
                            e
                        );
                    } else {
                        println!("Indexed file: {}", path.to_string_lossy());
                    }
                },
            );
        });
    }

    pub fn run(&self) -> io::Result<()> {
        for stream in self.listener.incoming() {
            let stream = stream?;
            let index = Arc::clone(&self.index);

            self.pool.execute(move || {
                handle_connection(stream, &index);
            });
        }

        Ok(())
    }
}

fn txt_files_in_dir(dir_path: &Path) -> io::Result<Vec<PathBuf>> {
    Ok(fs::read_dir(dir_path)?
        .filter_map(|entry| entry.ok())
        .map(|entry| entry.path())
        .filter(|path| path.is_file() && path.extension().map_or(false, |ext| ext == "txt"))
        .collect::<Vec<_>>())
}

fn watch_directory<F, G>(
    dir_path: &Path,
    present_files: Vec<PathBuf>,
    interval: Duration,
    on_new_file: F,
    on_modified_file: G,
) where
    F: Fn(&Path),
    G: Fn(&Path),
{
    let mut present_files = present_files
        .into_iter()
        .map(|path_buf| {
            let modified = path_buf.metadata().map_or(SystemTime::now(), |meta| {
                meta.modified().unwrap_or(SystemTime::now())
            });
            (path_buf, modified)
        })
        .collect::<HashMap<PathBuf, SystemTime>>();

    loop {
        if let Ok(new_paths) = txt_files_in_dir(dir_path) {
            for path in new_paths {
                let modified = path.metadata().map_or(SystemTime::now(), |meta| {
                    meta.modified().unwrap_or(SystemTime::now())
                });
                match present_files.get(&path) {
                    Some(prev_modified) => {
                        if *prev_modified < modified {
                            // File was modified
                            on_modified_file(&path);
                        }
                    }
                    None => {
                        // File was added
                        on_new_file(&path);
                    }
                }
                present_files.insert(path, modified);
            }
        }
        thread::sleep(interval);
    }
}

fn add_file_to_index(path: &Path, index: &InvertedIndex) -> io::Result<()> {
    let mut file = fs::File::open(path)?;
    let mut content = String::new();
    file.read_to_string(&mut content)?;

    let document = Document {
        name: path.file_name().unwrap().to_string_lossy().into_owned(),
        content,
    };

    index.add_document(document);
    Ok(())
}

#[derive(Serialize, Deserialize, Debug)]
enum HttpStatus {
    Ok,
    BadRequest,
    NotFound,
}

#[derive(Serialize, Deserialize, Debug)]
#[serde(untagged)]
enum Response {
    Help(WelcomeResponse),
    Search(SearchResponse),
    Document(DocumentResponse),
    Error(ErrorResponse),
}

#[derive(Serialize, Deserialize, Debug)]
struct WelcomeResponse {
    message: String,
    endpoints: Vec<String>,
}

#[derive(Serialize, Deserialize, Debug)]
struct SearchResponse {
    query: String,
    total_results: usize,
    results: Vec<DocReference>,
}

#[derive(Serialize, Deserialize, Debug)]
struct DocumentResponse {
    document_id: usize,
    filename: String,
    content: String,
}

#[derive(Serialize, Deserialize, Debug)]
struct ErrorResponse {
    error: String,
}

fn handle_connection(mut stream: TcpStream, index: &InvertedIndex) {
    let buf_reader = BufReader::new(&stream);
    let request_line = match buf_reader.lines().next() {
        Some(Ok(line)) => line,
        Some(Err(e)) => {
            eprintln!("Error while reading: {}", e);
            return;
        }
        None => {
            eprintln!("Nothing to read from the stream");
            return;
        }
    };

    let (status, response) = process_request(&request_line, index);
    let status_line = match status {
        HttpStatus::Ok => "HTTP/1.1 200 OK",
        HttpStatus::BadRequest => "HTTP/1.1 400 BAD REQUEST",
        HttpStatus::NotFound => "HTTP/1.1 404 NOT FOUND",
    };

    let json_contents = serde_json::to_string(&response).unwrap();

    let length = json_contents.len();
    let response = format!("{status_line}\r\nContent-Type: application/json\r\nContent-Length: {length}\r\n\r\n{json_contents}");

    if let Err(e) = stream.write_all(response.as_bytes()) {
        eprintln!("Failed to write response: {}", e);
    }
}

fn process_request(request_line: &str, index: &InvertedIndex) -> (HttpStatus, Response) {
    let parts: Vec<&str> = request_line.split_whitespace().collect();

    if parts.len() < 3 {
        return (HttpStatus::BadRequest, Response::Error(ErrorResponse { error: "Invalid Request".to_string() }));
    }

    let method = parts[0];
    let uri = parts[1];

    if method != "GET" {
        return (HttpStatus::BadRequest, Response::Error(ErrorResponse { error: "Invalid Request Method".to_string() }));
    }

    match uri {
        "/" => (
            HttpStatus::Ok,
            Response::Help(WelcomeResponse {
                message: "Welcome to the Inverted Index Search Server".to_string(),
                endpoints: vec!["/search?q=<query>".to_string(), "/document?docID=<id>".to_string()],
            })
        ),
        query if query.starts_with("/search?q=") => handle_search_request(&query[10..], index),
        query if query.starts_with("/document?docID=") => handle_document_request(&query[16..], index),
        _ => (HttpStatus::NotFound, Response::Error(ErrorResponse { error: "404 Not Found".to_string() })),
    }
}

fn handle_search_request(query: &str, index: &InvertedIndex) -> (HttpStatus, Response) {
    match urlencoding::decode(query) {
        Ok(term) => {
            let results = index.search(&term);

            (HttpStatus::Ok, Response::Search(SearchResponse {
                query: term.to_string(),
                total_results: results.len(),
                results,
            }))
        },
        Err(_) => (HttpStatus::BadRequest, Response::Error(ErrorResponse {
            error: "Invalid Search Query".to_string(),
        })),
    }
}

fn handle_document_request(query: &str, index: &InvertedIndex) -> (HttpStatus, Response) {
    match urlencoding::decode(query)
        .map_err(|_| ())
        .and_then(|arg| arg.parse::<usize>().map_err(|_| ()))
    {
        Ok(doc_id) => {
            if let Some(document) = index.get_document(doc_id) {
                (HttpStatus::Ok, Response::Document(DocumentResponse {
                    document_id: doc_id,
                    filename: document.name,
                    content: document.content,
                }))
            } else {
                (HttpStatus::Ok, Response::Error(ErrorResponse {
                    error: "No file with specified docID was found".to_string(),
                }))
            }
        },
        Err(_) => (HttpStatus::BadRequest, Response::Error(ErrorResponse {
            error: "Invalid Document ID".to_string(),
        })),
    }
}
