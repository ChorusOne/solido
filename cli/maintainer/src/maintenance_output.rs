use serde::Serialize;

/// A brief description of the maintenance performed. Not relevant functionally,
/// but helpful for automated testing, and just for info.
#[derive(Debug, Eq, PartialEq, Serialize)]
pub enum MaintenanceOutput {
    StakeDeposit {
        #[serde(serialize_with = "serialize_b58")]
        validator_vote_account: Pubkey,

        #[serde(serialize_with = "serialize_b58")]
        stake_account: Pubkey,

        #[serde(rename = "amount_lamports")]
        amount: Lamports,
    },

    UpdateExchangeRate,

    WithdrawInactiveStake {
        /// The vote account of the validator that we want to update.
        #[serde(serialize_with = "serialize_b58")]
        validator_vote_account: Pubkey,

        /// The expected difference that the update will observe.
        ///
        /// This is only an expected value, because a different transaction might
        /// execute between us observing the state and concluding that there is
        /// a difference, and our `WithdrawInactiveStake` instruction executing.
        #[serde(rename = "expected_difference_stake_lamports")]
        expected_difference_stake: Lamports,

        #[serde(rename = "unstake_withdrawn_to_reserve_lamports")]
        unstake_withdrawn_to_reserve: Lamports,
    },

    CollectValidatorFee {
        #[serde(serialize_with = "serialize_b58")]
        validator_vote_account: Pubkey,
        #[serde(rename = "fee_rewards_lamports")]
        fee_rewards: Lamports,
    },

    ClaimValidatorFee {
        #[serde(serialize_with = "serialize_b58")]
        validator_vote_account: Pubkey,
        #[serde(rename = "fee_rewards_st_lamports")]
        fee_rewards: StLamports,
    },

    MergeStake {
        #[serde(serialize_with = "serialize_b58")]
        validator_vote_account: Pubkey,
        #[serde(serialize_with = "serialize_b58")]
        from_stake: Pubkey,
        #[serde(serialize_with = "serialize_b58")]
        to_stake: Pubkey,
        from_stake_seed: u64,
        to_stake_seed: u64,
    },

    UnstakeFromInactiveValidator(Unstake),
    RemoveValidator {
        #[serde(serialize_with = "serialize_b58")]
        validator_vote_account: Pubkey,
    },
    UnstakeFromActiveValidator(Unstake),

    SellRewards {
        st_sol_amount: StLamports,
    },
}

#[derive(Debug, Eq, PartialEq, Serialize)]
pub struct Unstake {
    #[serde(serialize_with = "serialize_b58")]
    validator_vote_account: Pubkey,
    #[serde(serialize_with = "serialize_b58")]
    from_stake_account: Pubkey,
    #[serde(serialize_with = "serialize_b58")]
    to_unstake_account: Pubkey,
    from_stake_seed: u64,
    to_unstake_seed: u64,
    amount: Lamports,
}

impl fmt::Display for Unstake {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        writeln!(
            f,
            "  Validator vote account: {}",
            self.validator_vote_account
        )?;
        writeln!(
            f,
            "  Stake account:               {}, seed: {}",
            self.from_stake_account, self.from_stake_seed
        )?;
        writeln!(
            f,
            "  Unstake account:             {}, seed: {}",
            self.to_unstake_account, self.to_unstake_seed
        )?;
        writeln!(f, "  Amount:              {}", self.amount)?;
        Ok(())
    }
}

impl fmt::Display for MaintenanceOutput {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            MaintenanceOutput::StakeDeposit {
                validator_vote_account,
                stake_account,
                amount,
            } => {
                writeln!(f, "Staked deposit.")?;
                writeln!(f, "  Validator vote account: {}", validator_vote_account)?;
                writeln!(f, "  Stake account:          {}", stake_account)?;
                writeln!(f, "  Amount staked:          {}", amount)?;
            }
            MaintenanceOutput::UpdateExchangeRate => {
                writeln!(f, "Updated exchange rate.")?;
            }
            MaintenanceOutput::WithdrawInactiveStake {
                validator_vote_account,
                expected_difference_stake,
                unstake_withdrawn_to_reserve,
            } => {
                writeln!(f, "Withdrew inactive stake.")?;
                writeln!(
                    f,
                    "  Validator vote account:        {}",
                    validator_vote_account
                )?;
                writeln!(
                    f,
                    "  Expected difference in stake:  {}",
                    expected_difference_stake
                )?;
                writeln!(
                    f,
                    "  Amount withdrawn from unstake: {}",
                    unstake_withdrawn_to_reserve
                )?;
            }
            MaintenanceOutput::CollectValidatorFee {
                validator_vote_account,
                fee_rewards,
            } => {
                writeln!(f, "Collected validator fees.")?;
                writeln!(f, "  Validator vote account: {}", validator_vote_account)?;
                writeln!(f, "  Collected fee rewards:  {}", fee_rewards)?;
            }

            MaintenanceOutput::ClaimValidatorFee {
                validator_vote_account,
                fee_rewards,
            } => {
                writeln!(f, "Claimed validator fees.")?;
                writeln!(f, "  Validator vote account: {}", validator_vote_account)?;
                writeln!(f, "  Claimed fee:            {}", fee_rewards)?;
            }
            MaintenanceOutput::MergeStake {
                validator_vote_account,
                from_stake,
                to_stake,
                from_stake_seed,
                to_stake_seed,
            } => {
                writeln!(f, "Stake accounts merged")?;
                writeln!(f, "  Validator vote account: {}", validator_vote_account)?;
                writeln!(
                    f,
                    "  From stake:             {}, seed: {}",
                    from_stake, from_stake_seed
                )?;
                writeln!(
                    f,
                    "  To stake:               {}, seed: {}",
                    to_stake, to_stake_seed
                )?;
            }
            MaintenanceOutput::UnstakeFromInactiveValidator(unstake) => {
                writeln!(f, "Unstake from inactive validator\n{}", unstake)?;
            }
            MaintenanceOutput::UnstakeFromActiveValidator(unstake) => {
                writeln!(f, "Unstake from active validator\n{}", unstake)?;
            }
            MaintenanceOutput::RemoveValidator {
                validator_vote_account,
            } => {
                writeln!(f, "Remove validator")?;
                writeln!(f, "  Validator vote account: {}", validator_vote_account)?;
            }
            MaintenanceOutput::SellRewards { st_sol_amount } => {
                writeln!(f, "Sell stSOL rewards")?;
                writeln!(f, "  Amount:               {}", st_sol_amount)?;
            }
        }
        Ok(())
    }
}
