# AICE GitHub Dashboard Server

[![Coverage Status](https://codecov.io/gh/aicers/github-dashboard-server/branch/main/graphs/badge.svg)](https://codecov.io/gh/aicers/github-dashboard-server)

## Usage

Before running the app, create a TOML configuration file in the following
format:

```toml
[web]
address = "127.0.0.1:8000"
key = "key_path"
cert = "cert_path"

[[repositories]]
owner = "aicers"
name = "github-dashboard-server"

[[repositories]]
owner = "aicers"
name = "github-dashboard-client"

[certification]
token = "github_token_info"
ssh = ".ssh/id_ed25519"

[database]
db_path = "db_path"
```

- `address`: The address of web server.
- `key`: The TLS key path for the web server.
- `cert`: The TLS certificate path for the web server.
- `owner`: The owner of the GitHub repository.
- `name`: The name of the GitHub repository.
- `token`: A Github fine-grained
  [personal access token](https://docs.github.com/en/authentication/keeping-your-account-and-data-secure/managing-your-personal-access-tokens#creating-a-token).
  The token's lifetime should be less than one year for security purposes.
  Minimum required permissions are as follows:
  - Repository: Access to all repositories
  - Issues: Read-only access
  - Pull Requests: Read-only access
- `ssh`: The path to SSH private key for GitHub code checkout.
  - To provide an SSH passphrase, set the `SSH_PASSPHRASE` environment variable.
- `db_path`: The path to the database for creation/connection.

### Running the App

Build and serve the app using Cargo:

```sh
cargo run [-- FLAGS | OPTION]
```

When you run the program, the server reads the configuration file from the
default directory.

To run the application without specifying the configuration file path, save the
file to one of the following locations:

- Linux: $HOME/.config/github-dashboard-server/config.toml
- macOS: $HOME/Library/Application
  Support/com.einsis.github-dashboard-server/config.toml

The web server will use the address value specified in the configuration file.

### Accessing the Web Interface

- Open <https://localhost:8000> in your browser to run the app.
- Visit <https://localhost:8000/graphql/playground> to access the GraphQL
  playground.

## FLAGS

- `-h`, `--help`: Displays help information.
- `-V`, `--version`: Displays version information.

## OPTION

- `config_file`: The path to the TOML file containing server configuration
  details.
