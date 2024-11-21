use testresult::TestResult;

use crate::tests::network::EthereumConfig;

use super::Scenario;
use alloy_rpc_types::beacon::header::HeadersResponse;
use alloy_transport_http::Client;

pub struct Finality;

impl Scenario for Finality {
    async fn run(&self, config: EthereumConfig) -> TestResult {
        let EthereumConfig { cl_socket, .. } = config;

        let client = Client::new();

        let _ = client
            .get(format!(
                "http://{}/eth/v1/beacon/headers",
                cl_socket.unwrap()
            ))
            .send()
            .await?
            .json::<HeadersResponse>()
            .await?;

        Ok(())
    }
}
