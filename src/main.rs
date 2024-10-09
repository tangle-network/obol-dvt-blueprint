use bollard::container::{
    AttachContainerOptions, AttachContainerResults, Config, CreateContainerOptions, LogOutput,
    StartContainerOptions, StopContainerOptions, WaitContainerOptions,
};
use bollard::models::{ContainerCreateResponse, HostConfig};
use bollard::{Docker, API_DEFAULT_VERSION};
use color_eyre::{Report, Result};
use gadget_sdk as sdk;
use gadget_sdk::event_listener::{EventListener, IntoTangleEventListener};
use gadget_sdk::ext::subxt::ext::futures::StreamExt;
use obol_dvt_blueprint as blueprint;
use sdk::{config::ContextConfig, events_watcher::tangle::TangleEventsWatcher, tangle_subxt::*};
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::Arc;
use structopt::StructOpt;

fn default_data_dir() -> PathBuf {
    const MANIFEST_DIR: &str = env!("CARGO_MANIFEST_DIR");
    Path::new(MANIFEST_DIR).join("data")
}

struct DkgConfig {
    name: String,
    validator_count: u32,
    todo_bogus_enrs: Vec<String>,
    todo_bogus_withdraw_address: String,
}

#[tokio::main]
async fn main() -> Result<()> {
    init_logger();
    color_eyre::install()?;

    // Initialize the environment
    let config = ContextConfig::from_args();
    let env = sdk::config::load(config)?;

    if env.should_run_registration() {
        todo!();
    }

    let service_id = env.service_id.expect("should exist");
    let data_dir;
    match env.data_dir.clone() {
        Some(dir) => data_dir = dir,
        None => {
            tracing::warn!("Data dir not specified, using default");
            data_dir = default_data_dir();

            std::fs::create_dir_all(&data_dir)?;
        }
    }

    let config = DkgConfig {
        name: String::from("Example"),
        validator_count: 1,
        todo_bogus_enrs: vec![
            String::from("enr:-HW4QEKojwEHnw3srfI_ENACLE6OcAJcZEFOpW3-Q74jJzNXAApwZXZrPFQBW5x4TmvQJIfY1s41bJ2r_Tc1mQ6VvxyAgmlkgnY0iXNlY3AyNTZrMaEDaa9u2Gm_X_gSVLDyuB80Zae45Q-WIe9Nz-_5ou-AEuM"),
            String::from("enr:-HW4QBcvO2H-WNcwV98muOtDANCnGCNBv7uJG8Wy-88XbImaZBgFziOxpoiNVtZSfLPBXZxfbWRnOm5xkSWtIVteex6AgmlkgnY0iXNlY3AyNTZrMaECkmntjKTOsVLuC8XPwfEKhLqOAIm0G16pddwSGqAX3Co")
        ],
        todo_bogus_withdraw_address: String::from("0xfB6916095ca1df60bB79Ce92cE3Ea74c37c5d359")
    };

    let (docker, container_id) = setup_cluster(&data_dir, config).await?;
    let ctx = Arc::new(blueprint::ObolContext {
        docker,
        container_id,
    });

    // Create the event handler from the job
    let signer = env.first_sr25519_signer()?;
    let client = subxt::OnlineClient::from_url(&env.rpc_endpoint).await?;
    let update_job = blueprint::UpdateEventHandler {
        ctx: Arc::clone(&ctx),
        service_id,
        signer: signer.clone(),
    };
    let activate_job = blueprint::ActivateEventHandler {
        ctx: Arc::clone(&ctx),
        service_id,
        signer: signer.clone(),
    };

    tracing::info!("Starting the event watcher ...");

    let watcher = TangleEventsWatcher {
        span: env.span.clone(),
        client,
        handlers: vec![Box::new(update_job), Box::new(activate_job)],
    };

    watcher.into_tangle_event_listener().execute().await;

    Ok(())
}

