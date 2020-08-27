use crate::{Config, Error, SlasherDB, SlashingStatus};
use lmdb::{RwTransaction, Transaction};
use safe_arith::SafeArith;
use serde_derive::{Deserialize, Serialize};
use std::collections::{btree_map::Entry, BTreeMap};
use std::convert::TryFrom;
use std::sync::Arc;
use types::{AttesterSlashing, Epoch, EthSpec, IndexedAttestation};

pub const MAX_DISTANCE: u16 = u16::MAX;

/// Terminology:
///
/// Let
///     N = config.history_length
///     C = config.chunk_size
///     K = config.validator_chunk_size
///
/// Then
///
/// `chunk_index` in [0..N/C) is the column of a chunk in the 2D matrix
/// `validator_chunk_index` in [0..N/K) is the row of a chunk in the 2D matrix
/// `chunk_offset` in [0..C) is the horizontal (epoch) offset of a value within a 2D chunk
/// `validator_offset` in [0..K) is the vertical (validator) offset of a value within a 2D chunk
#[derive(Debug, Serialize, Deserialize)]
pub struct Chunk {
    data: Vec<u16>,
}

impl Chunk {
    // TODO: write tests for epochs greater than length
    pub fn get_target(
        &self,
        validator_index: u64,
        epoch: Epoch,
        config: &Config,
    ) -> Result<Epoch, Error> {
        assert_eq!(
            self.data.len(),
            config.chunk_size * config.validator_chunk_size
        );
        let validator_offset = config.validator_offset(validator_index);
        let chunk_offset = config.chunk_offset(epoch);
        let cell_index = config.cell_index(validator_offset, chunk_offset);
        self.data
            .get(cell_index)
            .map(|distance| epoch + u64::from(*distance))
            .ok_or_else(|| Error::ChunkIndexOutOfBounds(cell_index))
    }

    pub fn set_target(
        &mut self,
        validator_index: u64,
        epoch: Epoch,
        target_epoch: Epoch,
        config: &Config,
    ) -> Result<(), Error> {
        let validator_offset = config.validator_offset(validator_index);
        let chunk_offset = config.chunk_offset(epoch);
        let cell_index = config.cell_index(validator_offset, chunk_offset);

        let cell = self
            .data
            .get_mut(cell_index)
            .ok_or_else(|| Error::ChunkIndexOutOfBounds(cell_index))?;

        *cell = Self::epoch_distance(target_epoch, epoch)?;
        Ok(())
    }

    /// Compute the distance (difference) between two epochs.
    ///
    /// Error if the distance is greater than or equal to `MAX_DISTANCE`.
    pub fn epoch_distance(epoch: Epoch, base_epoch: Epoch) -> Result<u16, Error> {
        let distance_u64 = epoch
            .as_u64()
            .checked_sub(base_epoch.as_u64())
            .ok_or(Error::DistanceCalculationOverflow)?;

        let distance = u16::try_from(distance_u64).map_err(|_| Error::DistanceTooLarge)?;
        if distance < MAX_DISTANCE {
            Ok(distance)
        } else {
            Err(Error::DistanceTooLarge)
        }
    }
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(transparent)]
pub struct MinTargetChunk {
    chunk: Chunk,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(transparent)]
pub struct MaxTargetChunk {
    chunk: Chunk,
}

pub trait TargetArrayChunk: Sized + serde::Serialize + serde::de::DeserializeOwned {
    fn empty(config: &Config) -> Self;

    fn check_slashable<E: EthSpec>(
        &self,
        db: &SlasherDB<E>,
        txn: &mut RwTransaction<'_>,
        validator_index: u64,
        attestation: &IndexedAttestation<E>,
        config: &Config,
    ) -> Result<SlashingStatus<E>, Error>;

    fn update(
        &mut self,
        chunk_index: usize,
        validator_index: u64,
        start_epoch: Epoch,
        new_target_epoch: Epoch,
        current_epoch: Epoch,
        config: &Config,
    ) -> Result<bool, Error>;

