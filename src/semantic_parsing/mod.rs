#![allow(clippy::uninlined_format_args)]

use std::sync::Arc;

use anyhow::{Context, Result};
use langchain_rust::{
    agent::{AgentExecutor, ConversationalAgentBuilder},
    chain::Chain,
    language_models::llm::LLM,
    llm::client::{GenerationOptions, Ollama},
    memory::SimpleMemory,
    prompt_args,
    schemas::Message,
    tools::CommandExecutor,
};

use crate::graphql::Schema;

// Time: 6s, 9s, 7s
// Bad: Korean
const MODEL: &str = "llama3.2";

// Bad: Too much CPU usage, takes too much time
// const MODEL: &str = "deepseek-r1:8b";

// Bad: It uses `issues` query instead of `issueStat` query
// const MODEL: &str = "gemma3:4b";

// Bad: Takes quite long time: 30s ~ 90s
// Good: Accurate query generation. (last month)
// const MODEL: &str = "qwen3:8b";

// Time: 30s
// Bad: Korean
// const MODEL: &str = "phi4:14b";

// Time: 19s, 18s, 15s
// const MODEL: &str = "exaone3.5:7.8b";

// Time: 10s, 10s, 11s
// Bad: Queried `assignee` instead of `author`
// Bad: Korean
// const MODEL: &str = "exaone3.5:2.4b";

fn instruction(schema_doc: &str) -> String {
    let today = chrono::Utc::now().format("%Y-%m-%d").to_string();
    format!(
        "You are a helpful assistant that translates natural language into GraphQL queries.\n\n\
        There are some rules you must follow:\n\n\
        - Return {{}} if the answer cannot be found in the schema.\n\n\
        - Return a GraphQL query that answers the natural language query based on the schema.\n\n\
        - Don't make up an answer if one cannot be found.\n\n\
        - Don't use any queries that return a type ending in `Connection!`.\n\n\
        - Don't explain the query, just return it.\n\n\
        - If an answer is found, return it in the format query {{ ... }} or {{}}.\n\n\
        - When you return a query, it should be a valid GraphQL query that can be executed against the schema.\n\n
        - If the user query is unanswerable based on the schema, don't try to generate a query, just return empty query
        - Today's date is {}.\n\n\
        - Timezone: UTC.\n\n\
        Schema:\n{}\n\n
        ",
        today, schema_doc
    )
}

async fn semantic_parsing(schema_doc: &str, user_query: &str) -> Result<String> {
    let instruction = instruction(schema_doc);

    let system_msg = Message::new_system_message(instruction);
    let human_msg = Message::new_human_message(user_query.to_string());
    let messages = vec![system_msg, human_msg];

    let options = GenerationOptions::default()
        .temperature(0.0)
        .top_k(10)
        .top_p(0.1);
    let ollama = Ollama::default().with_model(MODEL).with_options(options);

    Ok(ollama.generate(&messages).await?.generation)
}

#[allow(dead_code)]
pub(crate) async fn llm_with_command_executor(user_query: &str) -> Result<String> {
    let command_executor = CommandExecutor::default();
    let ollama = Ollama::default().with_model(MODEL);

    let agent = ConversationalAgentBuilder::new()
        .tools(&[Arc::new(command_executor)])
        .build(ollama.clone())
        .unwrap();
    let memory = SimpleMemory::new();
    let executor = AgentExecutor::from_agent(agent).with_memory(memory.into());

    let input_variables = prompt_args! {
        "input" => user_query.to_string(),
    };

    let execution_result = executor
        .invoke(input_variables)
        .await
        .context("Failed to invoke agent")?;

    let system_msg = Message::new_system_message(format!(
        "You are a helpful assistant that explain the result of the command execution.\n\n\
        The command execution result is:\n\n{}\n\n\
        Please provide a detailed explanation of the command execution result.
        You don't have to verify the result of the command execution, just prettify it.",
        execution_result
    ));
    let human_msg = Message::new_human_message(user_query.to_string());
    let messages = vec![system_msg, human_msg];
    Ok(ollama.generate(&messages).await?.generation)
}

pub(crate) async fn invoke(
    _schema: Schema,
    schema_doc: &str,
    user_query: &str,
    execution_result: &str,
) -> Result<String> {
    // Estimate the time it takes to generate the query
    let start = std::time::Instant::now();
    let query = semantic_parsing(schema_doc, user_query).await?;
    let end = start.elapsed();
    println!(
        "Time taken to generate the query: {} seconds",
        end.as_secs_f64()
    );
    println!("Generated GraphQL Query:\n{query}");

    let ollama = Ollama::default().with_model(MODEL);

    // let execution_result = schema.execute(&query).await;
    // let execution_result = r#"{
    //         "data": {
    //             "issueStat": {
    //                     "openIssueCount": 24,
    //                     "totalIssueCount": 30
    //                 }
    //             }
    //         }"#;

    let system_msg = Message::new_system_message(format!(
        "You are a helpful assistant that explain the result of a GraphQL query execution.\n\n
        GraphQL schema is:\n{}\n\n
        The user query is:\n{}\n\n
        The GraphQL query is :\n{}\n
        The GraphQL query is created based on the user query and the schema.\n
        You can utilize the GraphQL query and filter if the query result matches the user query.\n\n
        The GraphQL query execution result is:\n\n{:?}\n\n
        Please provide a brief explanation for each field in the GraphQL execution result.\n
        Don't expose the field names, just explain the meaning of the result.\n
        You're answering a non-technical user, so please avoid technical jargon and explain the result in a simple way.\n\n
        If the user query is in Korean, please answer in Korean.\n\n
        If the user query is in English, please answer in English.\n\n
        ",
        schema_doc, user_query, query, execution_result,
    ));
    let human_msg = Message::new_human_message(user_query.to_string());
    let messages = vec![system_msg, human_msg];
    Ok(ollama.generate(&messages).await?.generation)
}