const IMAGE: &str = "obolnetwork/charon:v1.1.1";
async fn setup_cluster(data_dir: &Path, config: DkgConfig) -> Result<(Docker, String)> {
    tracing::info!("Connecting to local docker server...");
    let docker = Docker::connect_with_socket("/var/run/docker.sock", 120, API_DEFAULT_VERSION)?;
    if let Err(e) = docker.ping().await {
        tracing::error!("Failed to ping docker server: {}", e);
        return Err(e.into());
    }

    let enr_path = data_dir.join(".charon").join("charon-enr-private-key");
    let enr;
    if enr_path.exists() {
        enr = std::fs::read_to_string(data_dir.join("enr.pub"))?;
    } else {
        tracing::info!("ENR not found, creating one...");
        enr = create_enr(&docker, data_dir).await?;
    }

    let dkg_conf_path = data_dir.join(".charon").join("cluster-definition.json");
    if !dkg_conf_path.exists() {
        tracing::info!("DKG configuration not found, creating one...");
        create_dkg(&docker, &enr, data_dir, config).await?;
    }

    let cluster_lock_path = data_dir.join(".charon").join("cluster-lock.json");
    if !cluster_lock_path.exists() {
        tracing::info!("Starting DKG ceremony");
        dkg_ceremony(&docker, data_dir).await?;
    }

    tracing::info!("Starting validator");
    let container_id = docker_compose(data_dir).await?;
    tracing::debug!("Started with container ID: {container_id}");

    Ok((docker, container_id))
}

async fn create_enr(docker: &Docker, data_dir: &Path) -> Result<String> {
    let id = create_container(data_dir, docker, Some(vec!["create", "enr"])).await?;

    tracing::debug!("Starting container");
    docker
        .start_container(&id, None::<StartContainerOptions<String>>)
        .await?;

    tracing::debug!("Attaching to container");
    let AttachContainerResults { mut output, .. } = docker
        .attach_container(
            &id,
            Some(AttachContainerOptions::<String> {
                stdout: Some(true),
                stderr: Some(true),
                stream: Some(true),
                ..Default::default()
            }),
        )
        .await?;

    let mut enr = None;
    while let Some(Ok(out)) = output.next().await {
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

    std::fs::write(&data_dir.join("enr.pub"), enr.as_bytes())?;

    docker
        .stop_container(&id, None::<StopContainerOptions>)
        .await?;

    Ok(enr)
}

async fn create_dkg(docker: &Docker, enr: &str, data_dir: &Path, config: DkgConfig) -> Result<()> {
    let todo_enrs = config.todo_bogus_enrs.join(",");
    let todo_enrs = format!("{enr},{todo_enrs}");

    let id = create_container(
        data_dir,
        docker,
        Some(vec![
            "create",
            "dkg",
            "--name",
            config.name.as_str(),
            "--num-validators",
            config.validator_count.to_string().as_str(),
            "--fee-recipient-addresses",
            "0x0000000000000000000000000000000000000000",
            "--withdrawal-addresses",
            config.todo_bogus_withdraw_address.as_str(),
            "--operator-enrs",
            todo_enrs.as_str(),
        ]),
    )
    .await?;

    tracing::debug!("Starting container");
    docker
        .start_container(&id, None::<StartContainerOptions<String>>)
        .await?;

    wait_for_container(&docker, &id).await?;

    docker
        .stop_container(&id, None::<StopContainerOptions>)
        .await?;

    Ok(())
}

async fn dkg_ceremony(docker: &Docker, data_dir: &Path) -> Result<()> {
    let id = create_container(data_dir, docker, Some(vec!["dkg", "--publish"])).await?;

    tracing::debug!("Starting container");
    docker
        .start_container(&id, None::<StartContainerOptions<String>>)
        .await?;

    wait_for_container(&docker, &id).await?;

    docker
        .stop_container(&id, None::<StopContainerOptions>)
        .await?;

    Ok(())
}

const CHARON_DATA: &str = "/opt/charon";
async fn create_container(
    data_dir: &Path,
    docker: &Docker,
    command: Option<Vec<&str>>,
) -> Result<String> {
    tracing::debug!("Creating container");

    let config = Config {
        image: Some(IMAGE),
        cmd: command,
        attach_stdout: Some(true),
        host_config: Some(HostConfig {
            binds: Some(vec![format!("{}:{CHARON_DATA}", data_dir.display())]),
            auto_remove: Some(true),
            ..Default::default()
        }),
        ..Default::default()
    };

    let ContainerCreateResponse { id, warnings } = docker
        .create_container(None::<CreateContainerOptions<String>>, config)
        .await?;
    for warning in warnings {
        tracing::warn!("{}", warning);
    }

    Ok(id)
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
async fn docker_compose(dir: &Path) -> Result<String> {
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

fn init_logger() {
    let env_filter = tracing_subscriber::EnvFilter::from_default_env();
    tracing_subscriber::fmt()
        .compact()
        .with_target(true)
        .with_env_filter(env_filter)
        .init();
}
