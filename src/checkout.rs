use std::time::Duration;
use std::{
    env::var,
    path::{Path, PathBuf},
    sync::Arc,
};

use anyhow::{anyhow, Result};
use directories::ProjectDirs;
use git2::{Cred, FetchOptions, RemoteCallbacks, Repository};
use tokio::time;
use tracing::{error, info};

use crate::settings::Repository as RepoInfo;

const FETCH_HEAD: &str = "FETCH_HEAD";
const LOCAL_BASE_REPO: &str = "repos";
const MAIN_BRANCH: &str = "main";
const REMOTE_NAME: &str = "origin";
const REMOTE_BASE_URL: &str = "git@github.com";
const ENV_HOME: &str = "HOME";
const ENV_SSH_PASSPHRASE: &str = "SSH_PASSPHRASE";

fn callbacks(ssh: &str) -> Result<RemoteCallbacks<'_>> {
    let mut callbacks = RemoteCallbacks::new();
    let ssh_path = Path::new(&var(ENV_HOME)?).join(ssh);
    let passphrase = std::env::var(ENV_SSH_PASSPHRASE).ok();
    callbacks.credentials(move |_url, username_from_url, _allowed_types| {
        Cred::ssh_key(
            username_from_url.unwrap_or("git"),
            None,
            &ssh_path,
            passphrase.as_deref(),
        )
    });
    Ok(callbacks)
}

fn fetchoption(ssh: &str) -> Result<FetchOptions<'_>> {
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
    if let Some(proj_dirs) = ProjectDirs::from_path(PathBuf::from(LOCAL_BASE_REPO)) {
        Ok(proj_dirs.cache_dir().join(repo_name))
    } else {
        Err(anyhow!("Faild to load cache directory"))
    }
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

pub(super) async fn fetch_periodically(
    repositories: Arc<Vec<RepoInfo>>,
    duration: Duration,
    ssh: String,
) {
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
