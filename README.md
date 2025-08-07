# AICE GitHub Dashboard Server

[![Coverage Status](https://codecov.io/gh/aicers/github-dashboard-server/branch/main/graphs/badge.svg)](https://codecov.io/gh/aicers/github-dashboard-server)

## Usage

Run app with the prepared configuration file and following command:

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

### Flags

- `-h`, `--help`: Displays help information.
- `-V`, `--version`: Displays version information.

## Configuration

In the configuration file, you can specify the following options:

### `[web]`

<!-- markdownlint-disable MD013 -->

| Field     | Description                                   | Required | Default        |
| --------- | --------------------------------------------- | -------- | -------------- |
| `address` | IP address and port the web server listens on | No       | 127.0.0.1:8000 |

<!-- markdownlint-enable MD013-->

### `[[repositories]]`

| Field   | Description                        | Required | Default |
| ------- | ---------------------------------- | -------- | ------- |
| `owner` | The owner of the GitHub repository | Yes      | -       |
| `name`  | The name of the GitHub repository  | Yes      | -       |

### `[certification]`

<!-- markdownlint-disable MD013 -->

| Field   | Description                                          | Required | Default |
| ------- | ---------------------------------------------------- | -------- | ------- |
| `token` | A GitHub fine-grained personal access token          | Yes      | -       |
| `ssh`   | The path to SSH private key for GitHub code checkout | Yes      | -       |

<!-- markdownlint-enable MD013-->

- `token`: The [personal access token](https://docs.github.com/en/authentication/keeping-your-account-and-data-secure/managing-your-personal-access-tokens#creating-a-token)'s
  lifetime should be less than one year for security purposes. Minimum required
  permissions are as follows:
  - Repository: Access to all repositories
  - Issues: Read-only access
  - Pull Requests: Read-only access
- `ssh`: To provide an SSH passphrase, set the `SSH_PASSPHRASE` environment variable.

### `[database]`

<!-- markdownlint-disable MD013 -->

| Field     | Description                                     | Required | Default          |
| --------- | ----------------------------------------------- | -------- | ---------------- |
| `db_path` | Folder where the fjall database files are stored | No       | github-dashboard |

<!-- markdownlint-enable MD013-->

## Configuration Example

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

## GitHub GraphQL API Testing

The GitHub GraphQL API used in this project was last tested on 2025-05-19. It is
advisable to regularly review [breaking changes](https://docs.github.com/en/graphql/overview/breaking-changes)
in the GitHub GraphQL API.
