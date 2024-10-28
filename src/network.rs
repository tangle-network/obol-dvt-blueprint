//! ```text
//! +---------------------+          +---------------------+
//! |       Leader        |          |   Other Nodes (N)   |
//! +---------------------+          +---------------------+
//!         |                                 |
//!         |<------ HereIAm -----------------| (1) Initial ping
//!         |                                 |
//!         |------- RequestEnr ------------->| (2) Broadcast, after all peers available
//!         |                                 |
//!         |<------ SendEnr(String) ---------| (3) Response
//!         |                                 |
//!         |------- EnrReceived ------------>| (4) Acknowledgment
//!         |                                 |
//!         |------- DkgConfigGenerated ----->| (5) After all ENRs received
//!         |                                 |
//!         |<------ DkgConfigReceived -------| (6) Acknowledgment
//!         |                                 |
//!         |------- ExchangeEnd ------------>| (7) Broadcast, Final acknowledgment
//! ```

// TODO: Potential improvements

// The flow is: service starts, operators share their ENRs, the config is generated, and then distributed.
//
// In that flow, what if:
// * A peer drops out at any point
// * A peer doesn't share its ENR
// * The peers dont get the config from the leader
//    * Could just go to the next operator, round-robin style
//    * Did the leader not send it? Was there a network error?

use super::{DkgConfig, ObolContext};
use color_eyre::eyre::eyre;
use color_eyre::{Report, Result};
use gadget_sdk as sdk;
use libp2p::swarm::{NetworkBehaviour, SwarmEvent};
use libp2p::{identify, noise, ping, relay, tcp, yamux};
use sdk::config::StdGadgetConfiguration;
use sdk::ext::sp_core::Pair;
use sdk::futures::StreamExt;
use sdk::keystore::BackendExt;
use sdk::libp2p;
use sdk::network::channels::UserID;
use sdk::network::gossip::GossipHandle;
use sdk::network::setup::NetworkConfig;
use sdk::network::{IdentifierInfo, Network};
use serde::{Deserialize, Serialize};

// TODO: For testing, want to ensure all peers are running
async fn spin(env: &StdGadgetConfiguration, identity: libp2p::identity::Keypair) -> Result<()> {
    tracing::info!("Spinning till all peers available");
    #[derive(NetworkBehaviour)]
    struct Behaviour {
        relay: relay::Behaviour,
        ping: ping::Behaviour,
        identify: identify::Behaviour,
    }

    let mut swarm = libp2p::SwarmBuilder::with_existing_identity(identity)
        .with_tokio()
        .with_tcp(
            tcp::Config::default(),
            noise::Config::new,
            yamux::Config::default,
        )?
        .with_quic()
        .with_behaviour(|key| Behaviour {
            relay: relay::Behaviour::new(key.public().to_peer_id(), Default::default()),
            ping: ping::Behaviour::new(ping::Config::new()),
            identify: identify::Behaviour::new(identify::Config::new(
                "/TODO/0.0.1".to_string(),
                key.public(),
            )),
        })?
        .build();

    swarm.listen_on(format!("/ip4/{}/tcp/{}", env.bind_addr, env.bind_port).parse()?)?;

    // Connect to bootnodes
    for addr in &env.bootnodes {
        swarm.dial(addr.clone())?;
    }

    // Spin until all bootnodes are connected
    let mut connected_bootnodes = 0;
    let mut all_peers_discovered = false;

    loop {
        tracing::info!("Checking events");
        match swarm.select_next_some().await {
            SwarmEvent::ConnectionEstablished { peer_id, .. } => {
                tracing::info!("Connected to {:?}", peer_id);
                connected_bootnodes += 1;

                if connected_bootnodes == env.bootnodes.len() {
                    tracing::info!("All bootnodes connected!");
                    all_peers_discovered = true;
                    break;
                }
            }
            SwarmEvent::ConnectionClosed { peer_id, .. } => {
                if !all_peers_discovered {
                    // TODO
                    tracing::error!("{:?} dropped, how to handle?", peer_id);
                }
            }
            e => {
                tracing::error!("{e:?}");
            }
        }
    }

    tracing::info!("Done spinning");

    Ok(())
}

pub async fn start_p2p_network(env: &StdGadgetConfiguration) -> Result<GossipHandle> {
    let ecdsa = env.keystore()?.ecdsa_key()?;
    let identity = libp2p::identity::Keypair::generate_ed25519();

    spin(&env, identity.clone()).await?;

    let network_config = NetworkConfig::new_service_network(
        identity,
        ecdsa.signer().clone(),
        env.bootnodes.clone(),
        env.bind_addr,
        env.bind_port,
        "/obol-dvt-config",
    );

    let handle =
        sdk::network::setup::start_p2p_network(network_config).map_err(|e| eyre!(e.to_string()))?;

    Ok(handle)
}

