use crate::tests::network::EthereumConfig;
use anyhow::Context;
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
        self, ActiveSyncCommittee, TrustedSyncCommittee,
    },
};

use super::Scenario;

pub struct FinalityEndpoint;

impl Scenario for FinalityEndpoint {
    async fn run(&self, config: EthereumConfig) -> TestResult {
        let EthereumConfig { cl_socket, .. } = config;

        let beacon_client = beacon_api::client::BeaconApiClient::new(format!(
            "http://{}",
            cl_socket.context("no cl_socket")?
        ))
        .await?;

        let spec = beacon_client.spec().await?;
        println!("{}", serde_json::to_string_pretty(&spec)?);

        let seconds_per_sync_committee_period = spec.data.seconds_per_slot
            * spec.data.slots_per_epoch
            * spec.data.epochs_per_sync_committee_period;

        println!(
            "wait for sync committee period: {} seconds",
            seconds_per_sync_committee_period
        );

        tokio::time::sleep(tokio::time::Duration::from_secs(
            seconds_per_sync_committee_period,
        ))
        .await;

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

        Ok(())
    }
}

pub struct FinalityProtobuf;

impl Scenario for FinalityProtobuf {
    async fn run(&self, config: EthereumConfig) -> TestResult {
        let EthereumConfig { cl_socket, .. } = config;

        let beacon_client = beacon_api::client::BeaconApiClient::new(format!(
            "http://{}",
            cl_socket.context("no cl_socket")?
        ))
        .await?;

        let spec = beacon_client.spec().await?;
        println!("{}", serde_json::to_string_pretty(&spec)?);

        let seconds_per_sync_committee_period = spec.data.seconds_per_slot
            * spec.data.slots_per_epoch
            * spec.data.epochs_per_sync_committee_period;

        println!(
            "wait for sync committee period: {} seconds",
            seconds_per_sync_committee_period
        );

        tokio::time::sleep(tokio::time::Duration::from_secs(
            seconds_per_sync_committee_period,
        ))
        .await;

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

        let sync_committee_proto: SyncCommitteeProto =
            serde_json::from_slice(&serde_json::to_vec(&bootstrap.data.current_sync_committee)?)?;

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

        let consensus_update_proto: LightClientUpdateProto =
            serde_json::from_slice(&serde_json::to_vec(&finality_update.data)?)?;

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
