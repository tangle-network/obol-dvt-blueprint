use color_eyre::Result;
use gadget_sdk as sdk;
use obol_dvt_blueprint as blueprint;
use sdk::ctx::{ServicesContext, TangleClientContext};
use sdk::docker;
use sdk::ext::subxt::tx::Signer;
use sdk::job_runner::MultiJobRunner;
use std::path::{Path, PathBuf};
use std::sync::Arc;

fn default_data_dir() -> PathBuf {
    const MANIFEST_DIR: &str = env!("CARGO_MANIFEST_DIR");
    Path::new(MANIFEST_DIR).join("data")
}

#[sdk::main(env)]
async fn main() -> Result<()> {
    color_eyre::install()?;

    let data_dir;
    match env.data_dir.clone() {
        Some(dir) => data_dir = dir,
        None => {
            tracing::warn!("Data dir not specified, using default");
            data_dir = default_data_dir();

            std::fs::create_dir_all(&data_dir)?;
        }
    }

    let docker = docker::connect_to_docker(None).await?;
    let dv_operator = blueprint::Operator::new(docker, data_dir.clone()).await?;
    let network = blueprint::start_p2p_network(&env).await?;

    let mut ctx = blueprint::ObolContext {
        network,
        dv_operator,
        env,
    };

    let client = ctx.tangle_client().await?;
    let signer = ctx.env.first_sr25519_signer()?;

    let operators = ctx.current_service_operators(&client).await?;
    let my_operator_position = operators
        .iter()
        .position(|op| op.0 == signer.account_id())
        .expect("operator should be present for the service");

    let leader = my_operator_position == 0;

    if leader {
        blueprint::request_all_enrs(&mut ctx, operators.len() - 1).await?;
    } else {
        blueprint::request_config(&ctx, my_operator_position).await?;
    }

    ctx.dv_operator.start_dkg_ceremony().await?;
    ctx.dv_operator.start_validator().await?;

    // Create the event handler from the job
    tracing::info!("Starting the event watcher ...");

    let ctx = Arc::new(ctx);
    let service_id = ctx.env.service_id.expect("should exist");
    let update_job = blueprint::UpdateEventHandler {
        ctx: Arc::clone(&ctx),
        service_id,
        signer: signer.clone(),
        client: client.clone(),
    };

    MultiJobRunner::new(ctx.env.clone())
        .job(update_job)
        .run()
        .await?;

    Ok(())
}
