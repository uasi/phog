# phog

phog downloads images from Twitter. phog remembers which images you have downloaded and never downloads them again.

## Installation

Precompiled binaries are available for Windows, macOS and Linux. https://github.com/uasi/phog/releases

## Usage

### Logging in to Twitter

Run `phog login`.

```
$ phog login

Preparing login URL...
Open the URL below and log in to Twitter to get a PIN code.

https://api.twitter.com/oauth/authorize?oauth_token=XXXXXX

Enter the PIN code (Ctrl-C to quit): 1234567
Logged in successfully.
```

### Downloading images

- Use `phog get --user <screen-name>...` to download from user timelines.
- Use `phog get --likes <screen-name>...` to download from likes.
- `<screen-name>...` is a list of screen names separated by a comma.


```
$ phog get --user user1,@user2,https://twitter.com/user3 --likes user4

Fetched 200 tweets from user1.
Recorded 200 tweets.
Fetched 200 tweets from user2.
Recorded 200 tweets.
Fetched 200 tweets from user3.
Recorded 200 tweets.
Fetched 100 likes from user4.
Recorded 100 tweets.
Downloading photos to "/Users/me/Downloads".
Downloading 2 photosets.
Downloaded @user1-0123456789012345678-img1-XXXXXXXXXXXXXXX.jpg
Downloaded @user1-0123456789012345678-img2-XXXXXXXXXXXXXXY.jpg
Downloaded @user1-0123456789012345679-img1-XXXXXXXXXXXXXXZ.jpg
Done.
```

### Configuration

See `~/.config/phog/config.toml` (Linux/macOS) or `%APPDATA%\phog\config.toml` (Windows).

## Building

```
# Optional; if the key and secret are omitted, `phog login` will be disabled.
$ export PHOG_COMPILE_ENV__CONSUMER_KEY=<Twitter API key>
$ export PHOG_COMPILE_ENV__CONSUMER_SECRET=<Twitter API secret>

$ cargo build --release
```
