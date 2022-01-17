use std::env;
use std::fs::{self, File};
use std::io::{self, BufWriter, Read};
use std::path::PathBuf;

use crate::result::*;
use once_cell::sync::OnceCell;
use serde::{Deserialize, Serialize};

use crate::cli::APP_NAME;

pub static CONSUMER_KEY: Option<&str> = option_env!("PHOG_COMPILE_ENV__CONSUMER_KEY");
pub static CONSUMER_SECRET: Option<&str> = option_env!("PHOG_COMPILE_ENV__CONSUMER_SECRET");

static CREDENTIALS: OnceCell<Credentials> = OnceCell::new();
static SETTINGS: OnceCell<Settings> = OnceCell::new();

#[derive(Clone, Serialize, Deserialize)]
pub struct AccessToken {
    pub access_token: String,
    pub access_token_secret: String,
}

#[derive(Clone, Deserialize, Serialize)]
pub struct Credentials {
    pub consumer_key: String,
    pub consumer_secret: String,
    pub access_token: String,
    pub access_token_secret: String,
}

#[derive(Clone, Default, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub struct Settings {
    #[serde(default)]
    pub download: DownloadSettings,
    #[serde(default, alias = "fetch")]
    pub record: RecordSettings,
}

#[derive(Clone, Default, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub struct DownloadSettings {
    pub dir: Option<PathBuf>,
}

#[derive(Clone, Default, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub struct RecordSettings {
    pub default_likes: Option<Vec<String>>,
    pub default_user: Option<Vec<String>>,
}

pub fn init() -> Result<()> {
    static DEFAULT_CONFIG_TOML: &str = include_str!("../data/default_config.toml");

    let path = config_dir_path();
    fs::create_dir_all(&path)
        .with_context(|| format!("Could not create the config directory at {:?}", &path))?;

    let path = data_dir_path();
    fs::create_dir_all(&path)
        .with_context(|| format!("Could not create the data directory at {:?}", &path))?;

    let path = settings_path();
    if !path.exists() {
        fs::write(&path, DEFAULT_CONFIG_TOML)
            .with_context(|| format!("Could not create the config file at {:?}", &path))?;
    }

    Ok(())
}

pub fn access_token_path() -> PathBuf {
    data_dir_path().join("access_token.json")
}

pub fn credentials_path() -> PathBuf {
    data_dir_path().join("credentials.json")
}

pub fn config_dir_path() -> PathBuf {
    fn user_config_dir() -> Option<PathBuf> {
        if cfg!(target_os = "macos") {
            dirs::home_dir().map(|p| p.join(".config"))
        } else {
            dirs::config_dir()
        }
    }

    static CONFIG_DIR: OnceCell<PathBuf> = OnceCell::new();

    CONFIG_DIR
        .get_or_init(|| {
            if let Ok(path) = env::var("PHOG_CONFIG_DIR") {
                return PathBuf::from(path);
            }
            if let Some(path) = user_config_dir() {
                return path.join(APP_NAME);
            }
            panic!("Could not locate the user's config directory");
        })
        .clone()
}

pub fn data_dir_path() -> PathBuf {
    fn user_data_dir() -> Option<PathBuf> {
        if cfg!(target_os = "macos") {
            dirs::home_dir().map(|p| p.join(".local/share"))
        } else {
            dirs::data_dir()
        }
    }

    static DATA_DIR: OnceCell<PathBuf> = OnceCell::new();

    DATA_DIR
        .get_or_init(|| {
            if let Ok(path) = env::var("PHOG_DATA_DIR") {
                return PathBuf::from(path);
            }
            if let Some(path) = user_data_dir() {
                return path.join(APP_NAME);
            }
            panic!("Could not locate the user's data directory");
        })
        .clone()
}

pub fn database_path() -> PathBuf {
    data_dir_path().join("db.sqlite3")
}

pub fn settings_path() -> PathBuf {
    config_dir_path().join("config.toml")
}

pub fn credentials() -> Result<Credentials> {
    CREDENTIALS
        .get_or_try_init(load_credentials)
        .map(|c| c.clone())
}

pub fn settings() -> Result<Settings> {
    SETTINGS.get_or_try_init(load_settings).map(|s| s.clone())
}

pub fn save_access_token(token: String, secret: String) -> Result<()> {
    let access_token = AccessToken {
        access_token: token,
        access_token_secret: secret,
    };

    let mut f = File::create(access_token_path()).context("Could not create access_token.json")?;
    let w = BufWriter::new(&mut f);
    serde_json::to_writer(w, &access_token).context("Could not save access_token.json")?;
    set_mode_600(&mut f)?;

    Ok(())
}

fn load_access_token() -> Result<AccessToken> {
    let f = File::open(access_token_path()).context("Could not open access_token.json")?;
    let access_token: AccessToken =
        serde_json::from_reader(f).context("Could not load access_token.json")?;
    Ok(access_token)
}

pub fn save_credentials(credentials: Credentials) -> Result<()> {
    let mut f = File::create(access_token_path()).context("Could not create credentials.json")?;
    let w = BufWriter::new(&mut f);
    serde_json::to_writer(w, &credentials).context("Could not save credentials.json")?;
    set_mode_600(&mut f)?;
    Ok(())
}

fn load_credentials() -> Result<Credentials> {
    let path = credentials_path();
    if path.is_file() {
        let f = File::open(path).context("Could not open credentials.json")?;
        let credentials = serde_json::from_reader(&f).context("Could not load credentials.json")?;
        return Ok(credentials);
    }

    let (consumer_key, consumer_secret) = match (CONSUMER_KEY, CONSUMER_SECRET) {
        (Some(key), Some(secret)) => (key.to_owned(), secret.to_owned()),
        _ => bail!(
            "Could not find login information. Try `{} login --with-credentials`.",
            APP_NAME
        ),
    };

    match load_access_token() {
        Ok(AccessToken {
            access_token,
            access_token_secret,
        }) => Ok(Credentials {
            consumer_key,
            consumer_secret,
            access_token,
            access_token_secret,
        }),
        Err(e) => Err(e).with_context(|| {
            format!(
                "Could not find login information. Try `{} login`.",
                APP_NAME
            )
        })?,
    }
}

fn load_settings() -> Result<Settings> {
    let mut f = File::open(settings_path()).context("Could not open config.toml")?;
    let mut buf = String::new();
    f.read_to_string(&mut buf)
        .context("Could not read config.toml")?;
    let mut settings = toml::from_str(&buf).context("Could not load config.toml")?;
    expand_tilde_in_paths(&mut settings);
    Ok(settings)
}

fn expand_tilde_in_paths(settings: &mut Settings) {
    if let Some(dir) = settings.download.dir.as_ref().and_then(|p| p.to_str()) {
        if dir.starts_with('~') {
            let home = dirs::home_dir().expect("Could not locate the user's home directory");
            if dir == "~" {
                settings.download.dir = Some(home);
            } else if let Some(stripped_dir) = dir.strip_prefix("~/") {
                settings.download.dir = Some(home.join(stripped_dir));
            }
            // `~foo/` is not supported.
        }
    }
}

#[cfg(target_family = "unix")]
fn set_mode_600(f: &mut File) -> io::Result<()> {
    use std::os::unix::fs::PermissionsExt;
    let mut permissions = f.metadata()?.permissions();
    permissions.set_mode(0o600);
    Ok(())
}

#[cfg(not(target_family = "unix"))]
fn set_mode_600(_f: &mut File) -> io::Result<()> {
    Ok(())
}
