use anyhow::Result;
use graphql_client::GraphQLQuery;
use reqwest::Client;
use serde::{Deserialize, Serialize};

const GITHUB_URL: &str = "https://api.github.com/graphql";
const APP_USER_AGENT: &str = concat!(env!("CARGO_PKG_NAME"), "/", env!("CARGO_PKG_VERSION"),);

#[derive(GraphQLQuery)]
#[graphql(
    schema_path = "src/github_schema.graphql",
    query_path = "src/open_issues.graphql",
    response_derives = "Debug"
)]
pub struct OpenIssues;

#[derive(Debug, Deserialize, Serialize)]
pub struct Data {
    data: Repository,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct Repository {
    repository: Issues,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct Issues {
    issues: Nodes,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct Nodes {
    nodes: Vec<GitHubIssue>,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct GitHubIssue {
    pub number: i32,
    pub title: String,
}

pub async fn send_github_issue_query(
    owner: &str,
    name: &str,
    token: &str,
) -> Result<Vec<GitHubIssue>> {
    let variables = open_issues::Variables {
        owner: owner.to_string(),
        name: name.to_string(),
    };
    let request_body = OpenIssues::build_query(variables);
    let client = Client::builder().user_agent(APP_USER_AGENT).build()?;
    let res = client
        .post(GITHUB_URL)
        .bearer_auth(token)
        .json(&request_body)
        .send()
        .await?;

    let respose_body = res.text().await?;
    let issue_result = serde_json::from_str::<Data>(&respose_body)?
        .data
        .repository
        .issues
        .nodes;
    Ok(issue_result)
}
