pub mod types;

use self::types::*;
use reqwest::{IntoUrl, Response};
use serde::{de::DeserializeOwned, Serialize};
use std::convert::TryFrom;
use std::fmt;

pub use reqwest;
pub use reqwest::{StatusCode, Url};

#[derive(Debug)]
pub enum Error {
    Reqwest(reqwest::Error),
    ServerMessage(ErrorMessage),
    StatusCode(StatusCode),
}

impl Error {
    pub fn status(&self) -> Option<StatusCode> {
        match self {
            Error::Reqwest(error) => error.status(),
            Error::ServerMessage(msg) => StatusCode::try_from(msg.code).ok(),
            Error::StatusCode(status) => Some(*status),
        }
    }
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{:?}", self)
    }
}

#[derive(Clone)]
pub struct BeaconNodeClient {
    client: reqwest::Client,
    server: Url,
}

impl BeaconNodeClient {
    /// Returns `Err(())` if the URL is invalid.
    pub fn new(mut server: Url) -> Result<Self, ()> {
        server.path_segments_mut()?.push("eth").push("v1");

        Ok(Self {
            client: reqwest::Client::new(),
            server,
        })
    }

    /// Returns `Err(())` if the URL is invalid.
    pub fn from_components(mut server: Url, client: reqwest::Client) -> Result<Self, ()> {
        server.path_segments_mut()?.push("eth").push("v1");

        Ok(Self { client, server })
    }

    async fn get<T: DeserializeOwned, U: IntoUrl>(&self, url: U) -> Result<T, Error> {
        let response = self.client.get(url).send().await.map_err(Error::Reqwest)?;
        ok_or_error(response)
            .await?
            .json()
            .await
            .map_err(Error::Reqwest)
    }

    async fn get_opt<T: DeserializeOwned, U: IntoUrl>(&self, url: U) -> Result<Option<T>, Error> {
        let response = self.client.get(url).send().await.map_err(Error::Reqwest)?;
        match ok_or_error(response).await {
            Ok(resp) => resp.json().await.map(Option::Some).map_err(Error::Reqwest),
            Err(err) => {
                if err.status() == Some(StatusCode::NOT_FOUND) {
                    Ok(None)
                } else {
                    Err(err)
                }
            }
        }
    }

    async fn post<T: Serialize, U: IntoUrl>(&self, url: U, body: &T) -> Result<(), Error> {
        let response = self
            .client
            .post(url)
            .json(body)
            .send()
            .await
            .map_err(Error::Reqwest)?;
        ok_or_error(response).await?;
        Ok(())
    }

    /// `GET beacon/genesis`
    ///
    /// ## Errors
    ///
    /// May return a `404` if beacon chain genesis has not yet occurred.
    pub async fn get_beacon_genesis(&self) -> Result<GenericResponse<GenesisData>, Error> {
        let mut path = self.server.clone();

        path.path_segments_mut()
            .expect("path is base")
            .push("beacon")
            .push("genesis");

        self.get(path).await
    }

    /// `GET beacon/states/{state_id}/root`
    ///
    /// Returns `Ok(None)` on a 404 error.
    pub async fn get_beacon_states_root(
        &self,
        state_id: StateId,
    ) -> Result<Option<GenericResponse<RootData>>, Error> {
        let mut path = self.server.clone();

        path.path_segments_mut()
            .expect("path is base")
            .push("beacon")
            .push("states")
            .push(&state_id.to_string())
            .push("root");

        self.get_opt(path).await
    }

    /// `GET beacon/states/{state_id}/fork`
    ///
    /// Returns `Ok(None)` on a 404 error.
    pub async fn get_beacon_states_fork(
        &self,
        state_id: StateId,
    ) -> Result<Option<GenericResponse<Fork>>, Error> {
        let mut path = self.server.clone();

        path.path_segments_mut()
            .expect("path is base")
            .push("beacon")
            .push("states")
            .push(&state_id.to_string())
            .push("fork");

        self.get_opt(path).await
    }

