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
    Testnet, Mainnet
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
    /// Use config file (./.bos-loader.toml) to set account_id and path, causes other args to be ignored
    #[arg(short = 'c')]
    use_config: bool,

    #[clap(short, long, value_enum, default_value_t=Network::Testnet)]
    network: Network 
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

fn handle_request(account_id: &str, path: PathBuf, network: &Network) -> HashMap<String, ComponentCode> {
    let mut components = HashMap::new();
    get_file_list(&path, account_id, &mut components, String::from(""), &network);
    components
}

fn replace_placeholders(code: &str, account_id: &str, network: &Network) -> String {
    let mut replacements = HashMap::new();
    replacements.insert("${REPL_ACCOUNT}", account_id);
    replacements.insert("${REPL_NEAR_URL}", if network == &Network::Testnet {"test.near.org"} else {"near.org"});
    replacements.insert("${REPL_SOCIALDB_CONTRACT}", if network == &Network::Testnet {"v1.social08.testnet"} else {"social.near"});

    let mut modified_string = String::from(code);

    for (substring, value) in replacements {
        modified_string = modified_string.replace(substring, value);
    }

    modified_string
}

fn get_file_list(
    path: &PathBuf,
    account_id: &str,
    components: &mut HashMap<String, ComponentCode>,
    prefix: String,
    network: &Network
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
                network
            );
            continue;
        }
        let mut file_key: Vec<&str> = file_name.split('.').collect();
        let extension = file_key.pop();

        match extension {
            Some("jsx") => {}
            // Some("tsx") => {} // enable once tsx is supported
            _ => continue,
        }

        let fkey = file_key.join(".");
        let key = format!("{account_id}/widget/{prefix}{fkey}");
        let mut file = fs::File::open(&file_path).unwrap();
        let mut contents = String::new();
        file.read_to_string(&mut contents).unwrap();
        contents = replace_placeholders(contents.as_str(), account_id, network);
        components.insert(key, ComponentCode { code: contents });
    }
}

#[tokio::main]
async fn main() {
    let args = Args::parse();

    let account_paths: Vec<AccountPath>;
    if args.use_config {
        let settings = Config::builder()
            .add_source(config::File::with_name("./.bos-loader").required(false))
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

    let display_paths = account_paths.clone();
    let cors = warp::cors()
        .allow_any_origin()
        .allow_methods(&[Method::GET]);
    let api = warp::get()
        .map(move || {
            let mut components: HashMap<String, ComponentCode> = HashMap::new();
            let network = args.network.to_owned();
            for account_path in account_paths.iter() {
                components.extend(handle_request(
                    &account_path.account,
                    account_path.path.to_owned(),
                    &network
                ));
            }
            warp::reply::json(&components)
        })
        .with(cors);

    let display_paths_str = display_paths
        .iter()
        .map(|ap| format!("{} as account {}", ap.path.to_string_lossy(), ap.account))
        .collect::<Vec<String>>()
        .join("\n");
    println!(
        "\nServing .jsx files on http://127.0.0.1:3030\n\n{}",
        display_paths_str
    );

    warp::serve(api).run(([127, 0, 0, 1], 3030)).await;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_replace_placeholders_testnet() {
        let input_string = String::from("<div> This is ${REPL_SOCIALDB_CONTRACT} </div> <Widget src=\"${REPL_ACCOUNT}/widget/SomeWidget\"> <div>${REPL_NEAR_URL}</div>");
        let expected_output = String::from("<div> This is v1.social08.testnet </div> <Widget src=\"MY_ACCOUNT/widget/SomeWidget\"> <div>test.near.org</div>");

        let modified_string = replace_placeholders(&input_string, "MY_ACCOUNT", &Network::Testnet);

        assert_eq!(modified_string, expected_output);
    }

    #[test]
    fn test_replace_placeholders_mainnet() {
        let input_string = String::from("<div> This is ${REPL_SOCIALDB_CONTRACT} </div> <Widget src=\"${REPL_ACCOUNT}/widget/SomeWidget\"> <div>${REPL_NEAR_URL}</div>");
        let expected_output = String::from("<div> This is social.near </div> <Widget src=\"MY_ACCOUNT/widget/SomeWidget\"> <div>near.org</div>");

        let modified_string = replace_placeholders(&input_string, "MY_ACCOUNT", &Network::Mainnet);

        assert_eq!(modified_string, expected_output);
    }

    #[test]
    fn test_replace_placeholders_wrong_notation() {
        let input_string = String::from("${REPL_SOCIALDB_CONTRACT REPL_ACCOUNT $REPL_ACCOUNT ${WRONG_PLACEHOLDER}");
        let expected_output = String::from(input_string.clone());
        
        let modified_string = replace_placeholders(&input_string, "MY_ACCOUNT", &Network::Testnet);

        assert_eq!(modified_string, expected_output);
    }
}
