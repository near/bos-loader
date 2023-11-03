use clap::Parser;
use config::Config;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::io::Read;
use std::path::PathBuf;
use warp::{http::Method, Filter};

#[derive(clap::ValueEnum, Clone, Debug, PartialEq, Eq)]
enum Network {
    Testnet,
    Mainnet,
}

#[derive(Parser, Debug)]
#[command(
    author,
    version,
    about,
    long_about = "Serves the contents of BOS component files (.jsx) in a specified directory as a JSON object properly formatted for preview on a BOS gateway"
)]
struct Args {
    /// NEAR account to use as component author in preview
    account_id: Option<String>,
    /// Path to directory containing component files
    #[clap(short, long, default_value = ".", value_hint = clap::ValueHint::DirPath)]
    path: PathBuf,
    /// Use config file in current dir (./.bos-loader.toml) to set account_id and path, causes other args to be ignored
    #[arg(short = 'c')]
    use_config: bool,
    /// Run in BOS Web Engine mode
    #[arg(short = 'w')]
    web_engine: bool,
    /// Path to file with replacements map
    #[clap(short, long, value_hint = clap::ValueHint::DirPath)]
    replacements: Option<PathBuf>,
}

#[derive(Serialize, Deserialize)]
struct FileList {
    components: HashMap<String, ComponentCode>,
}

#[derive(Serialize, Deserialize)]
struct ComponentCode {
    code: String,
}

#[derive(Serialize, Deserialize, Clone)]
struct AccountPath {
    account: String,
    path: PathBuf,
}

fn handle_request(
    account_id: &str,
    path: PathBuf,
    replacements_map: &HashMap<String, String>,
    web_engine: &bool,
) -> HashMap<String, ComponentCode> {
    let mut components = HashMap::new();
    get_file_list(
        &path,
        account_id,
        &mut components,
        String::from(""),
        replacements_map,
        web_engine,
    );
    components
}

fn replace_placeholders(
    code: &str,
    account_id: &str,
    replacements_map: &HashMap<String, String>,
) -> String {
    let mut replacements = HashMap::clone(replacements_map);
    replacements.insert("${REPL_ACCOUNT}".to_owned(), account_id.to_owned());

    let mut modified_string = String::from(code);

    for (substring, value) in replacements {
        modified_string = modified_string.replace(substring.as_str(), value.as_str());
    }

    modified_string
}

fn read_replacements(
    path: Option<PathBuf>,
) -> Result<HashMap<String, String>, Box<dyn std::error::Error>> {
    let path = match path {
        Some(p) => p,
        None => return Ok(HashMap::new()), // Return an empty HashMap if the path is missing
    };

    let contents = fs::read_to_string(path)?;
    let json: serde_json::Value = serde_json::from_str(&contents)?;

    let map: HashMap<String, String> = json
        .as_object()
        .ok_or("Invalid JSON format")?
        .iter()
        .filter_map(|(k, v)| {
            if let serde_json::Value::String(v) = v {
                let key = format!("{}{}{}", "${", k, "}");
                Some((key, v.clone()))
            } else {
                None
            }
        })
        .collect();
    if map.contains_key("${REPL_ACCOUNT}") {
        panic!("The replacements file can't contain the REPL_ACCOUNT key. This key is reserved.")
    }

    Ok(map)
}

fn get_file_list(
    path: &PathBuf,
    account_id: &str,
    components: &mut HashMap<String, ComponentCode>,
    prefix: String,
    replacements_map: &HashMap<String, String>,
    web_engine: &bool,
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
                replacements_map,
                web_engine,
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
        let join_string = if *web_engine { "/" } else { "/widget/" };
        let key = format!("{account_id}{join_string}{prefix}{fkey}");
        let mut file = fs::File::open(&file_path).unwrap();
        let mut contents = String::new();
        file.read_to_string(&mut contents).unwrap();
        contents = replace_placeholders(contents.as_str(), account_id, replacements_map);
        components.insert(key, ComponentCode { code: contents });
    }
}

