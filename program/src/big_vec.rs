// Copied from SPL stake-pool library at 1a0155e34bf96489db2cd498be79ca417c87c09f and modified

//! Big vector type, used with vectors that can't be serde'd

use {
    crate::error::LidoError,
    arrayref::array_ref,
    borsh::{BorshDeserialize, BorshSerialize},
    solana_program::{program_error::ProgramError, program_pack::Pack},
    std::marker::PhantomData,
};

/// Contains easy to use utilities for a big vector of Borsh-compatible types,
/// to avoid managing the entire struct on-chain and blow through stack limits.
#[derive(Debug)]
pub struct BigVec<'data, T> {
    /// Underlying data buffer, pieces of which are serialized
    pub data: &'data mut [u8],
    phantom: PhantomData<T>,
}

const VEC_SIZE_BYTES: usize = std::mem::size_of::<u32>();

impl<'data, T: Pack + Clone> BigVec<'data, T> {
    /// Get the length of the vector
    pub fn len(&self) -> u32 {
        let vec_len = array_ref![self.data, 0, VEC_SIZE_BYTES];
        u32::from_le_bytes(*vec_len)
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    pub fn new(data: &'data mut [u8]) -> Self {
        Self {
            data,
            phantom: PhantomData,
        }
    }

    // Get start and end positions of slice at index
    fn get_slice_bounds(&mut self, index: u32) -> Result<(usize, usize), ProgramError> {
        if index >= self.len() {
            return Err(LidoError::AccountListIndexOutOfBounds.into());
        }
        let index = index as usize;
        let start_index = VEC_SIZE_BYTES.saturating_add(index.saturating_mul(T::LEN));
        let end_index = start_index.saturating_add(T::LEN);

        if self.data.len() < end_index {
            return Err(ProgramError::AccountDataTooSmall);
        }

        if end_index - start_index != T::LEN {
            // This only happends if start_index is very close to usize::MAX,
            // which means that T::LEN should be huge. Solana does not allow such values on-chain
            return Err(LidoError::AccountListIndexOutOfBounds.into());
        }

        Ok((start_index, end_index))
    }

    /// Get element at position
    pub fn get_mut(&mut self, index: u32) -> Result<&mut T, ProgramError> {
        let (start_index, end_index) = self.get_slice_bounds(index)?;
        let ptr = self.data[start_index..end_index].as_ptr();
        Ok(unsafe { &mut *(ptr as *mut T) })
    }

    /// Removes an element from the vector and returns it.
    /// The removed element is replaced by the last element of the vector.
    /// This does not preserve ordering, but is O(1). If you need to preserve the element order, use remove instead
    pub fn swap_remove(&mut self, index: u32) -> Result<T, ProgramError> {
        let (start_index, end_index) = self.get_slice_bounds(index)?;
        let ptr = self.data[start_index..end_index].as_ptr();
        let value = unsafe { (*(ptr as *const T)).clone() };

        let data_start_index = VEC_SIZE_BYTES;
        let data_end_index =
            data_start_index.saturating_add((self.len() as usize).saturating_mul(T::LEN));

        // if not last element replace it with last
        if index != self.len() - 1 {
            self.data
                .copy_within(data_end_index - T::LEN..data_end_index, start_index);
            // If ever performance will be an issue try this code:
            // unsafe {
            //     sol_memmove(
            //         self.data[start_index..end_index].as_mut_ptr(),
            //         self.data[data_end_index - T::LEN..data_end_index].as_mut_ptr(),
            //         T::LEN,
            //     );
            // }
        }

        let new_len = self.len() - 1;
        let mut vec_len_ref = &mut self.data[0..VEC_SIZE_BYTES];
        new_len.serialize(&mut vec_len_ref)?;

        Ok(value)
    }

    /// Add new element to the end
    pub fn push(&mut self, element: T) -> Result<(), ProgramError> {
        let data_len = self.data.len();
        let mut vec_len_ref = &mut self.data[0..VEC_SIZE_BYTES];
        let mut vec_len = u32::try_from_slice(vec_len_ref)?;

        let start_index = VEC_SIZE_BYTES + vec_len as usize * T::LEN;
        let end_index = start_index + T::LEN;

        if data_len < end_index {
            return Err(ProgramError::AccountDataTooSmall);
        }

        vec_len += 1;
        vec_len.serialize(&mut vec_len_ref)?;

        let element_ref = &mut self.data[start_index..end_index];
        element.pack_into_slice(element_ref);
        Ok(())
    }

    /// Get an iterator for the type provided
    pub fn iter<'vec>(&'vec self) -> Iter<'data, 'vec, T> {
        Iter {
            len: self.len() as usize,
            current: 0,
            current_index: VEC_SIZE_BYTES,
            inner: self,
            phantom: PhantomData,
        }
    }

    /// Find matching data in the array
    pub fn find(&self, data: &[u8], predicate: fn(&[u8], &[u8]) -> bool) -> Option<&T> {
        let len = self.len() as usize;
        let mut current = 0;
        let mut current_index = VEC_SIZE_BYTES;
        while current != len {
            let end_index = current_index + T::LEN;
            let current_slice = &self.data[current_index..end_index];
            if predicate(current_slice, data) {
                return Some(unsafe { &*(current_slice.as_ptr() as *const T) });
            }
            current_index = end_index;
            current += 1;
        }
        None
    }
}

