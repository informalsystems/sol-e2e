use super::erc20::Erc20;
use super::Scenario;
use crate::relayer::Relayer;
use crate::tests::network::EthereumConfig;
use alloy::consensus;
use alloy::primitives::{Address, FixedBytes};
use alloy::providers::Provider;
use alloy::providers::WalletProvider;
use alloy::transports::http::reqwest;
use alloy::{
    network::{Ethereum, EthereumWallet, NetworkWallet},
    primitives::U256,
    providers::ProviderBuilder,
};
use alloy_signer_local::{coins_bip39::English, MnemonicBuilder};
use alloy_sol_types::sol;
use anyhow::Context;
use futures::TryStreamExt;
use protos::union::ibc::lightclients::ethereum::v1::ClientState as ClientStateProto;
use protos::union::ibc::lightclients::ethereum::v1::ConsensusState as ConsensusStateProto;
use protos::union::ibc::lightclients::ethereum::v1::Header as HeaderProto;
use protos::union::ibc::lightclients::ethereum::v1::LightClientUpdate as LightClientUpdateProto;
use protos::union::ibc::lightclients::ethereum::v1::Misbehaviour as MisbehaviorProto;
use protos::union::ibc::lightclients::ethereum::v1::SyncCommittee as SyncCommitteeProto;
use testresult::TestResult;
use unionlabs::hash::H256;
use unionlabs::ibc::core::client::height::Height;
use unionlabs::ibc::lightclients::ethereum::account_proof::AccountProof;
use unionlabs::ibc::lightclients::ethereum::account_update::AccountUpdate;
use unionlabs::ibc::lightclients::ethereum::client_state::ClientState;
use unionlabs::ibc::lightclients::ethereum::consensus_state::ConsensusState;
use unionlabs::ibc::lightclients::ethereum::header::Header;
use unionlabs::ibc::lightclients::ethereum::light_client_update::LightClientUpdate;
use unionlabs::ibc::lightclients::ethereum::misbehaviour::Misbehaviour;
use unionlabs::ibc::lightclients::ethereum::trusted_sync_committee;
use unionlabs::{
    ethereum::config::Minimal,
    ibc::lightclients::ethereum::trusted_sync_committee::{
        ActiveSyncCommittee, TrustedSyncCommittee,
    },
};

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
        let EthereumConfig {
            el_socket,
            cl_socket,
            mnemonics,
            block_time,
        } = config;

        let cl_socket = cl_socket.context("no cl_socket")?;

        let beacon_client =
            beacon_api::client::BeaconApiClient::new(format!("http://{}", cl_socket)).await?;

        let spec = beacon_client.spec().await?.data;
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

        let ibc_handler_address = {
            let url = format!("http://{}", el_socket).to_string();

            let mnemonic = &mnemonics[0];

            let wallet = MnemonicBuilder::<English>::default()
                .phrase(mnemonic)
                .build()?;

            let ethereum_wallet = EthereumWallet::new(wallet);

            let provider = ProviderBuilder::new()
                .with_recommended_fillers()
                .wallet(ethereum_wallet)
                .on_builtin(&url)
                .await?;

            let name = "MyToken".to_string();
            let symbol = "MTK".to_string();
            let decimals = 18u8;
            let total_supply = U256::from(1_000_000);

            let contract = Erc20::deploy(
                &provider,
                name.clone(),
                symbol.clone(),
                decimals,
                total_supply,
            )
            .await?;

            tokio::time::sleep(core::time::Duration::from_secs(block_time)).await;

            contract.address().clone()
        };

        let relayer = Relayer {
            ibc_handler_address,
            cl_endpoint: cl_socket.clone(),
            el_endpoint: cl_socket.clone(),
        };

        tokio::time::sleep(core::time::Duration::from_secs(
            spec.seconds_per_slot * spec.period(),
        ))
        .await;

        let finalized_header = beacon_client.finality_update().await?.data.finalized_header;

        let (client_state, consensus_state, trusted_sync_committee) =
            relayer.initialize(finalized_header.beacon.slot - 1).await?;

        println!(
            "ClientState: {}",
            serde_json::to_string_pretty(&client_state)?
        );
        println!(
            "ConsensusState: {}",
            serde_json::to_string_pretty(&consensus_state)?
        );
        println!(
            "TrustedSyncCommittee: {}",
            serde_json::to_string_pretty(&trusted_sync_committee)?
        );

        tokio::time::sleep(core::time::Duration::from_secs(
            spec.seconds_per_slot * spec.period() * 5,
        ))
        .await;

        let (headers, trusted_sync_committee) = relayer.header(trusted_sync_committee).await?;

        println!("Headers: {}", serde_json::to_string_pretty(&headers)?);
        println!(
            "TrustedSyncCommittee: {}",
            serde_json::to_string_pretty(&trusted_sync_committee)?
        );

        Ok(())
    }
}
