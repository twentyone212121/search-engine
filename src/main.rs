mod thread_pool;
mod inverted_index;

use thread_pool::ThreadPool;
use inverted_index::{InvertedIndex, Document};
use std::{
    fs, io::{self, prelude::*, BufReader}, net::{TcpListener, TcpStream}, path::{Path, PathBuf}, sync::Arc
};

fn main() -> io::Result<()> {
    let listener = TcpListener::bind("127.0.0.1:7878").unwrap();
    let pool = ThreadPool::new(4);
    let index = Arc::new(InvertedIndex::new());
    let corpus_dir = "/Users/deniskyslytsyn/Documents/КПІшка))/7 семестр/ПО курс/aclImdb/train/pos";

    for path in txt_files_in_dir(corpus_dir)? {
        let index = Arc::clone(&index);
        pool.execute(move || {
            if let Err(e) = add_file_to_index(path.as_path(), &index) {
                eprintln!("Error processing file {}: {}", path.to_string_lossy(), e);
            }
        })
    }
    pool.join();
    println!("Indexing complete. Total documents: {}", index.document_count());
    println!("Unique terms: {}", index.term_count());

    for stream in listener.incoming().take(10) {
        let stream = stream.unwrap();

        let index = Arc::clone(&index);

        pool.execute(move || {
            handle_connection(stream, &index);
        });
    }
    println!("Shutting down.");

    Ok(())
}

fn txt_files_in_dir(dir_path: &str) -> io::Result<Vec<PathBuf>> {
    Ok(fs::read_dir(dir_path)?
        .filter_map(|entry| entry.ok())
        .map(|entry| entry.path())
        .filter(|path| path.is_file() && path.extension().map_or(false, |ext| ext == "txt"))
        .collect::<Vec<_>>()
    )
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
    println!("Indexed file: {:?}", path);
    Ok(())
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

    // Parse the request to check for search query
    let parts: Vec<&str> = request_line.split_whitespace().collect();
    let (status_line, contents) = if parts.len() >= 3 && parts[0] == "GET" {
        match parse_search_query(parts[1], index) {
            Some(search_results) => ("HTTP/1.1 200 OK", search_results),
            None => match parts[1] {
                "/" => ("HTTP/1.1 200 OK", "Welcome to the Inverted Index Search Server. Use /search?q=your_query to search.".to_string()),
                _ => ("HTTP/1.1 404 NOT FOUND", "404 Not Found".to_string())
            }
        }
    } else {
        ("HTTP/1.1 400 BAD REQUEST", "Invalid Request".to_string())
    };

    let length = contents.len();
    let response =
        format!("{status_line}\r\nContent-Length: {length}\r\n\r\n{contents}");
    stream.write_all(response.as_bytes()).unwrap();
}

fn parse_search_query(path: &str, index: &InvertedIndex) -> Option<String> {
    if !path.starts_with("/search?q=") {
        return None;
    }

    // URL decode the query
    let query = match urlencoding::decode(&path[10..]) {
        Ok(decoded) => decoded.to_string(),
        Err(_) => return Some("Error decoding search query".to_string()),
    };

    // Perform the search
    let results = index.search(&query);

    // Format the results
    if results.is_empty() {
        Some(format!("No results found for query: {}", query))
    } else {
        let mut output = format!("Search results for '{}':\n", query);
        for (doc_id, references) in results {
            output.push_str(&format!("Document ID: {}\n", doc_id));
            output.push_str(&format!("References: {:?}\n\n", references));
        }
        Some(output)
    }
}
