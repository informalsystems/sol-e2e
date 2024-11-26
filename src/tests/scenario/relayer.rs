use alloy::network::EthereumWallet;
use alloy::primitives::U256;
use alloy::providers::ProviderBuilder;
use alloy::transports::http::reqwest;
use alloy_signer_local::coins_bip39::English;
use alloy_signer_local::MnemonicBuilder;
use anyhow::Context;
use futures::TryStreamExt;
use testresult::TestResult;
use unionlabs::ethereum::config::Minimal;

use crate::relayer::Relayer;
use crate::tests::network::EthereumConfig;
use crate::tests::scenario::erc20::Erc20;
use crate::tests::scenario::Scenario;

pub struct RelayerMsg;

impl Scenario for RelayerMsg {
    async fn run(&self, config: EthereumConfig) -> TestResult {
        let EthereumConfig {
            el_socket,
            cl_socket,
            mnemonics,
            ..
        } = config;

        let cl_socket = cl_socket.context("no cl_socket")?;

        let beacon_client =
            beacon_api::client::BeaconApiClient::new(format!("http://{}", cl_socket)).await?;

        let spec = beacon_client.spec().await?.data;
        println!("{}", serde_json::to_string_pretty(&spec)?);

        let finalized_header = match beacon_client.finality_update().await {
            Ok(finality_update) => finality_update.data.finalized_header,
            Err(_) => {
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
                beacon_client.finality_update().await?.data.finalized_header
            }
        };

        {
            // current period should be at least 2

            let current_period = finalized_header.beacon.slot / spec.period();

            println!("current period: {}", current_period);

            if current_period < 2 {
                tokio::time::sleep(core::time::Duration::from_secs(
                    spec.seconds_per_slot * spec.period() * (2 - current_period),
                ))
                .await;
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

            *contract.address()
        };

        println!("IBC Handler: {}", ibc_handler_address);

        let relayer = Relayer::<Minimal> {
            ibc_handler_address,
            cl_socket,
            el_socket,
            _phantom: Default::default(),
        };

        println!(
            "building initialize state at slot {}",
            finalized_header.beacon.slot - 1
        );

        // initialize the relayer at a finalized header
        let (client_state, consensus_state, trusted_sync_committee) =
            relayer.initialize(finalized_header.beacon.slot).await?;

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
            spec.seconds_per_slot * spec.period() * 3,
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