#[tokio::main]
async fn main() {
    let args = Args::parse();

    let account_paths: Vec<AccountPath>;
    if args.use_config {
        let settings = Config::builder()
            .add_source(config::File::with_name("./.bos-loader.toml"))
            .build()
            .expect("Failed to load config file");
        account_paths = settings
            .get::<Vec<AccountPath>>("paths")
            .expect("A valid path configuration was not found in config file");
    } else {
        account_paths = vec![AccountPath {
            account: args
                .account_id
                .expect("Account ID must be provided when not using configuration file")
                .clone(),
            path: args.path.clone(),
        }];
    }

    let replacements_path = args.replacements;
    let replacements_map: HashMap<String, String> = match read_replacements(replacements_path) {
        Ok(m) => m,
        Err(e) => panic!(
            "Something went wrong while parsing the replacement file: {}",
            e
        ),
    };

    let display_paths = account_paths.clone();
    let cors = warp::cors()
        .allow_any_origin()
        .allow_methods(&[Method::GET]);
    let api = warp::get()
        .map(move || {
            let mut components: HashMap<String, ComponentCode> = HashMap::new();
            for account_path in account_paths.iter() {
                components.extend(handle_request(
                    &account_path.account,
                    account_path.path.to_owned(),
                    &replacements_map,
                    &args.web_engine,
                ));
            }
            warp::reply::json(&FileList { components })
        })
        .with(cors);

    let display_paths_str = display_paths
        .iter()
        .map(|ap| format!("{} as account {}", ap.path.to_string_lossy(), ap.account))
        .collect::<Vec<String>>()
        .join("\n");
    println!(
        "\nServing .jsx/.tsx files on http://127.0.0.1:3030\n\n{}",
        display_paths_str
    );

    warp::serve(api).run(([127, 0, 0, 1], 3030)).await;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_replace_placeholders() {
        let input_string = String::from("<div> This is ${REPL_PLACEHOLDER1} </div> <Widget src=\"${REPL_ACCOUNT}/widget/SomeWidget\"> <div>${REPL_PLACEHOLDER2}</div>");
        let expected_output = String::from("<div> This is value1 </div> <Widget src=\"MY_ACCOUNT/widget/SomeWidget\"> <div>value2</div>");

        let replacements: HashMap<String, String> = vec![
            ("${REPL_PLACEHOLDER1}".to_owned(), "value1".to_owned()),
            ("${REPL_PLACEHOLDER2}".to_owned(), "value2".to_owned()),
        ]
        .into_iter()
        .collect();

        let modified_string = replace_placeholders(&input_string, "MY_ACCOUNT", &replacements);

        assert_eq!(modified_string, expected_output);
    }

    #[test]
    fn test_replace_placeholders_empty_map() {
        let input_string = String::from("<div> This is ${REPL_PLACEHOLDER1} </div> <Widget src=\"${REPL_ACCOUNT}/widget/SomeWidget\"> <div>${REPL_PLACEHOLDER2}</div>");
        let expected_output = String::from("<div> This is ${REPL_PLACEHOLDER1} </div> <Widget src=\"MY_ACCOUNT/widget/SomeWidget\"> <div>${REPL_PLACEHOLDER2}</div>");

        let modified_string = replace_placeholders(&input_string, "MY_ACCOUNT", &HashMap::new());

        assert_eq!(modified_string, expected_output);
    }

    #[test]
    fn test_replace_placeholders_wrong_notation() {
        let input_string =
            String::from("${REPL_ACCOUNT REPL_ACCOUNT $REPL_ACCOUNT ${WRONG_PLACEHOLDER}");
        let expected_output = String::from(input_string.clone());

        let replacements: HashMap<String, String> = vec![
            ("${REPL_PLACEHOLDER1}".to_owned(), "value1".to_owned()),
            ("${REPL_PLACEHOLDER2}".to_owned(), "value2".to_owned()),
        ]
        .into_iter()
        .collect();

        let modified_string = replace_placeholders(&input_string, "MY_ACCOUNT", &replacements);

        assert_eq!(modified_string, expected_output);
    }

    #[test]
    fn test_read_replacements() {
        let path: PathBuf = "./test/replacements.json".into();

        let expected_output: HashMap<String, String> = vec![
            ("${REPL_PLACEHOLDER1}".to_owned(), "value1".to_owned()),
            ("${REPL_PLACEHOLDER2}".to_owned(), "value2".to_owned()),
        ]
        .into_iter()
        .collect();

        let map = read_replacements(Some(path)).unwrap();

        assert_eq!(map, expected_output);
    }

    #[test]
    #[should_panic(
        expected = "The replacements file can't contain the REPL_ACCOUNT key. This key is reserved."
    )]
    fn test_read_replacements_repl_account() {
        let path: PathBuf = "./test/replacements.wrong.json".into();

        read_replacements(Some(path)).unwrap();
    }

    // TODO: add tests for config file multi-account setup
}