#[cfg(test)]
mod tests {
    use std::{fs, path::Path};

    use super::*;
    use crate::Database;

    const QUESTIONS: &[&str] = &[
        "How many P0 issues were closed between 2023-01-01 and 2023-06-30?",
        "What is the total number of open issues assigned to user 'alice' in repo 'project-x'?",
        "How many issues authored by users on vacation were updated in the first quarter of 2024?",
        "How many issues with more than 5 comments and body length 1000 were closed by 'bob'?",
        "How many P1 issues are open and were created after 2024-01-01?",

        "What is the count of issues updated in June 2024 with IP addresses in the range 192.168.0.0 to 192.168.0.255?",
        "How many issues were created by 'charlie' and assigned to 'dave'?",
        "What is the number of closed P2 issues in 'repo-abc' created before 2024-01-01?",
        "How many issues with exactly 3 comments and 2 days to close were updated in May 2025?",
        "How many issues were created by users with IP address 192.168.1.1 and status 'OnBusiness'?",

        "How many issues were created between 2023-01-01 and 2023-03-01 with priority P1?",
        "What is the total number of open P2 issues in repo 'frontend'?",
        "How many issues were updated in July 2024 and have exactly 0 comments?",
        "What is the number of issues authored by users with status OnBusiness and registered IP range 10.0.0.1–10.0.0.255?",
        "How many P0 issues in repo 'backend' were closed in April 2024?",

        "What is the count of issues assigned to 'lucas' and created after 2023-11-01 with body length 500?",
        "How many issues were created and closed in Q1 2024 by users with IP 192.168.10.10?",
        "What is the total number of issues in 'mobile-app' with 5 comments and 2 days to close?",
        "How many issues were authored by 'daniel' and assigned to 'emily' with priority P0?",
        "How many issues were updated after 2024-01-01 with status OnVacation and exactly 3 comments?",

        "What is the number of open issues created before 2024-06-01 in 'data-pipeline'?",
        "How many P1 issues were closed and updated in May 2024?",
        "What is the count of issues created by users with IP range 172.16.0.0–172.16.0.255 and with priority P2?",
        "How many issues were created and closed by the same user 'alex' with 0 body length?",
        "What is the number of issues with exactly 2 comments created between January and March 2025?",

        "How many issues in repo 'infra' were updated in December 2023 and closed in January 2024?",
        "What is the total number of issues assigned to both 'alice' and 'bob' with status OnBusiness?",
        "How many issues were created and updated in Q2 2024 with 10 comments?",
        "How many P0 issues were created by users with IP 203.0.113.42 and status OnVacation?",
        "What is the number of issues created after 2024-08-01 with daysToClose equal to 7 and 1 comment?",
    ];

    async fn query(user_query: &str, execution_result: &str) {
        let db_path = Path::new("db");
        let database = Database::connect(db_path)
            .context("Problem while Connect Sled Database.")
            .unwrap();
        let schema = crate::graphql::schema(database);
        let schema_doc = fs::read_to_string("src/semantic_parsing/schema.graphql").unwrap();
        let response = invoke(schema, &schema_doc, user_query, execution_result)
            .await
            .unwrap();
        println!("Response: {response:?}");
    }

    #[tokio::test]
    async fn test_one_semantic_parsing() {
        let schema_doc =
            fs::read_to_string("src/semantic_parsing/schema_contrived.graphql").unwrap();

        let query = "How many issues were updated in Q1 2024 by users on vacation?";
        let response = semantic_parsing(&schema_doc, query)
            .await
            .context("Failed to parse the query")
            .unwrap();
        println!("{query}");
        println!("{response}");
    }

    #[tokio::test]
    async fn test_all_semantic_parsing() {
        let schema_doc =
            fs::read_to_string("src/semantic_parsing/schema_contrived.graphql").unwrap();

        for (i, query) in QUESTIONS.iter().enumerate() {
            let response = semantic_parsing(&schema_doc, query)
                .await
                .context("Failed to parse the query")
                .unwrap();
            println!("# {}. {}", i + 1, query);
            println!("{response}");
            println!("# ----------------------------------------------------------\n");
        }
    }

    #[tokio::test]
    async fn total_issue() {
        let user_query = "How many total issues were opened by danbi2990?";
        let execution_result = r#"{
            "data": {
                "issueStat": {
                        "openIssueCount": 24,
                        "totalIssueCount": 30
                    }
                }
            }"#;
        query(user_query, execution_result).await;
    }

    #[tokio::test]
    async fn merged_pull_request() {
        let user_query = "How many merged pull requests were created by danbi2990 last month?";
        let execution_result = r#"{
            "data": {
                "pullRequestStat": {
                        "mergedPullRequestCount": 24
                    }
                }
            }"#;
        query(user_query, execution_result).await;
    }

    #[tokio::test]
    async fn korean_total_issue() {
        let user_query = "danbi2990이 생성한 이슈는 몇 개야?";
        let execution_result = r#"{
            "data": {
                "issueStat": {
                        "totalIssueCount": 30
                    }
                }
            }"#;
        query(user_query, execution_result).await;
    }
}
