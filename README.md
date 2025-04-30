# AICE GitHub Dashboard Server

[![Coverage Status](https://codecov.io/gh/aicers/github-dashboard-server/branch/main/graphs/badge.svg)](https://codecov.io/gh/aicers/github-dashboard-server)

## Usage

Before running the app, create a TOML configuration file in the following format:

```toml
[web]
address = "127.0.0.1:8000"

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
db_path = "github-dashboard"
```

- `address`: IP address and port the web server listens on. (Default:
  127.0.0.1:8000)
- `owner`: The owner of the GitHub repository.
- `name`: The name of the GitHub repository.
- `token`: A Github fine-grained [personal access token](https://docs.github.com/en/authentication/keeping-your-account-and-data-secure/managing-your-personal-access-tokens#creating-a-token).
  The token's lifetime should be less than one year for security purposes.
  Minimum required permissions are as follows:
  - Repository: Access to all repositories
  - Issues: Read-only access
  - Pull Requests: Read-only access
- `ssh`: The path to SSH private key for GitHub code checkout.
  - To provide an SSH passphrase, set the `SSH_PASSPHRASE` environment variable.
- `db_path`: Folder where the sled database files are stored. Created
  automatically if it doesn’t exist. (Default: github-dashboard)

### Running the App

Run app with the prepared configuration file and following command.

```sh
cargo run -- -c <CONFIG_PATH> \
--cert <CERT_PATH> \
--key <KEY_PATH>
```

### Arguments

| Name            | Description                          | Required |
| --------------- | ------------------------------------ | -------- |
| `<CONFIG_PATH>` | Path to the TOML configuration file. | Yes      |
| `<CERT_PATH>`   | Path to the certificate file.        | Yes      |
| `<KEY_PATH>`    | Path to the private key file.        | Yes      |

### Accessing the Web Interface

- Open <https://localhost:8000> in your browser to run the app.
- Visit <https://localhost:8000/graphql/playground> to access the GraphQL playground.

## FLAGS

- `-h`, `--help`: Displays help information.
- `-V`, `--version`: Displays version information.

## GitHub GraphQL API Testing

The GitHub GraphQL API used in this project was last tested on 2025-05-19. It is
advisable to regularly review [breaking changes](https://docs.github.com/en/graphql/overview/breaking-changes)
in the GitHub GraphQL API.
