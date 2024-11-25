use anyhow::Context;
use futures::TryStreamExt;
use testresult::TestResult;

use crate::tests::network::EthereumConfig;
use crate::tests::scenario::Scenario;

pub struct BeaconEndpoint;

impl Scenario for BeaconEndpoint {
    async fn run(&self, config: EthereumConfig) -> TestResult {
        let EthereumConfig { cl_socket, .. } = config;

        let cl_socket = cl_socket.context("no cl_socket")?;

        let beacon_client =
            beacon_api::client::BeaconApiClient::new(format!("http://{}", cl_socket)).await?;

        let spec = beacon_client.spec().await?;
        println!("{}", serde_json::to_string_pretty(&spec)?);

        {
            let mut stream = reqwest::Client::new()
                .get(format!("http://{}/eth/v1/events", cl_socket))
                .query(&[("topics", "light_client_finality_update")])
                .send()
                .await?
                .bytes_stream();

            loop {
                if let Some(event) = stream.try_next().await? {
                    if event.starts_with(b"event: light_client_finality_update\n") {
                        break;
                    }
                }
            }
        }

        // let seconds_per_sync_committee_period = spec.data.seconds_per_slot
        //     * spec.data.slots_per_epoch
        //     * spec.data.epochs_per_sync_committee_period;

        // println!(
        //     "wait for sync committee period: {} seconds",
        //     seconds_per_sync_committee_period
        // );

        // tokio::time::sleep(tokio::time::Duration::from_secs(
        //     seconds_per_sync_committee_period,
        // ))
        // .await;

        let finality_update = beacon_client.finality_update().await?;
        println!("{}", serde_json::to_string_pretty(&finality_update)?);

        let finalized_slot = finality_update.data.finalized_header.beacon.slot;

        let finalized_header = beacon_client.header(finalized_slot.into()).await?;
        println!("{}", serde_json::to_string_pretty(&finalized_header)?);

        let finalized_root = finalized_header.data.root;

        let finalized_block = beacon_client.block(finalized_slot.into()).await?;
        println!("{}", serde_json::to_string_pretty(&finalized_block)?);

        let bootstrap = beacon_client.bootstrap(finalized_root).await?;
        println!("{}", serde_json::to_string_pretty(&bootstrap)?);

        let resp = beacon_client.genesis().await?;
        println!("{}", serde_json::to_string_pretty(&resp)?);

        let light_client_updates = beacon_client.light_client_updates(0, 1).await?;
        println!("{}", serde_json::to_string_pretty(&light_client_updates)?);

        assert_eq!(finality_update.data.finalized_header, bootstrap.data.header);

        Ok(())
    }
}