    /// `GET beacon/states/{state_id}/finality_checkpoints`
    ///
    /// Returns `Ok(None)` on a 404 error.
    pub async fn get_beacon_states_finality_checkpoints(
        &self,
        state_id: StateId,
    ) -> Result<Option<GenericResponse<FinalityCheckpointsData>>, Error> {
        let mut path = self.server.clone();

        path.path_segments_mut()
            .expect("path is base")
            .push("beacon")
            .push("states")
            .push(&state_id.to_string())
            .push("finality_checkpoints");

        self.get_opt(path).await
    }

    /// `GET beacon/states/{state_id}/validators`
    ///
    /// Returns `Ok(None)` on a 404 error.
    pub async fn get_beacon_states_validators(
        &self,
        state_id: StateId,
    ) -> Result<Option<GenericResponse<Vec<ValidatorData>>>, Error> {
        let mut path = self.server.clone();

        path.path_segments_mut()
            .expect("path is base")
            .push("beacon")
            .push("states")
            .push(&state_id.to_string())
            .push("validators");

        self.get_opt(path).await
    }

    /// `GET beacon/states/{state_id}/committees?slot,index`
    ///
    /// Returns `Ok(None)` on a 404 error.
    pub async fn get_beacon_states_committees(
        &self,
        state_id: StateId,
        epoch: Epoch,
        slot: Option<Slot>,
        index: Option<u64>,
    ) -> Result<Option<GenericResponse<Vec<CommitteeData>>>, Error> {
        let mut path = self.server.clone();

        path.path_segments_mut()
            .expect("path is base")
            .push("beacon")
            .push("states")
            .push(&state_id.to_string())
            .push("committees")
            .push(&epoch.to_string());

        if let Some(slot) = slot {
            path.query_pairs_mut()
                .append_pair("slot", &slot.to_string());
        }

        if let Some(index) = index {
            path.query_pairs_mut()
                .append_pair("index", &index.to_string());
        }

        self.get_opt(path).await
    }

    /// `GET beacon/states/{state_id}/validators/{validator_id}`
    ///
    /// Returns `Ok(None)` on a 404 error.
    pub async fn get_beacon_states_validator_id(
        &self,
        state_id: StateId,
        validator_id: &ValidatorId,
    ) -> Result<Option<GenericResponse<ValidatorData>>, Error> {
        let mut path = self.server.clone();

        path.path_segments_mut()
            .expect("path is base")
            .push("beacon")
            .push("states")
            .push(&state_id.to_string())
            .push("validators")
            .push(&validator_id.to_string());

        self.get_opt(path).await
    }

    /// `GET beacon/headers?slot,parent_root`
    ///
    /// Returns `Ok(None)` on a 404 error.
    pub async fn get_beacon_headers(
        &self,
        slot: Option<Slot>,
        parent_root: Option<Hash256>,
    ) -> Result<Option<GenericResponse<Vec<BlockHeaderData>>>, Error> {
        let mut path = self.server.clone();

        path.path_segments_mut()
            .expect("path is base")
            .push("beacon")
            .push("headers");

        if let Some(slot) = slot {
            path.query_pairs_mut()
                .append_pair("slot", &slot.to_string());
        }

        if let Some(root) = parent_root {
            path.query_pairs_mut()
                .append_pair("parent_root", &format!("{:?}", root));
        }

        self.get_opt(path).await
    }

    /// `GET beacon/headers/{block_id}`
    ///
    /// Returns `Ok(None)` on a 404 error.
    pub async fn get_beacon_headers_block_id(
        &self,
        block_id: BlockId,
    ) -> Result<Option<GenericResponse<BlockHeaderData>>, Error> {
        let mut path = self.server.clone();

        path.path_segments_mut()
            .expect("path is base")
            .push("beacon")
            .push("headers")
            .push(&block_id.to_string());

        self.get_opt(path).await
    }

    /// `POST beacon/blocks`
    ///
    /// Returns `Ok(None)` on a 404 error.
    pub async fn post_beacon_blocks<T: EthSpec>(
        &self,
        block: &SignedBeaconBlock<T>,
    ) -> Result<(), Error> {
        let mut path = self.server.clone();

        path.path_segments_mut()
            .expect("path is base")
            .push("beacon")
            .push("blocks");

        self.post(path, block).await?;

        Ok(())
    }

