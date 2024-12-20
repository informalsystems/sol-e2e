use core::net::SocketAddr;

use alloy::primitives::Address;
use alloy::providers::{Provider, ProviderBuilder};
use anyhow::Context;
use beacon_api::client::{BeaconApiClient, BlockId};
use ics008_wasm_client::MerklePath;
use protos::union::ibc::lightclients::ethereum::v1::{
    LightClientUpdate as LightClientUpdateProto, SyncCommittee as SyncCommitteeProto,
};
use unionlabs::ethereum::config::{
    BYTES_PER_LOGS_BLOOM, MAX_EXTRA_DATA_BYTES, SYNC_COMMITTEE_SIZE,
};
use unionlabs::ethereum::{ibc_commitment_key_v2, IBC_HANDLER_COMMITMENTS_SLOT};
use unionlabs::ibc::core::client::height::Height;
use unionlabs::ibc::lightclients::ethereum::account_proof::AccountProof;
use unionlabs::ibc::lightclients::ethereum::account_update::AccountUpdate;
use unionlabs::ibc::lightclients::ethereum::client_state::ClientState;
use unionlabs::ibc::lightclients::ethereum::consensus_state::ConsensusState;
use unionlabs::ibc::lightclients::ethereum::header::Header;
use unionlabs::ibc::lightclients::ethereum::light_client_update::UnboundedLightClientUpdate;
use unionlabs::ibc::lightclients::ethereum::misbehaviour::Misbehaviour;
use unionlabs::ibc::lightclients::ethereum::storage_proof::StorageProof;
use unionlabs::ibc::lightclients::ethereum::trusted_sync_committee::{
    ActiveSyncCommittee, TrustedSyncCommittee,
};
use unionlabs::uint::U256;

pub struct Relayer<C: Clone + SYNC_COMMITTEE_SIZE + BYTES_PER_LOGS_BLOOM + MAX_EXTRA_DATA_BYTES> {
    pub ibc_handler_address: Address,
    pub cl_socket: SocketAddr,
    pub el_socket: SocketAddr,
    pub _phantom: core::marker::PhantomData<C>,
}

impl<C: Clone + SYNC_COMMITTEE_SIZE + BYTES_PER_LOGS_BLOOM + MAX_EXTRA_DATA_BYTES> Relayer<C> {
    pub async fn beacon_client(&self) -> anyhow::Result<BeaconApiClient> {
        Ok(BeaconApiClient::new(format!("http://{}", self.cl_socket)).await?)
    }

    pub async fn provider(&self) -> anyhow::Result<impl Provider> {
        Ok(ProviderBuilder::new()
            .with_recommended_fillers()
            .on_builtin(&format!("http://{}", self.el_socket))
            .await?)
    }

    pub async fn account_proof<const N: usize>(
        &self,
        slot: u64,
        merkle_paths: [MerklePath; N],
    ) -> anyhow::Result<(AccountProof, [StorageProof; N])> {
        let beacon = self.beacon_client().await?;
        let provider = self.provider().await?;

        let execution_height = beacon.execution_height(BlockId::Slot(slot)).await?;

        let response = provider
            .get_proof(
                self.ibc_handler_address,
                merkle_paths
                    .into_iter()
                    .map(|merkle_path| {
                        merkle_path
                            .key_path
                            .into_iter()
                            // take only the first key_path from a merkle path
                            // https://github.com/gjermundgaraba/union/blob/10355e6/light-clients/ethereum-light-client/src/client.rs#L87
                            .next()
                            .context("key_path is empty")
                            .map(|key| {
                                ibc_commitment_key_v2(key.into(), IBC_HANDLER_COMMITMENTS_SLOT)
                                    .to_be_bytes()
                                    .into()
                            })
                    })
                    .collect::<Result<_, _>>()?,
            )
            .block_id(execution_height.into())
            .await
            .unwrap();

        let account_proof = AccountProof {
            storage_root: response.storage_hash.into(),
            proof: response
                .account_proof
                .into_iter()
                .map(|x| x.to_vec())
                .collect(),
        };

        let storage_proofs = <[_; N]>::try_from(response.storage_proof)
            .map_err(|x| anyhow::anyhow!("length should be {} but got {}", N, x.len()))?
            .map(|proof| StorageProof {
                key: U256::from_be_bytes(proof.key.as_b256().0),
                value: U256::from_limbs(proof.value.into_limbs()),
                proof: proof
                    .proof
                    .into_iter()
                    .map(|bytes| bytes.to_vec())
                    .collect(),
            });

        Ok((account_proof, storage_proofs))
    }

