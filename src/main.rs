use clap::Parser;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::io::Read;
use warp::{http::Method, Filter};

#[derive(Parser, Debug)]
#[command(author, version, about, long_about = None)]
struct Args {
    account_id: String,
}

#[derive(Serialize, Deserialize)]
struct FileList {
    components: HashMap<String, ComponentCode>,
}

#[derive(Serialize, Deserialize)]
struct ComponentCode {
    code: String,
}

fn handle_request(account_id: &str) -> FileList {
    let path = "./src"; // replace with your local directory path
    let file_list = get_file_list(path, account_id);
    file_list
}

fn get_file_list(path: &str, account_id: &str) -> FileList {
    let mut components = HashMap::new();
    let paths = fs::read_dir(path).unwrap();
    for path in paths {
        let file_path = path.unwrap().path();
        let file_name = file_path.file_name().unwrap().to_string_lossy().to_string();
        let mut file_key: Vec<&str> = file_name.split('.').collect();
        file_key.pop();
        let fkey = file_key.join("");
        let key = format!("{account_id}/widget/{fkey}");
        let mut file = fs::File::open(&file_path).unwrap();
        let mut contents = String::new();
        file.read_to_string(&mut contents).unwrap();
        components.insert(key, ComponentCode { code: contents });
    }
    FileList { components }
}

#[tokio::main]
async fn main() {
    let args = Args::parse();

    let cors = warp::cors()
        .allow_any_origin()
        .allow_methods(&[Method::GET]);
    let api = warp::get()
        .map(move || {
            let account = args.account_id.to_owned();
            let files = handle_request(&account);
            warp::reply::json(&files)
        })
        .with(cors);

    warp::serve(api).run(([127, 0, 0, 1], 3030)).await;
}
