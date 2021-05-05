## Fee Management 


Status - As of 6 May 2021 
- First Draft. To be discussed with Fynn. 

### What do we want to achieve?

- We want to take X% of the staking rewards as fees and distribute them across Lido stakeholders. 
- X is set by Lido Governance (default : 10%) 
- Who are these stakeholders and what is the distribution pattern?
    - Insurance Fund : a% (of the X%)
    - Treasury Fund : b% 
    - Chorus One Fund : c% 
    - Validator Fund : d% (Further, this is distributed equally across the current list of validators)
    
   - (a+b+c+d) = 100% 

- TODO : Check with Felix about Chorus One 

- Following from the design in Lido for ETH, ideally the fee would be distributed across stakeholders  in the form of LSOL tokens. 

--- 

### How do we achieve this? 
- Jon has added a generic `Fee Cut on Rewards` Mechanism to the Stake Pool Program 

	- the mechanism ensures fee goes to a fee beneficiary : in the form of stake pool tokens. 

   	- we need to make sure this mechanism is configured correctly in our deployment. 
		- eg. Fee Cut, Beneficiary

- Additionally, we need functionality to distribute these fees across all stakeholders in the form of LSOL tokens. 
- This would require additions to the Lido Program 



--- 

### Context : Stake Pool Program 


- In the Stake Pool program, one can set the Fee percentage on initialisation - with the relevant parameter being ```stake_pool.fee``` 

- Also, on initialisation, one can set the Fee Beneficiary : ```stake_pool.manager_fee_account```
- Everytime ```UpdateStakePoolBalance``` instruction is called, the latest staking rewards are calculated and `Fee Percentage` of these new rewards go to the `Fee Beneficiary` in the form of Stake Pool Tokens

- Additionally, manager can update these params post-deployment of the Stake Pool.  

#### How do we configure this correctly to achieve our objective?

- Set the Fee percentage as X (numerator, denominator) on initialisation (highly likely : the default value of X will be 10%) 
- Set the Fee Beneficiary to be the `Reward Authority PDA of the Lido Program` 

- We want the MultiSig Governance to be able to update these two params
-  In general, we will set `Manager of Stake Pool` to be our MultiSig Governance program - so this should be taken care of. 


--- 

### Context : Lido Program 

#### What additions do we need in the Lido Program?

1. Create a Rewards Authority (PDA) in the Lido Program 
2. Set it as Fee Beneficiary in the Stake Pool Program 
3. Periodically*, this Rewards Authority will receive fees in the form of Stake Pool Tokens. 
4. The intent is to distribute this fees across relevant Lido stakeholders in the form of LSOL.


#### Configuration Params required in Lido Program (can be changed by governance) 

* Account Information For 
	- InsuranceFund
	- TreasuryFund
	- ChorusOneFund

* Params (Save in Lido State. Editable by Governance. With Defaults) 
	- InsuranceFee  	`(a%)`
	- TreasuryFee  	`(b%)`
	- ChorusFee    	`(c%)`
	- ValidatorFee 	`(d%)`
	- We can store all as (numerator, denominator) pairs with the 4 fractions adding up to 1.
        
####  Instructions required in Lido Program 

 
We need the following instructions (and corresponding ```process_*``` functions)

#### A. Instructions to update above parameters


#### B. Instructions to distribute fees (as LSOL tokens) 

- My understanding here is - because of Solana transaction limits - we will need 2 instructions here.

- `DistributeFees` : that calculates fees for each stakeholder (in LSOL) and marks it in their name. 
    - This will require maintaining a data structure - effectively a map from `Recipient` to `LSOL Amount they are entitled to`. 


- `WithdrawFees` : with this, anyone can withdraw the LSOL fees  to a recipient (upto the amount the recipient are entitled to) 

- TODO : Confirm with Fynn : Both DistributeFees and WithdrawFees can't happen in one step as number of recipients is 3+V (and we don't want to bound V to `< 10` for sure) 


#### DistributeFees
- ideally called by a bot every epoch 
- mints new LSOL in lieu of the (stake pool token) fees received 
- this LSOL is minted to the Rewards Authority, in lieu of which the Rewards authority transfers the stake pool token it received to the deposit authority. 
- updates the list of who is entitled to withdraw how many LSOL as fees (list contains Insurance, Treasury, Chorus One and all validators as recipients) 


#### WithdrawFees(recipient, amount) 
- DistributeFees just transfers all newly minted LSOL to the Rewards Authority
- And updates the map of which stakeholder is entitled to how much LSOL
- With the WithdrawFees function, claimants can withdraw the LSOL they are entitled to (in the map) from the Rewards Authority 
	- note : this can be called by anyone, not just the recipient
	- will try to transfer LSOL rewards from Rewards Authority to the recipient
		-  if they are entitled to them as part of the list maintained by DistributeRewards. 




### DistributeFees : In Detail 

- Starting State : Rewards Authority has received `t` stake pool tokens (as fee beneficiary of the Stake Pool Program)

- 1. Transfer t new rewards from Reward Authority to Deposit Authority 
- 2. Mint new LSOL s.t. `t/X = new LSOL to be minted / current total LSOL`
- 3. Ensure these new LSOL tokens are minted to the Rewards Authority 
- 4. For the LSOL rewards, we Maintain a map of which stakeholder will get how much LSOL
	- Update the map to reflect who all are entitled to these newly minted LSOL (and how much) 
		- Insurance : a%
		- Treasury : b%
		- Chorus One : c%
		- Validators - Refer latest list in Stake Pool Program and distribute d% across the latest list of validators 
        - TODO : Check with Fynn about map datastructure. Is map DS supported? Storage limits? This might bound number of Valdiators (~100ish?)


---- 



### What needs to be called periodically?

A. PoolBalanceUpdate of Stake Pool Program  (once every epoch by a bot)
B. DistributeFees of Lido Program  (once every epoch by a bot)
C. WithdrawFees(recipient, amount) - possibly the bot can call this periodically for all 3+V stakeholders - to keep the storage light?




---- 


### WIP Notes

Following notes are quite WIP-ish. 

Dependents


B. Add / Remove Validator 
- Note : If validator list is updated in an epoch, it's best to check if `PoolBalanceUpdate` (stake pool program) and `DistributeFees` (lido program) have occurred in this epoch. 
	- The former has a check flag in Stake Pool Program 
	-  The latter can be checked by `Stake Pool Token Balance of Reward Authority === 0`
	-  We need (Former && Latter) 



--- 
TODO : Check with Vasily
- How do we address the distribution of rewards across validators - in the case where they have unequal non-zero commission rates set already?


--- 


