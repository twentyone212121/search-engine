mod inverted_index;
mod search_server;
mod thread_pool;

use clap::Parser;
use std::io;
use std::net::{IpAddr, Ipv4Addr};
use std::path::PathBuf;

use search_server::SearchServer;

#[derive(Parser, Debug)]
#[command(version, about, long_about = None)]
struct Config {
    #[arg(long, default_value_t = IpAddr::V4(Ipv4Addr::LOCALHOST))]
    ip: IpAddr,

    #[arg(long, default_value_t = 8080)]
    port: u16,

    #[arg(long)]
    corpus_dir: PathBuf,

    #[arg(long, default_value_t = 4)]
    thread_num: usize,
}

fn main() -> io::Result<()> {
    let config = Config::parse();

    let server =
        SearchServer::new(config.ip, config.port, config.corpus_dir, config.thread_num)?;

    server.run()
}
