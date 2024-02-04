use clap::Parser;
use config::Config;
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::{collections::HashMap, fs, io::Read, path::PathBuf};
use warp::{http::Method, Filter};

#[derive(Parser, Debug)]
#[command(
    author,
    version,
    about,
    long_about = "Serves the contents of BOS component files (.jsx) in a specified directory as a JSON object properly formatted for preview on a BOS gateway"
)]
struct Args {
    /// Path to directory containing component files
    #[clap(short, long, default_value = ".", value_hint = clap::ValueHint::DirPath)]
    path: PathBuf,
    /// Port to serve on
    #[arg(long, default_value = "3030")]
    port: u16,
    /// NEAR account to use as component author in preview
    account_id: Option<String>,
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
    path: PathBuf,
    account_id: String,
}

struct HandleRequestOptions<'hro> {
    path: &'hro PathBuf,
    web_engine: bool,
    account_id: &'hro str,
    replacements_map: &'hro HashMap<String, String>,
}

fn handle_request(
    HandleRequestOptions {
        path,
        web_engine,
        account_id,
        replacements_map,
    }: HandleRequestOptions,
) -> Result<HashMap<String, ComponentCode>, Box<dyn std::error::Error>> {
    let mut components = HashMap::new();

    load_components(LoadComponentsOptions {
        path,
        account_id,
        prefix: "".to_string(),
        web_engine,
        components: &mut components,
        replacements_map,
    })?;

    Ok(components)
}

fn replace_placeholders(
    code: &str,
    account_id: &str,
    replacements_map: &HashMap<String, String>,
) -> String {
    let mut modified_string = code.to_string();
    let mut replacements = replacements_map.clone();
    replacements.insert("${REPL_ACCOUNT}".to_owned(), account_id.to_owned());

    for (substring, value) in replacements.iter() {
        modified_string = modified_string.replace(substring, value);
    }

    modified_string
}

fn read_replacements(path: PathBuf) -> Result<HashMap<String, String>, Box<dyn std::error::Error>> {
    let contents = fs::read_to_string(path)?;

    let map = serde_json::from_str::<HashMap<String, String>>(&contents)
        .map_err(|_| "Invalid JSON format")?
        .iter()
        .map(|(key, value)| (format!("{}{}{}", "${", key, "}"), value.to_owned()))
        .collect::<HashMap<String, String>>();

    if map.contains_key("${REPL_ACCOUNT}") {
        panic!("The replacements file can't contain the REPL_ACCOUNT key. This key is reserved.");
    }

    Ok(map)
}

struct LoadComponentsOptions<'lco> {
    path: &'lco PathBuf,
    prefix: String,
    web_engine: bool,
    account_id: &'lco str,
    components: &'lco mut HashMap<String, ComponentCode>,
    replacements_map: &'lco HashMap<String, String>,
}

fn load_components(
    LoadComponentsOptions {
        path,
        prefix,
        web_engine,
        account_id,
        components,
        replacements_map,
    }: LoadComponentsOptions,
) -> Result<(), Box<dyn std::error::Error>> {
    let paths = fs::read_dir(path)
        .map_err(|err| format!("Could not read directory {:?} \n Error: {:?}", path, err))?;

    for path_result in paths {
        let path = path_result?;

        let file_path = path.path();
        let file_name = file_path
            .file_name()
            .ok_or(format!("Could not get file name from path {:?}", file_path))?
            .to_string_lossy()
            .to_string();

        if path
            .file_type()
            .map_err(|err| {
                format!(
                    "Could not get file type from path {:?} \n Error: {:?}",
                    file_path, err
                )
            })?
            .is_dir()
        {
            load_components(LoadComponentsOptions {
                path: &file_path,
                account_id,
                prefix: format!("{prefix}{file_name}."),
                web_engine,
                components,
                replacements_map,
            })?;

            continue;
        }

        let mut file_name_parts: Vec<&str> = file_name.split('.').collect();

        if let Some(extension) = file_name_parts.pop() {
            if extension != "jsx" && extension != "tsx" {
                continue;
            }
        }

        let file_key = file_name_parts.join(".");
        let join_string = if web_engine { "/" } else { "/widget/" };
        let key = format!("{account_id}{join_string}{prefix}{file_key}");

        let mut code = String::new();
        let mut file = fs::File::open(&file_path)
            .map_err(|err| format!("Failed to open file {:?} \n Error: {:?}", file_path, err))?;

        file.read_to_string(&mut code)
            .map_err(|err| format!("Failed to read file {:?} \n Error: {:?}", file_path, err))?;

        code = replace_placeholders(&code, account_id, replacements_map);
        components.insert(key, ComponentCode { code });
    }

    Ok(())
}

#[tokio::main]
async fn main() {
    let Args {
        account_id,
        path,
        use_config,
        web_engine,
        port,
        replacements,
    } = Args::parse();

    let account_paths = if use_config {
        let settings = Config::builder()
            .add_source(config::File::with_name("./.bos-loader.toml"))
            .build()
            .expect("Failed to load config file");

        settings
            .get::<Vec<AccountPath>>("paths")
            .expect("A valid path configuration was not found in config file")
    } else {
        vec![AccountPath {
            path: path,
            account_id: account_id
                .expect("Account ID must be provided when not using configuration file"),
        }]
    };

    let replacements_map = if let Some(replacements_path) = replacements {
        read_replacements(replacements_path)
            .map_err(|err| {
                format!(
                    "Something went wrong while parsing the replacement file: {}",
                    err
                )
            })
            .unwrap()
    } else {
        HashMap::new()
    };

    let display_paths_str = account_paths
        .iter()
        .map(|AccountPath { path, account_id }| format!("{:?} as account {}", path, account_id))
        .collect::<Vec<String>>()
        .join("\n");

    let cors = warp::cors()
        .allow_any_origin()
        .allow_methods(&[Method::GET]);

    let api = warp::get()
        .map(move || {
            let mut all_components = HashMap::new();

            for AccountPath { path, account_id } in account_paths.iter() {
                match handle_request(HandleRequestOptions {
                    path,
                    web_engine,
                    account_id,
                    replacements_map: &replacements_map,
                }) {
                    Ok(components) => {
                        all_components.extend(components);
                    }
                    Err(err) => {
                        let error = format!(
                            "Error handling request for account_id {}, path {:?} \n Error: {:?}",
                            account_id, path, err
                        );

                        println!("{error}");

                        return warp::reply::json(&json!({
                            "error": error,
                        }));
                    }
                }
            }

            warp::reply::json(&FileList {
                components: all_components,
            })
        })
        .with(cors);

    println!(
        "\nServing .jsx/.tsx files on http://127.0.0.1:{}\n\n{}",
        port, display_paths_str
    );

    warp::serve(api).run(([127, 0, 0, 1], port)).await;
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

        let map = read_replacements(path).unwrap();

        assert_eq!(map, expected_output);
    }

    #[test]
    #[should_panic(
        expected = "The replacements file can't contain the REPL_ACCOUNT key. This key is reserved."
    )]
    fn test_read_replacements_repl_account() {
        let path: PathBuf = "./test/replacements.wrong.json".into();

        read_replacements(path).unwrap();
    }

    // TODO: add tests for config file multi-account setup
}
