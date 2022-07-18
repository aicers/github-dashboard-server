# AICE GitHub Dashboard Server

## Usage

Before running the app, create a toml extension file and write it in the format below.

```toml
 [web]
 address = "127.0.0.1:8080"

 [repository]
 owner = "aicers"
 name = "github-dashboard-server"
```

* `address`: Address of web server.
* `owner`: The owner of the github repository
* `name`: The name of the github repository

Build and serve the app with Cargo as follows:

```sh
cargo run -- [config_file]
```

* `config_file`: Toml extension file name including path.

The web server will run using the address value in the config file.

## FLAGS

* `-h`, `--help`: Prints help information
* `-V`, `--version`: Prints version information
