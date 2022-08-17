# AICE GitHub Dashboard Server

## Usage

Before running the app, create a toml extension file and write it in the format below.

```toml
 [web]
 address = "127.0.0.1:8080"

 [[repositories]]
 owner = "aicers"
 name = "github-dashboard-server"

 [[repositories]]
 owner = "aicers"
 name = "github-dashboard-client"

 [certification]
 token = "github_token_info"

 [database]
 db_name = "db_name"
```

* `address`: Address of web server.
* `owner`: The owner of the github repository.
* `names`: The name of the github repository.
* `token`: Generated github access token value. (Token Generation: [github-access-token](https://docs.github.com/en/authentication/keeping-your-account-and-data-secure/creating-a-personal-access-token#creating-a-token))
* `db_name`: The name of the db to create/connect.

Build and serve the app with Cargo as follows:

```sh
cargo run [-- FLAGS | OPTION]
```

When you run the program, server reads the config file from the default folder.

To run without giving the config file option, save the file to the path below.

```sh
"/Users/[username]/Library/Application Support/com.einsis.github-dashboard-server/config.toml"
```

The web server will run using the address value in the config file.

Connect to `http://localhost:8080` in your browser to run the app,

* `http://localhost:8080/graphql/playground` to playground

## FLAGS

* `-h`, `--help`: Prints help information
* `-V`, `--version`: Prints version information

## OPTION

* `config_file`: The path to the toml file containing server config info.
