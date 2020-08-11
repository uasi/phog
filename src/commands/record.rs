use anyhow::Result;
use smol::block_on;
use structopt::StructOpt;

use crate::config;
use crate::database::Connection;
use crate::recording::{fetch::MAX_DEPTH, Extract, Fetch};
use crate::twitter::Client;

#[derive(Debug, Default, Eq, PartialEq, StructOpt)]
pub struct Args {
    #[structopt(flatten)]
    extract_args: ExtractArgs,
    #[structopt(flatten)]
    fetch_args: FetchArgs,
}

#[derive(Debug, Default, Eq, PartialEq, StructOpt)]
pub struct ExtractArgs {
    #[structopt(short, long, help = "Extracts tweet URLs from the clipboard")]
    pub paste: bool,
    #[structopt(
        short,
        long,
        help = "Watches the clipboard and extracts tweet URLs continuously"
    )]
    pub watch: bool,
}

#[derive(Debug, Default, Eq, PartialEq, StructOpt)]
pub struct FetchArgs {
    #[structopt(
        long,
        requires = "fetch-source",
        group = "fetch-modifier",
        help = "Fetches all available tweets in the sources"
    )]
    pub all: bool,
    #[structopt(
        long,
        validator(validate_depth),
        requires = "fetch-source",
        group = "fetch-modifier",
        help = "Limits the number of paginated requests to the same source"
    )]
    pub depth: Option<usize>,
    #[structopt(
        short = "f",
        long = "fetch",
        group = "fetch-source",
        help = "Fetches even if only extract options are specified"
    )]
    pub force: bool,
    #[structopt(
        short,
        long,
        require_delimiter(true),
        group = "fetch-source",
        value_name = "screen-name",
        next_line_help(true),
        help = "Fetches likes from the users\n\
            \n\
            <screen-name> is a screen name (@ is optional) or the URL to the status page of a user.\n\
            Each <screen-name> should be separated by a comma.\n\
            Example: --likes user1,@user2,https://twitter.com/user3\n\
            \n\
            If <screen-name> is omitted and only the --likes flag is given,\n\
            the record.default-likes variable in the config file is used as screen names."
    )]
    pub likes: Option<Vec<String>>,
    #[structopt(
        short,
        long,
        require_delimiter(true),
        group = "fetch-source",
        value_name = "screen-name",
        next_line_help(true),
        help = "Fetches tweets from the users\n\
            \n\
            <screen-name> is a screen name (@ is optional) or the URL to the status page of a user.\n\
            Each <screen-name> should be separated by a comma.\n\
            Example: --user user1,@user2,https://twitter.com/user3\n\
            \n\
            If <screen-name> is omitted and only the --user flag is given,\n\
            the record.default-user variable in the config file is used as screen names."
    )]
    pub user: Option<Vec<String>>,
}

impl Args {
    pub fn is_empty(&self) -> bool {
        self == &Self::default()
    }

    pub fn should_fetch(&self) -> bool {
        // Fetch does not need to be run if only extract options are specified.
        self.fetch_args.force || !self.fetch_args.is_empty() || self.is_empty()
    }
}

impl FetchArgs {
    pub fn is_empty(&self) -> bool {
        self == &Self::default()
    }

    pub fn load_defaults(mut self, settings: config::Settings) -> Result<Self> {
        fn is_flag_only(opt: &Option<Vec<String>>) -> bool {
            opt.as_ref().map(|v| v.len()) == Some(0)
        }

        let no_targets = self.likes.is_none() && self.user.is_none();

        if no_targets || is_flag_only(&self.likes) {
            self.likes = settings.record.default_likes;
        }
        if no_targets || is_flag_only(&self.user) {
            self.user = settings.record.default_user;
        }

        Ok(self)
    }
}

pub fn run(args: Args) -> Result<()> {
    let db = Connection::open(config::database_path())?;
    db.create()?;
    let should_fetch = args.should_fetch();
    // Extract should always be run as stdin may be provided at any time.
    run_extract(args.extract_args, &db)?;
    if should_fetch {
        run_fetch(args.fetch_args, &db)?;
    }
    Ok(())
}

fn run_extract(args: ExtractArgs, db: &Connection) -> Result<()> {
    log::trace!("starting extraction; args={:?}", args);
    let extract = Extract::new(&db);
    if args.watch {
        extract.from_clipboard_watcher()
    } else if args.paste {
        extract.from_clipboard()
    } else {
        extract.from_stdin()
    }
}

fn run_fetch(args: FetchArgs, db: &Connection) -> Result<()> {
    let args = args.load_defaults(config::settings()?)?;
    log::trace!("starting fetch; args={:?}", args);

    let credentials = config::credentials()?;
    let client = Client::new(credentials);
    let uses_since_id = !args.all && args.depth.is_none();
    let depth = match args.depth {
        Some(n) if n == 0 => MAX_DEPTH,
        Some(n) => n,
        None => MAX_DEPTH,
    };

    let fetch = Fetch::new(&db, client);

    if let Some(likes) = args.likes {
        block_on(fetch.from_likes(likes))?;
    }
    if let Some(user) = args.user {
        block_on(fetch.from_user(user, uses_since_id, depth))?;
    }

    Ok(())
}

fn validate_depth(depth: String) -> std::result::Result<(), String> {
    match depth.parse::<usize>() {
        Ok(n) if n <= MAX_DEPTH => Ok(()),
        Ok(_) => Err(format!("depth should be <= {}", MAX_DEPTH)),
        Err(_) => Err("depth should be a number".to_owned()),
    }
}

#[cfg(test)]
mod args_tests {
    use crate::config;

    use super::{Args, FetchArgs};

    #[test]
    fn should_fetch() {
        {
            let args = Args::default();
            assert!(args.should_fetch());
        }
        {
            let mut args = Args::default();
            args.extract_args.paste = true;
            assert!(!args.should_fetch());
        }
        {
            let mut args = Args::default();
            args.fetch_args.force = true;
            assert!(args.should_fetch());
        }
        {
            let mut args = Args::default();
            args.fetch_args.likes = Some(vec![]);
            assert!(args.should_fetch());
        }
        {
            let mut args = Args::default();
            args.extract_args.paste = true;
            args.fetch_args.likes = Some(vec![]);
            assert!(args.should_fetch());
        }
    }

    #[test]
    fn fetch_args_load_defaults() {
        let fetch_args = FetchArgs::default();

        assert!(fetch_args.likes.is_none());
        assert!(fetch_args.user.is_none());

        let mut settings = config::Settings::default();
        settings.record.default_likes = Some(vec!["default_likes_user".to_owned()]);
        settings.record.default_user = Some(vec!["default_user_user".to_owned()]);
        let fetch_args = fetch_args.load_defaults(settings.clone()).unwrap();

        assert_eq!(fetch_args.likes, settings.record.default_likes);
        assert_eq!(fetch_args.user, settings.record.default_user);
    }
}
