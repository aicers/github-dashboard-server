use anyhow::Result;
use graphql_client::{GraphQLQuery, Response};
use reqwest::Client;

#[derive(GraphQLQuery)]
#[graphql(
    schema_path = "src/github_schema.graphql",
    query_path = "src/open_issues.graphql",
    response_derives = "Debug"
)]
pub struct OpenIssues;

const GITHUB_URL: &str = "https://api.github.com/graphql";

pub async fn send_github_query(
    owner: &str,
    name: &str,
) -> Result<Response<open_issues::ResponseData>> {
    let variables = open_issues::Variables {
        owner: owner.to_string(),
        name: name.to_string(),
    };
    let request_body = OpenIssues::build_query(variables);
    let client = Client::new();
    let res = client.post(GITHUB_URL).json(&request_body).send().await?;
    let response_body: Response<open_issues::ResponseData> = res.json().await?;
    Ok(response_body)
}