/// Iterator wrapper over a BigVec
pub struct Iter<'data, 'vec, T> {
    len: usize,
    current: usize,
    current_index: usize,
    inner: &'vec BigVec<'data, T>,
    phantom: PhantomData<T>,
}

impl<'data, 'vec, T: Pack + 'data> Iterator for Iter<'data, 'vec, T> {
    type Item = &'data T;

    fn next(&mut self) -> Option<Self::Item> {
        if self.current == self.len {
            None
        } else {
            let end_index = self.current_index + T::LEN;
            let value = Some(unsafe {
                &*(self.inner.data[self.current_index..end_index].as_ptr() as *const T)
            });
            self.current += 1;
            self.current_index = end_index;
            value
        }
    }
}

#[cfg(test)]
mod tests {
    use {
        super::*,
        solana_program::{program_memory::sol_memcmp, program_pack::Sealed},
    };

    #[derive(Debug, PartialEq, Clone)]
    struct TestStruct {
        value: u64,
    }

    impl Sealed for TestStruct {}

    impl Pack for TestStruct {
        const LEN: usize = 8;
        fn pack_into_slice(&self, data: &mut [u8]) {
            let mut data = data;
            self.value.serialize(&mut data).unwrap();
        }
        fn unpack_from_slice(src: &[u8]) -> Result<Self, ProgramError> {
            Ok(TestStruct {
                value: u64::try_from_slice(src).unwrap(),
            })
        }
    }

    impl TestStruct {
        fn new(value: u64) -> Self {
            Self { value }
        }
    }

    fn from_slice<'data, 'other>(
        data: &'data mut [u8],
        vec: &'other [u64],
    ) -> BigVec<'data, TestStruct> {
        let mut big_vec = BigVec::new(data);
        for element in vec {
            big_vec.push(TestStruct::new(*element)).unwrap();
        }
        big_vec
    }

    fn check_big_vec_eq(big_vec: &BigVec<TestStruct>, slice: &[u64]) {
        assert!(big_vec
            .iter()
            .map(|x| &x.value)
            .zip(slice.iter())
            .all(|(a, b)| a == b));
    }

    #[test]
    fn push() {
        let mut data = [0u8; 4 + 8 * 3];
        let mut v = BigVec::new(&mut data);
        v.push(TestStruct::new(1)).unwrap();
        check_big_vec_eq(&v, &[1]);
        v.push(TestStruct::new(2)).unwrap();
        check_big_vec_eq(&v, &[1, 2]);
        v.push(TestStruct::new(3)).unwrap();
        check_big_vec_eq(&v, &[1, 2, 3]);
        assert_eq!(
            v.push(TestStruct::new(4)).unwrap_err(),
            ProgramError::AccountDataTooSmall
        );
    }

    #[test]
    fn at_position() {
        let mut data = [0u8; 4 + 8 * 3];
        let mut v = from_slice(&mut data, &[1, 2, 3]);

        let elem = v.get_mut(0);
        assert_eq!(elem.unwrap().value, 1);

        let elem = v.get_mut(1).unwrap();
        assert_eq!(elem.value, 2);

        elem.value = 22;
        let elem = v.get_mut(1);
        assert_eq!(elem.unwrap().value, 22);

        let elem = v.get_mut(2);
        assert_eq!(elem.unwrap().value, 3);

        let elem = v.get_mut(3).unwrap_err();
        assert_eq!(elem, LidoError::AccountListIndexOutOfBounds.into());

        let mut data = [0u8; 4 + 0];
        let mut v = from_slice(&mut data, &[]);

        let elem = v.get_mut(0).unwrap_err();
        assert_eq!(elem, LidoError::AccountListIndexOutOfBounds.into());
    }

    #[test]
    fn swap_remove() {
        let mut data = [0u8; 4 + 8 * 4];
        let mut v = from_slice(&mut data, &[1, 2, 3, 4]);

        let elem = v.swap_remove(1);
        check_big_vec_eq(&v, &[1, 4, 3]);
        assert_eq!(elem.unwrap().value, 2);

        let elem = v.swap_remove(0);
        check_big_vec_eq(&v, &[3, 4]);
        assert_eq!(elem.unwrap().value, 1);

        let elem = v.swap_remove(2).unwrap_err();
        check_big_vec_eq(&v, &[3, 4]);
        assert_eq!(elem, LidoError::AccountListIndexOutOfBounds.into());

        let elem = v.swap_remove(1);
        check_big_vec_eq(&v, &[3]);
        assert_eq!(elem.unwrap().value, 4);

        let elem = v.swap_remove(0);
        check_big_vec_eq(&v, &[]);
        assert_eq!(elem.unwrap().value, 3);

        let elem = v.swap_remove(0).unwrap_err();
        check_big_vec_eq(&v, &[]);
        assert_eq!(elem, LidoError::AccountListIndexOutOfBounds.into());
    }

    fn find_predicate(a: &[u8], b: &[u8]) -> bool {
        if a.len() != b.len() {
            false
        } else {
            sol_memcmp(a, b, a.len()) == 0
        }
    }

    #[test]
    fn find() {
        let mut data = [0u8; 4 + 8 * 4];
        let v = from_slice(&mut data, &[1, 2, 3, 4]);
        assert_eq!(
            v.find(&1u64.to_le_bytes(), find_predicate),
            Some(&TestStruct::new(1))
        );
        assert_eq!(
            v.find(&4u64.to_le_bytes(), find_predicate),
            Some(&TestStruct::new(4))
        );
        assert_eq!(v.find(&5u64.to_le_bytes(), find_predicate), None);
    }
}
