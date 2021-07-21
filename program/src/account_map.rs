//! A type that stores a map (dictionary) from public key to some value `T`.

use borsh::{BorshDeserialize, BorshSchema, BorshSerialize};
use serde::Serialize;
use solana_program::pubkey::Pubkey;

use crate::error::LidoError;
use crate::util::serialize_b58;

/// An entry in `AccountMap`.
#[derive(
    Clone, Default, Debug, Eq, PartialEq, BorshSerialize, BorshDeserialize, BorshSchema, Serialize,
)]
pub struct PubkeyAndEntry<T> {
    #[serde(serialize_with = "serialize_b58")]
    pub pubkey: Pubkey,
    pub entry: T,
}

/// A map from public key to `T`, implemented as a vector of key-value pairs.
#[derive(
    Clone, Default, Debug, Eq, PartialEq, BorshSerialize, BorshDeserialize, BorshSchema, Serialize,
)]
pub struct AccountMap<T> {
    pub entries: Vec<PubkeyAndEntry<T>>,
    pub maximum_entries: u32,
}
pub trait EntryConstantSize {
    const SIZE: usize;
}

pub type AccountSet = AccountMap<()>;

impl<T: Default + EntryConstantSize> AccountMap<T> {
    /// Creates a new instance with the `maximum_entries` positions filled with the default value
    pub fn new_fill_default(maximum_entries: u32) -> Self {
        let mut v = Vec::with_capacity(maximum_entries as usize);
        for _ in 0..maximum_entries {
            v.push(PubkeyAndEntry {
                pubkey: Pubkey::default(),
                entry: T::default(),
            });
        }
        AccountMap {
            entries: v,
            maximum_entries,
        }
    }

    /// Creates a new empty instance
    pub fn new(maximum_entries: u32) -> Self {
        AccountMap {
            entries: Vec::new(),
            maximum_entries,
        }
    }

    pub fn len(&self) -> usize {
        self.entries.len()
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    pub fn add(&mut self, address: Pubkey, value: T) -> Result<(), LidoError> {
        if self.len() == self.maximum_entries as usize {
            return Err(LidoError::MaximumNumberOfAccountsExceeded);
        }
        if !self.entries.iter().any(|pe| pe.pubkey == address) {
            self.entries.push(PubkeyAndEntry {
                pubkey: address,
                entry: value,
            });
        } else {
            return Err(LidoError::DuplicatedEntry);
        }
        Ok(())
    }

    pub fn remove(&mut self, address: &Pubkey) -> Result<T, LidoError> {
        let idx = self
            .entries
            .iter()
            .position(|pe| &pe.pubkey == address)
            .ok_or(LidoError::InvalidAccountMember)?;
        Ok(self.entries.swap_remove(idx).entry)
    }

    pub fn get(&self, address: &Pubkey) -> Result<&PubkeyAndEntry<T>, LidoError> {
        self.entries
            .iter()
            .find(|pe| &pe.pubkey == address)
            .ok_or_else(|| LidoError::InvalidAccountMember)
    }

    pub fn get_mut(&mut self, address: &Pubkey) -> Result<&mut PubkeyAndEntry<T>, LidoError> {
        self.entries
            .iter_mut()
            .find(|pe| &pe.pubkey == address)
            .ok_or_else(|| LidoError::InvalidAccountMember)
    }

    /// Return how many bytes are needed to serialize an instance holding `max_entries`.
    pub fn required_bytes(max_entries: usize) -> usize {
        let key_size = std::mem::size_of::<Pubkey>();
        let value_size = T::SIZE;
        let entry_size = key_size + value_size;

        // 8 bytes for the length and u32 field, then the entries themselves.
        8 + entry_size * max_entries as usize
    }

    /// Return how many entries could fit in a buffer of the given size.
    pub fn maximum_entries(buffer_size: usize) -> usize {
        let key_size = std::mem::size_of::<Pubkey>();
        let value_size = T::SIZE;
        let entry_size = key_size + value_size;

        buffer_size.saturating_sub(8) / entry_size
    }

    /// Iterate just the values, not the keys.
    pub fn iter_entries(&self) -> IterEntries<T> {
        IterEntries {
            iter: self.entries.iter(),
        }
    }

    /// Iterate just the values mutably, not the keys.
    pub fn iter_entries_mut(&mut self) -> IterEntriesMut<T> {
        IterEntriesMut {
            iter: self.entries.iter_mut(),
        }
    }
}

pub struct IterEntries<'a, T: 'a> {
    iter: std::slice::Iter<'a, PubkeyAndEntry<T>>,
}

impl<'a, T: 'a> std::iter::Iterator for IterEntries<'a, T> {
    type Item = &'a T;

    fn next(&mut self) -> Option<&'a T> {
        self.iter.next().map(|pubkey_entry| &pubkey_entry.entry)
    }
}

pub struct IterEntriesMut<'a, T: 'a> {
    iter: std::slice::IterMut<'a, PubkeyAndEntry<T>>,
}

impl<'a, T: 'a> std::iter::Iterator for IterEntriesMut<'a, T> {
    type Item = &'a mut T;

    fn next(&mut self) -> Option<&'a mut T> {
        self.iter.next().map(|pubkey_entry| &mut pubkey_entry.entry)
    }
}

#[cfg(test)]
mod test {
    use super::*;

    impl EntryConstantSize for u32 {
        const SIZE: usize = 4;
    }

    #[test]
    fn test_account_map_limit() {
        let mut map = AccountMap::new(1);
        let result_0 = map.add(Pubkey::new_unique(), 0_u32);
        let result_1 = map.add(Pubkey::new_unique(), 1_u32);
        assert_eq!(result_0, Ok(()));
        assert_eq!(result_1, Err(LidoError::MaximumNumberOfAccountsExceeded));
    }

    #[test]
    fn test_account_map_duplicate() {
        let mut map = AccountMap::new(2);
        let key = Pubkey::new_unique();
        let result_0 = map.add(key, 0_u32);
        let result_1 = map.add(key, 1_u32);
        assert_eq!(result_0, Ok(()));
        assert_eq!(result_1, Err(LidoError::DuplicatedEntry));
    }

    #[test]
    fn test_account_map_add_remove() {
        let mut map = AccountMap::new(1);
        let key = Pubkey::new_unique();
        map.add(key, 0_u32).unwrap();

        assert_eq!(map.get(&key).map(|pe| pe.entry), Ok(0));
        assert_eq!(map.get_mut(&key).map(|pe| pe.entry), Ok(0));
        assert_eq!(map.remove(&key), Ok(0));

        assert_eq!(map.get(&key), Err(LidoError::InvalidAccountMember));
        assert_eq!(map.get_mut(&key), Err(LidoError::InvalidAccountMember));
        assert_eq!(map.remove(&key), Err(LidoError::InvalidAccountMember));
    }
}
