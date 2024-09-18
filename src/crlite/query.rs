/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

use crate::{AsQuery, Clubcard, Equation, Queryable};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::cmp::max;
use std::collections::HashMap;

pub type CRLiteClubcard = Clubcard<4, CRLiteCoverage, ()>;

type LogId = [u8; 32];
type TimestampInterval = (u64, u64);

#[derive(Serialize, Deserialize)]
pub struct CRLiteCoverage(pub(crate) HashMap<LogId, TimestampInterval>);

#[derive(Clone, Debug)]
pub struct CRLiteQuery<'a> {
    pub(crate) issuer: &'a [u8; 32],
    pub(crate) serial: &'a [u8],
    pub(crate) log_timestamps: Option<&'a [([u8; 32], u64)]>,
}

impl<'a> CRLiteQuery<'a> {
    pub fn new(
        issuer: &'a [u8; 32],
        serial: &'a [u8],
        log_timestamps: Option<&'a [([u8; 32], u64)]>,
    ) -> CRLiteQuery<'a> {
        CRLiteQuery {
            issuer,
            serial,
            log_timestamps,
        }
    }
}

impl<'a> AsQuery<4> for CRLiteQuery<'a> {
    fn block(&self) -> &[u8] {
        self.issuer.as_ref()
    }

    fn as_query(&self, m: usize) -> Equation<4> {
        let mut digest = [0u8; 32];
        let mut hasher = Sha256::new();
        hasher.update(self.issuer);
        hasher.update(self.serial);
        hasher.finalize_into((&mut digest).into());

        let mut a = [0u64; 4];
        for (i, x) in digest
            .chunks_exact(8) // TODO: use array_chunks::<8>() when stable
            .map(|x| TryInto::<[u8; 8]>::try_into(x).unwrap())
            .map(u64::from_le_bytes)
            .enumerate()
        {
            a[i] = x;
        }
        a[0] |= 1;
        let s = (a[3] as usize) % max(1, m);
        Equation::homogeneous(s, a)
    }

    fn discriminant(&self) -> &[u8] {
        self.serial
    }
}

impl<'a> Queryable<4> for CRLiteQuery<'a> {
    type UniverseMetadata = CRLiteCoverage;

    // The set of CRLiteKeys is partitioned by issuer, and each
    // CRLiteKey knows its issuer. So there's no need for additional
    // partition metadata.
    type PartitionMetadata = ();

    fn in_universe(&self, universe: &Self::UniverseMetadata) -> bool {
        let Some(log_timestamps) = self.log_timestamps else {
            return false;
        };
        for (log_id, timestamp) in log_timestamps {
            if let Some((low, high)) = universe.0.get(log_id) {
                if low <= timestamp && timestamp <= high {
                    return true;
                }
            }
        }
        false
    }
}

#[derive(Debug)]
pub enum ClubcardError {
    Serialize,
    Deserialize,
    UnsupportedVersion,
}

impl Clubcard<4, CRLiteCoverage, ()> {
    const SERIALIZATION_VERSION: u16 = 0xffff;

    /// Serialize this clubcard.
    pub fn to_bytes(&self) -> Result<Vec<u8>, ClubcardError> {
        let mut out = u16::to_le_bytes(Self::SERIALIZATION_VERSION).to_vec();
        bincode::serialize_into(&mut out, self).map_err(|_| ClubcardError::Serialize)?;
        Ok(out)
    }

    /// Deserialize a clubcard.
    pub fn from_bytes(bytes: &[u8]) -> Result<Self, ClubcardError> {
        let (version_bytes, rest) = bytes.split_at(std::mem::size_of::<u16>());
        let Ok(version_bytes) = version_bytes.try_into() else {
            return Err(ClubcardError::Deserialize);
        };
        let version = u16::from_le_bytes(version_bytes);
        if version != Self::SERIALIZATION_VERSION {
            return Err(ClubcardError::UnsupportedVersion);
        }
        bincode::deserialize(rest).map_err(|_| ClubcardError::Deserialize)
    }
}