    pub async fn initialize(
        &self,
        slot: u64,
    ) -> anyhow::Result<(ClientState, ConsensusState, TrustedSyncCommittee<C>)> {
        let beacon = self.beacon_client().await?;
        let provider = self.provider().await?;

        let chain_id = provider.get_chain_id().await?;

        let genesis = beacon.genesis().await?.data;

        let trusted_header = beacon.header(BlockId::Slot(slot)).await?.data;
        let bootstrap = beacon.bootstrap(trusted_header.root).await?.data;

        let spec = beacon.spec().await?.data;

        anyhow::ensure!(bootstrap.header.beacon.slot == slot);

        let light_client_update = {
            let current_period = slot / spec.period();

            let light_client_updates = beacon.light_client_updates(current_period, 1).await?;

            let [update] = <[_; 1]>::try_from(light_client_updates.0)
                .map_err(|x| anyhow::anyhow!("length should be 1 but got {}", x.len()))?;

            anyhow::ensure!(update.data.finalized_header.beacon.slot <= slot);
            anyhow::ensure!(slot - update.data.finalized_header.beacon.slot < spec.period());

            update.data.clone()
        };

        let client_state = ClientState {
            chain_id: chain_id.to_string().parse()?,
            genesis_validators_root: genesis.genesis_validators_root,
            genesis_time: genesis.genesis_time,
            fork_parameters: spec.to_fork_parameters(),
            seconds_per_slot: spec.seconds_per_slot,
            slots_per_epoch: spec.slots_per_epoch,
            epochs_per_sync_committee_period: spec.epochs_per_sync_committee_period,
            latest_slot: slot,
            min_sync_committee_participants: 0,
            frozen_height: Height {
                revision_number: 0,
                revision_height: 0,
            },
            ibc_commitment_slot: IBC_HANDLER_COMMITMENTS_SLOT,
            ibc_contract_address: self.ibc_handler_address.0 .0.into(),
        };

        let account_update = self
            .account_proof(bootstrap.header.beacon.slot, [])
            .await?
            .0;

        let consensus_state = ConsensusState {
            slot: bootstrap.header.beacon.slot,
            state_root: bootstrap.header.execution.state_root,
            storage_root: account_update.storage_root,
            // Normalize to nanos in order to be compliant with cosmos
            timestamp: bootstrap.header.execution.timestamp * 1_000_000_000,
            current_sync_committee: bootstrap.current_sync_committee.aggregate_pubkey,
            next_sync_committee: light_client_update
                .next_sync_committee
                .clone()
                .map(|nsc| nsc.aggregate_pubkey),
        };

        let trusted_sync_committee = TrustedSyncCommittee {
            trusted_height: Height {
                revision_number: 0,
                revision_height: slot,
            },
            sync_committee: if let Some(sync_committee) = light_client_update.next_sync_committee {
                ActiveSyncCommittee::Next(SyncCommitteeProto::from(sync_committee).try_into()?)
            } else {
                ActiveSyncCommittee::Current(
                    SyncCommitteeProto::from(bootstrap.current_sync_committee).try_into()?,
                )
            },
        };

        Ok((client_state, consensus_state, trusted_sync_committee))
    }

