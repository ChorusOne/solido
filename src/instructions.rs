use std::mem::size_of;

use solana_program::program_error::ProgramError;

use crate::model::InitArgs;

#[repr(C)]
#[derive(Clone, Debug, PartialEq)]
pub enum StakePoolInstruction {
    ///   0) Initializes a new StakePool.
    ///
    ///   0. `[w]` New StakePool to create.
    ///   1. `[s]` Owner
    ///   2. `[w]` Uninitialized validator stake list storage account
    ///   3. `[w]` Uninitialized credit list storage account
    ///   4. `[]` pool token Mint. Must be non zero, owned by withdraw authority.
    ///   5. `[]` Pool Account to deposit the generated fee for owner.
    ///   6. `[w]` Credit reserve token account
    ///   7. `[]` Clock sysvar
    ///   8. `[]` Rent sysvar
    ///   9. `[]` Token program id
    Initialize(InitArgs),

    ///   6) Deposit some stake into the pool.  The output is a "pool" token representing ownership
    ///   into the pool. Inputs are converted to the current ratio.
    ///
    ///   0. `[w]` Stake pool
    ///   1. `[]` Stake pool withdraw authority
    ///   2. `[w]` Reserve account (PDA)
    ///   3. `[ws?]` User account to take SOLs from (signed if not wrapped token)
    ///   4. `[w]` User account to receive pool tokens
    ///   5. `[w]` Account to receive pool fee tokens
    ///   6. `[w]` Pool token mint account
    ///   7. `[]` Rent sysvar
    ///   8. `[]` System program
    ///   9. `[]` Pool token program id,
    ///   in case of wrapped SOLs:
    ///   10. `[w]` Temp account (PDA)
    ///   11. `[w]` native token mint ("So11111111111111111111111111111111111111112")
    Deposit(u64),
}

impl StakePoolInstruction {
    /// Deserializes a byte buffer into an [StakePoolInstruction](enum.StakePoolInstruction.html).
    /// TODO efficient unpacking here
    pub fn deserialize(input: &[u8]) -> Result<Self, ProgramError> {
        if input.len() < size_of::<u8>() {
            return Err(ProgramError::InvalidAccountData);
        }
        Ok(match input[0] {
            0 => {
                let val: &InitArgs = unpack(input)?;
                Self::Initialize(*val)
            }
            1 => {
                let val: &u64 = unpack(input)?;
                Self::Deposit(*val)
            }
            _ => return Err(ProgramError::InvalidAccountData),
        })
    }
}

/// Unpacks a reference from a bytes buffer.
pub fn unpack<T>(input: &[u8]) -> Result<&T, ProgramError> {
    if input.len() < size_of::<u8>() + size_of::<T>() {
        return Err(ProgramError::InvalidAccountData);
    }
    #[allow(clippy::cast_ptr_alignment)]
    let val: &T = unsafe { &*(&input[1] as *const u8 as *const T) };
    Ok(val)
}