    /// `GET beacon/blocks`
    ///
    /// Returns `Ok(None)` on a 404 error.
    pub async fn get_beacon_blocks<T: EthSpec>(
        &self,
        block_id: BlockId,
    ) -> Result<Option<GenericResponse<SignedBeaconBlock<T>>>, Error> {
        let mut path = self.server.clone();

        path.path_segments_mut()
            .expect("path is base")
            .push("beacon")
            .push("blocks")
            .push(&block_id.to_string());

        self.get_opt(path).await
    }

    /// `GET beacon/blocks/{block_id}/root`
    ///
    /// Returns `Ok(None)` on a 404 error.
    pub async fn get_beacon_blocks_root(
        &self,
        block_id: BlockId,
    ) -> Result<Option<GenericResponse<RootData>>, Error> {
        let mut path = self.server.clone();

        path.path_segments_mut()
            .expect("path is base")
            .push("beacon")
            .push("blocks")
            .push(&block_id.to_string())
            .push("root");

        self.get_opt(path).await
    }

    /// `GET beacon/blocks/{block_id}/attestations`
    ///
    /// Returns `Ok(None)` on a 404 error.
    pub async fn get_beacon_blocks_attestations<T: EthSpec>(
        &self,
        block_id: BlockId,
    ) -> Result<Option<GenericResponse<Vec<Attestation<T>>>>, Error> {
        let mut path = self.server.clone();

        path.path_segments_mut()
            .expect("path is base")
            .push("beacon")
            .push("blocks")
            .push(&block_id.to_string())
            .push("attestations");

        self.get_opt(path).await
    }

    /// `POST beacon/pool/attestations`
    pub async fn post_beacon_pool_attestations<T: EthSpec>(
        &self,
        attestation: &Attestation<T>,
    ) -> Result<(), Error> {
        let mut path = self.server.clone();

        path.path_segments_mut()
            .expect("path is base")
            .push("beacon")
            .push("pool")
            .push("attestations");

        self.post(path, attestation).await?;

        Ok(())
    }

    /// `GET beacon/pool/attestations`
    pub async fn get_beacon_pool_attestations<T: EthSpec>(
        &self,
    ) -> Result<GenericResponse<Vec<Attestation<T>>>, Error> {
        let mut path = self.server.clone();

        path.path_segments_mut()
            .expect("path is base")
            .push("beacon")
            .push("pool")
            .push("attestations");

        self.get(path).await
    }

    /// `POST beacon/pool/attester_slashings`
    pub async fn post_beacon_pool_attester_slashings<T: EthSpec>(
        &self,
        slashing: &AttesterSlashing<T>,
    ) -> Result<(), Error> {
        let mut path = self.server.clone();

        path.path_segments_mut()
            .expect("path is base")
            .push("beacon")
            .push("pool")
            .push("attester_slashings");

        self.post(path, slashing).await?;

        Ok(())
    }

    /// `GET beacon/pool/attester_slashings`
    pub async fn get_beacon_pool_attester_slashings<T: EthSpec>(
        &self,
    ) -> Result<GenericResponse<Vec<AttesterSlashing<T>>>, Error> {
        let mut path = self.server.clone();

        path.path_segments_mut()
            .expect("path is base")
            .push("beacon")
            .push("pool")
            .push("attester_slashings");

        self.get(path).await
    }

    /// `POST beacon/pool/proposer_slashings`
    pub async fn post_beacon_pool_proposer_slashings(
        &self,
        slashing: &ProposerSlashing,
    ) -> Result<(), Error> {
        let mut path = self.server.clone();

        path.path_segments_mut()
            .expect("path is base")
            .push("beacon")
            .push("pool")
            .push("proposer_slashings");

        self.post(path, slashing).await?;

        Ok(())
    }

