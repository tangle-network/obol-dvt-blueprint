mod network;
mod operator;

pub use network::*;
pub use operator::*;

use gadget_sdk as sdk;
use sdk::config::StdGadgetConfiguration;
use sdk::ctx::{ServicesContext, TangleClientContext};
use sdk::event_listener::tangle::jobs::{services_post_processor, services_pre_processor};
use sdk::event_listener::tangle::TangleEventListener;
use sdk::job;
use sdk::network::gossip::GossipHandle;
use sdk::tangle_subxt::tangle_testnet_runtime::api::services::events::JobCalled;
use std::convert::Infallible;
use std::sync::Arc;

#[derive(TangleClientContext, ServicesContext)]
pub struct ObolContext {
    pub dv_operator: Operator,
    pub network: GossipHandle,
    #[config]
    pub env: StdGadgetConfiguration,
}

pub struct DkgConfig {
    pub name: String,
    pub validator_count: u32,
    pub enrs: Vec<String>,
    // TODO: These should be request arguments
    pub todo_bogus_fee_recipient_address: String,
    pub todo_bogus_withdraw_address: String,
}

#[job(
    id = 0,
    params(a),
    result(_),
    event_listener(
        listener = TangleEventListener<Arc<ObolContext>, JobCalled>,
        pre_processor = services_pre_processor,
        post_processor = services_post_processor,
    )
)]
pub fn update(ctx: Arc<ObolContext>, a: u32) -> Result<u32, Infallible> {
    Ok(0)
}
