use std::time::Duration;
use std::{
    env::var,
    path::{Path, PathBuf},
    sync::Arc,
};

use anyhow::{anyhow, Result};
use git2::{Cred, FetchOptions, RemoteCallbacks, Repository};
use tokio::time;
use tracing::{error, info};

use crate::conf::RepoInfo;

const FETCH_HEAD: &str = "FETCH_HEAD";
const LOCAL_BASE_REPO: &str = "repos";
const MAIN_BRANCH: &str = "main";
const REMOTE_NAME: &str = "origin";
const REMOTE_BASE_URL: &str = "git@github.com";

fn callbacks(ssh: &str) -> Result<RemoteCallbacks> {
    let mut callbacks = RemoteCallbacks::new();
    let ssh_path = Path::new(&var("HOME")?).join(ssh);
    callbacks.credentials(move |_url, username_from_url, _allowed_types| {
        Cred::ssh_key(username_from_url.unwrap(), None, &ssh_path, None)
    });
    Ok(callbacks)
}

fn fetchoption(ssh: &str) -> Result<FetchOptions> {
    let mut fo = git2::FetchOptions::new();
    fo.remote_callbacks(callbacks(ssh)?);
    Ok(fo)
}

fn init_repo(repo_owner: &str, repo_name: &str, ssh: &str) -> Result<()> {
    let mut builder = git2::build::RepoBuilder::new();
    builder.fetch_options(fetchoption(ssh)?);
    let path = local_repo_path(repo_name)?;
    if !path.exists() {
        std::fs::create_dir_all(&path)?;
        builder.clone(
            &format!("{REMOTE_BASE_URL}:{repo_owner}/{repo_name}.git"),
            &path,
        )?;
    }
    Ok(())
}

fn local_repo_path(repo_name: &str) -> Result<PathBuf> {
    let cache_dir = var("XDG_CACHE_HOME")
        .map(PathBuf::from)
        .or_else(|_| {
            var("HOME")
                .map(PathBuf::from)
                .map(|home| home.join(".cache"))
        })
        .map_err(|_| anyhow!("Failed to determine cache directory"))?;

    Ok(cache_dir.join(LOCAL_BASE_REPO).join(repo_name))
}

fn pull_repo(name: &str, ssh: &str) -> Result<()> {
    let repo = Repository::open(local_repo_path(name)?)?;
    repo.find_remote(REMOTE_NAME)?
        .fetch(&[MAIN_BRANCH], Some(&mut fetchoption(ssh)?), None)?;
    let fetch_head = repo.find_reference(FETCH_HEAD)?;
    let fetch_commit = repo.reference_to_annotated_commit(&fetch_head)?;
    let (analysis, _) = repo.merge_analysis(&[&fetch_commit])?;
    if analysis.is_up_to_date() {
        info!("Already up to date");
    } else if analysis.is_fast_forward() {
        let refname = format!("refs/heads/{MAIN_BRANCH}");
        let mut reference = repo.find_reference(&refname)?;
        reference.set_target(fetch_commit.id(), "Fast-Forward")?;
        repo.set_head(&refname)?;
        repo.checkout_head(Some(git2::build::CheckoutBuilder::default().force()))?;
        info!("New commit pull success");
    }
    Ok(())
}

pub async fn fetch_periodically(repositories: Arc<Vec<RepoInfo>>, duration: Duration, ssh: String) {
    for repo_info in repositories.iter() {
        if let Err(error) = init_repo(&repo_info.owner, &repo_info.name, &ssh) {
            error!("{}", error);
        }
    }
    let mut itv = time::interval(duration);
    loop {
        itv.tick().await;
        for repo_info in repositories.iter() {
            if let Err(error) = pull_repo(&repo_info.name, &ssh) {
                error!("Problem while git pull. {}", error);
            }
        }
    }
}
