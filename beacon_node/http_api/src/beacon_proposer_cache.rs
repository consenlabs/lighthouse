use beacon_chain::{BeaconChain, BeaconChainError, BeaconChainTypes};
use eth2::types::ProposerData;
use fork_choice::ProtoBlock;
use state_processing::per_slot_processing;
use types::{Epoch, EthSpec, Hash256, PublicKeyBytes};

const EPOCHS_TO_SKIP: u64 = 2;

pub struct BeaconProposerCache {
    epoch: Epoch,
    epoch_boundary_root: Hash256,
    proposers: Vec<ProposerData>,
}

impl BeaconProposerCache {
    pub fn new<T: BeaconChainTypes>(chain: &BeaconChain<T>) -> Result<Self, BeaconChainError> {
        let (head_root, head_block) = Self::current_head_block(chain)?;

        let epoch = {
            let epoch_now = chain.epoch()?;
            let head_epoch = head_block.slot.epoch(T::EthSpec::slots_per_epoch());
            if epoch_now > head_epoch + EPOCHS_TO_SKIP {
                head_epoch
            } else {
                epoch_now
            }
        };

        Self::for_head_block(chain, epoch, head_root, head_block)
    }

    fn for_head_block<T: BeaconChainTypes>(
        chain: &BeaconChain<T>,
        current_epoch: Epoch,
        head_root: Hash256,
        head_block: ProtoBlock,
    ) -> Result<Self, BeaconChainError> {
        let mut head_state = chain
            .get_state(&head_block.state_root, Some(head_block.slot))?
            .ok_or_else(|| BeaconChainError::MissingBeaconState(head_block.state_root))?;

        while head_state.current_epoch() < current_epoch {
            // Skip slots until the current epoch, providing `Hash256::zero()` as the state root
            // since we don't require it to be valid to identify producers.
            per_slot_processing(&mut head_state, Some(Hash256::zero()), &chain.spec)?;
        }

        let proposers = current_epoch
            .slot_iter(T::EthSpec::slots_per_epoch())
            .map(|slot| {
                head_state
                    .get_beacon_proposer_index(slot, &chain.spec)
                    .map_err(BeaconChainError::from)
                    .and_then(|i| {
                        let pubkey = chain
                            .validator_pubkey(i)?
                            .ok_or_else(|| BeaconChainError::ValidatorPubkeyCacheIncomplete(i))?;

                        Ok(ProposerData {
                            pubkey: PublicKeyBytes::from(pubkey),
                            slot,
                        })
                    })
            })
            .collect::<Result<_, _>>()?;

        let epoch_boundary_slot = head_state
            .current_epoch()
            .start_slot(T::EthSpec::slots_per_epoch());
        let epoch_boundary_root = if head_state.slot == epoch_boundary_slot {
            head_root
        } else {
            *head_state.get_block_root(epoch_boundary_slot)?
        };

        Ok(Self {
            epoch: current_epoch,
            epoch_boundary_root,
            proposers,
        })
    }

    pub fn get_proposers<T: BeaconChainTypes>(
        &mut self,
        chain: &BeaconChain<T>,
        epoch: Epoch,
    ) -> Result<Vec<ProposerData>, warp::Rejection> {
        let current_epoch = chain.epoch().map_err(crate::reject::beacon_chain_error)?;
        if current_epoch != epoch {
            return Err(crate::reject::custom_bad_request(format!(
                "requested epoch is {} but only current epoch {} is allowed",
                epoch, current_epoch
            )));
        }

        let (head_root, head_block) =
            Self::current_head_block(chain).map_err(crate::reject::beacon_chain_error)?;
        let epoch_boundary_root = head_block.target_root;

        if self.epoch != current_epoch || self.epoch_boundary_root != epoch_boundary_root {
            *self = Self::for_head_block(chain, current_epoch, head_root, head_block)
                .map_err(crate::reject::beacon_chain_error)?;
        }

        Ok(self.proposers.clone())
    }

    fn current_head_block<T: BeaconChainTypes>(
        chain: &BeaconChain<T>,
    ) -> Result<(Hash256, ProtoBlock), BeaconChainError> {
        let head_root = chain.head_beacon_block_root()?;

        chain
            .fork_choice
            .read()
            .get_block(&head_root)
            .ok_or_else(|| BeaconChainError::MissingBeaconBlock(head_root))
            .map(|head_block| (head_root, head_block))
    }
}
