use crate::DkgConfig;
use bollard::Docker;
use color_eyre::{Report, Result};
use gadget_sdk as sdk;
use gadget_sdk::docker::bollard::container::{LogOutput, LogsOptions};
use sdk::docker::{bollard, Container};
use sdk::ext::subxt::ext::futures::StreamExt;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::Arc;

pub struct Operator {
    data_dir: PathBuf,
    enr: String,
    docker: Arc<Docker>,
    span: tracing::Span,
}

const IMAGE: &str = "obolnetwork/charon:v1.1.1";
const CHARON_DATA: &str = "/opt/charon";

impl Operator {
    pub async fn new(docker: Arc<Docker>, mut data_dir: PathBuf) -> Result<Operator> {
        let span = tracing::info_span!("operator", path = %data_dir.display());

        let repo_path = std::path::absolute(data_dir.join("charon-distributed-validator-node"))?;

        if !repo_path.exists() {
            tracing::warn!("Git repo does not exist, cloning...");
            let output = Command::new("git")
                .arg("clone")
                .arg("https://github.com/ObolNetwork/charon-distributed-validator-node.git")
                .arg(&repo_path)
                .output()?;

            if !output.status.success() {
                return Err(Report::msg(
                    "Failed to clone charon-distributed-validator-node",
                ));
            }

            // TODO: Remove, allow own env
            std::fs::copy(
                repo_path.join(".env.sample.holesky"),
                &repo_path.join(".env"),
            )?;
        }

        data_dir = repo_path;

        let enr_path = data_dir.join(".charon").join("charon-enr-private-key");
        let enr;
        if enr_path.exists() {
            tracing::info!("ENR exists, reading from {}", enr_path.display());
            enr = std::fs::read_to_string(&data_dir.join("enr.pub"))?;
        } else {
            tracing::info!("ENR not found, creating one...");
            enr = create_enr(&docker, &data_dir).await?;
            tracing::info!("Successfully created ENR");
        }

        Ok(Operator {
            data_dir,
            enr,
            docker,
            span,
        })
    }

    pub fn enr(&self) -> &str {
        &self.enr
    }

    pub fn data_dir(&self) -> &Path {
        self.data_dir.as_path()
    }

    #[tracing::instrument(parent = &self.span, skip_all)]
    pub async fn create_dkg_config(&mut self, config: Option<DkgConfig>) -> Result<()> {
        let dkg_conf_path = self
            .data_dir
            .join(".charon")
            .join("cluster-definition.json");
        if dkg_conf_path.exists() {
            tracing::info!("DKG config exists at: {}", dkg_conf_path.display());
        }

        let Some(config) = config else {
            return Ok(());
        };

        if dkg_conf_path.exists() {
            return Ok(());
        }

        tracing::info!("DKG configuration not found, creating one...");

        let other_operator_enrs = config.enrs.join(",");
        let enrs = format!("{},{other_operator_enrs}", self.enr);

        let mut container = Container::new(&self.docker, IMAGE.to_string());

        container
            .cmd(vec![
                "create",
                "dkg",
                "--name",
                config.name.as_str(),
                "--num-validators",
                config.validator_count.to_string().as_str(),
                "--fee-recipient-addresses",
                config.todo_bogus_fee_recipient_address.as_str(),
                "--withdrawal-addresses",
                config.todo_bogus_withdraw_address.as_str(),
                "--operator-enrs",
                enrs.as_str(),
            ])
            .binds(vec![format!("{}:{CHARON_DATA}", self.data_dir.display())]);

        container.start(true).await?;
        container.remove(None).await?;

        tracing::info!("Successfully created DKG config");

        Ok(())
    }

    #[tracing::instrument(parent = &self.span, skip_all)]
    pub async fn fetch_dkg_config(&self) -> Result<String> {
        let content = tokio::fs::read_to_string(
            &self
                .data_dir
                .join(".charon")
                .join("cluster-definition.json"),
        )
        .await?;

        Ok(content)
    }

    #[tracing::instrument(parent = &self.span, skip_all)]
    pub async fn copy_in_dkg_config(&self, config: String) -> Result<()> {
        tokio::fs::write(
            self.data_dir
                .join(".charon")
                .join("cluster-definition.json"),
            config,
        )
        .await?;

        Ok(())
    }

    #[tracing::instrument(parent = &self.span, skip_all)]
    pub async fn start_dkg_ceremony(&self) -> Result<()> {
        let cluster_lock_path = self.data_dir.join(".charon").join("cluster-lock.json");
        if cluster_lock_path.exists() {
            tracing::info!("Skipping DKG ceremony, already performed");
            return Ok(());
        }

        tracing::info!("Starting DKG ceremony...");
        let mut container = Container::new(&self.docker, IMAGE.to_string());

        container
            .cmd(vec!["dkg", "--publish"])
            .binds(vec![format!("{}:{CHARON_DATA}", self.data_dir.display())]);

        container.start(true).await?;
        container.remove(None).await?;

        if !cluster_lock_path.exists() {
            todo!("How to handle failure?");
        }

        tracing::info!("DKG ceremony succeeded");

        Ok(())
    }

    #[tracing::instrument(parent = &self.span, skip_all)]
    pub async fn start_validator(&self) -> Result<String> {
        tracing::info!("Starting validator");
        let container_id = docker_compose(&self.data_dir).await?;
        tracing::debug!("Started with container ID: {container_id}");

        Ok(container_id)
    }
}

async fn create_enr(docker: &Docker, data_dir: &Path) -> Result<String> {
    let mut container = Container::new(docker, IMAGE.to_string());

    container
        .cmd(vec!["create", "enr"])
        .binds(vec![format!("{}:{CHARON_DATA}", data_dir.display())]);

    container.create().await?;
    container.start(true).await?;

    let Some(mut logs) = container
        .logs(Some(LogsOptions {
            stdout: true,
            stderr: true,
            follow: true,
            ..Default::default()
        }))
        .await
    else {
        tracing::error!("Failed to create ENR, no output available");
        return Err(Report::msg("Failed to create ENR").into());
    };

    let mut enr = None;
    while let Some(Ok(out)) = logs.next().await {
        if let LogOutput::StdErr { message } = out {
            tracing::error!("{}", String::from_utf8_lossy(&message));
            continue;
        }

        for line in out.as_ref().split(|b| *b == b'\n') {
            if line.starts_with(b"enr:-") {
                enr = Some(String::from_utf8_lossy(line.as_ref()).into_owned());
                break;
            }
        }
    }

    let Some(enr) = enr else {
        tracing::error!("Failed to create ENR");
        return Err(Report::msg("Failed to create ENR").into());
    };

    std::fs::write(data_dir.join("enr.pub"), enr.as_bytes())?;

    container.remove(None).await?;

    Ok(enr)
}

/// Run `docker compose up -d` and return the container ID
pub(crate) async fn docker_compose(dir: &Path) -> Result<String> {
    let _ = Command::new("docker-compose")
        .arg("up")
        .arg("-d")
        .current_dir(dir)
        .output()?;

    let out = Command::new("docker-compose")
        .arg("ps")
        .arg("-q")
        .arg("charon")
        .current_dir(dir)
        .output()?;

    Ok(String::from_utf8_lossy(&out.stdout).to_string())
}