    /// `GET beacon/pool/proposer_slashings`
    pub async fn get_beacon_pool_proposer_slashings(
        &self,
    ) -> Result<GenericResponse<Vec<ProposerSlashing>>, Error> {
        let mut path = self.server.clone();

        path.path_segments_mut()
            .expect("path is base")
            .push("beacon")
            .push("pool")
            .push("proposer_slashings");

        self.get(path).await
    }

    /// `POST beacon/pool/voluntary_exits`
    pub async fn post_beacon_pool_voluntary_exits(
        &self,
        exit: &SignedVoluntaryExit,
    ) -> Result<(), Error> {
        let mut path = self.server.clone();

        path.path_segments_mut()
            .expect("path is base")
            .push("beacon")
            .push("pool")
            .push("voluntary_exits");

        self.post(path, exit).await?;

        Ok(())
    }

    /// `GET beacon/pool/voluntary_exits`
    pub async fn get_beacon_pool_voluntary_exits(
        &self,
    ) -> Result<GenericResponse<Vec<SignedVoluntaryExit>>, Error> {
        let mut path = self.server.clone();

        path.path_segments_mut()
            .expect("path is base")
            .push("beacon")
            .push("pool")
            .push("voluntary_exits");

        self.get(path).await
    }

    /// `GET config/fork_schedule`
    pub async fn get_config_fork_schedule(&self) -> Result<GenericResponse<Vec<Fork>>, Error> {
        let mut path = self.server.clone();

        path.path_segments_mut()
            .expect("path is base")
            .push("config")
            .push("fork_schedule");

        self.get(path).await
    }

    /// `GET config/fork_schedule`
    pub async fn get_config_spec(&self) -> Result<GenericResponse<YamlConfig>, Error> {
        let mut path = self.server.clone();

        path.path_segments_mut()
            .expect("path is base")
            .push("config")
            .push("spec");

        self.get(path).await
    }

    /// `GET config/deposit_contract`
    pub async fn get_config_deposit_contract(
        &self,
    ) -> Result<GenericResponse<DepositContractData>, Error> {
        let mut path = self.server.clone();

        path.path_segments_mut()
            .expect("path is base")
            .push("config")
            .push("deposit_contract");

        self.get(path).await
    }

    /// `GET node/version`
    pub async fn get_node_version(&self) -> Result<GenericResponse<VersionData>, Error> {
        let mut path = self.server.clone();

        path.path_segments_mut()
            .expect("path is base")
            .push("node")
            .push("version");

        self.get(path).await
    }

    /// `GET node/syncing`
    pub async fn get_node_syncing(&self) -> Result<GenericResponse<SyncingData>, Error> {
        let mut path = self.server.clone();

        path.path_segments_mut()
            .expect("path is base")
            .push("node")
            .push("syncing");

        self.get(path).await
    }

    /// `GET debug/beacon/states/{state_id}`
    pub async fn get_debug_beacon_states<T: EthSpec>(
        &self,
        state_id: StateId,
    ) -> Result<Option<GenericResponse<BeaconState<T>>>, Error> {
        let mut path = self.server.clone();

        path.path_segments_mut()
            .expect("path is base")
            .push("debug")
            .push("beacon")
            .push("states")
            .push(&state_id.to_string());

        self.get_opt(path).await
    }

    /// `GET debug/beacon/heads`
    pub async fn get_debug_beacon_heads(
        &self,
    ) -> Result<GenericResponse<Vec<ChainHeadData>>, Error> {
        let mut path = self.server.clone();

        path.path_segments_mut()
            .expect("path is base")
            .push("debug")
            .push("beacon")
            .push("heads");

        self.get(path).await
    }

    /// `GET validator/duties/attester/{epoch}?index`
    ///
    /// ## Note
    ///
    /// The `index` query parameter accepts a list of validator indices.
    pub async fn get_validator_duties_attester(
        &self,
        epoch: Epoch,
        index: Option<&[u64]>,
    ) -> Result<GenericResponse<Vec<AttesterData>>, Error> {
        let mut path = self.server.clone();

        path.path_segments_mut()
            .expect("path is base")
            .push("validator")
            .push("duties")
            .push("attester")
            .push(&epoch.to_string());

        if let Some(index) = index {
            let string = index
                .iter()
                .map(|i| i.to_string())
                .collect::<Vec<_>>()
                .join(",");
            path.query_pairs_mut().append_pair("index", &string);
        }

        self.get(path).await
    }

