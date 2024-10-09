pub use bollard;
use bollard::container::{
    Config, CreateContainerOptions, LogOutput, LogsOptions, RemoveContainerOptions,
    StartContainerOptions, StopContainerOptions, WaitContainerOptions,
};
use bollard::models::{ContainerCreateResponse, HostConfig};
use bollard::{Docker, API_DEFAULT_VERSION};
use color_eyre::{Report, Result};
use gadget_sdk::ext::subxt::ext::futures::{Stream, StreamExt};
use std::path::Path;
use std::process::Command;
use std::sync::Arc;

#[derive(Debug)]
pub struct Container<'a> {
    id: Option<String>,
    image: String,
    connection: &'a Docker,
    options: ContainerOptions,
}

#[derive(Debug, Default, Clone)]
struct ContainerOptions {
    env: Option<Vec<String>>,
    cmd: Option<Vec<String>>,
    binds: Option<Vec<String>>,
}

impl<'a> Container<'a> {
    pub fn new(connection: &'a Docker, image: String) -> Self {
        Self {
            id: None,
            image,
            connection,
            options: ContainerOptions::default(),
        }
    }

    pub fn env(&mut self, env: impl Iterator<Item = impl Into<String>>) -> &mut Self {
        self.options.env = Some(env.into_iter().map(Into::into).collect());
        self
    }

    pub fn cmd(&mut self, cmd: impl IntoIterator<Item = impl Into<String>>) -> &mut Self {
        self.options.cmd = Some(cmd.into_iter().map(Into::into).collect());
        self
    }

    pub fn binds(&mut self, binds: impl IntoIterator<Item = impl Into<String>>) -> &mut Self {
        self.options.binds = Some(binds.into_iter().map(Into::into).collect());
        self
    }

    /// Get the container ID if it has been created
    ///
    /// This will only have a value if [`Container::create`] or [`Container::start`] has been
    /// called prior.
    pub fn id(&self) -> Option<&str> {
        self.id.as_deref()
    }

    #[tracing::instrument]
    pub async fn create(&mut self) -> Result<()> {
        tracing::debug!("Creating container");

        let config = Config {
            image: Some(self.image.clone()),
            cmd: self.options.cmd.clone(),
            attach_stdout: Some(true),
            host_config: Some(HostConfig {
                binds: self.options.binds.clone(),
                ..Default::default()
            }),
            ..Default::default()
        };

        let ContainerCreateResponse { id, warnings } = self
            .connection
            .create_container(None::<CreateContainerOptions<String>>, config)
            .await?;
        for warning in warnings {
            tracing::warn!("{}", warning);
        }

        self.id = Some(id);
        Ok(())
    }

    #[tracing::instrument]
    pub async fn start(&mut self, wait_for_exit: bool) -> Result<()> {
        if self.id.is_none() {
            self.create().await?;
        }

        tracing::debug!("Starting container");
        let id = self.id.as_ref().unwrap();
        self.connection
            .start_container(&id, None::<StartContainerOptions<String>>)
            .await?;

        if wait_for_exit {
            self.wait().await?;
        }

        Ok(())
    }

    #[tracing::instrument]
    pub async fn stop(&mut self) -> Result<()> {
        let Some(id) = &self.id else {
            tracing::warn!("Container not started");
            return Ok(());
        };

        self.connection
            .stop_container(&id, None::<StopContainerOptions>)
            .await?;

        Ok(())
    }

    #[tracing::instrument]
    pub async fn remove(mut self) -> Result<()> {
        let Some(id) = self.id.take() else {
            tracing::warn!("Container not started");
            return Ok(());
        };

        self.connection
            .remove_container(&id, None::<RemoveContainerOptions>)
            .await?;
        Ok(())
    }

    #[tracing::instrument]
    pub async fn wait(&self) -> Result<()> {
        let Some(id) = &self.id else {
            tracing::warn!("Container not created");
            return Ok(());
        };

        wait_for_container(self.connection, id).await?;
        Ok(())
    }

    #[tracing::instrument]
    pub async fn logs(
        &self,
        logs_options: Option<LogsOptions<String>>,
    ) -> Result<Option<impl Stream<Item = Result<LogOutput, bollard::errors::Error>>>> {
        let Some(id) = &self.id else {
            tracing::warn!("Container not created");
            return Ok(None);
        };

        Ok(Some(self.connection.logs(id, logs_options)))
    }
}

pub async fn connect_to_docker() -> Result<Arc<Docker>> {
    tracing::info!("Connecting to local docker server...");
    let docker = Docker::connect_with_socket("/var/run/docker.sock", 120, API_DEFAULT_VERSION)?;
    if let Err(e) = docker.ping().await {
        tracing::error!("Failed to ping docker server: {}", e);
        return Err(e.into());
    }

    Ok(Arc::new(docker))
}

async fn wait_for_container(docker: &Docker, id: &str) -> Result<()> {
    let options = WaitContainerOptions {
        condition: "not-running",
    };

    let mut wait_stream = docker.wait_container(&id, Some(options));

    while let Some(msg) = wait_stream.next().await {
        match msg {
            Ok(msg) => {
                if msg.status_code == 0 {
                    break;
                }

                if let Some(err) = msg.error {
                    tracing::error!("Container failed: {:?}", err.message);
                    return Err(Report::msg(err.message.unwrap_or_default()).into());
                }
            }
            Err(e) => {
                match &e {
                    bollard::errors::Error::DockerContainerWaitError { error, code } => {
                        tracing::error!("Container failed with status code `{}`: {error}", code);
                    }
                    _ => tracing::error!("Container failed with error: {:?}", e),
                }
                return Err(e.into());
            }
        }
    }

    Ok(())
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