pub async fn request_all_enrs(ctx: &mut ObolContext, expected_count: usize) -> Result<Vec<String>> {
    let mut enrs = Vec::with_capacity(expected_count);

    let my_ecdsa_key = ctx.env.keystore()?.ecdsa_key()?.public();

    let _span = tracing::info_span!("leader", key = %my_ecdsa_key).entered();

    // TODO ??
    let my_user_id = 0;

    let mut peers = 0;
    let mut configs_received = 0;
    while let Some(msg) = ctx.network.next_message().await {
        let payload: Msg = sdk::network::deserialize(&msg.payload)?;

        match payload {
            Msg::HereIAm => {
                tracing::info!("Received HereIAm from peer #{}", msg.sender.user_id);
                peers += 1;

                if peers == expected_count {
                    tracing::info!("Requesting all ENRs");

                    let enr_request = GossipHandle::build_protocol_message(
                        IdentifierInfo {
                            block_id: None,
                            session_id: None,
                            retry_id: None,
                            task_id: None,
                        },
                        my_user_id,
                        None, // Broadcast
                        &Msg::RequestEnr,
                        Some(my_ecdsa_key),
                        None,
                    );

                    ctx.network.send_message(enr_request).await?;
                }
            }
            Msg::SendEnr(enr) => {
                tracing::info!("Received a new ENR from peer #{}", msg.sender.user_id);

                enrs.push(enr);

                let response = GossipHandle::build_protocol_message(
                    IdentifierInfo {
                        block_id: None,
                        session_id: None,
                        retry_id: None,
                        task_id: None,
                    },
                    my_user_id,
                    Some(msg.sender.user_id),
                    &Msg::EnrReceived,
                    Some(my_ecdsa_key),
                    None,
                );

                ctx.network.send_message(response).await?;

                if enrs.len() == expected_count {
                    let dkg_config = create_dkg_config(ctx, enrs.clone()).await?;

                    tracing::info!("Broadcasting DKG config to peers");
                    let broadcast = GossipHandle::build_protocol_message(
                        IdentifierInfo {
                            block_id: None,
                            session_id: None,
                            retry_id: None,
                            task_id: None,
                        },
                        my_user_id,
                        None,
                        &Msg::DkgConfigGenerated(dkg_config.clone()),
                        Some(my_ecdsa_key),
                        None,
                    );

                    ctx.network.send_message(broadcast).await?;
                }
            }
            Msg::DkgConfigReceived => {
                // TODO: And if they dont...?
                tracing::info!(
                    "Peer #{} received the DKG config successfully",
                    msg.sender.user_id
                );

                configs_received += 1;
                if configs_received == expected_count {
                    tracing::info!("Broadcasting exchange end to peers");
                    let broadcast = GossipHandle::build_protocol_message(
                        IdentifierInfo {
                            block_id: None,
                            session_id: None,
                            retry_id: None,
                            task_id: None,
                        },
                        my_user_id,
                        None,
                        &Msg::ExchangeEnd,
                        Some(my_ecdsa_key),
                        None,
                    );

                    ctx.network.send_message(broadcast).await?;
                    break;
                }
            }
            _ => continue,
        }
    }

    if enrs.len() != expected_count {
        return Err(Report::msg("Not all ENRs were acquired").into());
    }

    Ok(enrs)
}

async fn create_dkg_config(ctx: &mut ObolContext, enrs: Vec<String>) -> Result<String> {
    let dkg_config = DkgConfig {
        name: "Example".to_string(),
        validator_count: 1,
        enrs,
        todo_bogus_fee_recipient_address: String::from(
            "0x0000000000000000000000000000000000000000",
        ),
        todo_bogus_withdraw_address: String::from("0xfB6916095ca1df60bB79Ce92cE3Ea74c37c5d359"),
    };

    ctx.dv_operator.create_dkg_config(Some(dkg_config)).await?;
    let content = ctx.dv_operator.fetch_dkg_config().await?;
    Ok(content)
}

pub async fn request_config(ctx: &ObolContext, my_operator_position: usize) -> Result<()> {
    let my_ecdsa_key = ctx.env.keystore()?.ecdsa_key()?.public();

    let _span =
        tracing::info_span!("peer", user_id = %my_operator_position, key = %my_ecdsa_key).entered();

    // TODO ??
    let my_user_id = my_operator_position as UserID;
    let mut leader_user_id = 0;

    let initial_ping = GossipHandle::build_protocol_message(
        IdentifierInfo {
            block_id: None,
            session_id: None,
            retry_id: None,
            task_id: None,
        },
        my_user_id,
        Some(leader_user_id),
        &Msg::HereIAm,
        Some(my_ecdsa_key),
        None,
    );

    ctx.network.send_message(initial_ping).await?;
    while let Some(msg) = ctx.network.next_message().await {
        let payload: Msg = sdk::network::deserialize(&msg.payload)?;

        match payload {
            Msg::DkgConfigGenerated(msg) => {
                tracing::info!("Received DKG config, copying...");

                ctx.dv_operator.copy_in_dkg_config(msg).await?;

                let response = GossipHandle::build_protocol_message(
                    IdentifierInfo {
                        block_id: None,
                        session_id: None,
                        retry_id: None,
                        task_id: None,
                    },
                    my_user_id,
                    Some(leader_user_id),
                    &Msg::DkgConfigReceived,
                    Some(my_ecdsa_key),
                    None,
                );

                ctx.network.send_message(response).await?;
            }
            Msg::RequestEnr => {
                tracing::info!("Leader requested ENR, sending...");

                leader_user_id = msg.sender.user_id;
                let response = GossipHandle::build_protocol_message(
                    IdentifierInfo {
                        block_id: None,
                        session_id: None,
                        retry_id: None,
                        task_id: None,
                    },
                    my_user_id,
                    Some(leader_user_id),
                    &Msg::SendEnr(ctx.dv_operator.enr().to_string()),
                    Some(my_ecdsa_key),
                    None,
                );
                ctx.network.send_message(response).await?;
            }
            Msg::EnrReceived => {
                tracing::info!("Leader received my ENR");
            }
            Msg::ExchangeEnd => {
                tracing::info!("Ending exchange by leader request...");
                break;
            }
            _ => continue,
        }
    }

    Ok(())
}

#[derive(Serialize, Deserialize)]
enum Msg {
    HereIAm,

    RequestEnr,
    SendEnr(String),
    EnrReceived,

    DkgConfigGenerated(String),
    DkgConfigReceived,

    ExchangeEnd,
}
