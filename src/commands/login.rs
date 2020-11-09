use std::io::Write;

use structopt::StructOpt;

use crate::cli::APP_NAME;
use crate::config::{self, Credentials, CONSUMER_KEY, CONSUMER_SECRET};
use crate::result::*;
use crate::rt::block_on;
use crate::twitter::Client;

#[derive(Debug, StructOpt)]
pub struct Args {
    #[structopt(long, help = "Uses Twitter API keys to log in")]
    pub with_credentials: bool,
}

pub fn run(args: Args) -> Result<()> {
    if args.with_credentials {
        return login_with_credentials();
    }

    match (CONSUMER_KEY, CONSUMER_SECRET) {
        (Some(key), Some(secret)) => login_with_pin(key.into(), secret.into()),
        _ => {
            eprintln!(
                "Warning: {} was not compiled with a Twitter API key pair.",
                APP_NAME
            );
            login_with_credentials()
        }
    }
}

fn login_with_pin(consumer_key: String, consumer_secret: String) -> Result<()> {
    println!("Preparing login URL...");

    let consumer_token = egg_mode::KeyPair::new(consumer_key, consumer_secret);
    let request_token = block_on(egg_mode::auth::request_token(&consumer_token, "oob"))?;
    let auth_url = egg_mode::auth::authorize_url(&request_token);

    println!("Open the URL below and log in to Twitter to get a PIN code.");
    println!("\n{}", auth_url);

    let code = prompt("\nEnter the PIN code (Ctrl-C to quit): ")?;

    let (access_token, ..) = block_on(egg_mode::auth::access_token(
        consumer_token,
        &request_token,
        code,
    ))
    .context("Could not log in to Twitter")?;

    match access_token {
        egg_mode::auth::Token::Access { access, .. } => {
            config::save_access_token(access.key.into(), access.secret.into())
                .context("Could not save login information")?;
            println!("Logged in successfully.");
        }
        _ => panic!("expected access token but got bearer token"),
    }

    Ok(())
}

fn login_with_credentials() -> Result<()> {
    println!("Open https://developer.twitter.com/en/apps, create or select an app, and open the Keys and Tokens tab.");
    println!("Enter keys and tokens (Ctrl-C to quit)...");

    let consumer_key = prompt("\nAPI key: ")?;
    let consumer_secret = prompt("\nAPI secret key: ")?;
    let access_token = prompt("\nAccess token: ")?;
    let access_token_secret = prompt("\nAccess token secret: ")?;

    let credentials = Credentials {
        consumer_key,
        consumer_secret,
        access_token,
        access_token_secret,
    };

    let client = Client::new(credentials.clone());
    client
        .verify_tokens()
        .context("Provided credentials are invalid")?;

    config::save_credentials(credentials)?;
    println!("\nLogged in successfully.");

    Ok(())
}

fn prompt(msg: &str) -> Result<String> {
    print!("{}", msg);
    std::io::stdout().flush()?;

    let mut input = String::new();
    std::io::stdin().read_line(&mut input).unwrap();
    Ok(input.trim().into())
}