    pub async fn header(
        &self,
        mut trusted_sync_committee: TrustedSyncCommittee<C>,
    ) -> anyhow::Result<(Vec<Header<C>>, TrustedSyncCommittee<C>)> {
        let beacon = self.beacon_client().await?;

        let spec = beacon.spec().await?.data;

        let trusted_slot = trusted_sync_committee.trusted_height.revision_height;

        let latest_finalized_update = beacon.finality_update().await?.data;

        let target_slot = latest_finalized_update.finalized_header.beacon.slot;

        anyhow::ensure!(
            trusted_slot < target_slot,
            "trusted slot must be less than target slot",
        );

        let trusted_period = trusted_slot / spec.period();

        let target_period = target_slot / spec.period();

        let light_client_updates = beacon
            .light_client_updates(trusted_period, target_period - trusted_period + 1)
            .await?
            .0
            .into_iter()
            .map(|x| x.data)
            .filter(|x| {
                trusted_slot < x.finalized_header.beacon.slot
                    && x.finalized_header.beacon.slot <= target_slot
            })
            .collect::<Vec<_>>();

        let mut headers = if light_client_updates.is_empty() {
            vec![]
        } else {
            anyhow::ensure!(
                light_client_updates
                    .first()
                    .context("first light client update")?
                    .finalized_header
                    .beacon
                    .slot
                    - trusted_slot
                    <= spec.period()
            );

            anyhow::ensure!(
                target_slot
                    - light_client_updates
                        .last()
                        .context("last light client update")?
                        .finalized_header
                        .beacon
                        .slot
                    < spec.period()
            );

            let mut headers = Vec::with_capacity(light_client_updates.len());

            for update in light_client_updates {
                let new_trusted_sync_committee = TrustedSyncCommittee {
                    trusted_height: Height {
                        revision_number: 0,
                        revision_height: update.finalized_header.beacon.slot,
                    },
                    sync_committee: if let Some(sync_committee) =
                        update.next_sync_committee.as_ref()
                    {
                        ActiveSyncCommittee::Next(
                            SyncCommitteeProto::from(sync_committee.clone()).try_into()?,
                        )
                    } else {
                        ActiveSyncCommittee::Current(
                            trusted_sync_committee.sync_committee.get().clone(),
                        )
                    },
                };

                let account_update = AccountUpdate {
                    account_proof: self
                        .account_proof(update.finalized_header.beacon.slot, [])
                        .await?
                        .0,
                };

                let consensus_update = LightClientUpdateProto::from(update).try_into()?;

                headers.push(Header {
                    trusted_sync_committee,
                    consensus_update,
                    account_update,
                });

                trusted_sync_committee = new_trusted_sync_committee;
            }

            headers
        };

        if !headers
            .last()
            .map(|x| x.consensus_update.finalized_header.beacon.slot == target_slot)
            .unwrap_or_default()
        {
            let new_trusted_sync_committee = TrustedSyncCommittee {
                trusted_height: Height {
                    revision_number: 0,
                    revision_height: target_slot,
                },
                sync_committee: ActiveSyncCommittee::Current(
                    trusted_sync_committee.sync_committee.get().clone(),
                ),
            };

            let update = UnboundedLightClientUpdate {
                attested_header: latest_finalized_update.attested_header,
                next_sync_committee: None,
                next_sync_committee_branch: None,
                finalized_header: latest_finalized_update.finalized_header,
                finality_branch: latest_finalized_update.finality_branch,
                sync_aggregate: latest_finalized_update.sync_aggregate,
                signature_slot: latest_finalized_update.signature_slot,
            };

            let consensus_update = LightClientUpdateProto::from(update).try_into()?;

            let account_update = AccountUpdate {
                account_proof: self.account_proof(target_slot, []).await?.0,
            };

            headers.push(Header {
                trusted_sync_committee,
                consensus_update,
                account_update,
            });

            trusted_sync_committee = new_trusted_sync_committee;
        }

        Ok((headers, trusted_sync_committee))
    }

    pub async fn misbehaviour(&self) -> anyhow::Result<Misbehaviour<C>> {
        let _beacon = self.beacon_client().await?;
        let _provider = self.provider().await?;

        // /eth/v1/beacon/pool/attester_slashings

        unimplemented!()
    }
}
