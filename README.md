# github-dashboard-server
AICE GitHub Dashboard Server

## Usage

Before running the app, create a toml extension file and write it in the format below.
```
 [web]
 address = "127.0.0.1:8080"
```
- `address`: Address of web server.

Build and serve the app with Cargo as follows:
```
 cargo run -- [config_file]
```
- `config_file`: Toml extension file name including path.

The web server will run using the address value in the config file.