    fn first_start_epoch(source_epoch: Epoch, current_epoch: Epoch) -> Option<Epoch>;

    fn next_chunk_index_and_start_epoch(
        chunk_index: usize,
        start_epoch: Epoch,
        config: &Config,
    ) -> Result<(usize, Epoch), Error>;

    fn select_db<E: EthSpec>(db: &SlasherDB<E>) -> lmdb::Database;

    fn load<E: EthSpec>(
        db: &SlasherDB<E>,
        txn: &mut RwTransaction<'_>,
        validator_chunk_index: usize,
        chunk_index: usize,
        config: &Config,
    ) -> Result<Option<Self>, Error> {
        let disk_key = config.disk_key(validator_chunk_index, chunk_index);
        match txn.get(Self::select_db(db), &disk_key.to_be_bytes()) {
            Ok(chunk_bytes) => Ok(Some(bincode::deserialize(chunk_bytes)?)),
            Err(lmdb::Error::NotFound) => Ok(None),
            Err(e) => Err(e.into()),
        }
    }

    fn store<E: EthSpec>(
        &self,
        db: &SlasherDB<E>,
        txn: &mut RwTransaction<'_>,
        validator_chunk_index: usize,
        chunk_index: usize,
        config: &Config,
    ) -> Result<(), Error> {
        let disk_key = config.disk_key(validator_chunk_index, chunk_index);
        let value = bincode::serialize(self)?;
        txn.put(
            Self::select_db(db),
            &disk_key.to_be_bytes(),
            &value,
            SlasherDB::<E>::write_flags(),
        )?;
        Ok(())
    }
}

impl TargetArrayChunk for MinTargetChunk {
    fn empty(config: &Config) -> Self {
        MinTargetChunk {
            chunk: Chunk {
                data: vec![MAX_DISTANCE; config.chunk_size * config.validator_chunk_size],
            },
        }
    }

    fn check_slashable<E: EthSpec>(
        &self,
        db: &SlasherDB<E>,
        txn: &mut RwTransaction<'_>,
        validator_index: u64,
        attestation: &IndexedAttestation<E>,
        config: &Config,
    ) -> Result<SlashingStatus<E>, Error> {
        let min_target =
            self.chunk
                .get_target(validator_index, attestation.data.source.epoch, config)?;
        if attestation.data.target.epoch > min_target {
            let attestation = db
                .get_attestation_for_validator(txn, validator_index, min_target)?
                .ok_or_else(|| Error::MissingAttesterRecord {
                    validator_index,
                    target_epoch: min_target,
                })?;
            Ok(SlashingStatus::SurroundsExisting(Box::new(attestation)))
        } else {
            Ok(SlashingStatus::NotSlashable)
        }
    }

    fn update(
        &mut self,
        chunk_index: usize,
        validator_index: u64,
        start_epoch: Epoch,
        new_target_epoch: Epoch,
        current_epoch: Epoch,
        config: &Config,
    ) -> Result<bool, Error> {
        let min_epoch = Epoch::from(
            current_epoch
                .as_usize()
                .saturating_sub(config.history_length - 1),
        );
        let mut epoch = start_epoch;
        while config.chunk_index(epoch) == chunk_index {
            if new_target_epoch < self.chunk.get_target(validator_index, epoch, config)? {
                self.chunk
                    .set_target(validator_index, epoch, new_target_epoch, config)?;
            } else {
                // We can stop.
                return Ok(false);
            }
            if epoch == min_epoch {
                return Ok(false);
            }
            epoch -= 1;
        }
        // Continue to the next chunk.
        assert_ne!(chunk_index, 0);
        Ok(true)
    }

    fn first_start_epoch(source_epoch: Epoch, _current_epoch: Epoch) -> Option<Epoch> {
        if source_epoch > 0 {
            Some(source_epoch - 1)
        } else {
            None
        }
    }

    fn next_chunk_index_and_start_epoch(
        chunk_index: usize,
        start_epoch: Epoch,
        config: &Config,
    ) -> Result<(usize, Epoch), Error> {
        let chunk_size = config.chunk_size as u64;
        Ok((
            chunk_index.safe_sub(1)?,
            start_epoch / chunk_size * chunk_size - 1,
        ))
    }

