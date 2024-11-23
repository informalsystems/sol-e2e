use crate::tests::network::EthereumConfig;
use alloy::transports::http::reqwest;
use anyhow::Context;
use futures::TryStreamExt;
use protos::union::ibc::lightclients::ethereum::v1::Header as HeaderProto;
use protos::union::ibc::lightclients::ethereum::v1::LightClientUpdate as LightClientUpdateProto;
use protos::union::ibc::lightclients::ethereum::v1::SyncCommittee as SyncCommitteeProto;
use testresult::TestResult;
use unionlabs::hash::H256;
use unionlabs::ibc::core::client::height::Height as EthHeight;
use unionlabs::ibc::lightclients::ethereum::account_proof::AccountProof;
use unionlabs::ibc::lightclients::ethereum::account_update::AccountUpdate;
use unionlabs::ibc::lightclients::ethereum::header::Header as EthHeader;
use unionlabs::{
    ethereum::config::Minimal,
    ibc::lightclients::ethereum::trusted_sync_committee::{
        ActiveSyncCommittee, TrustedSyncCommittee,
    },
};

use super::Scenario;

pub struct FinalityEndpoint;

impl Scenario for FinalityEndpoint {
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

pub struct FinalityProtobuf;

impl Scenario for FinalityProtobuf {
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

        //
        // header
        //

        let sync_committee_proto = SyncCommitteeProto::from(bootstrap.data.current_sync_committee);

        let sync_committee =
            ActiveSyncCommittee::<Minimal>::Current(sync_committee_proto.try_into()?);

        // reusing the same finalized slot as dummy
        let trusted_sync_committee = TrustedSyncCommittee {
            trusted_height: EthHeight {
                revision_height: 0,
                revision_number: finalized_slot,
            },
            sync_committee,
        };

        let consensus_update_proto = LightClientUpdateProto {
            attested_header: Some(finality_update.data.attested_header.into()),
            // TODO: the light_client_update domain type ignores next_sync_committee
            next_sync_committee: None,
            next_sync_committee_branch: vec![],
            finalized_header: Some(finality_update.data.finalized_header.into()),
            finality_branch: finality_update
                .data
                .finality_branch
                .map(|bytes| bytes.into())
                .into(),
            sync_aggregate: Some(finality_update.data.sync_aggregate.into()),
            signature_slot: finality_update.data.signature_slot.into(),
        };

        // dummy account update
        let account_update = AccountUpdate {
            account_proof: AccountProof {
                storage_root: H256::default(),
                proof: vec![],
            },
        };

        let header = EthHeader {
            trusted_sync_committee,
            consensus_update: consensus_update_proto.try_into()?,
            account_update,
        };

        println!(
            "{}",
            serde_json::to_string_pretty(&HeaderProto::from(header))?
        );

        Ok(())
    }
}
