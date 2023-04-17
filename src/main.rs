use clap::Parser;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::io::Read;
use std::path::PathBuf;
use warp::{http::Method, Filter};

#[derive(Parser, Debug)]
#[command(author, version, about, long_about = None)]
struct Args {
    account_id: String,
    #[clap(short, long, default_value = ".")]
    path: PathBuf,
}

#[derive(Serialize, Deserialize)]
struct FileList {
    components: HashMap<String, ComponentCode>,
}

#[derive(Serialize, Deserialize)]
struct ComponentCode {
    code: String,
}

fn handle_request(account_id: &str, path: PathBuf) -> FileList {
    let mut components = HashMap::new();
    get_file_list(&path, account_id, &mut components, String::from(""));
    FileList { components }
}

fn get_file_list(
    path: &PathBuf,
    account_id: &str,
    components: &mut HashMap<String, ComponentCode>,
    prefix: String,
) {
    let paths = fs::read_dir(path).unwrap();
    for path_res in paths {
        let path = path_res.unwrap();
        let file_path = path.path();
        let file_name = file_path.file_name().unwrap().to_string_lossy().to_string();
        if path.file_type().unwrap().is_dir() {
            get_file_list(
                &file_path,
                account_id,
                components,
                prefix.to_owned() + &file_name + ".",
            );
            continue;
        }
        let mut file_key: Vec<&str> = file_name.split('.').collect();
        let extension = file_key.pop();

        match extension {
            Some("jsx") => {}
            Some("tsx") => {}
            _ => continue,
        }

        let fkey = file_key.join(".");
        let key = format!("{account_id}/widget/{prefix}{fkey}");
        let mut file = fs::File::open(&file_path).unwrap();
        let mut contents = String::new();
        file.read_to_string(&mut contents).unwrap();
        components.insert(key, ComponentCode { code: contents });
    }
}

#[tokio::main]
async fn main() {
    let args = Args::parse();

    let serve_path = args.path.clone();

    let cors = warp::cors()
        .allow_any_origin()
        .allow_methods(&[Method::GET]);
    let api = warp::get()
        .map(move || {
            let account = args.account_id.to_owned();
            let path = args.path.to_owned();
            let files = handle_request(&account, path);
            warp::reply::json(&files)
        })
        .with(cors);

    println!(
        "\nFiles in {} ending in .jsx or .tsx will be served\n",
        serve_path.display()
    );

    warp::serve(api).run(([127, 0, 0, 1], 3030)).await;
}