    fn select_db<E: EthSpec>(db: &SlasherDB<E>) -> lmdb::Database {
        db.min_targets_db
    }
}

impl TargetArrayChunk for MaxTargetChunk {
    fn empty(config: &Config) -> Self {
        MaxTargetChunk {
            chunk: Chunk {
                data: vec![0; config.chunk_size * config.validator_chunk_size],
            },
        }
    }

    fn check_slashable<E: EthSpec>(
        &self,
        db: &SlasherDB<E>,
        txn: &mut RwTransaction<'_>,
        validator_index: u64,
        attestation: &IndexedAttestation<E>,
        config: &Config,
    ) -> Result<SlashingStatus<E>, Error> {
        let max_target =
            self.chunk
                .get_target(validator_index, attestation.data.source.epoch, config)?;
        if attestation.data.target.epoch < max_target {
            let attestation = db
                .get_attestation_for_validator(txn, validator_index, max_target)?
                .ok_or_else(|| Error::MissingAttesterRecord {
                    validator_index,
                    target_epoch: max_target,
                })?;
            Ok(SlashingStatus::SurroundedByExisting(Box::new(attestation)))
        } else {
            Ok(SlashingStatus::NotSlashable)
        }
    }

    fn update(
        &mut self,
        chunk_index: usize,
        validator_index: u64,
        start_epoch: Epoch,
        new_target_epoch: Epoch,
        current_epoch: Epoch,
        config: &Config,
    ) -> Result<bool, Error> {
        let mut epoch = start_epoch;
        while config.chunk_index(epoch) == chunk_index {
            if new_target_epoch > self.chunk.get_target(validator_index, epoch, config)? {
                self.chunk
                    .set_target(validator_index, epoch, new_target_epoch, config)?;
            } else {
                // We can stop.
                return Ok(false);
            }
            if epoch == current_epoch {
                return Ok(false);
            }
            epoch += 1;
        }
        // Continue to the next chunk.
        Ok(true)
    }

    fn first_start_epoch(source_epoch: Epoch, current_epoch: Epoch) -> Option<Epoch> {
        if source_epoch < current_epoch {
            Some(source_epoch + 1)
        } else {
            None
        }
    }

    // Go to next chunk, and first epoch of that chunk
    fn next_chunk_index_and_start_epoch(
        chunk_index: usize,
        start_epoch: Epoch,
        config: &Config,
    ) -> Result<(usize, Epoch), Error> {
        let chunk_size = config.chunk_size as u64;
        Ok((
            chunk_index.safe_add(1)?,
            (start_epoch / chunk_size + 1) * chunk_size,
        ))
    }

    fn select_db<E: EthSpec>(db: &SlasherDB<E>) -> lmdb::Database {
        db.max_targets_db
    }
}

pub fn get_chunk_for_update<'a, E: EthSpec, T: TargetArrayChunk>(
    db: &SlasherDB<E>,
    txn: &mut RwTransaction<'_>,
    updated_chunks: &'a mut BTreeMap<usize, T>,
    validator_chunk_index: usize,
    chunk_index: usize,
    config: &Config,
) -> Result<&'a mut T, Error> {
    Ok(match updated_chunks.entry(chunk_index) {
        Entry::Occupied(occupied) => occupied.into_mut(),
        Entry::Vacant(vacant) => {
            let chunk = if let Some(disk_chunk) =
                T::load(db, txn, validator_chunk_index, chunk_index, config)?
            {
                disk_chunk
            } else {
                T::empty(config)
            };
            vacant.insert(chunk)
        }
    })
}

