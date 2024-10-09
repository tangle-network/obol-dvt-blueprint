use bollard::Docker;
use gadget_sdk as sdk;
use sdk::job;
use std::convert::Infallible;
use std::sync::Arc;

pub struct ObolContext {
    pub docker: Docker,
    pub container_id: String,
}

#[job(id = 1, params(a), result(_), verifier(evm = "ObolDvtBlueprint"))]
pub fn update(ctx: Arc<ObolContext>, a: u32) -> Result<u32, Infallible> {
    Ok(0)
}

#[job(id = 2, params(a), result(_), verifier(evm = "ObolDvtBlueprint"))]
pub fn activate(ctx: Arc<ObolContext>, a: u32) -> Result<u32, Infallible> {
    Ok(0)
}