    /// `GET validator/duties/proposer/{epoch}`
    pub async fn get_validator_duties_proposer(
        &self,
        epoch: Epoch,
    ) -> Result<GenericResponse<Vec<ProposerData>>, Error> {
        let mut path = self.server.clone();

        path.path_segments_mut()
            .expect("path is base")
            .push("validator")
            .push("duties")
            .push("proposer")
            .push(&epoch.to_string());

        self.get(path).await
    }

    /// `GET validator/duties/attester/{epoch}?index`
    ///
    /// ## Note
    ///
    /// The `index` query parameter accepts a list of validator indices.
    pub async fn get_validator_blocks<T: EthSpec>(
        &self,
        slot: Slot,
        randao_reveal: SignatureBytes,
        graffiti: Option<&Graffiti>,
    ) -> Result<GenericResponse<BeaconBlock<T>>, Error> {
        let mut path = self.server.clone();

        path.path_segments_mut()
            .expect("path is base")
            .push("validator")
            .push("blocks")
            .push(&slot.to_string());

        path.query_pairs_mut()
            .append_pair("randao_reveal", &randao_reveal.to_string());

        if let Some(graffiti) = graffiti {
            path.query_pairs_mut()
                .append_pair("graffiti", &graffiti.to_string());
        }

        self.get(path).await
    }

    /// `GET validator/attestation_data?slot,committee_index`
    pub async fn get_validator_attestation_data(
        &self,
        slot: Slot,
        committee_index: CommitteeIndex,
    ) -> Result<GenericResponse<AttestationData>, Error> {
        let mut path = self.server.clone();

        path.path_segments_mut()
            .expect("path is base")
            .push("validator")
            .push("attestation_data");

        path.query_pairs_mut()
            .append_pair("slot", &slot.to_string())
            .append_pair("committee_index", &committee_index.to_string());

        self.get(path).await
    }

    /// `GET validator/attestation_data?slot,committee_index`
    pub async fn get_validator_aggregate_attestation<T: EthSpec>(
        &self,
        slot: Slot,
        attestation_data_root: Hash256,
    ) -> Result<Option<GenericResponse<Attestation<T>>>, Error> {
        let mut path = self.server.clone();

        path.path_segments_mut()
            .expect("path is base")
            .push("validator")
            .push("aggregate_attestation");

        path.query_pairs_mut()
            .append_pair("slot", &slot.to_string())
            .append_pair(
                "attestation_data_root",
                &format!("{:?}", attestation_data_root),
            );

        self.get_opt(path).await
    }

    /// `POST validator/aggregate_and_proofs`
    pub async fn post_validator_aggregate_and_proof<T: EthSpec>(
        &self,
        aggregate: &SignedAggregateAndProof<T>,
    ) -> Result<(), Error> {
        let mut path = self.server.clone();

        path.path_segments_mut()
            .expect("path is base")
            .push("validator")
            .push("aggregate_and_proofs");

        self.post(path, aggregate).await?;

        Ok(())
    }

    /// `POST validator/beacon_committee_subscriptions`
    pub async fn post_validator_beacon_committee_subscriptions(
        &self,
        subscriptions: &[BeaconCommitteeSubscription],
    ) -> Result<(), Error> {
        let mut path = self.server.clone();

        path.path_segments_mut()
            .expect("path is base")
            .push("validator")
            .push("beacon_committee_subscriptions");

        self.post(path, &subscriptions).await?;

        Ok(())
    }
}

/// Returns `Ok(response)` if the response is a `200 OK` response. Otherwise, creates an
/// appropriate error message.
async fn ok_or_error(response: Response) -> Result<Response, Error> {
    let status = response.status();

    if status == StatusCode::OK {
        Ok(response)
    } else if let Ok(message) = response.json().await {
        Err(Error::ServerMessage(message))
    } else {
        Err(Error::StatusCode(status))
    }
}