#[allow(clippy::too_many_arguments)]
pub fn apply_attestation_for_validator<E: EthSpec, T: TargetArrayChunk>(
    db: &SlasherDB<E>,
    txn: &mut RwTransaction<'_>,
    updated_chunks: &mut BTreeMap<usize, T>,
    validator_chunk_index: usize,
    validator_index: u64,
    attestation: &IndexedAttestation<E>,
    current_epoch: Epoch,
    config: &Config,
) -> Result<SlashingStatus<E>, Error> {
    let mut chunk_index = config.chunk_index(attestation.data.source.epoch);
    let mut current_chunk = get_chunk_for_update(
        db,
        txn,
        updated_chunks,
        validator_chunk_index,
        chunk_index,
        config,
    )?;

    let slashing_status =
        current_chunk.check_slashable(db, txn, validator_index, attestation, config)?;

    // TODO: consider removing this early return and updating the array
    if slashing_status != SlashingStatus::NotSlashable {
        return Ok(slashing_status);
    }

    let mut start_epoch = if let Some(start_epoch) =
        T::first_start_epoch(attestation.data.source.epoch, current_epoch)
    {
        start_epoch
    } else {
        return Ok(slashing_status);
    };
    chunk_index = config.chunk_index(start_epoch);

    loop {
        current_chunk = get_chunk_for_update(
            db,
            txn,
            updated_chunks,
            validator_chunk_index,
            chunk_index,
            config,
        )?;
        let keep_going = current_chunk.update(
            chunk_index,
            validator_index,
            start_epoch,
            attestation.data.target.epoch,
            current_epoch,
            config,
        )?;
        if !keep_going {
            break;
        }

        let (next_chunk_index, next_start_epoch) =
            T::next_chunk_index_and_start_epoch(chunk_index, start_epoch, config)?;
        chunk_index = next_chunk_index;
        start_epoch = next_start_epoch;
    }

    Ok(SlashingStatus::NotSlashable)
}

pub fn update<E: EthSpec>(
    db: &SlasherDB<E>,
    txn: &mut RwTransaction<'_>,
    validator_chunk_index: usize,
    batch: Vec<Arc<IndexedAttestation<E>>>,
    current_epoch: Epoch,
    config: &Config,
) -> Result<Vec<AttesterSlashing<E>>, Error> {
    // Split the batch up into horizontal segments.
    // Map chunk indexes in the range `0..self.config.chunk_size` to attestations
    // for those chunks.
    let mut chunk_attestations = BTreeMap::new();
    for attestation in batch {
        chunk_attestations
            .entry(config.chunk_index(attestation.data.source.epoch))
            .or_insert_with(Vec::new)
            .push(attestation);
    }

    let mut slashings = update_array::<_, MinTargetChunk>(
        db,
        txn,
        validator_chunk_index,
        &chunk_attestations,
        current_epoch,
        config,
    )?;
    slashings.extend(update_array::<_, MaxTargetChunk>(
        db,
        txn,
        validator_chunk_index,
        &chunk_attestations,
        current_epoch,
        config,
    )?);
    Ok(slashings)
}

pub fn update_array<E: EthSpec, T: TargetArrayChunk>(
    db: &SlasherDB<E>,
    txn: &mut RwTransaction<'_>,
    validator_chunk_index: usize,
    chunk_attestations: &BTreeMap<usize, Vec<Arc<IndexedAttestation<E>>>>,
    current_epoch: Epoch,
    config: &Config,
) -> Result<Vec<AttesterSlashing<E>>, Error> {
    let mut slashings = vec![];
    // Map from chunk index to updated chunk at that index.
    let mut updated_chunks = BTreeMap::new();

    for attestations in chunk_attestations.values() {
        for attestation in attestations {
            for validator_index in
                config.attesting_validators_for_chunk(attestation, validator_chunk_index)
            {
                let slashing_status = apply_attestation_for_validator::<E, T>(
                    db,
                    txn,
                    &mut updated_chunks,
                    validator_chunk_index,
                    validator_index,
                    attestation,
                    current_epoch,
                    config,
                )?;
                if let Some(slashing) = slashing_status.into_slashing(attestation) {
                    slashings.push(slashing);
                }
            }
        }
    }

    // Store chunks on disk.
    for (chunk_index, chunk) in updated_chunks {
        chunk.store(db, txn, validator_chunk_index, chunk_index, config)?;
    }

    Ok(slashings)
}
