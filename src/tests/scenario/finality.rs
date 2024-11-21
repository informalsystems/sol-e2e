use testresult::TestResult;

use crate::tests::network::EthereumConfig;
use helios_consensus_core::consensus_spec::MinimalConsensusSpec;
use helios_ethereum::rpc::http_rpc::HttpRpc;
use helios_ethereum::rpc::ConsensusRpc;

use super::Scenario;

pub struct Finality;

impl Scenario for Finality {
    async fn run(&self, config: EthereumConfig) -> TestResult {
        let EthereumConfig { cl_socket, .. } = config;

        let helios_client = <HttpRpc as ConsensusRpc<MinimalConsensusSpec>>::new(&format!(
            "http://{}",
            cl_socket.unwrap()
        ));

        while <HttpRpc as ConsensusRpc<MinimalConsensusSpec>>::get_finality_update(&helios_client)
            .await
            .is_err()
        {
            tokio::time::sleep(core::time::Duration::from_secs(1)).await;
        }

        let finality_update =
            <HttpRpc as ConsensusRpc<MinimalConsensusSpec>>::get_finality_update(&helios_client)
                .await?;

        println!("{:#?}", finality_update);

        let optimistic_update =
            <HttpRpc as ConsensusRpc<MinimalConsensusSpec>>::get_optimistic_update(&helios_client)
                .await?;

        println!("{:#?}", optimistic_update);

        let block =
            <HttpRpc as ConsensusRpc<MinimalConsensusSpec>>::get_block(&helios_client, 1).await?;

        println!("{:#?}", block);

        Ok(())
    }
}
