use anyhow::anyhow;
use async_recursion::async_recursion;
use clap::Parser;
use config::Config;
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::{collections::HashMap, path::PathBuf, sync::Arc};
use tokio::{fs, io::AsyncReadExt, sync::Mutex};
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
    account: Option<String>,
    /// Use config file in current dir (./.bos-loader.toml) to set account and path, causes other args to be ignored
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

#[derive(Serialize, Deserialize, Clone)]
struct ComponentCode {
    code: String,
}

#[derive(Serialize, Deserialize, Clone)]
struct AccountPath {
    path: PathBuf,
    account: String,
}

struct HandleRequestOptions {
    path: PathBuf,
    account: String,
    web_engine: bool,
    replacements_map: Arc<HashMap<String, String>>,
}

async fn handle_request(
    HandleRequestOptions {
        path,
        account,
        web_engine,
        replacements_map,
    }: HandleRequestOptions,
) -> Result<Arc<Mutex<HashMap<String, ComponentCode>>>, anyhow::Error> {
    let components = Arc::new(Mutex::new(HashMap::new()));

    load_components(LoadComponentsOptions {
        path,
        account,
        prefix: "".to_string(),
        web_engine,
        components: components.clone(),
        replacements_map,
    })
    .await?;

    Ok(components)
}

fn replace_placeholders(
    code: &str,
    account: &str,
    replacements_map: &HashMap<String, String>,
) -> String {
    let mut modified_string = code.to_string();
    let mut replacements = replacements_map.clone();
    replacements.insert("${REPL_ACCOUNT}".to_owned(), account.to_owned());

    for (substring, value) in replacements.iter() {
        modified_string = modified_string.replace(substring, value);
    }

    modified_string
}

async fn read_replacements(path: PathBuf) -> Result<Arc<HashMap<String, String>>, anyhow::Error> {
    let contents = fs::read_to_string(&path)
        .await
        .map_err(|err| anyhow!("Failed to read path {:?} \n Error: {:?}", path, err))?;

    let map = serde_json::from_str::<HashMap<String, String>>(&contents)
        .map_err(|_| anyhow!("Invalid JSON format"))?
        .iter()
        .map(|(key, value)| (format!("{}{}{}", "${", key, "}"), value.to_owned()))
        .collect::<HashMap<String, String>>();

    if map.contains_key("${REPL_ACCOUNT}") {
        panic!("The replacements file can't contain the REPL_ACCOUNT key. This key is reserved.");
    }

    Ok(Arc::new(map))
}

struct LoadComponentsOptions {
    path: PathBuf,
    prefix: String,
    account: String,
    web_engine: bool,
    components: Arc<Mutex<HashMap<String, ComponentCode>>>,
    replacements_map: Arc<HashMap<String, String>>,
}

#[async_recursion]
async fn load_components(
    LoadComponentsOptions {
        path,
        prefix,
        account,
        web_engine,
        components,
        replacements_map,
    }: LoadComponentsOptions,
) -> Result<(), anyhow::Error> {
    let mut paths = fs::read_dir(path.clone())
        .await
        .map_err(|err| anyhow!("Could not read directory {:?} \n Error: {:?}", path, err))?;

    while let Some(directory_entry) = paths.next_entry().await.map_err(|err| {
        anyhow!(
            "Could not read directory entries for path {:?} \n Error: {:?}",
            path,
            err
        )
    })? {
        let file_path = directory_entry.path();
        let file_name = file_path
            .file_name()
            .ok_or(anyhow!("Could not get file name from path {:?}", file_path))?
            .to_string_lossy()
            .to_string();

        if directory_entry
            .file_type()
            .await
            .map_err(|err| {
                anyhow!(
                    "Could not get file type from path {:?} \n Error: {:?}",
                    file_path,
                    err
                )
            })?
            .is_dir()
        {
            load_components(LoadComponentsOptions {
                path: file_path,
                account: account.clone(),
                prefix: format!("{prefix}{file_name}."),
                web_engine,
                components: components.clone(),
                replacements_map: replacements_map.clone(),
            })
            .await?;

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
        let key = format!("{account}{join_string}{prefix}{file_key}");

        let mut code = String::new();
        let mut file = fs::File::open(&file_path)
            .await
            .map_err(|err| anyhow!("Failed to open file {:?} \n Error: {:?}", file_path, err))?;

        file.read_to_string(&mut code)
            .await
            .map_err(|err| anyhow!("Failed to read file {:?} \n Error: {:?}", file_path, err))?;

        code = replace_placeholders(&code, &account, &replacements_map.clone());
        components.lock().await.insert(key, ComponentCode { code });
    }

    Ok(())
}

#[tokio::main]
async fn main() {
    let Args {
        path,
        port,
        account,
        use_config,
        web_engine,
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
            path,
            account: account
                .expect("Account ID must be provided when not using configuration file"),
        }]
    };

    let replacements_map = if let Some(replacements_path) = replacements {
        read_replacements(replacements_path)
            .await
            .map_err(|err| {
                format!(
                    "Something went wrong while parsing the replacement file: {}",
                    err
                )
            })
            .unwrap()
    } else {
        Arc::new(HashMap::new())
    };

    let display_paths_str = account_paths
        .iter()
        .map(|AccountPath { path, account }| format!("{:?} as account {}", path, account))
        .collect::<Vec<String>>()
        .join("\n");

    let cors = warp::cors()
        .allow_any_origin()
        .allow_methods(&[Method::GET]);

    let api = warp::get()
        .and_then(move || {
            let account_paths = account_paths.clone();
            let replacements_map = replacements_map.clone();

            async move {
                let mut all_components = HashMap::new();

                for AccountPath { path, account } in account_paths {
                    match handle_request(HandleRequestOptions {
                        path: path.clone(),
                        web_engine,
                        account: account.clone(),
                        replacements_map: replacements_map.clone(),
                    })
                    .await
                    {
                        Ok(components) => {
                            let components_lock = components.lock().await;

                            all_components.extend(components_lock.clone());
                        }
                        Err(err) => {
                            let error = format!(
                                "Error handling request for account {}, path {:?} \n Error: {:?}",
                                account, path, err
                            );

                            println!("{error}");

                            return Ok::<_, warp::Rejection>(warp::reply::json(&json!({
                                "error": error,
                            })));
                        }
                    }
                }

                Ok(warp::reply::json(&FileList {
                    components: all_components,
                }))
            }
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

    #[tokio::test]
    async fn test_read_replacements() {
        let path: PathBuf = "./test/replacements.json".into();

        let expected_output: HashMap<String, String> = vec![
            ("${REPL_PLACEHOLDER1}".to_owned(), "value1".to_owned()),
            ("${REPL_PLACEHOLDER2}".to_owned(), "value2".to_owned()),
        ]
        .into_iter()
        .collect();

        let map = read_replacements(path).await.unwrap();

        assert_eq!(map, expected_output.into());
    }

    #[tokio::test]
    #[should_panic(
        expected = "The replacements file can't contain the REPL_ACCOUNT key. This key is reserved."
    )]
    async fn test_read_replacements_repl_account() {
        let path: PathBuf = "./test/replacements.wrong.json".into();

        read_replacements(path).await.unwrap();
    }

    // TODO: add tests for config file multi-account setup
}
